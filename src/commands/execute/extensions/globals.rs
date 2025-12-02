use super::output_userdata::OutputChannels;

pub fn register(
    lua: &mlua::Lua,
    output_tx: flume::Sender<String>,
    print_tx: flume::Sender<String>,
) -> mlua::Result<()> {
    lua.globals().set(
        "sleep",
        lua.create_async_function(|_lua, ms: u32| async move {
            tokio::time::sleep(std::time::Duration::from_millis(ms as u64)).await;
            Ok(())
        })?,
    )?;

    lua.globals().set(
        "yield",
        lua.globals()
            .get("coroutine")
            .and_then(|c: mlua::Table| c.get::<mlua::Function>("yield"))?,
    )?;

    lua.globals().set(
        "inspect",
        lua.load(include_str!("../../../../vendor/inspect.lua/inspect.lua"))
            .eval::<mlua::Value>()?,
    )?;

    // Create output channels userdata
    let channels = OutputChannels::new(output_tx, print_tx);

    // Store channels in registry for later updates
    lua.set_named_registry_value("_output_channels", channels.clone())?;

    // Create output() function that uses the userdata
    lua.globals().set(
        "output",
        lua.create_function(move |_lua, values: mlua::Variadic<String>| {
            let channels_ud: mlua::AnyUserData = _lua.named_registry_value("_output_channels")?;
            let channels = channels_ud.borrow::<OutputChannels>()?;
            let output = values.into_iter().collect::<Vec<_>>().join("\t");
            channels.send_output(output.clone())?;
            Ok(output)
        })?,
    )?;

    // Create print() function that uses the userdata
    lua.globals().set(
        "print",
        lua.create_function(move |_lua, values: mlua::Variadic<String>| {
            let channels_ud: mlua::AnyUserData = _lua.named_registry_value("_output_channels")?;
            let channels = channels_ud.borrow::<OutputChannels>()?;
            let output = values.into_iter().collect::<Vec<_>>().join("\t");
            channels.send_print(output.clone())?;
            Ok(output)
        })?,
    )?;

    Ok(())
}

/// Update the output channels for execution-scoped output
pub fn update_channels(
    lua: &mlua::Lua,
    output_tx: flume::Sender<String>,
    print_tx: flume::Sender<String>,
) -> mlua::Result<()> {
    let channels_ud: mlua::AnyUserData = lua.named_registry_value("_output_channels")?;
    let channels = channels_ud.borrow::<OutputChannels>()?;
    channels.update(output_tx, print_tx);
    Ok(())
}
