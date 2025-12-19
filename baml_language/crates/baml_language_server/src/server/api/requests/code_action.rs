use std::path::PathBuf;

use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, Command, request,
};
use serde_json::Value;

use crate::{
    DocumentKey, Session,
    server::{
        Result,
        api::{
            ResultExt,
            traits::{RequestHandler, SyncRequestHandler},
        },
        client::{Notifier, Requester},
    },
};

pub struct CodeActionHandler;

impl RequestHandler for CodeActionHandler {
    type RequestType = request::CodeActionRequest;
}

pub(crate) const OPEN_IN_BROWSER_COMMAND: &str = "baml.openBamlPanelInBrowser";

impl SyncRequestHandler for CodeActionHandler {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: CodeActionParams,
    ) -> Result<Option<Vec<CodeActionOrCommand>>> {
        let uri = params.text_document.uri.clone();
        let path = uri
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;
        let Ok(project) = session.get_or_create_project(&path) else {
            return Ok(None);
        };

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
                command: OPEN_IN_BROWSER_COMMAND.to_string(),
                arguments: function_name.map(|name| vec![Value::String(name)]),
            }),
            edit: None,
            diagnostics: None,
            is_preferred: Some(false),
            disabled: None,
            data: None,
        });

        Ok(Some(vec![action]))
    }
}
