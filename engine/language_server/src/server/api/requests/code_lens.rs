use std::{collections::HashMap, path::PathBuf};

use baml_lsp_types::BamlSpan;
use baml_runtime::InternalRuntimeInterface;
use itertools::Itertools;
use lsp_types::{request, CodeLensParams, Command, Position, Range};

use crate::{
    baml_project::Project,
    server::{
        api::{
            traits::{RequestHandler, SyncRequestHandler},
            ResultExt,
        },
        client::{Notifier, Requester},
        commands::{CodeLensCommand, OpenBamlPanel, RunBamlTest},
        Result,
    },
    DocumentKey, Session,
};

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
        if !url.to_string().contains("baml_src") {
            return Ok(None);
        }

        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;

        // session.reload(Some(notifier)).internal_error()?;
        let project = session
            .get_or_create_project(&path)
            .expect("Ensured that a project db exists");
        let fake_env = HashMap::new();
        let default_flags = vec!["beta".to_string()];
        let baml_diagnostics = match project.lock().baml_project.runtime(
            fake_env,
            session
                .baml_settings
                .feature_flags
                .as_ref()
                .unwrap_or(&default_flags),
        ) {
            Ok(runtime) => runtime.internal().diagnostics().clone(),
            Err(err) => err,
        };

        if baml_diagnostics.has_errors() {
            return Ok(None);
        }

        let mk_range = |span: &BamlSpan| {
            Range::new(
                // TODO(sam): I'm pretty sure there's a bug here - Position I believe is line number and
                // character index _within_ the line, not the byte index corresponding to the start.
                // But it doesn't make a difference for vscode, so not going to fix it right now.
                Position::new(span.start_line as u32, span.start as u32),
                Position::new(span.end_line as u32, span.end as u32),
            )
        };

        let project_lock = project.lock();

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
            .unwrap_or_default()
            .iter()
            .filter(|func| doc_matches(&func.span, &project_lock))
            .map(|func| {
                let range = mk_range(&func.span);
                let command = OpenBamlPanel {
                    project_id: project_lock.root_path().to_string_lossy().to_string(),
                    function_name: func.name.clone(),
                    show_tests: true,
                };
                lsp_types::CodeLens {
                    range,
                    command: command.to_lsp_command(),
                    data: None,
                }
            })
            .collect();

        tracing::info!("Function lenses calculated");

        // TODO(sam): there is a bug in here, where for a `test` block which test N functions,
        // we generate N^2 "Test {function}" code lenses, even though we should only generate N
        // such lenses. The reason is that we probably do some preemptive denormalization in
        // `list_testcases`, but I'm not sure how this behavior interacts with VSCode so for
        // now I'm leaving this as-is.
        let test_case_lenses: Vec<lsp_types::CodeLens> = project_lock
            .list_function_test_pairs()
            .unwrap_or_default()
            .iter()
            .filter(|testcase| doc_matches(&testcase.span, &project_lock))
            .map(|testcase| {
                let project_id = project_lock.root_path().to_string_lossy().to_string();
                (
                    testcase.function_name_span.as_ref(),
                    lsp_types::CodeLens {
                        range: mk_range(&testcase.span),
                        command: RunBamlTest {
                            project_id: project_id.clone(),
                            test_case_name: testcase.name.clone(),
                            function_name: testcase.function.name.clone(),
                            show_tests: true,
                        }
                        .to_lsp_command(),
                        data: None,
                    },
                )
            })
            .sorted_by_key(|(span, _)| span.map_or(None, |span| Some(span.start)))
            .map(|(_, codelens)| codelens)
            .collect();

        function_lenses.extend(test_case_lenses);
        function_lenses.sort_by_key(|lens| lens.range.start);
        tracing::debug!("Function lenses: {:#?}", function_lenses);
        Ok(Some(function_lenses))
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
