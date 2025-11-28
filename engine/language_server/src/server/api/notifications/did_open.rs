use lsp_types::{
    self as types, notification::DidOpenTextDocument, ConfigurationItem, ConfigurationParams,
    DidOpenTextDocumentParams, PublishDiagnosticsParams, TextDocumentItem,
};

use crate::{
    server::{
        api::{
            diagnostics::{not_in_baml_src_diagnostic, publish_session_lsp_diagnostics},
            notifications::{
                baml_src_version::BamlSrcVersionPayload,
                did_save_text_document::send_generator_version,
            },
            traits::{NotificationHandler, SyncNotificationHandler},
            ResultExt,
        },
        client::{Notifier, Requester},
        Result, Task,
    },
    session::Session,
    DocumentKey, TextDocument,
};
pub(crate) struct DidOpenTextDocumentHandler;

impl NotificationHandler for DidOpenTextDocumentHandler {
    type NotificationType = DidOpenTextDocument;
}

impl SyncNotificationHandler for DidOpenTextDocumentHandler {
    // #[tracing::instrument(
    //     name = "DidOpenTextDocumentHandler",
    //     skip(session, notifier, requester),
    //     ret
    // )]
    fn run(
        session: &mut Session,
        notifier: Notifier,
        requester: &mut Requester,
        params: DidOpenTextDocumentParams,
    ) -> Result<()> {
        tracing::info!("DidOpenTextDocumentHandler");

        let url = params.text_document.uri;
        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;
        let Ok(project) = session.get_or_create_project(&path) else {
            tracing::info!("BAML file not in baml_src directory: {}", url);
            notifier
                .notify::<lsp_types::notification::PublishDiagnostics>(not_in_baml_src_diagnostic(
                    &url,
                ))
                .internal_error()?;
            return Ok(());
        };

        // TODO: do this when server initializes instead of every time a file is opened
        // note this just schedules the task. It will run after the current task is done.
        tracing::info!("before workspace configuration request");
        requester
            .request::<types::request::WorkspaceConfiguration>(
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
            )
            .internal_error()?;

        // session.open_text_document(
        //    q DocumentKey::from_path(&file_path, &file_path).internal_error()?,
        //     TextDocument::new(params.text_document.text, params.text_document.version),
        // );

        session.reload(Some(notifier.clone())).internal_error()?;

        // We do this after the project reload since we may be loading this baml project with files (and creating a new runtime) in the .reload(), and we only want to send the generator version if we've had a runtime created. Ideally we don't depend on the runtime being created (since our version of the LSP may not be able to read all baml files), and it only reads the generator config blocks.
        tracing::info!("before send_generator_version");
        {
            let locked = project.lock();
            let default_flags = vec!["beta".to_string()];
            let effective_flags = session
                .baml_settings
                .feature_flags
                .as_ref()
                .unwrap_or(&default_flags);
            let client_version = session.baml_settings.get_client_version();

            let generator_version = locked.get_common_generator_version();
            tracing::info!("common generator version {:?}", generator_version);
            send_generator_version(&notifier, &locked, generator_version.as_ref().ok());
        }

        publish_session_lsp_diagnostics(&notifier, session, &url)?;

        Ok(())
    }
}
