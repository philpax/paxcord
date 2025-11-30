use serenity::all::{CommandInteraction, Http};

pub mod execute;
pub mod lua_command;
pub mod lua_registry;
pub mod reload;

pub use lua_registry::CommandRegistry;

#[serenity::async_trait]
pub trait CommandHandler: Send + Sync {
    fn name(&self) -> &str;
    async fn register(&self, http: &Http) -> anyhow::Result<()>;
    async fn run(&self, http: &Http, cmd: &CommandInteraction) -> anyhow::Result<()>;
}
