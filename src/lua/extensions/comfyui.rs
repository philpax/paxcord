use std::sync::{Arc, Mutex, OnceLock};

use mlua::{Lua, LuaSerdeExt, Result};

const COMFYUI_URL: &str = "http://127.0.0.1:8188";

/// Cached object info for lazy fetching (stored as JSON for thread-safety)
static OBJECT_INFO_CACHE: OnceLock<Mutex<Option<serde_json::Value>>> = OnceLock::new();

fn get_cache() -> &'static Mutex<Option<serde_json::Value>> {
    OBJECT_INFO_CACHE.get_or_init(|| Mutex::new(None))
}

pub fn register(lua: &Lua) -> Result<()> {
    let config = rucomfyui_mlua::IntegrationConfig::all();
    let comfy_module = rucomfyui_mlua::module(lua, &config)?;

    // Create a wrapper table that provides lazy client and object_info
    let comfy = lua.create_table()?;

    // comfy.graph - pass through from rucomfyui_mlua
    let graph_fn = comfy_module.get::<mlua::Function>("graph")?;
    comfy.set("graph", graph_fn)?;

    // comfy.client() - returns a client connected to the ComfyUI server
    comfy.set(
        "client",
        lua.create_function(|lua, ()| {
            let client = rucomfyui::Client::new(COMFYUI_URL);
            let config = rucomfyui_mlua::ClientConfig::all();
            rucomfyui_mlua::create_client_userdata(lua, client, config)
        })?,
    )?;

    // comfy.object_info() - lazily fetches and caches object info
    comfy.set(
        "object_info",
        lua.create_async_function(|lua, ()| async move {
            // Check cache first
            {
                let cache = get_cache().lock().unwrap();
                if let Some(ref cached) = *cache {
                    return lua.to_value(cached);
                }
            }

            // Fetch object info from the server
            let client = rucomfyui::Client::new(COMFYUI_URL);
            let object_info = client
                .get_object_info()
                .await
                .map_err(|e| mlua::Error::ExternalError(Arc::new(e)))?;

            // Convert to JSON for caching
            let json_value = serde_json::to_value(&object_info)
                .map_err(|e| mlua::Error::ExternalError(Arc::new(e)))?;

            // Cache it
            {
                let mut cache = get_cache().lock().unwrap();
                *cache = Some(json_value.clone());
            }

            lua.to_value(&json_value)
        })?,
    )?;

    lua.globals().set("comfy", comfy)?;

    Ok(())
}
