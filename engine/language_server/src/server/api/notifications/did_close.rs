use lsp_server::ErrorCode;
use lsp_types::notification::DidCloseTextDocument;
use lsp_types::DidCloseTextDocumentParams;
use std::path::PathBuf;

// use crate::server::api::diagnostics::clear_diagnostics;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
// use crate::server::api::LSPResult;
use crate::server::api::LSPResult;
use crate::server::api::ResultExt;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use crate::DocumentKey;
// use crate::system::{url_to_any_system_path, AnySystemPath};

pub(crate) struct DidCloseTextDocumentHandler;

impl NotificationHandler for DidCloseTextDocumentHandler {
    type NotificationType = DidCloseTextDocument;
}

impl SyncNotificationHandler for DidCloseTextDocumentHandler {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: DidCloseTextDocumentParams,
    ) -> Result<()> {
        tracing::info!("------------ DidCloseTextDocumentHandler");
        let url = params.text_document.uri;
        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;

        match session.get_or_create_project(&path) {
            None => {}
            Some(project) => {
                let document_key = DocumentKey::from_url(
                    &PathBuf::from(project.lock().unwrap().root_path()),
                    &url,
                )
                .internal_error()?;
                session
                    .close_document(&document_key)
                    .with_failure_code(ErrorCode::InternalError)?;
                // Remove the unsaved file from the project as well
                // TODO: ideally the baml project just has a view of unsaved files directly from the Session itself, and not maintain its own state / copy of the unsaved files
                project
                    .lock()
                    .unwrap()
                    .baml_project
                    .remove_unsaved_file(&document_key);
            }
        }
        session.reload(Some(_notifier)).internal_error()?;

        Ok(())
    }
}
