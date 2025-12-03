use std::sync::Arc;

use serenity::{
    all::{CommandInteraction, Http},
    builder::{CreateCommand, CreateInteractionResponse, CreateInteractionResponseMessage},
};
use tokio::sync::Mutex;

use crate::ai::Ai;
use crate::commands::CommandRegistry;
use crate::currency::CurrencyConverter;

pub struct Handler {
    lua_state: Arc<Mutex<mlua::Lua>>,
    command_registry: CommandRegistry,
    ai: Arc<Ai>,
    currency_converter: Arc<CurrencyConverter>,
    reload_tx: flume::Sender<()>,
}

impl Handler {
    pub fn new(
        lua_state: Arc<Mutex<mlua::Lua>>,
        command_registry: CommandRegistry,
        ai: Arc<Ai>,
        currency_converter: Arc<CurrencyConverter>,
        reload_tx: flume::Sender<()>,
    ) -> Self {
        Self {
            lua_state,
            command_registry,
            ai,
            currency_converter,
            reload_tx,
        }
    }
}

#[serenity::async_trait]
impl super::CommandHandler for Handler {
    async fn register(&self, http: &Http) -> anyhow::Result<()> {
        let cmd = CreateCommand::new("reload")
            .description("Reload all Lua scripts and re-register commands");
        serenity::all::Command::create_global_command(http, cmd).await?;
        Ok(())
    }

    async fn run(&self, http: &Http, cmd: &CommandInteraction) -> anyhow::Result<()> {
        cmd.create_response(
            http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new().content("Reloading scripts..."),
            ),
        )
        .await?;

        // Clear the command registry
        self.command_registry.lock().unwrap().clear();

        // Create new output channels for the global Lua state
        let (output_tx, _output_rx) = flume::unbounded::<String>();
        let (print_tx, _print_rx) = flume::unbounded::<String>();

        // Recreate the global Lua state
        let new_lua = crate::commands::execute::create_global_lua_state(
            self.ai.clone(),
            self.currency_converter.clone(),
            output_tx,
            print_tx,
            self.command_registry.clone(),
        );

        match new_lua {
            Ok(new_lua) => {
                *self.lua_state.lock().await = new_lua;

                // Signal to re-register commands
                let _ = self.reload_tx.send(());

                cmd.edit_response(
                    http,
                    serenity::all::EditInteractionResponse::new().content(format!(
                        "Successfully reloaded scripts and registered {} commands!",
                        self.command_registry.lock().unwrap().len()
                    )),
                )
                .await?;
            }
            Err(e) => {
                cmd.edit_response(
                    http,
                    serenity::all::EditInteractionResponse::new()
                        .content(format!("Failed to reload scripts: {}", e)),
                )
                .await?;
            }
        }

        Ok(())
    }
}
