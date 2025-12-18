use std::{collections::HashMap, collections::HashSet, sync::Arc};

use anyhow::Context as AnyhowContext;
use mlua::LuaSerdeExt as _;
use serenity::{
    Client,
    all::{
        AutocompleteChoice, Command, Context, CreateAutocompleteResponse,
        CreateInteractionResponse, CreateInteractionResponseMessage, EventHandler, Http,
        Interaction, Message, MessageId, Ready,
    },
    async_trait,
    model::prelude::GatewayIntents,
};

mod ai;
mod cancel;
mod commands;
mod config;
mod constant;
mod currency;
mod interaction_context;
mod lua;
mod outputter;
mod reply_handler;
mod util;

use config::Configuration;

use crate::{
    commands::lua_command::LuaCommandRegistry,
    interaction_context::InteractionContextStore,
    lua::{LuaReplyHandlerRegistry, create_global_lua_state},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Configuration::load()?;
    let discord_token = config
        .authentication
        .discord_token
        .as_deref()
        .context("Expected authentication.discord_token to be filled in config")?;

    let ai = Arc::new(ai::Ai::load(&config).await?);
    let currency_converter = Arc::new(currency::CurrencyConverter::new());

    let (cancel_tx, cancel_rx) = flume::unbounded::<MessageId>();

    // Create command registry, reply handler registry, and interaction context store
    let command_registry = LuaCommandRegistry::default();
    let reply_handler_registry = LuaReplyHandlerRegistry::default();
    let interaction_context_store = Arc::new(InteractionContextStore::new(
        config.discord.interaction_context_cache_size,
    ));

    // We intentionally do not use _output_rx/_attachment_rx, as we don't care about temporary output at the global level
    let (output_tx, _output_rx) = flume::unbounded::<String>();
    let (print_tx, print_rx) = flume::unbounded::<String>();
    let (attachment_tx, _attachment_rx) = flume::unbounded::<lua::extensions::Attachment>();

    tokio::spawn(async move {
        while let Ok(print) = print_rx.recv_async().await {
            println!("Global Lua print: {print}");
        }
    });

    let global_lua = create_global_lua_state(
        ai.clone(),
        currency_converter.clone(),
        output_tx,
        print_tx,
        attachment_tx,
        command_registry.clone(),
        reply_handler_registry.clone(),
    )?;

    // Build handlers
    let handlers = build_handlers(
        &config,
        cancel_rx.clone(),
        ai.clone(),
        currency_converter.clone(),
        global_lua.clone(),
        command_registry.clone(),
        interaction_context_store.clone(),
    );

    let mut client = Client::builder(
        discord_token,
        GatewayIntents::default()
            | GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT,
    )
    .event_handler(Handler {
        config: config.clone(),
        handlers: Arc::new(std::sync::Mutex::new(handlers)),
        cancel_tx,
        cancel_rx,
        interaction_context_store: interaction_context_store.clone(),
        reply_handler_registry: reply_handler_registry.clone(),
        global_lua: global_lua.clone(),
        command_registry: command_registry.clone(),
    })
    .await
    .context("Error creating client")?;

    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }

    Ok(())
}

fn build_handlers(
    config: &Configuration,
    cancel_rx: flume::Receiver<MessageId>,
    ai: Arc<ai::Ai>,
    currency_converter: Arc<currency::CurrencyConverter>,
    global_lua: mlua::Lua,
    command_registry: LuaCommandRegistry,
    interaction_context_store: Arc<InteractionContextStore>,
) -> HashMap<String, Arc<dyn commands::CommandHandler>> {
    let mut handlers: HashMap<String, Arc<dyn commands::CommandHandler>> = HashMap::new();

    // Add execute commands
    let execute_state = Arc::new(commands::execute::SharedState::new(
        config.discord.clone(),
        cancel_rx.clone(),
        ai.clone(),
        currency_converter.clone(),
    ));
    handlers.insert(
        "execute".to_string(),
        Arc::new(commands::execute::Handler::new(execute_state.clone())),
    );
    handlers.insert(
        "executemsg".to_string(),
        Arc::new(commands::execute::MsgHandler::new(execute_state)),
    );

    // Add Lua commands from registry
    let command_names: Vec<String> = command_registry.lock().unwrap().keys().cloned().collect();

    for name in command_names {
        handlers.insert(
            name.clone(),
            Arc::new(commands::lua_command::Handler::new(
                name,
                config.discord.clone(),
                command_registry.clone(),
                global_lua.clone(),
                interaction_context_store.clone(),
            )),
        );
    }

    handlers
}

