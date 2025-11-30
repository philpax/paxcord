use std::sync::Arc;

use serenity::{
    all::{CommandDataOptionValue, CommandInteraction, Http},
    futures::StreamExt as _,
};

use crate::{
    ai::Ai, commands::lua_registry::LuaCommand, config, currency::CurrencyConverter,
    outputter::Outputter,
};

pub struct Handler {
    name: String,
    discord_config: config::Discord,
    command_spec: LuaCommand,
    ai: Arc<Ai>,
    currency_converter: Arc<CurrencyConverter>,
}

impl Handler {
    pub fn new(
        name: String,
        discord_config: config::Discord,
        command_spec: LuaCommand,
        ai: Arc<Ai>,
        currency_converter: Arc<CurrencyConverter>,
    ) -> Self {
        Self {
            name,
            discord_config,
            command_spec,
            ai,
            currency_converter,
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
        let mut outputter = Outputter::new(
            http,
            cmd,
            std::time::Duration::from_millis(self.discord_config.message_update_interval_ms),
            "Executing...",
        )
        .await?;

        // Create output/print channels for this execution
        let (output_tx, output_rx) = flume::unbounded::<String>();
        let (print_tx, print_rx) = flume::unbounded::<String>();

        // Create a fresh Lua state for this execution
        let lua = crate::commands::execute::create_lua_state(
            self.ai.clone(),
            self.currency_converter.clone(),
            output_tx,
            print_tx,
        )?;

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

        // Wrap handler code in a coroutine
        let code = format!(
            r#"
coroutine.create(function()
    local interaction = ...
    {}
end)
"#,
            self.command_spec.handler_code
        );

        let thread: mlua::Thread = lua.load(&code).eval()?;
        let mut thread = thread.into_async::<()>(interaction)?;

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
        let mut output_stream = output_rx.stream();
        let mut print_stream = print_rx.stream();

        loop {
            tokio::select! {
                biased;

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
                        Some(Ok(_)) => {
                            outputter.update(&output.to_final_output()).await?;
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
