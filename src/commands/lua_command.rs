use std::sync::Arc;

use serenity::{
    all::{CommandDataOptionValue, CommandInteraction, Http},
    futures::StreamExt as _,
};
use tokio::sync::Mutex;

use crate::{commands::lua_registry::CommandRegistry, config, outputter::Outputter};

pub struct Handler {
    name: String,
    discord_config: config::Discord,
    command_registry: CommandRegistry,
    global_lua: Arc<Mutex<mlua::Lua>>,
}

impl Handler {
    pub fn new(
        name: String,
        discord_config: config::Discord,
        command_registry: CommandRegistry,
        global_lua: Arc<Mutex<mlua::Lua>>,
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

    // We intentionally hold the Lua lock across await points here. This is necessary because:
    // 1. Discord commands share the global Lua state (unlike /execute which creates fresh states)
    // 2. Lua objects (thread, functions, tables) are tied to the Lua state's lifetime
    // 3. We need to prevent concurrent modification of the shared state
    // This effectively serializes Discord command execution, which is correct given they share state.
    #[allow(clippy::await_holding_lock)]
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

        // Lock the global Lua state for this execution (held for entire duration)
        let lua = self.global_lua.lock().await;

        // Update output channels for this execution
        crate::commands::execute::extensions::update_output_channels(&lua, output_tx, print_tx)?;

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

        // Get the handler function from the registry
        // We access the command registry here while holding the lua lock
        // This is safe because we lock command_registry briefly
        let handler: mlua::Function = {
            let registry = self.command_registry.lock().unwrap();
            let command = registry
                .get(&self.name)
                .ok_or_else(|| anyhow::anyhow!("Command not found: {}", self.name))?;
            lua.registry_value(&command.handler)?
        };

        // Wrap the handler call in a coroutine
        let thread = lua.create_thread(handler)?;
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
