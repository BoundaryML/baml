use crate::baml_project::Project;
use crate::server::api::traits::{RequestHandler, SyncRequestHandler};
use crate::server::api::ResultExt;
use crate::server::client::Requester;
use crate::server::{client::Notifier, Result};
use crate::DocumentKey;
use crate::Session;
use baml_lsp_types::BamlSpan;
use baml_runtime::InternalRuntimeInterface;
use lsp_types::{request, CodeLensParams, Command, Position, Range};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct CodeLens;

impl RequestHandler for CodeLens {
    type RequestType = request::CodeLensRequest;
}

impl SyncRequestHandler for CodeLens {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        params: CodeLensParams,
    ) -> Result<Option<Vec<lsp_types::CodeLens>>> {
        tracing::info!("CodeLens request");
        let url = params.text_document.uri.clone();
        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;

        // session.reload(Some(notifier)).internal_error()?;
        let project = session
            .get_or_create_project(&path)
            .expect("Ensured that a project db exists");
        let fake_env = HashMap::new();
        let baml_diagnostics = match project.lock().unwrap().baml_project.runtime(fake_env) {
            Ok(runtime) => runtime.internal().diagnostics().clone(),
            Err(err) => err,
        };

        if baml_diagnostics.has_errors() {
            return Ok(None);
        }

        let mk_range = |span: &BamlSpan| {
            Range::new(
                Position::new(span.start_line as u32, span.start as u32),
                Position::new(span.end_line as u32, span.end as u32),
            )
        };

        let project_lock = project.lock().unwrap();

        let doc_matches = |span: &BamlSpan, project_lock: &Project| {
            let absolute_file = DocumentKey::from_url(project_lock.root_path(), &url);
            let absolute_target = DocumentKey::from_path(
                project_lock.root_path(),
                &PathBuf::from(span.file_path.clone()),
            );
            match (&absolute_file, &absolute_target) {
                (Ok(file), Ok(target)) => file.path() == target.path(),
                _ => {
                    tracing::error!(
                        "Could not construct either file path: {:?}, or target path: {:?}",
                        absolute_file,
                        absolute_target
                    );
                    false
                }
            }
        };

        let mut function_lenses: Vec<lsp_types::CodeLens> = project_lock
            .list_functions()
            .unwrap_or(vec![])
            .iter()
            .filter(|func| doc_matches(&func.span, &project_lock))
            .map(|func| {
                let range = mk_range(&func.span);
                let command = Command::new(
                    "▶ Open Playground ✨".to_string(),
                    "baml.openBamlPanel".to_string(),
                    Some(vec![serde_json::json!({
                        "projectId": project_lock.root_path(),
                        "functionName": func.name.clone(),
                        "showTests": true,
                    })]),
                );
                lsp_types::CodeLens {
                    range,
                    command: Some(command),
                    data: None,
                }
            })
            .collect();

        tracing::info!("Function lenses calculated");

        let test_case_lenses: Vec<lsp_types::CodeLens> = project_lock
            .list_testcases()
            .unwrap_or(vec![])
            .iter()
            .filter(|testcase| doc_matches(&testcase.span, &project_lock))
            .map(|testcase| {
                let range = mk_range(&testcase.span);
                let command_name = if testcase.parent_functions.len() > 1 {
                    format!("▶ Run for {} 💥 ", testcase.parent_functions[0].name)
                } else {
                    "▶ Run Test 💥".to_string()
                };
                let command = Command::new(
                    command_name,
                    "baml.runBamlTest".to_string(),
                    Some(vec![serde_json::json!({
                        "projectId": project_lock.root_path(),
                        "testCaseName": testcase.name.clone(),
                        "functionName": testcase.parent_functions[0].name.clone(),
                        "showTests": true,
                    })]),
                );
                lsp_types::CodeLens {
                    range,
                    command: Some(command),
                    data: None,
                }
            })
            .collect();

        function_lenses.extend(test_case_lenses);
        Ok(Some(function_lenses))
    }
}
