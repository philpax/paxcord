use std::sync::Arc;

const GLOBAL_OUTPUT_CHANNELS_KEY: &str = "_output_channels";

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
        lua.load(include_str!("../../../vendor/inspect.lua/inspect.lua"))
            .eval::<mlua::Value>()?,
    )?;

    let channels = OutputChannels::new(output_tx, print_tx);
    lua.set_named_registry_value(GLOBAL_OUTPUT_CHANNELS_KEY, channels)?;
    lua.globals().set(
        "output",
        lua.create_function(move |lua, values: mlua::Variadic<String>| {
            let channels_ud: mlua::AnyUserData =
                lua.named_registry_value(GLOBAL_OUTPUT_CHANNELS_KEY)?;
            let channels = channels_ud.borrow::<OutputChannels>()?;
            let output = values.into_iter().collect::<Vec<_>>().join("\t");
            channels.send_output(output.clone())?;
            Ok(output)
        })?,
    )?;
    lua.globals().set(
        "print",
        lua.create_function(move |lua, values: mlua::Variadic<String>| {
            let channels_ud: mlua::AnyUserData =
                lua.named_registry_value(GLOBAL_OUTPUT_CHANNELS_KEY)?;
            let channels = channels_ud.borrow::<OutputChannels>()?;
            let output = values.into_iter().collect::<Vec<_>>().join("\t");
            channels.send_print(output.clone())?;
            Ok(output)
        })?,
    )?;

    Ok(())
}

pub struct TemporaryChannelUpdate {
    lua: mlua::Lua,
    old_channels: Option<OutputChannels>,
}
impl Drop for TemporaryChannelUpdate {
    fn drop(&mut self) {
        if let Some(old_channels) = self.old_channels.take() {
            self.lua
                .set_named_registry_value(GLOBAL_OUTPUT_CHANNELS_KEY, old_channels)
                .ok();
        }
    }
}
impl TemporaryChannelUpdate {
    pub fn new(
        lua: mlua::Lua,
        output_tx: flume::Sender<String>,
        print_tx: flume::Sender<String>,
    ) -> mlua::Result<Self> {
        let channels_ud: mlua::AnyUserData =
            lua.named_registry_value(GLOBAL_OUTPUT_CHANNELS_KEY)?;
        let mut channels = channels_ud.borrow_mut::<OutputChannels>()?;
        let old_channels = channels.clone();

        *channels = OutputChannels::new(output_tx, print_tx);
        Ok(Self {
            lua,
            old_channels: Some(old_channels),
        })
    }
}
/// Userdata containing output and print channels that can be updated
#[derive(Clone)]
struct OutputChannels {
    pub output_tx: Option<flume::Sender<String>>,
    pub print_tx: Option<flume::Sender<String>>,
}
impl mlua::UserData for OutputChannels {}
impl OutputChannels {
    pub fn new(output_tx: flume::Sender<String>, print_tx: flume::Sender<String>) -> Self {
        Self {
            output_tx: Some(output_tx),
            print_tx: Some(print_tx),
        }
    }

    pub fn send_output(&self, msg: String) -> mlua::Result<()> {
        if let Some(tx) = self.output_tx.as_ref() {
            tx.send(msg)
                .map_err(|e| mlua::Error::ExternalError(Arc::new(e)))?;
        }
        Ok(())
    }

    pub fn send_print(&self, msg: String) -> mlua::Result<()> {
        if let Some(tx) = self.print_tx.as_ref() {
            tx.send(msg)
                .map_err(|e| mlua::Error::ExternalError(Arc::new(e)))?;
        }
        Ok(())
    }
}
