//! No-op `LspClientSenderTrait` impl for embedded contexts (e.g. bridge_cffi)
//! where no editor LSP client is connected.
//!
//! All outgoing notifications/responses/requests are silently dropped.

use bex_project::{LspClientSenderTrait, LspError};

#[derive(Clone, Default)]
pub struct NoOpLspSender;

impl LspClientSenderTrait for NoOpLspSender {
    fn send_notification(&self, _notif: lsp_server::Notification) -> Result<(), LspError> {
        Ok(())
    }
    fn send_response_impl(&self, _response: lsp_server::Response) -> Result<(), LspError> {
        Ok(())
    }
    fn make_request(&self, _req: lsp_server::Request) -> Result<(), LspError> {
        Ok(())
    }
}
