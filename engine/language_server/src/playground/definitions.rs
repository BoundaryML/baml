use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};
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
    /// Buffer for events that occur before the first client connects.
    pub event_buffer: VecDeque<String>,
    pub first_client_connected: bool,
    /// Track the number of active connections
    pub active_connections: usize,
}

impl Default for PlaygroundState {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaygroundState {
    pub fn new() -> Self {
        let (tx, rx) = broadcast::channel(100);
        Self {
            tx,
            _rx: rx,
            event_buffer: VecDeque::new(),
            first_client_connected: false,
            active_connections: 0,
        }
    }

    pub fn broadcast_update(&self, msg: String) -> anyhow::Result<()> {
        let n = self.tx.send(msg)?;
        tracing::debug!("broadcast sent to {n} receivers");
        Ok(())
    }

    /// Push an event to the buffer if no clients are connected.
    pub fn buffer_event(&mut self, event: String) {
        if self.active_connections == 0 {
            self.event_buffer.push_back(event);
        }
    }

    /// Drain the buffer, returning all buffered events.
    pub fn drain_event_buffer(&mut self) -> Vec<String> {
        self.event_buffer.drain(..).collect()
    }

    /// Mark that the first client has connected.
    pub fn mark_first_client_connected(&mut self) {
        self.first_client_connected = true;
    }

    /// Mark that a client has connected.
    pub fn mark_client_connected(&mut self) {
        self.active_connections += 1;
        if self.active_connections == 1 {
            self.first_client_connected = true;
        }
    }

    /// Mark that a client has disconnected.
    pub fn mark_client_disconnected(&mut self) {
        if self.active_connections > 0 {
            self.active_connections -= 1;
            if self.active_connections == 0 {
                self.first_client_connected = false;
                tracing::info!("All clients disconnected, resetting connection state");
            }
        }
    }
}
