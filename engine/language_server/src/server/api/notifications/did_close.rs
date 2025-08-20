use std::path::PathBuf;

use lsp_server::ErrorCode;
use lsp_types::{notification::DidCloseTextDocument, DidCloseTextDocumentParams};

// use crate::server::api::diagnostics::clear_diagnostics;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
// use crate::server::api::LSPResult;
use crate::server::api::LSPResult;
use crate::{
    server::{
        api::ResultExt,
        client::{Notifier, Requester},
        Result,
    },
    session::Session,
    DocumentKey,
};
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
        let url = params.text_document.uri;
        if !url.to_string().contains("baml_src") {
            return Ok(());
        }

        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;

        match session.get_or_create_project(&path) {
            None => {}
            Some(project) => {
                let document_key =
                    DocumentKey::from_url(&PathBuf::from(project.lock().root_path()), &url)
                        .internal_error()?;
                session
                    .close_document(&document_key)
                    .with_failure_code(ErrorCode::InternalError)?;
                // Remove the unsaved file from the project as well
                // TODO: ideally the baml project just has a view of unsaved files directly from the Session itself, and not maintain its own state / copy of the unsaved files
                project
                    .lock()
                    .baml_project
                    .remove_unsaved_file(&document_key);
            }
        }
        session.reload(Some(_notifier)).internal_error()?;

        Ok(())
    }
}
