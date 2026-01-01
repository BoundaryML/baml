use lsp_types::{CodeLensParams, request};

use crate::{
    Session,
    server::{
        Result,
        api::{
            ResultExt,
            traits::{RequestHandler, SyncRequestHandler},
        },
        client::{Notifier, Requester},
        commands::{CodeLensCommand, OpenBamlPanel},
    },
};

pub struct CodeLens;

impl RequestHandler for CodeLens {
    type RequestType = request::CodeLensRequest;
}

impl SyncRequestHandler for CodeLens {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: CodeLensParams,
    ) -> Result<Option<Vec<lsp_types::CodeLens>>> {
        let url = &params.text_document.uri;
        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;

        // Get the project to access the LspDatabase
        let Ok(project) = session.get_or_create_project(&path) else {
            return Ok(None);
        };

        let guard = project.lock();
        let lsp_db = guard.lsp_db();
        let project_id = guard.root_path().to_string_lossy().to_string();

        // Get all functions and filter to current file
        let functions = lsp_db.list_functions();

        let lenses: Vec<lsp_types::CodeLens> = functions
            .into_iter()
            .filter(|func| func.file_path == path)
            .map(|func| {
                let command = OpenBamlPanel {
                    project_id: project_id.clone(),
                    function_name: func.name,
                    show_tests: true,
                };

                lsp_types::CodeLens {
                    range: func.range,
                    command: command.to_lsp_command(),
                    data: None,
                }
            })
            .collect();

        Ok(Some(lenses))
    }
}

/// This is a no-op request that LSP4IJ (the Jetbrains language server client we use)
/// uses to translate `CodeLens` requests into `ExecuteCommand` requests. This doesn't
/// add any value for us, so we just implement this RPC as a reflector/proxy.
pub struct CodeLensResolve;

impl RequestHandler for CodeLensResolve {
    type RequestType = request::CodeLensResolve;
}

impl SyncRequestHandler for CodeLensResolve {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        params: lsp_types::CodeLens,
    ) -> Result<lsp_types::CodeLens> {
        Ok(params)
    }
}
