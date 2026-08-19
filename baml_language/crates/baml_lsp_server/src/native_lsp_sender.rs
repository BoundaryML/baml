//! Native LSP client sender over a crossbeam channel to the stdio writer.
//!
//! Reserves bounded [`crate::OutboundFrame`]s against the process outbound
//! budget and enqueues them for the stdout writer thread. Saturation and
//! oversize are reported as distinct outcomes instead of blocking or silently
//! dropping; the writer queue is always bounded. This is the stdio session's
//! [`Sink`](crate::lsp_runtime::Sink) and doubles as a session-less
//! [`ClientSender`] for the host itself.

use std::sync::{Arc, Weak};

use baml_lsp::{LspError, state::ClientSender};
use crossbeam_channel::Sender;

use crate::{OutboundBudget, OutboundFrame, OutboundReserveError, lsp_runtime::SinkDelivery};

#[derive(Clone)]
pub struct NativeLspSender {
    weak: Weak<Sender<OutboundFrame>>,
    budget: Weak<OutboundBudget>,
}

impl NativeLspSender {
    pub fn new(sender: &Arc<Sender<OutboundFrame>>, budget: &Arc<OutboundBudget>) -> Self {
        Self {
            weak: Arc::downgrade(sender),
            budget: Arc::downgrade(budget),
        }
    }

    /// Non-blocking delivery with the sink vocabulary: the writer channel and
    /// the budget both bound memory, so a full queue is transient
    /// backpressure, never closure.
    pub fn deliver(&self, message: &lsp_server::Message) -> SinkDelivery {
        let (Some(sender), Some(budget)) = (self.weak.upgrade(), self.budget.upgrade()) else {
            return SinkDelivery::Closed;
        };
        let frame = match budget.try_message(message) {
            Ok(frame) => frame,
            Err(OutboundReserveError::Saturated) => return SinkDelivery::Saturated,
            Err(OutboundReserveError::Oversized | OutboundReserveError::Serialization) => {
                return SinkDelivery::Oversized;
            }
        };
        match sender.try_send(frame) {
            Ok(()) => SinkDelivery::Sent,
            Err(crossbeam_channel::TrySendError::Full(_)) => SinkDelivery::Saturated,
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => SinkDelivery::Closed,
        }
    }

    fn send(&self, message: &lsp_server::Message) -> Result<(), LspError> {
        match self.deliver(message) {
            SinkDelivery::Sent => Ok(()),
            SinkDelivery::Saturated => Err(LspError::OutboundSaturated),
            SinkDelivery::Oversized => Err(LspError::OutboundOversized),
            SinkDelivery::Closed => Err(LspError::ClientClosed),
        }
    }
}

impl ClientSender for NativeLspSender {
    fn send_notification(&self, method: &str, params: serde_json::Value) -> Result<(), LspError> {
        self.send(&lsp_server::Message::Notification(
            lsp_server::Notification::new(method.to_owned(), params),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notification(method: &str) -> lsp_server::Message {
        lsp_server::Message::Notification(lsp_server::Notification::new(
            method.to_string(),
            serde_json::Value::Null,
        ))
    }

    #[test]
    fn full_native_queue_is_backpressure_not_transport_closure() {
        let (tx, _rx) = crossbeam_channel::bounded(1);
        let tx = Arc::new(tx);
        let budget = OutboundBudget::new();
        let sender = NativeLspSender::new(&tx, &budget);

        assert!(sender.send(&notification("test/first")).is_ok());
        assert!(matches!(
            sender.send(&notification("test/full")),
            Err(LspError::OutboundSaturated)
        ));

        drop(tx);
        assert!(matches!(
            sender.send(&notification("test/closed")),
            Err(LspError::ClientClosed)
        ));
    }

    #[test]
    fn oversized_native_frame_is_distinct_from_temporary_backpressure() {
        let (tx, _rx) = crossbeam_channel::bounded(1);
        let tx = Arc::new(tx);
        let budget = OutboundBudget::new();
        let sender = NativeLspSender::new(&tx, &budget);
        let oversized = lsp_server::Message::Notification(lsp_server::Notification::new(
            "test/oversized".to_string(),
            serde_json::Value::String("x".repeat(crate::MAX_OUTBOUND_FRAME_BYTES + 1)),
        ));

        assert!(matches!(
            sender.send(&oversized),
            Err(LspError::OutboundOversized)
        ));
        assert!(
            sender.send(&notification("test/after")).is_ok(),
            "an oversized frame must not poison the sender"
        );
    }

    #[test]
    fn client_sender_notifications_reach_the_writer_queue() {
        let (tx, rx) = crossbeam_channel::bounded(4);
        let tx = Arc::new(tx);
        let budget = OutboundBudget::new();
        let sender = NativeLspSender::new(&tx, &budget);

        sender
            .send_notification("window/logMessage", serde_json::json!({ "type": 3 }))
            .unwrap();
        let frame = rx.try_recv().unwrap();
        let value: serde_json::Value = serde_json::from_slice(frame.bytes()).unwrap();
        assert_eq!(value["method"], "window/logMessage");
        assert_eq!(value["jsonrpc"], "2.0");
    }
}
