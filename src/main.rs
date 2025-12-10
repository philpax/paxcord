use std::{collections::HashMap, collections::HashSet, sync::Arc};

use anyhow::Context as AnyhowContext;
use serenity::{
    Client,
    all::{
        Command, Context, CreateInteractionResponse, CreateInteractionResponseMessage,
        EventHandler, Http, Interaction, MessageId, Ready,
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
mod lua;
mod outputter;
mod util;

use config::Configuration;

use crate::{commands::lua_command::LuaCommandRegistry, lua::create_global_lua_state};

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

    // Create command registry and global Lua state
    let command_registry = LuaCommandRegistry::default();
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
    )?;

    // Build handlers
    let handlers = build_handlers(
        &config,
        cancel_rx.clone(),
        ai.clone(),
        currency_converter.clone(),
        global_lua.clone(),
        command_registry.clone(),
    );

    let mut client = Client::builder(discord_token, GatewayIntents::default())
        .event_handler(Handler {
            handlers: Arc::new(std::sync::Mutex::new(handlers)),
            cancel_tx,
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
            )),
        );
    }

    handlers
}

pub struct Handler {
    handlers: Arc<std::sync::Mutex<HashMap<String, Arc<dyn commands::CommandHandler>>>>,
    cancel_tx: flume::Sender<MessageId>,
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

        if let Err(err) = self.interaction_create_impl(&ctx.http, &interaction).await {
            respondable
                .create_or_edit(&ctx.http, &format!("Error: {err}"))
                .await
                .unwrap();
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
        http: &Http,
        interaction: &Interaction,
    ) -> anyhow::Result<()> {
        match interaction {
            Interaction::Command(cmd) => {
                let name = cmd.data.name.as_str();
                let handler = self.handlers.lock().unwrap().get(name).cloned();

                if let Some(handler) = handler {
                    handler.run(http, cmd).await?;
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
                        http,
                        CreateInteractionResponse::UpdateMessage(
                            CreateInteractionResponseMessage::new(),
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
