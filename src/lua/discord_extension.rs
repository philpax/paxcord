use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use mlua::{LuaSerdeExt as _, prelude::*};
use serde::Deserialize;
use serenity::all::CommandOptionType;

use crate::commands::lua_command::{LuaCommand, LuaCommandOption, LuaCommandRegistry};

/// Maximum number of choices/suggestions allowed per option (Discord limit)
const MAX_CHOICES: usize = 25;

/// A choice entry with name and value
#[derive(Deserialize)]
struct Choice {
    name: String,
    value: String,
}

/// Registry for reply handlers (command name -> Lua handler function)
pub type LuaReplyHandlerRegistry = Arc<Mutex<HashMap<String, LuaFunction>>>;

pub fn register(
    lua: &Lua,
    command_registry: LuaCommandRegistry,
    reply_handler_registry: LuaReplyHandlerRegistry,
) -> LuaResult<()> {
    let discord = lua.create_table()?;

    let registry_clone = command_registry.clone();
    let register_command = lua.create_function(move |lua, spec: LuaTable| {
        let name: String = spec.get("name")?;
        let description: String = spec.get("description")?;

        // Parse options
        let options: Vec<LuaCommandOption> = spec
            .get::<LuaTable>("options")
            .ok()
            .map(|opts| parse_options(lua, opts))
            .transpose()?
            .unwrap_or_default();

        // Get execute handler as a function and store in registry
        let handler: LuaFunction = spec.get("execute")?;

        registry_clone.lock().unwrap().insert(
            name.clone(),
            LuaCommand {
                name,
                description,
                options,
                handler,
            },
        );
        Ok(())
    })?;

    let reply_registry_clone = reply_handler_registry.clone();
    let register_reply_handler = lua.create_function(
        move |_lua, (command_name, handler): (String, LuaFunction)| {
            reply_registry_clone
                .lock()
                .unwrap()
                .insert(command_name, handler);
            Ok(())
        },
    )?;

    discord.set("register_command", register_command)?;
    discord.set("register_reply_handler", register_reply_handler)?;
    lua.globals().set("discord", discord)?;

    Ok(())
}

fn parse_options(lua: &Lua, opts: LuaTable) -> LuaResult<Vec<LuaCommandOption>> {
    let mut options = vec![];
    for pair in opts.sequence_values::<LuaTable>() {
        let opt = pair?;
        let name: String = opt.get("name")?;
        let description: String = opt.get("description")?;
        let type_: String = opt.get("type")?;
        let required: bool = opt.get("required").unwrap_or(false);

        let option_type = match type_.as_str() {
            "string" => CommandOptionType::String,
            "integer" => CommandOptionType::Integer,
            "number" => CommandOptionType::Number,
            "boolean" => CommandOptionType::Boolean,
            "user" => CommandOptionType::User,
            "channel" => CommandOptionType::Channel,
            "role" => CommandOptionType::Role,
            "mentionable" => CommandOptionType::Mentionable,
            "attachment" => CommandOptionType::Attachment,
            _ => {
                return Err(LuaError::runtime(format!("Unknown option type: {}", type_)));
            }
        };

        let min_value: Option<f64> = opt.get("min_value").ok();
        let max_value: Option<f64> = opt.get("max_value").ok();
        let min_length: Option<u16> = opt.get("min_length").ok();
        let max_length: Option<u16> = opt.get("max_length").ok();
        let autocomplete: bool = opt.get("autocomplete").unwrap_or(false);

        // Parse choices (strict) and suggestions (autocomplete) - mutually exclusive
        let choices: Vec<(String, String)> = opt
            .get::<LuaValue>("choices")
            .ok()
            .filter(|v| !v.is_nil())
            .map(|v| lua.from_value::<Vec<Choice>>(v))
            .transpose()?
            .unwrap_or_default()
            .into_iter()
            .map(|c| (c.name, c.value))
            .collect();

        if choices.len() > MAX_CHOICES {
            return Err(LuaError::runtime(format!(
                "Option '{}' has {} choices, but Discord allows a maximum of {}",
                name,
                choices.len(),
                MAX_CHOICES
            )));
        }

        let suggestions: Vec<(String, String)> = opt
            .get::<LuaValue>("suggestions")
            .ok()
            .filter(|v| !v.is_nil())
            .map(|v| lua.from_value::<Vec<Choice>>(v))
            .transpose()?
            .unwrap_or_default()
            .into_iter()
            .map(|c| (c.name, c.value))
            .collect();

        if suggestions.len() > MAX_CHOICES {
            return Err(LuaError::runtime(format!(
                "Option '{}' has {} suggestions, but Discord allows a maximum of {}",
                name,
                suggestions.len(),
                MAX_CHOICES
            )));
        }

        // Enable autocomplete if suggestions are provided
        let autocomplete = autocomplete || !suggestions.is_empty();

        options.push(LuaCommandOption {
            name,
            description,
            option_type,
            required,
            min_value,
            max_value,
            min_length,
            max_length,
            autocomplete,
            choices,
            suggestions,
        });
    }
    Ok(options)
}
