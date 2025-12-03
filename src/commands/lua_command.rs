use std::sync::Arc;

use serenity::all::{CommandDataOptionValue, CommandInteraction, Http};
use tokio::sync::Mutex;

use crate::{
    commands::{execute::extensions::TemporaryChannelUpdate, lua_registry::CommandRegistry},
    config,
};

use super::lua_executor::execute_lua_thread;

pub struct Handler {
    name: String,
    discord_config: config::Discord,
    command_registry: CommandRegistry,
    global_lua: Arc<Mutex<mlua::Lua>>,
}

impl Handler {
    pub fn new(
        name: String,
        discord_config: config::Discord,
        command_registry: CommandRegistry,
        global_lua: Arc<Mutex<mlua::Lua>>,
    ) -> Self {
        Self {
            name,
            discord_config,
            command_registry,
            global_lua,
        }
    }
}

#[serenity::async_trait]
impl super::CommandHandler for Handler {
    async fn register(&self, http: &Http) -> anyhow::Result<()> {
        let command_spec = self
            .command_registry
            .lock()
            .unwrap()
            .get(&self.name)
            .map(|cmd| cmd.to_discord_command());

        if let Some(cmd) = command_spec {
            serenity::all::Command::create_global_command(http, cmd).await?;
        }
        Ok(())
    }

    #[allow(clippy::await_holding_lock)]
    async fn run(&self, http: &Http, cmd: &CommandInteraction) -> anyhow::Result<()> {
        // Create output/print channels for this execution
        let (output_tx, output_rx) = flume::unbounded::<String>();
        let (print_tx, print_rx) = flume::unbounded::<String>();

        // Lock the global Lua state for this execution (held for entire duration)
        let lua = self.global_lua.lock().await;

        // Update output channels for this execution
        let _temporary_channel_update = TemporaryChannelUpdate::new(&lua, output_tx, print_tx)?;

        // Build interaction table
        let interaction = lua.create_table()?;
        let options = lua.create_table()?;

        // Parse options from Discord interaction
        for opt in &cmd.data.options {
            let value = &opt.value;
            match value {
                CommandDataOptionValue::String(s) => {
                    options.set(opt.name.as_str(), s.clone())?;
                }
                CommandDataOptionValue::Integer(i) => {
                    options.set(opt.name.as_str(), *i)?;
                }
                CommandDataOptionValue::Number(n) => {
                    options.set(opt.name.as_str(), *n)?;
                }
                CommandDataOptionValue::Boolean(b) => {
                    options.set(opt.name.as_str(), *b)?;
                }
                _ => {
                    // For now, skip complex types like User, Channel, Role, Attachment
                }
            }
        }

        interaction.set("options", options)?;

        let handler = self
            .command_registry
            .lock()
            .unwrap()
            .get(&self.name)
            .ok_or_else(|| anyhow::anyhow!("Command not found: {}", self.name))?
            .handler
            .clone();

        // Wrap the handler call in a coroutine
        let thread = lua.create_thread(handler)?;
        let thread = thread.into_async::<()>(interaction)?;

        // Execute the Lua thread using the shared executor (no cancellation support)
        execute_lua_thread(
            http,
            cmd,
            &self.discord_config,
            thread,
            output_rx,
            print_rx,
            None,
        )
        .await
    }
}
