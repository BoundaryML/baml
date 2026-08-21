//! The browser playground's engine: build it from the sources the language
//! server already holds, and tell the host what it can run.
//!
//! The desktop host needs a revision fence here — its build runs on a pool
//! and can be superseded mid-flight by an edit. The browser has neither
//! problem: the build is synchronous on the one thread, so by construction it
//! can only ever describe the sources it started from, and "the engine is
//! current" is just "no edit has landed since". What remains is the same
//! shape: check, emit, construct, install, report.

use std::sync::Arc;

use baml_lsp::{
    GlobalState, SourceRevision, diagnostics::collect_root_candidate, executor::spawn_read,
    position_codec::PositionEncoding, snapshot::Snapshot,
};
use bex_project::BexEngine;

use crate::playground_notify::{
    FunctionInfo, FunctionKind, FunctionOrigin, FunctionSourcePosition, LlmCapabilities,
    ProjectDiagnostic, ProjectUpdate, TestInfo,
};

/// The engine currently installed, and what it was built from.
pub(crate) struct InstalledEngine {
    /// The revision its program was emitted at. An edit moves the state's
    /// revision past this, which is exactly what `isBexCurrent` reports.
    pub(crate) source_revision: SourceRevision,
    /// Monotonic identity; runs and test trees key on it.
    pub(crate) generation: u64,
    pub(crate) engine: Arc<BexEngine>,
}

#[derive(Default)]
pub(crate) struct PlaygroundState {
    pub(crate) installed: Option<InstalledEngine>,
    next_generation: u64,
    /// The last build failure that source diagnostics did not already
    /// explain, scoped to the revision it happened at so a later edit stops
    /// surfacing it.
    build_failure: Option<(SourceRevision, String)>,
}

impl PlaygroundState {
    /// Give up the installed engine and drain it in the background.
    ///
    /// Dropping an engine without shutting it down skips its unhandled-spawn
    /// drain — the browser has no process exit to fall back on, so a tab that
    /// swaps runtimes would leak the old one's pending work.
    pub(crate) fn shutdown(&mut self) {
        let Some(installed) = self.installed.take() else {
            return;
        };
        wasm_bindgen_futures::spawn_local(async move {
            bex_project::Bex::shutdown(installed.engine).await;
        });
    }

    /// Install a freshly built engine, retiring whatever it replaces.
    fn install(&mut self, source_revision: SourceRevision, engine: BexEngine) {
        self.shutdown();
        engine.activate_profiling();
        self.next_generation += 1;
        self.installed = Some(InstalledEngine {
            source_revision,
            generation: self.next_generation,
            engine: Arc::new(engine),
        });
        self.build_failure = None;
    }
}

/// What one check of the workspace root produced.
struct RootCheck {
    revision: SourceRevision,
    diagnostics: Vec<ProjectDiagnostic>,
    /// `None` when the check found errors: emit is not attempted and the
    /// diagnostics are the explanation.
    program: Option<Box<bex_vm_types::Program>>,
    emit_error: Option<String>,
}

/// Run `job` against a snapshot. The inline executor finishes before `spawn`
/// returns, so the answer is available synchronously — the one place this
/// host differs from the desktop one, and the reason it needs no channel.
fn read<R: Send + 'static>(
    state: &GlobalState,
    job: impl FnOnce(&Snapshot) -> R + Send + 'static,
) -> Option<R> {
    let slot: Arc<std::sync::Mutex<Option<R>>> = Arc::new(std::sync::Mutex::new(None));
    let sink = Arc::clone(&slot);
    spawn_read(
        state.request_executor(),
        state.snapshot(baml_lsp::RequestCx::default()),
        move |snap| Ok(job(snap)),
        move |outcome| {
            if let Ok(Ok(value)) = outcome {
                *sink
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(value);
            }
        },
    );
    let mut slot = slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    slot.take()
}

fn check_workspace_root(state: &GlobalState) -> Option<RootCheck> {
    read(state, |snap| {
        let entry = snap.roots().workspace_roots().next()?;
        let candidate = collect_root_candidate(snap, entry.root).ok()?;
        let documents = baml_lsp::diagnostics::candidate_to_publishable(
            &candidate,
            PositionEncoding::UTF16,
            snap.roots(),
        );
        let mut check = RootCheck {
            revision: snap.revision(),
            diagnostics: flatten_diagnostics(&documents),
            program: None,
            emit_error: None,
        };
        if candidate.has_errors() {
            return Some(check);
        }
        // The check above is a full sweep of the root, so `get_bytecode`'s own
        // error gate would re-derive what `has_errors` just proved.
        match snap.db().get_bytecode_unchecked() {
            Ok(program) => check.program = Some(Box::new(program)),
            Err(error) => check.emit_error = Some(error.to_string()),
        }
        Some(check)
    })
    .flatten()
}

