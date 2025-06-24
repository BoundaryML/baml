use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::broadcast;

// Note: the name add_project should match exactly to the
// EventListener.tsx command definitions due to how serde serializes these into json
#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "command", content = "content")]
pub enum FrontendMessage {
    add_project {
        root_path: String,
        files: HashMap<String, String>,
    },
    remove_project {
        root_path: String,
    },
    select_function {
        root_path: String,
        function_name: String,
    },
    baml_settings_updated {
        settings: HashMap<String, String>,
    },
    run_test {
        test_name: String,
    },
}

#[derive(Debug)]
pub struct PlaygroundState {
    pub tx: broadcast::Sender<String>,
    // Keep a reference to the receiver to prevent the channel from being closed
    _rx: broadcast::Receiver<String>,
}

impl PlaygroundState {
    pub fn new() -> Self {
        let (tx, rx) = broadcast::channel(100);
        Self { tx, _rx: rx }
    }

    pub fn broadcast_update(&self, msg: String) -> anyhow::Result<()> {
        let n = self.tx.send(msg)?;
        tracing::debug!("broadcast sent to {n} receivers");
        Ok(())
    }
}
