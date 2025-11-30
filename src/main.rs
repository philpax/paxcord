use std::{collections::HashMap, collections::HashSet, sync::Arc};

use anyhow::Context as AnyhowContext;
use parking_lot::Mutex;
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
mod outputter;
mod util;

use config::Configuration;

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
    let (reload_tx, reload_rx) = flume::unbounded::<()>();

    // Create command registry and global Lua state
    let command_registry = commands::lua_registry::create_registry();
    let (output_tx, _output_rx) = flume::unbounded::<String>();
    let (print_tx, _print_rx) = flume::unbounded::<String>();

    let global_lua = Arc::new(Mutex::new(commands::execute::create_global_lua_state(
        ai.clone(),
        currency_converter.clone(),
        output_tx,
        print_tx,
        command_registry.clone(),
    )?));

    // Build handlers
    let handlers = build_handlers(
        &config,
        cancel_rx.clone(),
        reload_tx.clone(),
        ai.clone(),
        currency_converter.clone(),
        global_lua.clone(),
        command_registry.clone(),
    );

    let mut client = Client::builder(discord_token, GatewayIntents::default())
        .event_handler(Handler {
            handlers: Arc::new(Mutex::new(handlers)),
            cancel_tx,
            reload_rx,
            config: config.clone(),
            ai: ai.clone(),
            currency_converter: currency_converter.clone(),
            global_lua: global_lua.clone(),
            command_registry: command_registry.clone(),
            reload_tx,
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
    reload_tx: flume::Sender<()>,
    ai: Arc<ai::Ai>,
    currency_converter: Arc<currency::CurrencyConverter>,
    global_lua: Arc<Mutex<mlua::Lua>>,
    command_registry: commands::CommandRegistry,
) -> HashMap<String, Arc<dyn commands::CommandHandler>> {
    let mut handlers: HashMap<String, Arc<dyn commands::CommandHandler>> = HashMap::new();

    // Add execute command
    let base = commands::execute::Handler::new(
        config.discord.clone(),
        cancel_rx.clone(),
        ai.clone(),
        currency_converter.clone(),
    );
    handlers.insert(
        "execute".to_string(),
        Arc::new(commands::execute::slash::Handler::new(base)),
    );

    // Add reload command
    handlers.insert(
        "reload".to_string(),
        Arc::new(commands::reload::Handler::new(
            global_lua.clone(),
            command_registry.clone(),
            ai.clone(),
            currency_converter.clone(),
            reload_tx,
        )),
    );

    // Add Lua commands from registry
    let lua_commands = command_registry.lock().clone();
    for cmd in lua_commands {
        let name = cmd.name.clone();
        handlers.insert(
            name.clone(),
            Arc::new(commands::lua_command::Handler::new(
                name,
                config.discord.clone(),
                cmd,
                ai.clone(),
                currency_converter.clone(),
            )),
        );
    }

    handlers
}

pub struct Handler {
    handlers: Arc<Mutex<HashMap<String, Arc<dyn commands::CommandHandler>>>>,
    cancel_tx: flume::Sender<MessageId>,
    reload_rx: flume::Receiver<()>,
    config: Configuration,
    ai: Arc<ai::Ai>,
    currency_converter: Arc<currency::CurrencyConverter>,
    global_lua: Arc<Mutex<mlua::Lua>>,
    command_registry: commands::CommandRegistry,
    reload_tx: flume::Sender<()>,
}
#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        self.ready_impl(&ctx.http, ready)
            .await
            .expect("Error while registering commands");

        // Spawn reload handler
        let http = ctx.http.clone();
        let reload_rx = self.reload_rx.clone();
        let handlers = self.handlers.clone();
        let config = self.config.clone();
        let cancel_rx = flume::unbounded::<MessageId>().1; // New receiver for reloaded handlers
        let reload_tx = self.reload_tx.clone();
        let ai = self.ai.clone();
        let currency_converter = self.currency_converter.clone();
        let global_lua = self.global_lua.clone();
        let command_registry = self.command_registry.clone();

        tokio::spawn(async move {
            while reload_rx.recv_async().await.is_ok() {
                println!("Reload signal received, re-registering commands...");

                // Rebuild handlers
                let new_handlers = build_handlers(
                    &config,
                    cancel_rx.clone(),
                    reload_tx.clone(),
                    ai.clone(),
                    currency_converter.clone(),
                    global_lua.clone(),
                    command_registry.clone(),
                );

                // Update handlers
                *handlers.lock() = new_handlers;

                // Re-register all commands
                if let Err(e) = Command::set_global_commands(&http, vec![]).await {
                    eprintln!("Error clearing commands: {}", e);
                    continue;
                }

                // Collect handlers to avoid holding lock across await
                let handlers_vec: Vec<_> = handlers.lock().values().cloned().collect();
                for handler in handlers_vec {
                    if let Err(e) = handler.register(&http).await {
                        eprintln!("Error registering command {}: {}", handler.name(), e);
                    }
                }

                println!("Commands re-registered successfully!");
            }
        });
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

        // Check if we need to reset our registered commands
        let registered_commands: HashSet<_> = {
            let cmds = Command::get_global_commands(http).await?;
            cmds.iter().map(|c| c.name.clone()).collect()
        };
        let our_commands: HashSet<_> = self.handlers.lock().keys().cloned().collect();
        if registered_commands != our_commands {
            Command::set_global_commands(http, vec![]).await?;
        }

        // Collect handlers to avoid holding lock across await
        let handlers_vec: Vec<_> = self.handlers.lock().values().cloned().collect();
        for handler in handlers_vec {
            handler.register(http).await?;
        }

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
                let handler = self.handlers.lock().get(name).cloned();

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
