use std::sync::Arc;

use crate::ai::Ai;
use crate::currency::CurrencyConverter;

pub mod currency;
mod globals;
mod llm;
mod perchance;

pub use globals::TemporaryChannelUpdate;

pub fn register(
    lua: &mlua::Lua,
    ai: Arc<Ai>,
    currency_converter: Arc<CurrencyConverter>,
    output_tx: flume::Sender<String>,
    print_tx: flume::Sender<String>,
) -> mlua::Result<()> {
    globals::register(lua, output_tx, print_tx)?;
    llm::register(lua, ai)?;
    perchance::register(lua)?;
    currency::register(lua, currency_converter)?;
    Ok(())
}
