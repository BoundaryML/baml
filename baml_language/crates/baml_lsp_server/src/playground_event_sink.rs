//! EventSink that broadcasts runtime events to WebSocket playground clients.
//!
//! Encodes each `RuntimeEvent` to protobuf bytes, base64-encodes the bytes,
//! and sends them as `WsOutMessage::RuntimeEvent` via a broadcast channel.

use base64::Engine as _;
use tokio::sync::broadcast;

use crate::playground_ws::WsOutMessage;

pub struct PlaygroundEventSink {
    broadcast_tx: broadcast::Sender<WsOutMessage>,
}

impl PlaygroundEventSink {
    pub fn new(broadcast_tx: broadcast::Sender<WsOutMessage>) -> Self {
        Self { broadcast_tx }
    }
}

impl bex_events::EventSink for PlaygroundEventSink {
    fn send(&self, event: bex_events::RuntimeEvent) {
        let call_id = event.call_id.0;
        let options = bridge_ctypes::CffiHandleTableOptions::for_wire();
        match bridge_ctypes::runtime_event_to_bytes(&event, &options) {
            Ok(data) => {
                let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                let _ = self
                    .broadcast_tx
                    .send(WsOutMessage::RuntimeEvent { data: b64, call_id });
            }
            Err(e) => {
                tracing::error!("Failed to encode runtime event: {e}");
            }
        }
    }

    fn flush(&self) {
        // Broadcast is fire-and-forget, nothing to flush.
    }
}
