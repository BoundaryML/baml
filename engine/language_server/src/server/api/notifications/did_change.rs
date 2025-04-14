use std::time::Instant;

use lsp_types::notification::DidChangeTextDocument;
use lsp_types::{DidChangeTextDocumentParams, PublishDiagnosticsParams};

use crate::server::api::diagnostics::publish_diagnostics;
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
        tracing::info!("------- DidChangeTextDocumentHandler");
        let start_time_total = Instant::now();

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
        let document_key =
            DocumentKey::from_url(project.lock().unwrap().root_path(), &url).internal_error()?;

        session
            .update_text_document(
                &document_key,
                params.content_changes,
                params.text_document.version,
                Some(notifier.clone()),
            )
            .internal_error()?;

        tracing::info!("publishing diagnostics");

        publish_diagnostics(
            &notifier,
            project.clone(),
            Some(params.text_document.version),
        )?;

        let elapsed = start_time_total.elapsed();
        tracing::info!("didchange total took {:?}ms", elapsed.as_millis());
        Ok(())
    }
}
