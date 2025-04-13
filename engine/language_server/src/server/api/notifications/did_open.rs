use lsp_types::notification::DidOpenTextDocument;
use lsp_types::{DidOpenTextDocumentParams, PublishDiagnosticsParams};

use crate::server::api::diagnostics::publish_session_lsp_diagnostics;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::api::ResultExt;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;

pub(crate) struct DidOpenTextDocumentHandler;

impl NotificationHandler for DidOpenTextDocumentHandler {
    type NotificationType = DidOpenTextDocument;
}

impl SyncNotificationHandler for DidOpenTextDocumentHandler {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        params: DidOpenTextDocumentParams,
    ) -> Result<()> {
        tracing::info!("DidOpenTextDocumentHandler");

        let url = params.text_document.uri;
        session
            .ensure_project_db_for_baml_file(&url)
            .internal_error()?;
        session.reload(Some(notifier.clone())).internal_error()?;

        publish_session_lsp_diagnostics(&notifier, session, &url)?;

        Ok(())
    }
}
