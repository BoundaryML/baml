use lsp_types::notification::DidOpenTextDocument;
use lsp_types::{DidOpenTextDocumentParams, PublishDiagnosticsParams, TextDocumentItem};

use crate::server::api::diagnostics::publish_session_lsp_diagnostics;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::api::ResultExt;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use crate::{DocumentKey, TextDocument};

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

        let file_path = url
            .to_file_path()
            .internal_error_msg(&format!("Could not convert URL '{}' to file path", url))?;

        let project = session.get_or_create_project(&file_path);
        if project.is_none() {
            tracing::error!("Failed to get or create project for path: {:?}", file_path);
            show_err_msg!("Failed to get or create project for path: {:?}", file_path);
        }
        // session.open_text_document(
        //     DocumentKey::from_path(&file_path, &file_path).internal_error()?,
        //     TextDocument::new(params.text_document.text, params.text_document.version),
        // );

        session.reload(Some(notifier.clone())).internal_error()?;

        publish_session_lsp_diagnostics(&notifier, session, &url)?;

        Ok(())
    }
}
