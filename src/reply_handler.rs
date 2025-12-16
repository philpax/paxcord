use std::collections::HashMap;

use serenity::all::{ChannelId, GuildId, Http, Message, MessageId, UserId};

use crate::interaction_context::OptionValue;

/// A message in the conversation chain
#[derive(Clone, Debug)]
pub struct ChainMessage {
    /// The message ID
    pub id: MessageId,
    /// The message content
    pub content: String,
    /// The author's user ID
    pub author_id: UserId,
    /// The author's username
    pub author_name: String,
    /// Whether this message is from the bot
    pub is_bot: bool,
    /// The channel ID
    pub channel_id: ChannelId,
    /// The guild ID (None for DMs)
    pub guild_id: Option<GuildId>,
    /// Attachments (URLs)
    pub attachments: Vec<String>,
}

impl ChainMessage {
    pub fn from_message(msg: &Message) -> Self {
        Self {
            id: msg.id,
            content: msg.content.clone(),
            author_id: msg.author.id,
            author_name: msg.author.name.clone(),
            is_bot: msg.author.bot,
            channel_id: msg.channel_id,
            guild_id: msg.guild_id,
            attachments: msg.attachments.iter().map(|a| a.url.clone()).collect(),
        }
    }
}

/// The full context for a reply chain
#[derive(Clone, Debug)]
pub struct ReplyChain {
    /// The original command that started this chain
    pub command_name: String,
    /// The original command options
    pub options: HashMap<String, OptionValue>,
    /// The message chain, from oldest to newest
    /// First message is typically the bot's response to the original command
    pub messages: Vec<ChainMessage>,
}

/// Build a message chain by walking up the reference chain
pub async fn build_message_chain(
    http: &Http,
    starting_message: &Message,
    max_depth: usize,
) -> anyhow::Result<Vec<ChainMessage>> {
    let mut chain = vec![ChainMessage::from_message(starting_message)];
    let mut current_msg = starting_message.clone();

    for _ in 0..max_depth {
        // Check if this message references another
        let Some(ref msg_ref) = current_msg.message_reference else {
            break;
        };

        let Some(referenced_msg_id) = msg_ref.message_id else {
            break;
        };

        // Try to use cached referenced_message first
        if let Some(ref referenced) = current_msg.referenced_message {
            chain.push(ChainMessage::from_message(referenced));
            current_msg = (**referenced).clone();
        } else {
            // Fetch the message from Discord
            match current_msg
                .channel_id
                .message(http, referenced_msg_id)
                .await
            {
                Ok(fetched) => {
                    chain.push(ChainMessage::from_message(&fetched));
                    current_msg = fetched;
                }
                Err(_) => {
                    // Can't fetch message, stop here
                    break;
                }
            }
        }
    }

    // Reverse so oldest is first
    chain.reverse();
    Ok(chain)
}
