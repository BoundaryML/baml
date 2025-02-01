use crate::server::api::ResultExt;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use lsp_types as types;
use lsp_types::notification as notif;

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
        tracing::info!("About to run generator");
        session
            .ensure_project_db_for_baml_file(&url)
            .internal_error()?;
        session
            .project_db_for_path_mut(path)
            .expect("Ensured that a project db exists")
            .run_generators_without_debounce(
                |_| {
                    notifier
                        .notify_baml_info(&format!(
                            "BAML: Client generated! (Using installed baml-cli {})",
                            env!("CARGO_PKG_VERSION") // TODO: Use baml-cli version.
                        ))
                        .unwrap_or(())
                },
                |e| {
                    notifier
                        .notify_baml_error(&format!("Error generating: {e}"))
                        .unwrap_or(())
                },
            );
        Ok(())
    }
}
