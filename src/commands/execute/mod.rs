use std::sync::Arc;

use serenity::{
    all::{CommandInteraction, Http, MessageId},
    futures::StreamExt as _,
};

use crate::{ai::Ai, config, currency::CurrencyConverter, outputter::Outputter};

pub mod app;
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
        let mut outputter = Outputter::new(
            http,
            cmd,
            std::time::Duration::from_millis(self.discord_config.message_update_interval_ms),
            "Executing...",
        )
        .await?;
        let starting_message_id = outputter.starting_message_id();

        let code = parse_markdown_lua_block(unparsed_code).unwrap_or(unparsed_code);

        let (output_tx, output_rx) = flume::unbounded::<String>();
        let (print_tx, print_rx) = flume::unbounded::<String>();

        let lua = create_lua_state(
            self.ai.clone(),
            self.currency_converter.clone(),
            output_tx,
            print_tx,
        )?;
        let mut thread = load_async_expression::<Option<String>>(&lua, code)?;

        struct Output {
            output: String,
            print_log: Vec<String>,
        }
        impl Output {
            pub fn to_final_output(&self) -> String {
                let mut output = self.output.clone();
                if !self.print_log.is_empty() {
                    output.push_str("\n**Print Log**\n");
                    for print in self.print_log.iter() {
                        output.push_str(print);
                        output.push('\n');
                    }
                }
                output
            }
        }
        let mut output = Output {
            output: String::new(),
            print_log: vec![],
        };

        let mut errored = false;
        let mut cancel_stream = self.cancel_rx.stream();
        let mut output_stream = output_rx.stream();
        let mut print_stream = print_rx.stream();

        loop {
            tokio::select! {
                biased;

                // Check for cancellation (highest priority)
                Some(cancel_message_id) = cancel_stream.next() => {
                    if cancel_message_id == starting_message_id {
                        outputter.cancelled().await?;
                        errored = true;
                        break;
                    }
                    break;
                }

                // Handle values from output stream
                Some(value) = output_stream.next() => {
                    output.output = value;
                    outputter.update(&output.to_final_output()).await?;
                }

                // Handle values from print stream
                Some(value) = print_stream.next() => {
                    output.print_log.push(value);
                    outputter.update(&output.to_final_output()).await?;
                }

                // Handle thread stream
                thread_result = thread.next() => {
                    match thread_result {
                        Some(Ok(result)) => {
                            if let Some(result) = result {
                                output.output = result;
                                outputter.update(&output.to_final_output()).await?;
                            } else {
                                outputter.update(&output.to_final_output()).await?;
                            }
                        }
                        Some(Err(err)) => {
                            outputter.error(&err.to_string()).await?;
                            errored = true;
                            break;
                        }
                        None => {
                            // Thread stream exhausted
                            break;
                        }
                    }
                }
            }
        }

        if !errored {
            outputter.finish().await?;
        }

        Ok(())
    }
}

fn create_lua_state(
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

    // Load scripts/main.lua if it exists
    let main_script = std::path::Path::new("scripts/main.lua");
    if main_script.exists() {
        let code = std::fs::read_to_string(main_script)
            .map_err(|e| mlua::Error::RuntimeError(format!("Failed to read main.lua: {}", e)))?;
        lua.load(&code)
            .set_name("main.lua")
            .exec()?;
    }

    Ok(lua)
}

pub fn create_global_lua_state(
    ai: Arc<Ai>,
    currency_converter: Arc<CurrencyConverter>,
    output_tx: flume::Sender<String>,
    print_tx: flume::Sender<String>,
    command_registry: extensions::discord::CommandRegistry,
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

    // Load scripts/main.lua if it exists
    let main_script = std::path::Path::new("scripts/main.lua");
    if main_script.exists() {
        let code = std::fs::read_to_string(main_script)
            .map_err(|e| mlua::Error::RuntimeError(format!("Failed to read main.lua: {}", e)))?;
        lua.load(&code)
            .set_name("main.lua")
            .exec()?;
    }

    // Load scripts/commands.lua if it exists
    let commands_script = std::path::Path::new("scripts/commands.lua");
    if commands_script.exists() {
        let code = std::fs::read_to_string(commands_script)
            .map_err(|e| mlua::Error::RuntimeError(format!("Failed to read commands.lua: {}", e)))?;
        lua.load(&code)
            .set_name("commands.lua")
            .exec()?;
    }

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
