use std::path::PathBuf;

use lsp_types::{
    request, CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, Command,
};
use serde_json::Value;

use crate::{
    server::{
        api::{
            traits::{RequestHandler, SyncRequestHandler},
            ResultExt,
        },
        client::{Notifier, Requester},
        Result,
    },
    DocumentKey, Session,
};

pub struct CodeActionHandler;

impl RequestHandler for CodeActionHandler {
    type RequestType = request::CodeActionRequest;
}

impl SyncRequestHandler for CodeActionHandler {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: CodeActionParams,
    ) -> Result<Option<Vec<CodeActionOrCommand>>> {
        // TODO: this is the only way to do this in zed i think
        // tracing::info!("CodeActionHandler running");
        // let _ = session
        //     .webview_router_to_websocket_tx
        //     .send(
        //         playground_server::LangServerToWasmMessage::PlaygroundMessage(
        //             playground_server::FrontendMessage::lsp_message {
        //                 method: "textDocument/codeAction".to_string(),
        //                 params: serde_json::to_value(&params).unwrap(),
        //             },
        //         ),
        //     )
        //     .inspect_err(|e| {
        //         tracing::error!("Failed to send codeAction notification to playground: {e}");
        //     });

        let mut actions = vec![];

        let uri = params.text_document.uri.clone();
        if !uri.to_string().contains("baml_src") {
            return Ok(None);
        }

        let path = uri
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;
        let project = session
            .get_or_create_project(&path)
            .expect("Ensured that a project db exists");
        let document_key =
            DocumentKey::from_url(project.lock().root_path(), &uri).internal_error()?;

        // Get the first function from the current file if available
        let function_name = project
            .lock()
            .list_functions()
            .unwrap_or_default()
            .into_iter()
            .find(|f| f.span.file_path == document_key.path().to_string_lossy())
            .map(|f| f.name);

        let action = CodeActionOrCommand::CodeAction(CodeAction {
            title: "Open Playground".to_string(),
            kind: Some(CodeActionKind::EMPTY),
            command: Some(Command {
                title: "Open Playground".to_string(),
                command: "openPlayground".to_string(),
                arguments: function_name.map(|name| vec![Value::String(name)]),
            }),
            edit: None,
            diagnostics: None,
            is_preferred: Some(false),
            disabled: None,
            data: None,
        });
        actions.push(action);

        Ok(Some(actions))
    }
}
