use serenity::all::{CommandInteraction, Http};

pub mod execute;
pub mod lua_command;

#[serenity::async_trait]
pub trait CommandHandler: Send + Sync {
    async fn register(&self, http: &Http) -> anyhow::Result<()>;
    async fn run(&self, http: &Http, cmd: &CommandInteraction) -> anyhow::Result<()>;
}
