use std::{collections::HashMap, num::NonZeroUsize, sync::Mutex};

use lru::LruCache;
use serenity::all::{ChannelId, GuildId, MessageId, UserId};

/// Context stored for an interaction response, allowing us to handle replies
#[derive(Clone, Debug)]
pub struct InteractionContext {
    /// The command that was invoked
    pub command_name: String,
    /// The options passed to the command (name -> value as string/number)
    pub options: HashMap<String, OptionValue>,
    /// The user who invoked the command
    #[allow(dead_code)]
    pub user_id: UserId,
    /// The channel where the command was invoked
    #[allow(dead_code)]
    pub channel_id: ChannelId,
    /// The guild where the command was invoked (None for DMs)
    #[allow(dead_code)]
    pub guild_id: Option<GuildId>,
}

/// A command option value
#[derive(Clone, Debug)]
pub enum OptionValue {
    String(String),
    Integer(i64),
    Number(f64),
    Boolean(bool),
}

#[allow(dead_code)]
impl OptionValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            OptionValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            OptionValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            OptionValue::Number(n) => Some(*n),
            OptionValue::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            OptionValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }
}

/// Thread-safe LRU cache for interaction contexts
pub struct InteractionContextStore {
    cache: Mutex<LruCache<MessageId, InteractionContext>>,
}

impl InteractionContextStore {
    /// Create a new store with the given capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(capacity).expect("capacity must be non-zero"),
            )),
        }
    }

    /// Store context for a message ID (the bot's response message)
    pub fn store(&self, message_id: MessageId, context: InteractionContext) {
        self.cache.lock().unwrap().put(message_id, context);
    }

    /// Get context for a message ID
    pub fn get(&self, message_id: &MessageId) -> Option<InteractionContext> {
        self.cache.lock().unwrap().get(message_id).cloned()
    }
}
