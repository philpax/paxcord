use std::sync::Arc;

use crate::{ai::Ai, commands::lua_command::LuaCommandRegistry, currency::CurrencyConverter};

mod discord_extension;

mod executor;
pub use executor::{LuaOutputChannels, execute_lua_thread};

pub mod extensions;

pub fn create_barebones_lua_state(
    ai: Arc<Ai>,
    currency_converter: Arc<CurrencyConverter>,
    output_tx: flume::Sender<String>,
    print_tx: flume::Sender<String>,
    attachment_tx: flume::Sender<extensions::Attachment>,
) -> mlua::Result<mlua::Lua> {
    let lua = mlua::Lua::new_with(
        {
            use mlua::StdLib as SL;
            SL::COROUTINE | SL::MATH | SL::STRING | SL::TABLE | SL::UTF8 | SL::VECTOR
        },
        mlua::LuaOptions::new().catch_rust_panics(true),
    )?;

    extensions::register(
        &lua,
        ai,
        currency_converter,
        output_tx,
        print_tx,
        attachment_tx,
    )?;
    load_lua_file(&lua, "scripts/main.lua")?;

    Ok(lua)
}

pub fn create_global_lua_state(
    ai: Arc<Ai>,
    currency_converter: Arc<CurrencyConverter>,
    output_tx: flume::Sender<String>,
    print_tx: flume::Sender<String>,
    attachment_tx: flume::Sender<extensions::Attachment>,
    lua_command_registry: LuaCommandRegistry,
) -> mlua::Result<mlua::Lua> {
    let lua =
        create_barebones_lua_state(ai, currency_converter, output_tx, print_tx, attachment_tx)?;
    discord_extension::register(&lua, lua_command_registry)?;
    load_lua_file(&lua, "scripts/commands.lua")?;

    Ok(lua)
}

fn load_lua_file(lua: &mlua::Lua, path: &str) -> mlua::Result<()> {
    let script_path = std::path::Path::new(path);
    if script_path.exists() {
        let code = std::fs::read_to_string(script_path)
            .map_err(|e| mlua::Error::RuntimeError(format!("Failed to read {path}: {e}")))?;
        lua.load(&code).set_name(path).exec()?;
    }
    Ok(())
}

pub fn load_async_expression<R: mlua::FromLuaMulti>(
    lua: &mlua::Lua,
    expression: &str,
) -> anyhow::Result<mlua::AsyncThread<R>> {
    // First try: treat as expression, wrap with return and inspect for display
    let with_return = lua
        .load(
            format!(
                r#"
coroutine.create(function()
    local result = ({expression})
    if result ~= nil then
        return inspect(result)
    end
    return nil
end)
"#
            )
            .trim(),
        )
        .eval::<mlua::Thread>()
        .and_then(|t| t.into_async::<R>(()));

    match with_return {
        Ok(thread) => Ok(thread),
        Err(with_return_err) => {
            // Second try: treat as statements (for-loops, etc.) - no implicit return
            let without_return = lua
                .load(
                    format!(
                        r#"
coroutine.create(function()
{expression}
end)
"#
                    )
                    .trim(),
                )
                .eval::<mlua::Thread>()
                .and_then(|t| t.into_async::<R>(()));

            match without_return {
                Ok(thread) => Ok(thread),
                Err(without_return_err) => {
                    anyhow::bail!(
                        "Failed to load expression with return: {with_return_err:?} | without return: {without_return_err:?}"
                    );
                }
            }
        }
    }
}
