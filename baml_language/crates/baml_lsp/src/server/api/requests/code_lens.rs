use baml_project::position::span_to_lsp_range;
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
        let Ok(project_handle) = session.get_or_create_project(&path) else {
            return Ok(None);
        };

        let guard = project_handle.lock();
        let lsp_db = guard.lsp_db();
        let project_id = guard.root_path().to_string_lossy().to_string();

        // Get the project for symbol lookup
        let Some(project) = lsp_db.project() else {
            return Ok(None);
        };

        // Get all functions using baml_project
        let functions = baml_project::list_functions(lsp_db.db(), project);

        let lenses: Vec<lsp_types::CodeLens> = functions
            .into_iter()
            .filter(|func| func.file_path == path)
            .filter_map(|func| {
                // Get the source file to convert span to range
                let source_file = lsp_db.get_file(&func.file_path)?;
                let text = source_file.text(lsp_db.db());
                let range = span_to_lsp_range(text, &func.span);

                let command = OpenBamlPanel {
                    project_id: project_id.clone(),
                    function_name: func.name,
                    show_tests: true,
                };

                Some(lsp_types::CodeLens {
                    range,
                    command: command.to_lsp_command(),
                    data: None,
                })
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
