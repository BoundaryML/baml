use crate::baml_project::ProjectType;
use crate::server::api::diagnostics::publish_session_lsp_diagnostics;
use crate::server::api::notifications::baml_src_version::BamlSrcVersionPayload;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::api::ResultExt;
use crate::server::client::{Notifier, Requester};
use crate::server::{Result, Task};
use crate::session::Session;
use crate::{DocumentKey, TextDocument};
use lsp_types::notification::DidOpenTextDocument;
use lsp_types::{
    self as types, ConfigurationItem, ConfigurationParams, DidOpenTextDocumentParams,
    PublishDiagnosticsParams, TextDocumentItem,
};
pub(crate) struct DidOpenTextDocumentHandler;

impl NotificationHandler for DidOpenTextDocumentHandler {
    type NotificationType = DidOpenTextDocument;
}

impl SyncNotificationHandler for DidOpenTextDocumentHandler {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        requester: &mut Requester,
        params: DidOpenTextDocumentParams,
    ) -> Result<()> {
        tracing::info!("DidOpenTextDocumentHandler");

        let url = params.text_document.uri;

        // TODO: do this when server initializes instead of every time a file is opened
        // note this just schedules the task. It will run after the current task is done.
        requester.request::<types::request::WorkspaceConfiguration>(
            ConfigurationParams {
                items: vec![types::ConfigurationItem {
                    scope_uri: None,
                    section: Some("baml".to_string()),
                }],
            },
            |response| {
                Task::local(move |session, _, _, _| {
                    tracing::info!("Workspace configuration request received: {:?}", response);
                    if let Some(first_response) = response.first() {
                        session.update_baml_settings(first_response.clone());
                    }
                })
            },
        );

        let file_path = url
            .to_file_path()
            .internal_error_msg(&format!("Could not convert URL '{}' to file path", url))?;

        let project = session.get_or_create_project(&file_path);
        match project {
            Ok(ProjectType::Valid(Some(project))) => {
                let project = project;
                let version = project.lock().unwrap().get_common_generator_version();
                if let Ok(version) = version {
                    notifier
                        .0
                        .send(lsp_server::Message::Notification(
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
                        ))
                        .internal_error()?;
                }
                publish_session_lsp_diagnostics(&notifier, session, &url)?;
            }
            Ok(ProjectType::MissingBamlSrc) => {
                tracing::error!("Failed to get or create project for path: {:?}", file_path);
                show_err_msg!("File must be inside a baml_src directory: {:?}", file_path);
            }
            _ => {
                tracing::error!("Failed to get or create project for path: {:?}", file_path);
                show_err_msg!("Failed to get or create project for path: {:?}", file_path);
            }
        }

        session.reload(Some(notifier.clone())).internal_error()?;

        Ok(())
    }
}
