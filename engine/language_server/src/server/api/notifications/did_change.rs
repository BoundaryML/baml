use std::time::Instant;

use lsp_types::notification::DidChangeTextDocument;
use lsp_types::{DidChangeTextDocumentParams, PublishDiagnosticsParams};

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
        tracing::info!("------- DidChangeTextDocumentHandlerrrr");
        let start_time = Instant::now();

        let url = params.text_document.uri;
        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;
        session
            .ensure_project_db_for_baml_file(&url)
            .internal_error()?;
        let elapsed = start_time.elapsed();
        tracing::info!(
            "ensure_project_db_for_baml_file took {:?}ms",
            elapsed.as_millis()
        );

        let start_time = Instant::now();
        let project = session
            .project_db_for_path_mut(path)
            .expect("We ensured above that the project exists");
        let document_key =
            DocumentKey::from_url(project.lock().unwrap().root_path(), &url).internal_error()?;
        let elapsed = start_time.elapsed();
        tracing::info!("project_db_for_path_mut took {:?}ms", elapsed.as_millis());

        let start_time = Instant::now();
        session
            .update_text_document(
                &document_key,
                params.content_changes,
                params.text_document.version,
                Some(notifier.clone()),
            )
            .internal_error()?;
        let elapsed = start_time.elapsed();
        tracing::info!("update_text_document took {:?}ms", elapsed.as_millis());

        let diagnostics = session_lsp_diagnostics(session, &url);
        notifier
            .notify::<lsp_types::notification::PublishDiagnostics>(PublishDiagnosticsParams {
                uri: url,
                version: Some(params.text_document.version),
                diagnostics,
            })
            .map_err(|e| anyhow::anyhow!("did_change err: {}", e))
            .internal_error()?;
        Ok(())
    }
}
