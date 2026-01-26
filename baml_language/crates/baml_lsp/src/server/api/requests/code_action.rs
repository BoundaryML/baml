use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, Command, request,
};
use serde_json::Value;

use crate::{
    Session,
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
        let Ok(project_handle) = session.get_or_create_project(&path) else {
            return Ok(None);
        };

        let guard = project_handle.lock();
        let lsp_db = guard.lsp_db();

        // Get the project for symbol lookup
        let Some(project) = lsp_db.project() else {
            return Ok(None);
        };

        // Get the first function from the current file if available
        let function_name = baml_project::list_functions(lsp_db.db(), project)
            .into_iter()
            .find(|f| f.file_path == path)
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
