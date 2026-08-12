//! Native LSP client sender over crossbeam channel to stdio.
//!
//! Implements `bex_project::LspClientSenderTrait` by reserving bounded
//! [`crate::OutboundFrame`]s and enqueueing them for the stdio writer thread.
//! Saturation and oversize map to their typed `LspError`s instead of blocking
//! or silently dropping; the writer queue is always bounded.

use std::sync::Weak;

use bex_project::LspError;
use crossbeam_channel::Sender;

use crate::{OutboundBudget, OutboundFrame, OutboundReserveError};

#[derive(Clone)]
pub struct NativeLspSender {
    weak: Weak<Sender<OutboundFrame>>,
    budget: Weak<OutboundBudget>,
}

impl NativeLspSender {
    pub fn new(
        sender: &std::sync::Arc<Sender<OutboundFrame>>,
        budget: &std::sync::Arc<OutboundBudget>,
    ) -> Self {
        Self {
            weak: std::sync::Arc::downgrade(sender),
            budget: std::sync::Arc::downgrade(budget),
        }
    }

    fn send(&self, message: lsp_server::Message) -> Result<(), LspError> {
        let sender = self.weak.upgrade().ok_or(LspError::ClientClosed)?;
        let budget = self.budget.upgrade().ok_or(LspError::ClientClosed)?;
        let frame = budget.try_message(message).map_err(|error| match error {
            OutboundReserveError::Saturated => LspError::OutboundSaturated,
            OutboundReserveError::Oversized | OutboundReserveError::Serialization => {
                LspError::OutboundOversized
            }
        })?;
        match sender.try_send(frame) {
            Ok(()) => Ok(()),
            Err(crossbeam_channel::TrySendError::Full(_)) => Err(LspError::OutboundSaturated),
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => Err(LspError::ClientClosed),
        }
    }
}

impl bex_project::LspClientSenderTrait for NativeLspSender {
    fn send_notification(&self, notif: lsp_server::Notification) -> Result<(), LspError> {
        self.send(lsp_server::Message::Notification(notif))
    }

    fn send_response_impl(&self, response: lsp_server::Response) -> Result<(), LspError> {
        self.send(lsp_server::Message::Response(response))
    }

    fn make_request(&self, req: lsp_server::Request) -> Result<(), LspError> {
        self.send(lsp_server::Message::Request(req))
    }
}

#[cfg(test)]
mod tests {
    use bex_project::LspClientSenderTrait;

    use super::*;

    fn notification(method: &str) -> lsp_server::Notification {
        lsp_server::Notification::new(method.to_string(), serde_json::Value::Null)
    }

    #[test]
    fn full_native_queue_is_backpressure_not_transport_closure() {
        let (tx, _rx) = crossbeam_channel::bounded(1);
        let tx = std::sync::Arc::new(tx);
        let budget = OutboundBudget::new();
        let sender = NativeLspSender::new(&tx, &budget);

        assert!(sender.send_notification(notification("test/first")).is_ok());
        assert!(matches!(
            sender.send_notification(notification("test/full")),
            Err(LspError::OutboundSaturated)
        ));

        drop(tx);
        assert!(matches!(
            sender.send_notification(notification("test/closed")),
            Err(LspError::ClientClosed)
        ));
    }

    #[test]
    fn oversized_native_frame_is_distinct_from_temporary_backpressure() {
        let (tx, _rx) = crossbeam_channel::bounded(1);
        let tx = std::sync::Arc::new(tx);
        let budget = OutboundBudget::new();
        let sender = NativeLspSender::new(&tx, &budget);
        let oversized = lsp_server::Notification::new(
            "test/oversized".to_string(),
            serde_json::Value::String("x".repeat(crate::MAX_OUTBOUND_FRAME_BYTES + 1)),
        );

        assert!(matches!(
            sender.send_notification(oversized),
            Err(LspError::OutboundOversized)
        ));
        assert!(
            sender.send_notification(notification("test/after")).is_ok(),
            "an oversized frame must not poison the sender"
        );
    }
}
