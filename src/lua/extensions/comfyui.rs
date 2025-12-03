use mlua::{Lua, Result};

pub fn register(lua: &Lua) -> Result<()> {
    let config = rucomfyui_mlua::IntegrationConfig::all();
    let comfy = rucomfyui_mlua::module(lua, &config)?;
    lua.globals().set("comfy", comfy)?;
    Ok(())
}
