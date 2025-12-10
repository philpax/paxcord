use std::sync::Arc;

use serenity::all::{
    CommandInteraction, CreateAllowedMentions, CreateAttachment, CreateInteractionResponse,
    CreateInteractionResponseMessage, CreateMessage, EditMessage, Http, Message, MessageId, UserId,
};
use tokio::sync::oneshot;

use crate::lua::extensions::Attachment;

/// Commands that can be sent to the outputter task
enum OutputterCommand {
    Update(String),
    AddAttachment(Attachment),
    Error(String),
    Cancelled,
    Finish,
}

/// Handle to communicate with an outputter running in its own task
pub struct OutputterHandle {
    tx: flume::Sender<OutputterCommand>,
    starting_message_id: MessageId,
    join_handle: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl OutputterHandle {
    pub async fn new(
        http: Arc<Http>,
        cmd: &CommandInteraction,
        update_interval_ms: u64,
        initial_message: &str,
    ) -> anyhow::Result<Self> {
        cmd.create_response(
            &http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(initial_message)
                    .allowed_mentions(CreateAllowedMentions::new()),
            ),
        )
        .await?;
        let starting_message = cmd.get_response(&http).await?;
        let starting_message_id = starting_message.id;
        let user_id = cmd.user.id;

        let (tx, rx) = flume::unbounded();
        let (ready_tx, ready_rx) = oneshot::channel();

        let join_handle = tokio::spawn(async move {
            let mut outputter = Outputter {
                http,
                user_id,
                messages: vec![starting_message],
                chunks: vec![],
                pending_attachments: vec![],
                in_terminal_state: false,
                last_update: std::time::Instant::now(),
                last_update_duration: std::time::Duration::from_millis(update_interval_ms),
            };

            // Signal that we're ready
            let _ = ready_tx.send(());

            outputter.run(rx).await
        });

        // Wait for the task to be ready
        let _ = ready_rx.await;

        Ok(Self {
            tx,
            starting_message_id,
            join_handle,
        })
    }

    pub fn starting_message_id(&self) -> MessageId {
        self.starting_message_id
    }

    pub fn update(&self, message: &str) {
        let _ = self.tx.send(OutputterCommand::Update(message.to_string()));
    }

    pub fn add_attachment(&self, attachment: Attachment) {
        let _ = self.tx.send(OutputterCommand::AddAttachment(attachment));
    }

    pub fn error(&self, err: &str) {
        let _ = self.tx.send(OutputterCommand::Error(err.to_string()));
    }

    pub fn cancelled(&self) {
        let _ = self.tx.send(OutputterCommand::Cancelled);
    }

    pub fn finish(&self) {
        let _ = self.tx.send(OutputterCommand::Finish);
    }

    /// Wait for the outputter task to complete
    pub async fn join(self) -> anyhow::Result<()> {
        // Drop the sender to signal the task to finish processing
        drop(self.tx);
        self.join_handle.await?
    }
}

struct Outputter {
    http: Arc<Http>,

    user_id: UserId,
    messages: Vec<Message>,
    chunks: Vec<String>,
    pending_attachments: Vec<CreateAttachment>,

    in_terminal_state: bool,

    last_update: std::time::Instant,
    last_update_duration: std::time::Duration,
}

impl Outputter {
    const MESSAGE_CHUNK_SIZE: usize = 1500;

    async fn run(&mut self, rx: flume::Receiver<OutputterCommand>) -> anyhow::Result<()> {
        use serenity::futures::StreamExt as _;

        let mut rx_stream = rx.stream();
        let mut sync_interval = tokio::time::interval(self.last_update_duration);

        loop {
            tokio::select! {
                biased;

                // Handle commands from the handle
                cmd = rx_stream.next() => {
                    match cmd {
                        Some(OutputterCommand::Update(message)) => {
                            self.update(&message).await?;
                        }
                        Some(OutputterCommand::AddAttachment(attachment)) => {
                            self.add_attachment(attachment);
                        }
                        Some(OutputterCommand::Error(err)) => {
                            self.on_error(&err).await?;
                        }
                        Some(OutputterCommand::Cancelled) => {
                            self.on_error("The generation was cancelled.").await?;
                        }
                        Some(OutputterCommand::Finish) => {
                            self.finish().await?;
                        }
                        None => {
                            // Channel closed, exit the loop
                            break;
                        }
                    }
                }

                // Periodically sync any pending updates
                _ = sync_interval.tick() => {
                    self.sync_if_pending().await?;
                }
            }
        }

        Ok(())
    }

