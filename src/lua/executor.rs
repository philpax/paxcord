use std::sync::Arc;

use serenity::{
    all::{CommandInteraction, Http, Message, MessageId, UserId},
    futures::StreamExt as _,
};

use crate::{config, lua::extensions::Attachment, outputter::OutputterHandle};

/// Channels for receiving output from Lua execution
pub struct LuaOutputChannels {
    pub output_rx: flume::Receiver<String>,
    pub print_rx: flume::Receiver<String>,
    pub attachment_rx: flume::Receiver<Attachment>,
}

/// Executes a Lua async thread with output handling and optional cancellation support.
/// Returns the message ID of the bot's response.
pub async fn execute_lua_thread(
    http: Arc<Http>,
    cmd: &CommandInteraction,
    discord_config: &config::Discord,
    mut thread: mlua::AsyncThread<Option<String>>,
    channels: LuaOutputChannels,
    mut cancel_rx: Option<flume::Receiver<MessageId>>,
) -> anyhow::Result<MessageId> {
    let outputter = OutputterHandle::new(
        http,
        cmd,
        discord_config.message_update_interval_ms,
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
    let mut thread_result: Option<String> = None;
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
                    outputter.cancelled();
                    errored = true;
                    break;
                }
                break;
            }

            // Handle values from output stream
            Some(value) = output_stream.next() => {
                output.output = value;
                outputter.update(&output.to_final_output());
            }

            // Handle values from print stream
            Some(value) = print_stream.next() => {
                output.print_log.push(value);
                outputter.update(&output.to_final_output());
            }

            // Handle attachments
            Some(attachment) = attachment_stream.next() => {
                outputter.add_attachment(attachment);
            }

            // Handle thread stream
            thread_next = thread.next() => {
                match thread_next {
                    Some(Ok(result)) => {
                        // Capture the return value from the thread (already stringified by Lua)
                        if let Some(value) = result {
                            thread_result = Some(value);
                        }
                        outputter.update(&output.to_final_output());
                    }
                    Some(Err(err)) => {
                        outputter.error(&err.to_string());
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
        // If no explicit output was set but the thread returned a value, use that as output
        if output.output.is_empty()
            && let Some(result) = thread_result
        {
            output.output = result;
            outputter.update(&output.to_final_output());
        }
        outputter.finish();
    }

    outputter.join().await?;

    Ok(starting_message_id)
}

/// Helper to get next item from an optional receiver by creating a temporary stream.
/// If the receiver is None, this future will never resolve (pending forever).
async fn next_if_some<T>(rx: &mut Option<flume::Receiver<T>>) -> Option<T> {
    match rx.as_ref() {
        Some(receiver) => receiver.stream().next().await,
        None => std::future::pending().await,
    }
}

/// Executes a Lua async thread in response to a message reply
pub async fn execute_lua_reply_thread(
    http: Arc<Http>,
    reply_to: &Message,
    user_id: UserId,
    discord_config: &config::Discord,
    mut thread: mlua::AsyncThread<Option<String>>,
    channels: LuaOutputChannels,
) -> anyhow::Result<MessageId> {
    let outputter = OutputterHandle::new_reply(
        http,
        reply_to,
        user_id,
        discord_config.message_update_interval_ms,
        "Processing reply...",
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
    let mut thread_result: Option<String> = None;
    let mut output_stream = channels.output_rx.stream();
    let mut print_stream = channels.print_rx.stream();
    let mut attachment_stream = channels.attachment_rx.stream();

    let starting_message_id = outputter.starting_message_id();

    loop {
        tokio::select! {
            biased;

            // Handle values from output stream
            Some(value) = output_stream.next() => {
                output.output = value;
                outputter.update(&output.to_final_output());
            }

            // Handle values from print stream
            Some(value) = print_stream.next() => {
                output.print_log.push(value);
                outputter.update(&output.to_final_output());
            }

            // Handle attachments
            Some(attachment) = attachment_stream.next() => {
                outputter.add_attachment(attachment);
            }

            // Handle thread stream
            thread_next = thread.next() => {
                match thread_next {
                    Some(Ok(result)) => {
                        // Capture the return value from the thread
                        if let Some(value) = result {
                            thread_result = Some(value);
                        }
                        outputter.update(&output.to_final_output());
                    }
                    Some(Err(err)) => {
                        outputter.error(&err.to_string());
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
        // If no explicit output was set but the thread returned a value, use that as output
        if output.output.is_empty()
            && let Some(result) = thread_result
        {
            output.output = result;
            outputter.update(&output.to_final_output());
        }
        outputter.finish();
    }

    outputter.join().await?;

    Ok(starting_message_id)
}
