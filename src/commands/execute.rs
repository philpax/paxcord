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
    util::RespondableInteraction,
};

pub struct Handler(Arc<SharedState>);
impl Handler {
    pub fn new(shared_state: Arc<SharedState>) -> Self {
        Self(shared_state)
    }
}
#[serenity::async_trait]
impl CommandHandler for Handler {
    async fn register(&self, http: &Http) -> anyhow::Result<()> {
        Command::create_global_command(
            http,
            CreateCommand::new(constant::commands::EXECUTE)
                .description("Execute the given Lua code snippet.")
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::String,
                        constant::value::CODE,
                        "The Lua code to execute.",
                    )
                    .required(true),
                ),
        )
        .await?;
        Ok(())
    }

    async fn run(&self, http: &Http, cmd: &CommandInteraction) -> anyhow::Result<()> {
        let code = cmd
            .data
            .options
            .first()
            .and_then(|o| o.value.as_str())
            .ok_or_else(|| anyhow::anyhow!("no code specified"))?;
        self.0.execute_code(http, cmd, code).await
    }
}

pub struct MsgHandler(Arc<SharedState>);
impl MsgHandler {
    pub fn new(shared_state: Arc<SharedState>) -> Self {
        Self(shared_state)
    }
}
#[serenity::async_trait]
impl CommandHandler for MsgHandler {
    async fn register(&self, http: &Http) -> anyhow::Result<()> {
        Command::create_global_command(
            http,
            CreateCommand::new(constant::commands::EXECUTE_MSG)
                .description("Execute the Lua code block from the given message ID.")
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::String,
                        constant::value::MESSAGE_ID,
                        "The ID of the message containing the Lua code block.",
                    )
                    .required(true),
                ),
        )
        .await?;
        Ok(())
    }

    async fn run(&self, http: &Http, cmd: &CommandInteraction) -> anyhow::Result<()> {
        let message_id = cmd
            .data
            .options
            .first()
            .and_then(|o| o.value.as_str())
            .ok_or_else(|| anyhow::anyhow!("no message ID specified"))?;

        let message = cmd
            .channel_id
            .message(http, message_id.parse::<u64>()?)
            .await?;

        self.0.execute_code(http, cmd, &message.content).await
    }
}

pub struct SharedState {
    discord_config: config::Discord,
    cancel_rx: flume::Receiver<MessageId>,
    ai: Arc<Ai>,
    currency_converter: Arc<CurrencyConverter>,
}

impl SharedState {
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

    async fn execute_code(
        &self,
        http: &Http,
        cmd: &CommandInteraction,
        code: &str,
    ) -> anyhow::Result<()> {
        let code = parse_markdown_lua_block(code).unwrap_or(code);

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
