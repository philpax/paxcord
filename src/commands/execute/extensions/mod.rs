use std::sync::Arc;

use crate::ai::Ai;

mod currency;
mod globals;
mod llm;
mod perchance;

pub fn register(
    lua: &mlua::Lua,
    ai: Arc<Ai>,
    output_tx: flume::Sender<String>,
    print_tx: flume::Sender<String>,
) -> mlua::Result<()> {
    globals::register(lua, output_tx, print_tx)?;
    llm::register(lua, ai)?;
    perchance::register(lua)?;
    currency::register(lua)?;
    Ok(())
}
