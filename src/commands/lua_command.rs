use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use serenity::all::{
    CommandDataOptionValue, CommandInteraction, CommandOptionType, CreateCommand,
    CreateCommandOption, Http,
};

use crate::{
    config,
    lua::{
        execute_lua_thread,
        extensions::{Attachment, TemporaryChannelUpdate},
    },
};

pub struct Handler {
    name: String,
    discord_config: config::Discord,
    command_registry: LuaCommandRegistry,
    global_lua: mlua::Lua,
}
impl Handler {
    pub fn new(
        name: String,
        discord_config: config::Discord,
        command_registry: LuaCommandRegistry,
        global_lua: mlua::Lua,
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
        // Create output/print/attachment channels for this execution
        let (output_tx, output_rx) = flume::unbounded::<String>();
        let (print_tx, print_rx) = flume::unbounded::<String>();
        let (attachment_tx, attachment_rx) = flume::unbounded::<Attachment>();

        // Lock the global Lua state for this execution (held for entire duration)
        let lua = &self.global_lua;

        // Update output channels for this execution
        let _temporary_channel_update =
            TemporaryChannelUpdate::new(lua.clone(), output_tx, print_tx, attachment_tx)?;

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
            attachment_rx,
            None,
        )
        .await
    }
}

pub type LuaCommandRegistry = Arc<Mutex<HashMap<String, LuaCommand>>>;

// LuaCommand stores command metadata and handler function reference
pub struct LuaCommand {
    pub name: String,
    pub description: String,
    pub options: Vec<LuaCommandOption>,
    pub handler: mlua::Function,
}
#[derive(Clone)]
pub struct LuaCommandOption {
    pub name: String,
    pub description: String,
    pub option_type: CommandOptionType,
    pub required: bool,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub min_length: Option<u16>,
    pub max_length: Option<u16>,
    pub autocomplete: bool,
    pub choices: Vec<(String, String)>, // (name, value) for string choices
}
impl LuaCommand {
    pub fn to_discord_command(&self) -> CreateCommand {
        let mut cmd = CreateCommand::new(&self.name).description(&self.description);

        for opt in &self.options {
            let mut option = CreateCommandOption::new(opt.option_type, &opt.name, &opt.description)
                .required(opt.required);

            if let Some(min_value) = opt.min_value {
                option = option.min_number_value(min_value);
            }
            if let Some(max_value) = opt.max_value {
                option = option.max_number_value(max_value);
            }
            if let Some(min_length) = opt.min_length {
                option = option.min_length(min_length);
            }
            if let Some(max_length) = opt.max_length {
                option = option.max_length(max_length);
            }
            if opt.autocomplete {
                option = option.set_autocomplete(true);
            }

            // Add choices if present
            for (choice_name, choice_value) in &opt.choices {
                option = option.add_string_choice(choice_name, choice_value);
            }

            cmd = cmd.add_option(option);
        }

        cmd
    }
}
