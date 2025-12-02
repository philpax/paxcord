use mlua::prelude::*;
use serenity::all::CommandOptionType;

use crate::commands::lua_registry::{CommandRegistry, LuaCommand, LuaCommandOption};

pub fn register(lua: &Lua, registry: CommandRegistry) -> LuaResult<()> {
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
                    _ => {
                        return Err(LuaError::runtime(format!(
                            "Unknown option type: {}",
                            opt_type_str
                        )));
                    }
                };

                let min_value: Option<f64> = opt.get("min_value").ok();
                let max_value: Option<f64> = opt.get("max_value").ok();
                let min_length: Option<u16> = opt.get("min_length").ok();
                let max_length: Option<u16> = opt.get("max_length").ok();
                let autocomplete: bool = opt.get("autocomplete").unwrap_or(false);

                // Parse choices if present
                let choices: Vec<(String, String)> =
                    if let Ok(choices_table) = opt.get::<LuaTable>("choices") {
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

        // Get execute handler as a function
        let handler: LuaFunction = spec.get("execute")?;

        // Store the handler in the global _discord_command_handlers table
        let handlers_table: LuaTable = lua.globals().get("_discord_command_handlers")?;
        handlers_table.set(name.clone(), handler)?;

        let command = LuaCommand {
            name,
            description,
            options,
        };

        registry_clone.lock().unwrap().push(command);
        Ok(())
    })?;

    discord.set("register_command", register_command)?;
    lua.globals().set("discord", discord)?;

    Ok(())
}
