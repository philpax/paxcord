use mlua::prelude::*;
use serenity::all::CommandOptionType;

use crate::commands::lua_command::{LuaCommand, LuaCommandOption, LuaCommandRegistry};

pub fn register(lua: &Lua, registry: LuaCommandRegistry) -> LuaResult<()> {
    let discord = lua.create_table()?;

    let registry_clone = registry.clone();
    let register_command = lua.create_function(move |_lua, spec: LuaTable| {
        let name: String = spec.get("name")?;
        let description: String = spec.get("description")?;

        // Parse options
        let options: Vec<LuaCommandOption> = spec
            .get::<LuaTable>("options")
            .ok()
            .map(parse_options)
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

    discord.set("register_command", register_command)?;
    lua.globals().set("discord", discord)?;

    Ok(())
}

fn parse_options(opts: LuaTable) -> LuaResult<Vec<LuaCommandOption>> {
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

        // Parse choices if present
        let choices: Vec<(String, String)> = opt
            .get::<LuaTable>("choices")
            .ok()
            .map(|c| {
                let mut choices = vec![];
                for pair in c.sequence_values::<LuaTable>() {
                    let choice = pair?;
                    let choice_name: String = choice.get("name")?;
                    let choice_value: String = choice.get("value")?;
                    choices.push((choice_name, choice_value));
                }
                Ok::<_, mlua::Error>(choices)
            })
            .transpose()?
            .unwrap_or_default();

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
        });
    }
    Ok(options)
}
