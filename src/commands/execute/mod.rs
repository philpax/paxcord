use std::sync::Arc;

use serenity::all::{CommandInteraction, Http, MessageId};

use crate::{ai::Ai, config, currency::CurrencyConverter};

pub mod extensions;
pub mod slash;

#[derive(Clone)]
pub struct Handler {
    discord_config: config::Discord,
    cancel_rx: flume::Receiver<MessageId>,
    ai: Arc<Ai>,
    currency_converter: Arc<CurrencyConverter>,
}
impl Handler {
    pub fn new(
        discord_config: config::Discord,
        cancel_rx: flume::Receiver<MessageId>,
        ai: Arc<Ai>,
        currency_converter: Arc<CurrencyConverter>,
    ) -> Self {
        Self {
            discord_config,
            cancel_rx,
            ai,
            currency_converter,
        }
    }

    async fn run(
        &self,
        http: &Http,
        cmd: &CommandInteraction,
        unparsed_code: &str,
    ) -> anyhow::Result<()> {
        let code = parse_markdown_lua_block(unparsed_code).unwrap_or(unparsed_code);

        let (output_tx, output_rx) = flume::unbounded::<String>();
        let (print_tx, print_rx) = flume::unbounded::<String>();

        let lua = create_lua_state(
            self.ai.clone(),
            self.currency_converter.clone(),
            output_tx,
            print_tx,
        )?;
        let thread = load_async_expression::<Option<String>>(&lua, code)?;

        // Execute the Lua thread using the shared executor (with cancellation support)
        super::lua_executor::execute_lua_thread(
            http,
            cmd,
            &self.discord_config,
            thread,
            output_rx,
            print_rx,
            Some(self.cancel_rx.clone()),
        )
        .await
    }
}

fn load_lua_file(lua: &mlua::Lua, path: &str) -> mlua::Result<()> {
    let script_path = std::path::Path::new(path);
    if script_path.exists() {
        let code = std::fs::read_to_string(script_path)
            .map_err(|e| mlua::Error::RuntimeError(format!("Failed to read {path}: {e}")))?;
        lua.load(&code).set_name(path).exec()?;
    }
    Ok(())
}

pub fn create_lua_state(
    ai: Arc<Ai>,
    currency_converter: Arc<CurrencyConverter>,
    output_tx: flume::Sender<String>,
    print_tx: flume::Sender<String>,
) -> mlua::Result<mlua::Lua> {
    let lua = mlua::Lua::new_with(
        {
            use mlua::StdLib as SL;
            SL::COROUTINE | SL::MATH | SL::STRING | SL::TABLE | SL::UTF8 | SL::VECTOR
        },
        mlua::LuaOptions::new().catch_rust_panics(true),
    )?;

    extensions::register(&lua, ai, currency_converter, output_tx, print_tx)?;
    load_lua_file(&lua, "scripts/main.lua")?;

    Ok(lua)
}

pub fn create_global_lua_state(
    ai: Arc<Ai>,
    currency_converter: Arc<CurrencyConverter>,
    output_tx: flume::Sender<String>,
    print_tx: flume::Sender<String>,
    command_registry: crate::commands::CommandRegistry,
) -> mlua::Result<mlua::Lua> {
    let lua = mlua::Lua::new_with(
        {
            use mlua::StdLib as SL;
            SL::COROUTINE | SL::MATH | SL::STRING | SL::TABLE | SL::UTF8 | SL::VECTOR
        },
        mlua::LuaOptions::new().catch_rust_panics(true),
    )?;

    extensions::register(&lua, ai, currency_converter, output_tx, print_tx)?;
    extensions::discord::register(&lua, command_registry)?;

    load_lua_file(&lua, "scripts/main.lua")?;
    load_lua_file(&lua, "scripts/commands.lua")?;

    Ok(lua)
}

fn load_async_expression<R: mlua::FromLuaMulti>(
    lua: &mlua::Lua,
    expression: &str,
) -> anyhow::Result<mlua::AsyncThread<R>> {
    let with_return = lua
        .load(
            format!(
                r#"
coroutine.create(function()
    return {expression}
end)
"#
            )
            .trim(),
        )
        .eval::<mlua::Thread>()
        .and_then(|t| t.into_async::<R>(()));

    match with_return {
        Ok(thread) => Ok(thread),
        Err(with_return_err) => {
            let without_return = lua
                .load(
                    format!(
                        r#"
coroutine.create(function()
{expression}
end)
"#
                    )
                    .trim(),
                )
                .eval::<mlua::Thread>()
                .and_then(|t| t.into_async::<R>(()));

            match without_return {
                Ok(thread) => Ok(thread),
                Err(without_return_err) => {
                    anyhow::bail!(
                        "Failed to load expression with return: {with_return_err:?} | without return: {without_return_err:?}"
                    );
                }
            }
        }
    }
}

/// Parses a markdown code block of the form ```lua\n{CODE}\n``` and returns the code between the backticks.
/// Doesn't use regex.
fn parse_markdown_lua_block(code: &str) -> Option<&str> {
    // Find the start of the code block
    let start = code.find("```lua\n")?;
    let start = start + 7; // Skip past ```lua\n

    // Find the end of the code block
    let end = code[start..].find("\n```")?;

    // Return the slice between the markers
    Some(&code[start..start + end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_markdown_lua_block() {
        // Test basic parsing
        let input = "```lua\nprint('hello')\n```";
        assert_eq!(parse_markdown_lua_block(input), Some("print('hello')"));

        // Test with multiple lines
        let input = "```lua\nlocal x = 1\nlocal y = 2\nprint(x + y)\n```";
        assert_eq!(
            parse_markdown_lua_block(input),
            Some("local x = 1\nlocal y = 2\nprint(x + y)")
        );

        // Test with no code block
        let input = "This is not a code block";
        assert_eq!(parse_markdown_lua_block(input), None);

        // Test with wrong language
        let input = "```python\nprint('hello')\n```";
        assert_eq!(parse_markdown_lua_block(input), None);

        // Test with no closing backticks
        let input = "```lua\nprint('hello')";
        assert_eq!(parse_markdown_lua_block(input), None);
    }
}
