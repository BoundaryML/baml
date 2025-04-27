use crate::server::api::notifications::baml_src_version::BamlSrcVersionPayload;
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
        tracing::info!("Did save text document---------");
        let url = params.text_document.uri;
        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;
        session.clear_unsaved_files();
        session.reload(Some(notifier.clone())).internal_error()?;
        tracing::info!("About to run generator. URL path: {:?}", path);
        let project = session
            .get_or_create_project(&path)
            .expect("Ensured that a project db exists");

        let version = project.lock().unwrap().get_common_generator_version();
        if let Ok(version) = version {
            let _ = notifier.0.send(lsp_server::Message::Notification(
                lsp_server::Notification::new(
                    "baml_src_generator_version".to_string(),
                    BamlSrcVersionPayload {
                        version,
                        root_path: project
                            .lock()
                            .unwrap()
                            .root_path()
                            .to_string_lossy()
                            .to_string(),
                    },
                ),
            ));
        }

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

        Ok(())
    }
}

// Do not use this yet, it seems it has an outdated view of the project files and it generates
// stale baml clients
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
