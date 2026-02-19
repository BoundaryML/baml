//! Native LSP client sender over crossbeam channel to stdio.
//!
//! Implements `bex_project::LspClientSenderTrait` by writing
//! `lsp_server::Message` frames to a `crossbeam::channel::Sender`.

use std::sync::Weak;

use bex_project::LspError;
use crossbeam::channel::Sender;
use lsp_server::Message;

#[derive(Clone)]
pub struct NativeLspSender {
    weak: Weak<Sender<Message>>,
}

impl NativeLspSender {
    pub fn new(sender: &std::sync::Arc<Sender<Message>>) -> Self {
        Self {
            weak: std::sync::Arc::downgrade(sender),
        }
    }
}

impl bex_project::LspClientSenderTrait for NativeLspSender {
    fn send_notification(&self, notif: lsp_server::Notification) -> Result<(), LspError> {
        let Some(sender) = self.weak.upgrade() else {
            return Err(LspError::ClientClosed);
        };
        sender
            .send(Message::Notification(notif))
            .map_err(|_| LspError::ClientClosed)
    }

    fn send_response_impl(&self, response: lsp_server::Response) -> Result<(), LspError> {
        let Some(sender) = self.weak.upgrade() else {
            return Err(LspError::ClientClosed);
        };
        sender
            .send(Message::Response(response))
            .map_err(|_| LspError::ClientClosed)
    }

    fn make_request(&self, req: lsp_server::Request) -> Result<(), LspError> {
        let Some(sender) = self.weak.upgrade() else {
            return Err(LspError::ClientClosed);
        };
        sender
            .send(Message::Request(req))
            .map_err(|_| LspError::ClientClosed)
    }
}
