use std::sync::Arc;

use crate::currency::CurrencyConverter;

/// Register currency conversion functions with Lua
pub fn register(lua: &mlua::Lua, converter: Arc<CurrencyConverter>) -> mlua::Result<()> {
    let currency = lua.create_table()?;

    // Main conversion function
    currency.set(
        "convert",
        lua.create_async_function({
            let converter = converter.clone();
            move |_lua, (from, to, amount): (String, String, f64)| {
                let converter = converter.clone();
                async move {
                    converter
                        .convert(&from, &to, amount)
                        .await
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))
                }
            }
        })?,
    )?;

    // Function to get just the conversion rate
    currency.set(
        "rate",
        lua.create_async_function({
            let converter = converter.clone();
            move |_lua, (from, to): (String, String)| {
                let converter = converter.clone();
                async move {
                    converter
                        .rate(&from, &to)
                        .await
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))
                }
            }
        })?,
    )?;

    // Function to clear the cache
    currency.set(
        "clear_cache",
        lua.create_async_function({
            let converter = converter.clone();
            move |_lua, ()| {
                let converter = converter.clone();
                async move {
                    converter.clear_cache().await;
                    Ok(())
                }
            }
        })?,
    )?;

    lua.globals().set("currency", currency)?;

    Ok(())
}
