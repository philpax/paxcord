use std::sync::Arc;

use crate::ai::Ai;
use crate::currency::CurrencyConverter;

mod comfyui;
pub mod currency;
mod globals;
mod llm;
mod perchance;

pub use globals::{Attachment, TemporaryChannelUpdate};

pub fn register(
    lua: &mlua::Lua,
    ai: Arc<Ai>,
    currency_converter: Arc<CurrencyConverter>,
    output_tx: flume::Sender<String>,
    print_tx: flume::Sender<String>,
    attachment_tx: flume::Sender<Attachment>,
) -> mlua::Result<()> {
    globals::register(lua, output_tx, print_tx, attachment_tx)?;
    llm::register(lua, ai)?;
    perchance::register(lua)?;
    currency::register(lua, currency_converter)?;
    comfyui::register(lua)?;
    Ok(())
}
