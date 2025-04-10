use crate::server::api::ResultExt;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::{DocumentSnapshot, Session};
use lsp_types as types;
use lsp_types::notification as notif;
use std::borrow::Cow;

pub struct DidSaveTextDocument;

impl super::NotificationHandler for DidSaveTextDocument {
    type NotificationType = notif::DidSaveTextDocument;
}

impl super::SyncNotificationHandler for DidSaveTextDocument {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        params: types::DidSaveTextDocumentParams,
    ) -> Result<()> {
        let url = params.text_document.uri;
        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;
        session.reload(Some(notifier.clone())).internal_error()?;
        tracing::info!("About to run generator. URL path: {:?}", path);
        session
            .ensure_project_db_for_baml_file(&url)
            .internal_error()?;
        session
            .project_db_for_path_mut(path)
            .expect("Ensured that a project db exists")
            .lock()
            .unwrap()
            .run_generators_without_debounce(
                |message| {
                    tracing::info!("About to notify client that generator has run.");
                    notifier
                        .notify_baml_info(&format!("{}", message))
                        .unwrap_or(())
                },
                |e| {
                    tracing::error!("Error generating: {e}");
                    notifier
                        .notify_baml_error(&format!("Error generating: {e}"))
                        .unwrap_or(())
                },
            );
        Ok(())
    }
}

impl super::BackgroundDocumentNotificationHandler for DidSaveTextDocument {
    fn document_url(params: &types::DidSaveTextDocumentParams) -> Cow<types::Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        notifier: Notifier,
        params: types::DidSaveTextDocumentParams,
    ) -> Result<()> {
        let url = params.text_document.uri;
        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;

        tracing::info!("About to run generator in background. URL path: {:?}", path);

        // We already have the snapshot, so we don't need to reload

        // Note: In the background version, we need to get the project from the snapshot
        // instead of modifying the session directly
        if let Some(project) = snapshot.project() {
            project.lock().unwrap().run_generators_without_debounce(
                |message| {
                    tracing::info!("About to notify client that generator has run.");
                    notifier
                        .notify_baml_info(&format!("{}", message))
                        .unwrap_or(())
                },
                |e| {
                    tracing::error!("Error generating: {e}");
                    notifier
                        .notify_baml_error(&format!("Error generating: {e}"))
                        .unwrap_or(())
                },
            );
        } else {
            tracing::error!("No project found in snapshot for file {:?}", path);
            notifier
                .notify_baml_error(&format!("No project found for file {:?}", path))
                .unwrap_or(());
        }

        Ok(())
    }
}