    async fn update(&mut self, message: &str) -> anyhow::Result<()> {
        self.chunks = chunk_message(message, Self::MESSAGE_CHUNK_SIZE);
        self.sync_if_pending().await
    }

    /// Syncs messages if we're past the rate-limit threshold.
    async fn sync_if_pending(&mut self) -> anyhow::Result<()> {
        if self.in_terminal_state {
            return Ok(());
        }

        if self.last_update.elapsed() >= self.last_update_duration {
            self.sync_messages_with_chunks().await?;
            self.last_update = std::time::Instant::now();
        }

        Ok(())
    }

    fn add_attachment(&mut self, attachment: Attachment) {
        self.pending_attachments.push(CreateAttachment::bytes(
            attachment.data,
            attachment.filename,
        ));
    }

    async fn finish(&mut self) -> anyhow::Result<()> {
        for msg in &mut self.messages {
            msg.edit(&self.http, EditMessage::new().components(vec![]))
                .await?;
        }

        self.in_terminal_state = true;
        self.sync_messages_with_chunks().await?;

        // Add any pending attachments to the last message
        if !self.pending_attachments.is_empty()
            && let Some(last) = self.messages.last_mut()
        {
            let mut edit = EditMessage::new();
            for attachment in self.pending_attachments.drain(..) {
                edit = edit.new_attachment(attachment);
            }
            last.edit(&self.http, edit).await?;
        }

        Ok(())
    }

    async fn sync_messages_with_chunks(&mut self) -> anyhow::Result<()> {
        // Update existing messages to match chunks
        for (msg, chunk) in self.messages.iter_mut().zip(self.chunks.iter()) {
            msg.edit(&self.http, EditMessage::new().content(chunk))
                .await?;
        }

        if self.chunks.len() < self.messages.len() {
            // Delete excess messages
            for msg in self.messages.drain(self.chunks.len()..) {
                msg.delete(&self.http).await?;
            }
        } else if self.chunks.len() > self.messages.len() {
            // Remove the cancel button from all existing messages
            for msg in &mut self.messages {
                msg.edit(
                    &self.http,
                    EditMessage::new()
                        .components(vec![])
                        .allowed_mentions(CreateAllowedMentions::new()),
                )
                .await?;
            }

            // Create new messages for the remaining chunks
            for chunk in self.chunks[self.messages.len()..].iter() {
                let last = self.messages.last_mut().unwrap();
                let msg = reply_to_message_without_mentions(&self.http, last, chunk).await?;
                self.messages.push(msg);
            }
        }

        let Some(first_id) = self.messages.first().map(|m| m.id) else {
            return Ok(());
        };

        // Add the cancel button to the last message
        if !self.in_terminal_state
            && let Some(last) = self.messages.last_mut()
            && last.components.is_empty()
        {
            crate::cancel::add_button(&self.http, first_id, last, self.user_id).await?;
        }

        Ok(())
    }

    async fn on_error(&mut self, error_message: &str) -> anyhow::Result<()> {
        for msg in &mut self.messages {
            let cut_content = format!("~~{}~~", msg.content);
            msg.edit(
                &self.http,
                EditMessage::new()
                    .components(vec![])
                    .allowed_mentions(CreateAllowedMentions::new())
                    .content(cut_content),
            )
            .await?;
        }

        self.in_terminal_state = true;
        if let Some(last) = self.messages.last_mut() {
            reply_to_message_without_mentions(&self.http, last, error_message).await?;
        }

        Ok(())
    }
}

async fn reply_to_message_without_mentions(
    http: &Http,
    msg: &Message,
    content: &str,
) -> anyhow::Result<Message> {
    Ok(msg
        .channel_id
        .send_message(
            http,
            CreateMessage::new()
                .reference_message(msg)
                .content(content)
                .allowed_mentions(CreateAllowedMentions::new()),
        )
        .await?)
}

fn chunk_message(message: &str, chunk_size: usize) -> Vec<String> {
    let mut chunks: Vec<String> = vec!["".to_string()];

    for word in message.split(' ') {
        let Some(last) = chunks.last_mut() else {
            continue;
        };

        if last.len() > chunk_size {
            chunks.push(word.to_string());
        } else {
            if !last.is_empty() {
                last.push(' ');
            }
            last.push_str(word);
        }
    }

    chunks
}