pub struct Handler {
    config: Configuration,
    handlers: Arc<std::sync::Mutex<HashMap<String, Arc<dyn commands::CommandHandler>>>>,
    cancel_tx: flume::Sender<MessageId>,
    cancel_rx: flume::Receiver<MessageId>,
    interaction_context_store: Arc<InteractionContextStore>,
    reply_handler_registry: LuaReplyHandlerRegistry,
    global_lua: mlua::Lua,
    command_registry: LuaCommandRegistry,
}
#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        self.ready_impl(&ctx.http, ready)
            .await
            .expect("Error while registering commands");
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let Some(respondable) = util::interaction_to_respondable_interaction(&interaction) else {
            panic!("Unknown interaction type: {interaction:?}");
        };

        if let Err(err) = self
            .interaction_create_impl(ctx.http.clone(), &interaction)
            .await
        {
            respondable
                .create_or_edit(&ctx.http, &format!("Error: {err}"))
                .await
                .unwrap();
        }
    }

    async fn message(&self, ctx: Context, msg: Message) {
        // Ignore messages from bots
        if msg.author.bot {
            return;
        }

        // Check if this is a reply to another message
        if let Some(ref msg_ref) = msg.message_reference
            && let Some(referenced_msg_id) = msg_ref.message_id
            && let Err(err) = self
                .handle_reply(ctx.http.clone(), &msg, referenced_msg_id)
                .await
        {
            eprintln!("Error handling reply: {err}");
        }
    }
}
impl Handler {
    async fn ready_impl(&self, http: &Http, ready: Ready) -> anyhow::Result<()> {
        println!("{} is connected; registering commands...", ready.user.name);
        register_all_commands(http, &self.handlers).await?;
        println!("{} is good to go!", ready.user.name);
        Ok(())
    }

    async fn interaction_create_impl(
        &self,
        http: Arc<Http>,
        interaction: &Interaction,
    ) -> anyhow::Result<()> {
        match interaction {
            Interaction::Command(cmd) => {
                let name = cmd.data.name.as_str();
                let handler = self.handlers.lock().unwrap().get(name).cloned();

                if let Some(handler) = handler {
                    handler.run(http.clone(), cmd).await?;
                } else {
                    anyhow::bail!("no handler found for command: {name}");
                }
            }
            Interaction::Component(cmp) => {
                if let Some((message_id, user_id)) = cancel::parse_id(&cmp.data.custom_id) {
                    if cmp.user.id != user_id {
                        return Ok(());
                    }

                    self.cancel_tx.send(message_id).ok();
                    cmp.create_response(
                        &*http,
                        CreateInteractionResponse::UpdateMessage(
                            CreateInteractionResponseMessage::new(),
                        ),
                    )
                    .await
                    .ok();
                }
            }
            Interaction::Autocomplete(auto) => {
                let command_name = &auto.data.name;

                if let Some(focused) = auto.data.autocomplete() {
                    let option_name = focused.name;
                    let partial_value = focused.value;

                    // Look up the command and option to get suggestions
                    let suggestions = self
                        .command_registry
                        .lock()
                        .unwrap()
                        .get(command_name)
                        .and_then(|cmd| cmd.options.iter().find(|o| o.name == option_name))
                        .map(|opt| &opt.suggestions)
                        .cloned()
                        .unwrap_or_default();

                    // Filter suggestions by partial input (case-insensitive)
                    let partial_lower = partial_value.to_lowercase();
                    let choices: Vec<AutocompleteChoice> = suggestions
                        .iter()
                        .filter(|(name, _)| name.to_lowercase().contains(&partial_lower))
                        .map(|(name, value)| AutocompleteChoice::new(name.clone(), value.clone()))
                        .collect();

                    auto.create_response(
                        &*http,
                        CreateInteractionResponse::Autocomplete(
                            CreateAutocompleteResponse::new().set_choices(choices),
                        ),
                    )
                    .await
                    .ok();
                }
            }
            _ => {}
        };
        Ok(())
    }

