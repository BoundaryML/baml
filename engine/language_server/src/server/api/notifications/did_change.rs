use std::{collections::HashMap, time::Instant};

use lsp_types::{
    notification::DidChangeTextDocument, DidChangeTextDocumentParams, PublishDiagnosticsParams,
};
use playground_server::{FrontendMessage, WebviewRouterMessage};

use crate::{
    server::{
        api::{
            diagnostics::publish_diagnostics,
            traits::{NotificationHandler, SyncNotificationHandler},
            ResultExt,
        },
        client::{Notifier, Requester},
        Result,
    },
    session::Session,
    DocumentKey,
};

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
        let start_time_total = Instant::now();

        let url = params.text_document.uri;
        if !url.to_string().contains("baml_src") {
            return Ok(());
        }

        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;

        // Get or create the project using the unified method
        let project = session.get_or_create_project(&path);
        if project.is_none() {
            tracing::error!("Failed to get or create project for path: {:?}", path);
            show_err_msg!("Failed to get or create project for path: {:?}", path);
        }

        let project = project.unwrap();
        let document_key =
            DocumentKey::from_url(project.lock().root_path(), &url).internal_error()?;

        session
            .update_text_document(
                &document_key,
                params.content_changes,
                params.text_document.version,
                Some(notifier.clone()),
            )
            .internal_error()?;

        // Broadcast update to playground clients
        {
            let project = project.lock();
            let files_map: std::collections::HashMap<String, String> = project
                .baml_project
                .files
                .iter()
                .map(|(path, doc)| {
                    let key = path.path().to_string_lossy().to_string();
                    // If there's an unsaved version, use it
                    let contents = project
                        .baml_project
                        .unsaved_files
                        .get(path)
                        .map(|unsaved| unsaved.contents.clone())
                        .unwrap_or_else(|| doc.contents.clone());
                    (key, contents)
                })
                .collect();
            session
                .to_webview_router_tx
                .send(WebviewRouterMessage::CustomNotificationToWebview(
                    FrontendMessage::add_project {
                        root_path: project.root_path().to_string_lossy().to_string(),
                        files: files_map,
                    },
                ))
                .unwrap();
        }

        tracing::info!("publishing diagnostics");

        let default_flags = vec!["beta".to_string()];
        let effective_flags = session
            .baml_settings
            .feature_flags
            .as_ref()
            .unwrap_or(&default_flags);
        tracing::info!(
            "did_change: session feature_flags: {:?}, effective_flags: {:?}",
            session
                .baml_settings
                .feature_flags
                .as_ref()
                .unwrap_or(&default_flags),
            &effective_flags
        );
        publish_diagnostics(
            &notifier,
            project,
            Some(params.text_document.version),
            effective_flags,
            session,
        )?;

        let elapsed = start_time_total.elapsed();
        tracing::info!("didchange total took {:?}ms", elapsed.as_millis());
        Ok(())
    }
}
