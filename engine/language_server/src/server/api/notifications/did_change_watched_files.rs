use lsp_types as types;
use lsp_types::{notification as notif, PublishDiagnosticsParams, Url};

use crate::{
    server::{
        api::{diagnostics::publish_session_lsp_diagnostics, ResultExt},
        client::{Notifier, Requester},
        Result,
    },
    session::Session,
};

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
        tracing::info!("#### DidChangeWatchedFiles {:?}", params.changes);
        if !params
            .changes
            .iter()
            .any(|change| change.uri.to_string().contains("baml_src"))
        {
            return Ok(());
        }

        // Filter out CHANGED events - only process CREATED and DELETED
        let filtered_changes: Vec<_> = params
            .changes
            .into_iter()
            .filter(|file_event| file_event.typ != types::FileChangeType::CHANGED)
            .collect();

        // If there are no events to process after filtering, return early
        if filtered_changes.is_empty() {
            tracing::debug!("No CREATED or DELETED file events to process");
            return Ok(());
        }

        // Replace the original changes with the filtered ones
        let params = types::DidChangeWatchedFilesParams {
            changes: filtered_changes,
        };

        session.reload(Some(notifier.clone())).internal_error()?;

        let change_file_paths: Vec<Url> = params
            .changes
            .into_iter()
            .map(|file_event| file_event.uri)
            .collect();
        tracing::info!("change_file_paths urls: {:?}", change_file_paths);

        for url in change_file_paths {
            publish_session_lsp_diagnostics(&notifier, session, &url)?;
        }

        Ok(())
    }
}
