use serenity::all::{CommandInteraction, Http};

pub mod currency;
pub mod execute;
pub mod hallucinate;
pub mod lua_command;
pub mod reload;

#[serenity::async_trait]
pub trait CommandHandler: Send + Sync {
    fn name(&self) -> &str;
    async fn register(&self, http: &Http) -> anyhow::Result<()>;
    async fn run(&self, http: &Http, cmd: &CommandInteraction) -> anyhow::Result<()>;
}
