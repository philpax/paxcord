use std::sync::{Arc, Mutex};

/// Userdata containing output and print channels that can be updated
#[derive(Clone)]
pub struct OutputChannels {
    output_tx: Arc<Mutex<Option<flume::Sender<String>>>>,
    print_tx: Arc<Mutex<Option<flume::Sender<String>>>>,
}

impl OutputChannels {
    pub fn new(output_tx: flume::Sender<String>, print_tx: flume::Sender<String>) -> Self {
        Self {
            output_tx: Arc::new(Mutex::new(Some(output_tx))),
            print_tx: Arc::new(Mutex::new(Some(print_tx))),
        }
    }

    pub fn update(&self, output_tx: flume::Sender<String>, print_tx: flume::Sender<String>) {
        *self.output_tx.lock().unwrap() = Some(output_tx);
        *self.print_tx.lock().unwrap() = Some(print_tx);
    }

    pub fn send_output(&self, msg: String) -> mlua::Result<()> {
        if let Some(tx) = self.output_tx.lock().unwrap().as_ref() {
            tx.send(msg)
                .map_err(|e| mlua::Error::ExternalError(Arc::new(e)))?;
        }
        Ok(())
    }

    pub fn send_print(&self, msg: String) -> mlua::Result<()> {
        if let Some(tx) = self.print_tx.lock().unwrap().as_ref() {
            tx.send(msg)
                .map_err(|e| mlua::Error::ExternalError(Arc::new(e)))?;
        }
        Ok(())
    }
}

impl mlua::UserData for OutputChannels {}