    async fn handle_reply(
        &self,
        http: Arc<Http>,
        user_msg: &Message,
        referenced_msg_id: MessageId,
    ) -> anyhow::Result<()> {
        use crate::{
            lua::extensions::{Attachment, TemporaryChannelUpdate},
            lua::{LuaOutputChannels, execute_lua_reply_thread},
            reply_handler::{LuaReplyChain, ReplyChain, build_message_chain},
        };

        // First, check if the referenced message is in our interaction context store
        let context = match self.interaction_context_store.get(&referenced_msg_id) {
            Some(ctx) => ctx,
            None => {
                // Try to fetch the message and walk up the chain to find a cached context
                let referenced_msg = user_msg
                    .channel_id
                    .message(&http, referenced_msg_id)
                    .await?;

                // Build the chain and look for any cached context
                let chain = build_message_chain(&http, &referenced_msg, 50).await?;

                // Find a bot message with cached context
                let mut found_context = None;
                for chain_msg in &chain {
                    if chain_msg.is_bot
                        && let Some(ctx) = self.interaction_context_store.get(&chain_msg.id)
                    {
                        found_context = Some(ctx);
                        break;
                    }
                }

                match found_context {
                    Some(ctx) => ctx,
                    None => {
                        // No cached context found - ignore this reply
                        return Ok(());
                    }
                }
            }
        };

        // Check if there's a handler for this command
        let handler = self
            .reply_handler_registry
            .lock()
            .unwrap()
            .get(&context.command_name)
            .cloned();

        let Some(handler) = handler else {
            // No handler registered for this command
            return Ok(());
        };

        // Build the full message chain
        let chain = build_message_chain(&http, user_msg, 50).await?;

        // Create the ReplyChain
        let reply_chain = ReplyChain {
            command_name: context.command_name.clone(),
            options: context.options.clone(),
            messages: chain,
        };

        // Create output channels for this execution
        let (output_tx, output_rx) = flume::unbounded::<String>();
        let (print_tx, print_rx) = flume::unbounded::<String>();
        let (attachment_tx, attachment_rx) = flume::unbounded::<Attachment>();

        // Build the Lua table for the reply chain using serde
        let lua = &self.global_lua;
        let chain_table = lua.to_value(&LuaReplyChain::from(&reply_chain))?;

        // Create the thread and register channels
        let thread = lua.create_thread(handler)?;
        let _temporary_channel_update =
            TemporaryChannelUpdate::new(lua.clone(), &thread, output_tx, print_tx, attachment_tx)?;

        let thread = thread.into_async::<Option<String>>(chain_table)?;

        // Execute and get the response message ID
        let response_msg_id = execute_lua_reply_thread(
            http.clone(),
            user_msg,
            user_msg.author.id,
            &self.config.discord,
            thread,
            LuaOutputChannels {
                output_rx,
                print_rx,
                attachment_rx,
            },
            Some(self.cancel_rx.clone()),
        )
        .await?;

        // Store the context for the new response message so the chain can continue
        self.interaction_context_store
            .store(response_msg_id, context);

        Ok(())
    }
}

/// Registers all commands with Discord, clearing existing commands if they differ
async fn register_all_commands(
    http: &Http,
    handlers: &Arc<std::sync::Mutex<HashMap<String, Arc<dyn commands::CommandHandler>>>>,
) -> anyhow::Result<()> {
    // Check if we need to reset our registered commands
    let registered_commands: HashSet<_> = {
        let cmds = Command::get_global_commands(http).await?;
        cmds.iter().map(|c| c.name.clone()).collect()
    };
    let our_commands: HashSet<_> = handlers.lock().unwrap().keys().cloned().collect();
    if registered_commands != our_commands {
        Command::set_global_commands(http, vec![]).await?;
    }

    // Collect handlers to avoid holding lock across await
    let handlers_vec: Vec<_> = handlers.lock().unwrap().values().cloned().collect();
    for handler in handlers_vec {
        handler.register(http).await?;
    }

    Ok(())
}
