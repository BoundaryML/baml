use crate::server::api::traits::{RequestHandler, SyncRequestHandler};
use crate::server::api::ResultExt;
use crate::server::client::Requester;
use crate::server::{client::Notifier, Result};
use crate::{DocumentKey, Session};
use lsp_types::{
    request, CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, Command,
};
use serde_json::Value;
use std::path::PathBuf;

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
            DocumentKey::from_url(&project.lock().unwrap().root_path(), &uri).internal_error()?;

        // Get the first function from the current file if available
        let function_name = project
            .lock()
            .unwrap()
            .list_functions()
            .unwrap_or(vec![])
            .into_iter()
            .filter(|f| f.span.file_path == document_key.path().to_string_lossy())
            .next()
            .map(|f| f.name);

        // Get the playground port from session settings
        let port = session.baml_settings.playground_port.unwrap_or(3030);

        let action = CodeActionOrCommand::CodeAction(CodeAction {
            title: format!("Open Playground"),
            kind: Some(CodeActionKind::EMPTY),
            command: Some(Command {
                title: format!("Open Playground"),
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