/// Rebuild the engine for the current sources.
///
/// Called after every applied edit. There is no debounce: the browser has no
/// timer thread to resume on, and a build deferred here would only run on the
/// next keystroke — the same reason [`crate::BamlWasmRuntime::pump`] fires
/// tails at their deadline.
pub(crate) fn rebuild(
    state: &GlobalState,
    playground: &mut PlaygroundState,
    sys_ops: &Arc<sys_ops::SysOps>,
) {
    let Some(check) = check_workspace_root(state) else {
        return;
    };
    let Some(program) = check.program else {
        if let Some(message) = check.emit_error {
            log::warn!(
                "playground build: emit failed at {}: {message}",
                check.revision
            );
            playground.build_failure = Some((check.revision, message));
        } else {
            // Blocked by diagnostics: an older failure no longer explains
            // anything about this revision.
            playground.build_failure = None;
        }
        return;
    };
    match BexEngine::new_with_deferred_profiling_and_runtime_compiler(
        *program,
        Arc::clone(sys_ops),
        Vec::new(),
        Some(bex_project::runtime_compiler()),
    ) {
        Ok(engine) => {
            engine.set_unhandled_spawn_error_handler(Some(Arc::new(|error| {
                let cancelled = error.cancelled;
                let error = error.into_engine_error();
                if cancelled {
                    log::warn!("cancelled spawned task failed: {error}");
                } else {
                    log::error!("unhandled spawned task failed: {error}");
                }
            })));
            playground.install(check.revision, engine);
        }
        Err(error) => {
            let message = error.to_string();
            log::warn!("playground build: engine construction failed: {message}");
            playground.build_failure = Some((check.revision, message));
        }
    }
}

/// The project surface the host renders: what can be run, and what is wrong.
pub(crate) fn project_update(
    state: &GlobalState,
    playground: &PlaygroundState,
) -> Option<(String, ProjectUpdate)> {
    let check = check_workspace_root(state)?;
    let project = read(state, |snap| {
        snap.roots()
            .workspace_roots()
            .next()
            .map(|entry| entry.path.to_string_lossy().into_owned())
    })
    .flatten()?;

    let mut diagnostics = check.diagnostics;
    if let Some((revision, message)) = &playground.build_failure
        && *revision == check.revision
    {
        diagnostics.push(ProjectDiagnostic {
            severity: "error".to_string(),
            message: format!("Current build failed: {message}"),
        });
    }
    let is_bex_current = playground
        .installed
        .as_ref()
        .is_some_and(|installed| installed.source_revision == check.revision);
    let generation = playground
        .installed
        .as_ref()
        .map_or(0, |installed| installed.generation);

    let update = read(state, move |snap| {
        let db = snap.db();
        let listing = baml_ide::list_functions_with_metadata(db);
        let functions = listing
            .functions
            .into_iter()
            .map(|function| FunctionInfo {
                name: function.name,
                kind: if function.is_llm {
                    FunctionKind::Llm
                } else {
                    FunctionKind::Expr
                },
                origin: match function.origin {
                    baml_ide::FunctionOrigin::UserDefined => FunctionOrigin::UserDefined,
                    baml_ide::FunctionOrigin::Companion => FunctionOrigin::Companion,
                    baml_ide::FunctionOrigin::Internal => FunctionOrigin::Internal,
                    baml_ide::FunctionOrigin::AutoDerive => FunctionOrigin::AutoDerive,
                },
                signature: function.signature,
                source_position: FunctionSourcePosition {
                    file: function.source_position.file,
                    line: function.source_position.line,
                    column: function.source_position.column,
                },
                capabilities: function.is_llm.then_some(LlmCapabilities {
                    render_prompt: true,
                    build_request: true,
                    client_name: function.client_name,
                }),
                params: function
                    .params
                    .map(|params| params.into_iter().map(Into::into).collect()),
            })
            .collect();
        let tests = baml_ide::list_tests_with_metadata(db)
            .into_iter()
            .map(|test| TestInfo {
                name: test.name,
                function_name: test.function_name,
                args_json: test.args_json,
            })
            .collect();
        ProjectUpdate {
            is_bex_current,
            generation,
            functions,
            tests,
            types: Some(
                listing
                    .types
                    .into_iter()
                    .map(|(name, schema)| (name, schema.into()))
                    .collect(),
            ),
            diagnostics,
        }
    })?;
    Some((project, update))
}

