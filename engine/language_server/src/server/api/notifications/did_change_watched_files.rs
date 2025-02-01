use crate::server::api::diagnostics::session_lsp_diagnostics;
use crate::server::api::ResultExt;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use lsp_types as types;
use lsp_types::notification as notif;
use lsp_types::{PublishDiagnosticsParams, Url};

pub(crate) struct DidChangeWatchedFiles;

impl super::NotificationHandler for DidChangeWatchedFiles {
    type NotificationType = notif::DidChangeWatchedFiles;
}

impl super::SyncNotificationHandler for DidChangeWatchedFiles {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        params: types::DidChangeWatchedFilesParams,
    ) -> Result<()> {
        tracing::info!("DidChangeWatchedFiles");
        // session.reload_settings(&params.changes);

        session.reload(Some(notifier.clone())).internal_error()?;

        let change_file_paths: Vec<Url> = params
            .changes
            .into_iter()
            .map(|file_event| file_event.uri)
            .collect();
        tracing::info!("change_file_paths urls: {:?}", change_file_paths);

        if let Some(url) = change_file_paths.into_iter().next() {
            let diagnostics = session_lsp_diagnostics(session, &url);
            tracing::info!("DID_CHANGE_WATCHED_FILES DIAGNOSTICS: {:?}", diagnostics);

            // TODO: Only send this when clients do not support pull diagnostics?
            notifier
                .notify::<lsp_types::notification::PublishDiagnostics>(PublishDiagnosticsParams {
                    uri: url,
                    version: None,
                    diagnostics,
                })
                .map_err(|e| anyhow::anyhow!("did_change err: {}", e))
                .internal_error()?;
        }

        // if !params.changes.is_empty() {
        //     if session.resolved_client_capabilities().workspace_refresh {
        //         tracing::info!("Watchedfiles: workspace refresh");
        //         requester
        //             .request::<types::request::WorkspaceDiagnosticRefresh>((), |()| Task::nothing())
        //             .with_failure_code(lsp_server::ErrorCode::InternalError)?;
        //     } else {
        //         tracing::info!("Watchedfiles: Refreshing diagnostics for text documents");
        //         // publish diagnostics for text documents
        //         for url in session.text_document_urls() {
        //             let snapshot = session
        //                 .take_snapshot(url.clone())
        //                 .expect("snapshot should be available");
        //             // publish_diagnostics_for_document(&snapshot, &notifier)?;
        //         }
        //     }

        //     // always publish diagnostics for notebook files (since they don't use pull diagnostics)
        //     // for url in session.notebook_document_urls() {
        //     //     let snapshot = session
        //     //         .take_snapshot(url.clone())
        //     //         .expect("snapshot should be available");
        //     //     publish_diagnostics_for_document(&snapshot, &notifier)?;
        //     // }
        // }

        Ok(())
    }
}
