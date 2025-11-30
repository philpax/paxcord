use std::sync::Arc;

use mlua::prelude::*;
use parking_lot::Mutex;
use serenity::{
    all::{CommandDataOptionValue, CommandInteraction, Http},
    builder::{CreateInteractionResponse, CreateInteractionResponseMessage},
};

use crate::commands::lua_registry::LuaCommand;

pub struct Handler {
    name: String,
    lua_state: Arc<Mutex<Lua>>,
    command_spec: LuaCommand,
}

impl Handler {
    pub fn new(name: String, lua_state: Arc<Mutex<Lua>>, command_spec: LuaCommand) -> Self {
        Self {
            name,
            lua_state,
            command_spec,
        }
    }
}

#[serenity::async_trait]
impl super::CommandHandler for Handler {
    fn name(&self) -> &str {
        &self.name
    }

    async fn register(&self, http: &Http) -> anyhow::Result<()> {
        let cmd = self.command_spec.to_discord_command();
        serenity::all::Command::create_global_command(http, cmd).await?;
        Ok(())
    }

    async fn run(&self, http: &Http, cmd: &CommandInteraction) -> anyhow::Result<()> {
        // Send initial "thinking" response before acquiring lock
        cmd.create_response(
            http,
            CreateInteractionResponse::Defer(CreateInteractionResponseMessage::new()),
        )
        .await?;

        // Execute Lua handler in a scope to ensure lock is dropped
        let result = {
            let lua = self.lua_state.lock();

            // Retrieve handler from global table
            let handlers: LuaTable = lua.globals().get("_discord_command_handlers")?;
            let handler: LuaFunction = handlers.get(self.name.as_str())?;

            // Create interaction table to pass to Lua
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
                        // We can add support for these later if needed
                    }
                }
            }

            interaction.set("options", options)?;

            // Call the execute handler
            handler.call::<()>(interaction)
        };
        // Lock is dropped here

        if let Err(e) = result {
            // Send error message (lock is not held during await)
            cmd.edit_response(
                http,
                serenity::all::EditInteractionResponse::new().content(format!("Error: {}", e)),
            )
            .await?;
        }

        Ok(())
    }
}