/// One line per diagnostic, `file:line: message`, sorted so a rebuild that
/// changes nothing renders identically.
fn flatten_diagnostics(
    documents: &[baml_lsp::diagnostics::PublishableDocument],
) -> Vec<ProjectDiagnostic> {
    let mut out: Vec<ProjectDiagnostic> = documents
        .iter()
        .flat_map(|document| {
            let filename = document
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            document.diagnostics.iter().map(move |diagnostic| {
                let severity = match diagnostic.severity {
                    Some(lsp_types::DiagnosticSeverity::ERROR) => "error",
                    Some(lsp_types::DiagnosticSeverity::WARNING) => "warning",
                    _ => "info",
                };
                ProjectDiagnostic {
                    severity: severity.to_string(),
                    message: format!(
                        "{filename}:{}: {}",
                        diagnostic.range.start.line + 1,
                        diagnostic.message
                    ),
                }
            })
        })
        .collect();
    out.sort_by(|a, b| a.message.cmp(&b.message));
    out
}

#[cfg(test)]
mod tests {
    use baml_lsp::{SourceMutation, discovery::workspace_root_spec, executor::Executors};
    use wasm_bindgen::JsCast as _;
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::*;

    /// A source file that leans on the parts of the platform a browser cannot
    /// provide for itself: the clock and the CSPRNG.
    const FIXTURE: &str = "function stamp() -> bool throws never {\n    \
                           let start = baml.time.Instant.now();\n    \
                           start.elapsed().to_milliseconds() >= 0n\n}\n";

    /// JS callbacks that are never called: this test exercises the namespaces
    /// the runtime implements itself, not the ones it delegates to the host.
    fn inert_callbacks() -> crate::WasmCallbacks {
        js_sys::eval(
            "({ fetch: () => {}, env: () => {}, input: () => {}, exec: () => {}, \
              shell: () => {}, host_dispatch: () => {}, lsp_send_notification: () => {}, \
              lsp_send_response: () => {}, playground_send_notification: () => {} })",
        )
        .expect("the callback bundle evaluates")
        .unchecked_into()
    }

    /// The browser's `SysOps` table has to be complete, and compiling proves
    /// nothing about that: a namespace left out of the builder throws
    /// `baml.errors.Unsupported` from the middle of a user's program instead.
    /// `testing.run_test` opens by timing the test, so a missing `time` alone
    /// would cost every test run — which is exactly what the desktop host
    /// shipped with until it was caught in the field.
    #[wasm_bindgen_test]
    async fn the_browser_platform_runs_a_program_that_reads_the_clock() {
        let vfs: Arc<crate::wasm_vfs::WasmVfs> = Arc::new(
            js_sys::eval("({})")
                .expect("an empty object evaluates")
                .unchecked_into(),
        );
        let callbacks = inert_callbacks();
        let run_store = Arc::new(bex_events::run::InMemoryRunStore::default());
        let sys_ops = Arc::new(crate::build_wasm_sys_ops(
            &callbacks,
            &run_store,
            &sys_wasm::SendWrapper::new(js_sys::Function::new_no_args("")),
            &vfs,
        ));

        let mut state = GlobalState::new(Executors::inline(), None);
        let applied = state.apply(vec![SourceMutation::UpsertRoot {
            spec: workspace_root_spec(std::path::PathBuf::from("/browser-test")),
            files: vec![(
                std::path::PathBuf::from("/browser-test/main.baml"),
                FIXTURE.to_string(),
            )],
        }]);
        assert!(applied.rejected.is_empty(), "{:?}", applied.rejected);

        let mut playground = PlaygroundState::default();
        rebuild(&state, &mut playground, &sys_ops);
        let installed = playground
            .installed
            .as_ref()
            .expect("a clean project builds an engine");
        assert_eq!(installed.source_revision, applied.revision);
        assert_eq!(installed.generation, 1);

        let context =
            bex_project::FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
        let value = installed
            .engine
            .call_function("stamp", Vec::new(), context, true)
            .await
            .expect("a timed function runs on the browser platform");
        assert_eq!(value, bex_project::BexExternalValue::Bool(true));
    }
}
