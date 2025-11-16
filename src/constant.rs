/// names of values used in interactions
pub mod value {
    #[allow(dead_code)]
    pub const PROMPT: &str = "prompt";
    #[allow(dead_code)]
    pub const SEED: &str = "seed";
    #[allow(dead_code)]
    pub const MODEL: &str = "model";

    pub const MESSAGE_ID: &str = "message_id";
    pub const CODE: &str = "code";
}

/// names of non-user-configurable commands
pub mod commands {
    /// Used by the message command
    pub const EXECUTE_THIS_CODE_BLOCK: &str = "Execute this code block";
    /// Used by the slash command
    pub const EXECUTE: &str = "execute";
}
