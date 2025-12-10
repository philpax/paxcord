use std::sync::Arc;

use serenity::all::{
    Command, CommandInteraction, CommandOptionType, CreateCommand, CreateCommandOption, Http,
    MessageId,
};

use crate::{
    ai::Ai,
    commands::CommandHandler,
    config, constant,
    currency::CurrencyConverter,
    lua::{
        LuaOutputChannels, create_barebones_lua_state, execute_lua_thread, extensions::Attachment,
        load_async_expression,
    },
    util::{self, RespondableInteraction},
};

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
}
#[serenity::async_trait]
impl CommandHandler for Handler {
    async fn register(&self, http: &Http) -> anyhow::Result<()> {
        Command::create_global_command(
            http,
            CreateCommand::new(constant::commands::EXECUTE)
                .description("Execute the Lua code block from the given code snippet or message ID.")
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::String,
                        constant::value::CODE,
                        "The Lua code block to execute. Mutually exclusive with message ID.",
                    )
                    .required(false),
                )
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::String,
                        constant::value::MESSAGE_ID,
                        "The ID of the message to execute the code block from. Mutually exclusive with code.",
                    )
                    .required(false),
                )
        )
        .await?;
        Ok(())
    }

    async fn run(&self, http: &Http, cmd: &CommandInteraction) -> anyhow::Result<()> {
        let options = &cmd.data.options;

        let message_id =
            util::get_value(options, constant::value::MESSAGE_ID).and_then(util::value_to_string);

        let code = util::get_value(options, constant::value::CODE).and_then(util::value_to_string);

        let unparsed_code = match (message_id, code) {
            (Some(message_id), None) => {
                let message = cmd
                    .channel_id
                    .message(http, message_id.parse::<u64>()?)
                    .await?;

                message.content
            }
            (None, Some(code)) => code,
            (Some(_), Some(_)) => anyhow::bail!("message ID and code are mutually exclusive"),
            (None, None) => anyhow::bail!("no message ID or code specified"),
        };
        let code = parse_markdown_lua_block(&unparsed_code).unwrap_or(&unparsed_code);

        let (output_tx, output_rx) = flume::unbounded::<String>();
        let (print_tx, print_rx) = flume::unbounded::<String>();
        let (attachment_tx, attachment_rx) = flume::unbounded::<Attachment>();

        let lua = create_barebones_lua_state(
            self.ai.clone(),
            self.currency_converter.clone(),
            output_tx,
            print_tx,
            attachment_tx,
        )?;
        let thread = match load_async_expression::<Option<String>>(&lua, code) {
            Ok(thread) => thread,
            Err(err) => {
                cmd.create(http, &format!("Error: {err}")).await?;
                return Ok(());
            }
        };

        // Execute the Lua thread using the shared executor (with cancellation support)
        execute_lua_thread(
            http,
            cmd,
            &self.discord_config,
            thread,
            LuaOutputChannels {
                output_rx,
                print_rx,
                attachment_rx,
            },
            Some(self.cancel_rx.clone()),
        )
        .await
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
