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
            DocumentKey::from_url(project.lock().unwrap().root_path(), &uri).internal_error()?;

        // Get the first function from the current file if available
        let function_name = project
            .lock()
            .unwrap()
            .list_functions()
            .unwrap_or_default()
            .into_iter()
            .find(|f| f.span.file_path == document_key.path().to_string_lossy())
            .map(|f| f.name);

        // Get the actual playground port from session (determined by server after availability check)
        // Fall back to configured port if actual port not set yet
        #[cfg(feature = "playground-server")]
        let port = session
            .get_session_playground_port()
            .unwrap_or_else(|| session.baml_settings.playground_port.unwrap_or(3030));

        let action = CodeActionOrCommand::CodeAction(CodeAction {
            title: format!("Open Playground Localhost:{port}"),
            kind: Some(CodeActionKind::EMPTY),
            command: Some(Command {
                title: format!("Open Playground Localhost:{port}"),
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
