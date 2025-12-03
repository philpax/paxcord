use serenity::{
    all::{CommandInteraction, Http, MessageId},
    futures::StreamExt as _,
};

use crate::{config, lua::extensions::Attachment, outputter::Outputter};

/// Channels for receiving output from Lua execution
pub struct LuaOutputChannels {
    pub output_rx: flume::Receiver<String>,
    pub print_rx: flume::Receiver<String>,
    pub attachment_rx: flume::Receiver<Attachment>,
}

/// Executes a Lua async thread with output handling and optional cancellation support.
pub async fn execute_lua_thread<R>(
    http: &Http,
    cmd: &CommandInteraction,
    discord_config: &config::Discord,
    mut thread: mlua::AsyncThread<R>,
    channels: LuaOutputChannels,
    mut cancel_rx: Option<flume::Receiver<MessageId>>,
) -> anyhow::Result<()>
where
    R: mlua::FromLuaMulti + std::marker::Unpin,
{
    let mut outputter = Outputter::new(
        http,
        cmd,
        std::time::Duration::from_millis(discord_config.message_update_interval_ms),
        "Executing...",
    )
    .await?;

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
    let mut output_stream = channels.output_rx.stream();
    let mut print_stream = channels.print_rx.stream();
    let mut attachment_stream = channels.attachment_rx.stream();

    let starting_message_id = outputter.starting_message_id();

    loop {
        tokio::select! {
            biased;

            // Check for cancellation (highest priority) - pending forever if None
            Some(cancel_message_id) = next_if_some(&mut cancel_rx) => {
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

            // Handle attachments
            Some(attachment) = attachment_stream.next() => {
                outputter.add_attachment(attachment).await?;
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

/// Helper to get next item from an optional receiver by creating a temporary stream.
/// If the receiver is None, this future will never resolve (pending forever).
async fn next_if_some<T>(rx: &mut Option<flume::Receiver<T>>) -> Option<T> {
    match rx.as_ref() {
        Some(receiver) => receiver.stream().next().await,
        None => std::future::pending().await,
    }
}
