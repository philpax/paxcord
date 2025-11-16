use std::sync::Arc;
use mlua::prelude::*;
use parking_lot::Mutex;
use serenity::all::{CommandOptionType, CreateCommand, CreateCommandOption};

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

// LuaCommand stores command metadata only
// Handlers are stored in a global Lua table
#[derive(Clone)]
pub struct LuaCommand {
    pub name: String,
    pub description: String,
    pub options: Vec<LuaCommandOption>,
}

pub type CommandRegistry = Arc<Mutex<Vec<LuaCommand>>>;

pub fn create_registry() -> CommandRegistry {
    Arc::new(Mutex::new(Vec::new()))
}

pub fn register(lua: &Lua, registry: CommandRegistry) -> LuaResult<()> {
    // Create a global table to store command handlers
    let handlers_table = lua.create_table()?;
    lua.globals().set("_discord_command_handlers", handlers_table)?;

    let discord = lua.create_table()?;

    let registry_clone = registry.clone();
    let register_command = lua.create_function(move |lua, spec: LuaTable| {
        let name: String = spec.get("name")?;
        let description: String = spec.get("description")?;

        // Parse options
        let options: Vec<LuaCommandOption> = if let Ok(opts) = spec.get::<LuaTable>("options") {
            let mut options = Vec::new();
            for pair in opts.sequence_values::<LuaTable>() {
                let opt = pair?;
                let opt_name: String = opt.get("name")?;
                let opt_desc: String = opt.get("description")?;
                let opt_type_str: String = opt.get("type")?;
                let required: bool = opt.get("required").unwrap_or(false);

                let option_type = match opt_type_str.as_str() {
                    "string" => CommandOptionType::String,
                    "integer" => CommandOptionType::Integer,
                    "number" => CommandOptionType::Number,
                    "boolean" => CommandOptionType::Boolean,
                    "user" => CommandOptionType::User,
                    "channel" => CommandOptionType::Channel,
                    "role" => CommandOptionType::Role,
                    "mentionable" => CommandOptionType::Mentionable,
                    "attachment" => CommandOptionType::Attachment,
                    _ => return Err(LuaError::runtime(format!("Unknown option type: {}", opt_type_str))),
                };

                let min_value: Option<f64> = opt.get("min_value").ok();
                let max_value: Option<f64> = opt.get("max_value").ok();
                let min_length: Option<u16> = opt.get("min_length").ok();
                let max_length: Option<u16> = opt.get("max_length").ok();
                let autocomplete: bool = opt.get("autocomplete").unwrap_or(false);

                // Parse choices if present
                let choices: Vec<(String, String)> = if let Ok(choices_table) = opt.get::<LuaTable>("choices") {
                    let mut choices = Vec::new();
                    for pair in choices_table.sequence_values::<LuaTable>() {
                        let choice = pair?;
                        let choice_name: String = choice.get("name")?;
                        let choice_value: String = choice.get("value")?;
                        choices.push((choice_name, choice_value));
                    }
                    choices
                } else {
                    Vec::new()
                };

                options.push(LuaCommandOption {
                    name: opt_name,
                    description: opt_desc,
                    option_type,
                    required,
                    min_value,
                    max_value,
                    min_length,
                    max_length,
                    autocomplete,
                    choices,
                });
            }
            options
        } else {
            Vec::new()
        };

        // Store execute handler in global table
        let execute_fn: LuaFunction = spec.get("execute")?;
        let handlers: LuaTable = lua.globals().get("_discord_command_handlers")?;
        handlers.set(name.clone(), execute_fn)?;

        let command = LuaCommand {
            name,
            description,
            options,
        };

        registry_clone.lock().push(command);
        Ok(())
    })?;

    discord.set("register_command", register_command)?;
    lua.globals().set("discord", discord)?;

    Ok(())
}

impl LuaCommand {
    pub fn to_discord_command(&self) -> CreateCommand {
        let mut cmd = CreateCommand::new(&self.name).description(&self.description);

        for opt in &self.options {
            let mut option = CreateCommandOption::new(
                opt.option_type,
                &opt.name,
                &opt.description,
            )
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
