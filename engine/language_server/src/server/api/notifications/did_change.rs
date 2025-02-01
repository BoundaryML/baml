use lsp_types::notification::DidChangeTextDocument;
use lsp_types::{DidChangeTextDocumentParams, PublishDiagnosticsParams};
use std::path::PathBuf;

use crate::server::api::diagnostics::session_lsp_diagnostics;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::api::ResultExt;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use crate::DocumentKey;

pub(crate) struct DidChangeTextDocumentHandler;

impl NotificationHandler for DidChangeTextDocumentHandler {
    type NotificationType = DidChangeTextDocument;
}

impl SyncNotificationHandler for DidChangeTextDocumentHandler {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        params: DidChangeTextDocumentParams,
    ) -> Result<()> {
        tracing::info!("DidChangeTextDocumentHandler");

        // let url = params.text_document.uri;
        // let document_key = DocumentKey::from_url(&PathBuf::from(session.default_project_db().unwrap().root_path()), &url).internal_error()?;

        // session
        //     .set_unsaved_file(&document_key, params.content_changes)
        //     .internal_error()?;

        // session
        //     .ensure_project_db_for_baml_file(&url)
        //     .internal_error()?;
        // let project = session
        //     .default_project_db_mut()
        //     .expect("Already ensured this project exists");
        // project
        //     .update_runtime(Some(notifier.clone()))
        //     .internal_error()?;

        // let diagnostics = session_lsp_diagnostics(session, &url);

        // // TODO: Only send this when clients do not support pull diagnostics?
        // notifier
        //     .notify::<lsp_types::notification::PublishDiagnostics>(PublishDiagnosticsParams {
        //         uri: url,
        //         version: Some(params.text_document.version),
        //         diagnostics,
        //     })
        //     .map_err(|e| anyhow::anyhow!("did_change err: {}", e))
        //     .internal_error()?;

        // let Ok(path) = url_to_any_system_path(&params.text_document.uri) else {
        //     return Ok(());
        // };
        let url = params.text_document.uri;
        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;
        session
            .ensure_project_db_for_baml_file(&url)
            .internal_error()?;
        let project = session
            .project_db_for_path_mut(path)
            .expect("We ensured above that the project exists");
        let document_key = DocumentKey::from_url(project.root_path(), &url).internal_error()?;
        session
            .update_text_document(
                &document_key,
                params.content_changes,
                params.text_document.version,
                Some(notifier),
            )
            .internal_error()?;

        // let key = session.key_from_url(params.text_document.uri);

        // session
        //     .update_text_document(&key, params.content_changes, params.text_document.version)
        //     .with_failure_code(ErrorCode::InternalError)?;

        // match path {
        //     AnySystemPath::System(path) => {
        //         let db = match session.project_db_for_path_mut(path.as_std_path()) {
        //             Some(db) => db,
        //             None => session.default_project_db_mut(),
        //         };
        //         db.apply_changes(vec![ChangeEvent::file_content_changed(path)], None);
        //     }
        //     AnySystemPath::SystemVirtual(virtual_path) => {
        //         let db = session.default_project_db_mut();
        //         db.apply_changes(vec![ChangeEvent::ChangedVirtual(virtual_path)], None);
        //     }
        // }

        // TODO(dhruvmanila): Publish diagnostics if the client doesn't support pull diagnostics

        Ok(())
    }
}
