use std::{collections::HashMap, sync::Arc};

const OUTPUT_CHANNELS_MAP_KEY: &str = "_output_channels_map";
const DEFAULT_CHANNELS_KEY: usize = 0;

/// An attachment with filename and binary data
#[derive(Clone)]
pub struct Attachment {
    pub filename: String,
    pub data: Vec<u8>,
}

pub fn register(
    lua: &mlua::Lua,
    output_tx: flume::Sender<String>,
    print_tx: flume::Sender<String>,
    attachment_tx: flume::Sender<Attachment>,
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

    // Initialize channels map with default channels
    let mut channels_map = OutputChannelsMap::new();
    channels_map.insert(
        DEFAULT_CHANNELS_KEY,
        OutputChannels::new(output_tx, print_tx, attachment_tx),
    );
    lua.set_named_registry_value(OUTPUT_CHANNELS_MAP_KEY, channels_map)?;
    lua.globals().set(
        "output",
        lua.create_function(move |lua, values: mlua::Variadic<String>| {
            let output = values.into_iter().collect::<Vec<_>>().join("\t");
            with_current_channels(lua, |channels| channels.send_output(output.clone()))?;
            Ok(output)
        })?,
    )?;
    lua.globals().set(
        "print",
        lua.create_function(move |lua, values: mlua::Variadic<String>| {
            let output = values.into_iter().collect::<Vec<_>>().join("\t");
            with_current_channels(lua, |channels| channels.send_print(output.clone()))?;
            Ok(output)
        })?,
    )?;
    lua.globals().set(
        "attach",
        lua.create_function(move |lua, (filename, data): (String, mlua::String)| {
            let data = data.as_bytes().to_vec();
            with_current_channels(lua, |channels| {
                channels.send_attachment(Attachment {
                    filename: filename.clone(),
                    data,
                })
            })?;
            Ok(())
        })?,
    )?;

    Ok(())
}

pub struct TemporaryChannelUpdate {
    lua: mlua::Lua,
    thread_key: usize,
}
impl Drop for TemporaryChannelUpdate {
    fn drop(&mut self) {
        if let Ok(channels_map_ud) = self
            .lua
            .named_registry_value::<mlua::AnyUserData>(OUTPUT_CHANNELS_MAP_KEY)
            && let Ok(mut channels_map) = channels_map_ud.borrow_mut::<OutputChannelsMap>()
        {
            channels_map.remove(&self.thread_key);
        }
    }
}
impl TemporaryChannelUpdate {
    pub fn new(
        lua: mlua::Lua,
        thread: &mlua::Thread,
        output_tx: flume::Sender<String>,
        print_tx: flume::Sender<String>,
        attachment_tx: flume::Sender<Attachment>,
    ) -> mlua::Result<Self> {
        let thread_key = thread.to_pointer() as usize;
        let channels_map_ud: mlua::AnyUserData =
            lua.named_registry_value(OUTPUT_CHANNELS_MAP_KEY)?;
        let mut channels_map = channels_map_ud.borrow_mut::<OutputChannelsMap>()?;
        channels_map.insert(
            thread_key,
            OutputChannels::new(output_tx, print_tx, attachment_tx),
        );
        Ok(Self { lua, thread_key })
    }
}

/// Map from thread pointer to output channels
struct OutputChannelsMap {
    map: HashMap<usize, OutputChannels>,
}
impl mlua::UserData for OutputChannelsMap {}
impl OutputChannelsMap {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    fn insert(&mut self, key: usize, channels: OutputChannels) {
        self.map.insert(key, channels);
    }

    fn get_for_thread(&self, thread_key: usize) -> Option<&OutputChannels> {
        self.map
            .get(&thread_key)
            .or_else(|| self.map.get(&DEFAULT_CHANNELS_KEY))
    }

    fn remove(&mut self, key: &usize) {
        self.map.remove(key);
    }
}

/// Helper to get channels for the current thread
fn with_current_channels<T>(
    lua: &mlua::Lua,
    f: impl FnOnce(&OutputChannels) -> mlua::Result<T>,
) -> mlua::Result<Option<T>> {
    let thread_key = lua.current_thread().to_pointer() as usize;
    let channels_map_ud: mlua::AnyUserData = lua.named_registry_value(OUTPUT_CHANNELS_MAP_KEY)?;
    let channels_map = channels_map_ud.borrow::<OutputChannelsMap>()?;

    if let Some(channels) = channels_map.get_for_thread(thread_key) {
        Ok(Some(f(channels)?))
    } else {
        Ok(None)
    }
}

/// Userdata containing output and print channels
#[derive(Clone)]
struct OutputChannels {
    pub output_tx: Option<flume::Sender<String>>,
    pub print_tx: Option<flume::Sender<String>>,
    pub attachment_tx: Option<flume::Sender<Attachment>>,
}
impl OutputChannels {
    pub fn new(
        output_tx: flume::Sender<String>,
        print_tx: flume::Sender<String>,
        attachment_tx: flume::Sender<Attachment>,
    ) -> Self {
        Self {
            output_tx: Some(output_tx),
            print_tx: Some(print_tx),
            attachment_tx: Some(attachment_tx),
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

    pub fn send_attachment(&self, attachment: Attachment) -> mlua::Result<()> {
        if let Some(tx) = self.attachment_tx.as_ref() {
            tx.send(attachment)
                .map_err(|e| mlua::Error::ExternalError(Arc::new(e)))?;
        }
        Ok(())
    }
}
