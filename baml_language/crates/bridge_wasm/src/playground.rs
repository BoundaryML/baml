//! The browser playground's engine: build it from the sources the language
//! server already holds, and tell the host what it can run.
//!
//! The desktop host needs a revision fence here — its build runs on a pool
//! and can be superseded mid-flight by an edit. The browser has neither
//! problem: the build is synchronous on the one thread, so by construction it
//! can only ever describe the sources it started from, and "the engine is
//! current" is just "no edit has landed since". What remains is the same
//! shape: check, emit, construct, install, report.

use std::{rc::Rc, sync::Arc};

use baml_lsp::{
    GlobalState, SourceRevision, diagnostics::collect_root_candidate, executor::spawn_read,
    position_codec::PositionEncoding, snapshot::Snapshot,
};
use bex_project::BexEngine;

use crate::playground_notify::{
    FunctionInfo, FunctionKind, FunctionOrigin, FunctionSourcePosition, LlmCapabilities,
    ProjectDiagnostic, ProjectUpdate,
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
    /// Fences collection results: two collections against one engine
    /// generation must not install out of order.
    collection_epoch: u64,
    registry: Option<InstalledRegistry>,
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
        // The old engine's tests describe code that no longer exists.
        self.collection_epoch += 1;
        self.registry = None;
        self.build_failure = None;
    }
}

/// What one check of the workspace root produced.
struct RootCheck {
    revision: SourceRevision,
    diagnostics: Vec<ProjectDiagnostic>,
    /// `None` when the check found errors: emit is not attempted and the
    /// diagnostics are the explanation.
    program: Option<Box<bex_project::Program>>,
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
        ProjectUpdate {
            is_bex_current,
            generation,
            functions,
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

// ── Test registry ───────────────────────────────────────────────────────────

/// The collected test registry, and the identity it was collected under.
struct InstalledRegistry {
    generation: u64,
    collection_epoch: u64,
    /// `Some` when the project has tests; `None` when collection finished and
    /// found none (`$init_test` absent) — a different state from "not yet
    /// collected", which is `registry == None`.
    handle: Option<bex_project::Handle>,
    /// One mutation owner per registry: expansions mutate the registry object
    /// on the heap in place, so they serialize here. Single-threaded is not
    /// the same as un-interleaved — two expansions can interleave at their
    /// await points.
    expansion_gate: Rc<futures::lock::Mutex<()>>,
}

/// One collection attempt's identity, captured before the engine call so
/// every later step can check it is still the one that matters.
pub(crate) struct CollectionTicket {
    pub(crate) generation: u64,
    collection_epoch: u64,
    pub(crate) engine: Arc<BexEngine>,
    pub(crate) package: String,
}

/// A registry checked out for work (running a test, expanding a set).
pub(crate) struct RegistryLease {
    generation: u64,
    collection_epoch: u64,
    pub(crate) engine: Arc<BexEngine>,
    pub(crate) handle: bex_project::Handle,
    pub(crate) expansion_gate: Rc<futures::lock::Mutex<()>>,
}

/// Why a registry could not be leased.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegistryLeaseError {
    /// The generation asked for is not the installed one, or the installed
    /// engine no longer matches the sources.
    NeedsCurrentBuild,
    /// Tests have not been collected for this build yet.
    NoRegistry,
    /// Collection finished and the project has no tests.
    NoTests,
}

impl std::fmt::Display for RegistryLeaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::NeedsCurrentBuild => {
                "the engine is not current with the sources; wait for the rebuild"
            }
            Self::NoRegistry => "tests have not been collected for this build yet",
            Self::NoTests => "the project has no tests",
        };
        f.write_str(message)
    }
}

impl PlaygroundState {
    /// Open a collection attempt against the installed engine, if there is one
    /// current with `revision`. Collecting for a dead revision produces a tree
    /// that could never be installed.
    pub(crate) fn begin_test_collection(
        &mut self,
        revision: SourceRevision,
        package: String,
    ) -> Option<CollectionTicket> {
        let installed = self.installed.as_ref()?;
        if installed.source_revision != revision {
            return None;
        }
        self.collection_epoch += 1;
        Some(CollectionTicket {
            generation: installed.generation,
            collection_epoch: self.collection_epoch,
            engine: Arc::clone(&installed.engine),
            package,
        })
    }

    /// Install a collected registry, unless a newer build or a newer
    /// collection got there first.
    pub(crate) fn install_collected_registry(
        &mut self,
        ticket: &CollectionTicket,
        handle: Option<bex_project::Handle>,
    ) -> bool {
        if !self.ticket_is_current(ticket) {
            return false;
        }
        self.registry = Some(InstalledRegistry {
            generation: ticket.generation,
            collection_epoch: ticket.collection_epoch,
            handle,
            expansion_gate: Rc::new(futures::lock::Mutex::new(())),
        });
        true
    }

    /// Whether a collection's result is still the one the host should see.
    pub(crate) fn ticket_is_current(&self, ticket: &CollectionTicket) -> bool {
        self.installed
            .as_ref()
            .is_some_and(|installed| installed.generation == ticket.generation)
            && self.collection_epoch == ticket.collection_epoch
    }

    /// Check out the registry for work at `generation`, requiring the engine
    /// to still match the sources.
    pub(crate) fn lease_registry(
        &self,
        generation: u64,
        revision: SourceRevision,
    ) -> Result<RegistryLease, RegistryLeaseError> {
        let Some(installed) = self.installed.as_ref() else {
            return Err(RegistryLeaseError::NeedsCurrentBuild);
        };
        if installed.generation != generation || installed.source_revision != revision {
            return Err(RegistryLeaseError::NeedsCurrentBuild);
        }
        let Some(registry) = self.registry.as_ref() else {
            return Err(RegistryLeaseError::NoRegistry);
        };
        let Some(handle) = registry.handle.clone() else {
            return Err(RegistryLeaseError::NoTests);
        };
        Ok(RegistryLease {
            generation,
            collection_epoch: registry.collection_epoch,
            engine: Arc::clone(&installed.engine),
            handle,
            expansion_gate: Rc::clone(&registry.expansion_gate),
        })
    }

    /// Whether an expansion's result is still the one the host should see: the
    /// engine must be the leased generation *and* the registry object must be
    /// the leased one (a re-collection replaces it).
    pub(crate) fn lease_is_current(&self, lease: &RegistryLease) -> bool {
        self.installed
            .as_ref()
            .is_some_and(|installed| installed.generation == lease.generation)
            && self.registry.as_ref().is_some_and(|registry| {
                registry.generation == lease.generation
                    && registry.collection_epoch == lease.collection_epoch
            })
    }
}

/// A run's engine and the generation it is pinned to, captured together.
pub(crate) struct RunSnapshot {
    pub(crate) generation: u64,
    pub(crate) engine: Arc<BexEngine>,
}

impl PlaygroundState {
    /// The engine to launch a run on, if it is current with `revision`.
    ///
    /// One read, not two: the old browser host fetched the engine and the
    /// generation separately, which could pin a run to a generation the engine
    /// no longer was. A run never silently falls back to a last-known-good
    /// engine either — stale results are worse than a refusal the UI can
    /// explain.
    pub(crate) fn prepare_run(&self, revision: SourceRevision) -> Option<RunSnapshot> {
        let installed = self.installed.as_ref()?;
        (installed.source_revision == revision).then(|| RunSnapshot {
            generation: installed.generation,
            engine: Arc::clone(&installed.engine),
        })
    }

    /// The engine a run launched on, by its pinned generation — for cancel,
    /// which must reach the engine that owns the call even after a rebuild.
    pub(crate) fn engine_for_generation(&self, generation: u64) -> Option<Arc<BexEngine>> {
        self.installed
            .as_ref()
            .filter(|installed| installed.generation == generation)
            .map(|installed| Arc::clone(&installed.engine))
    }
}

/// Convert a serialized test tree (or a report) to JSON for the wire.
///
/// Only the primitive and structural variants that appear in test trees are
/// represented; handles, ADTs and function refs become `null`.
pub(crate) fn bex_value_to_json(value: &bex_project::BexExternalValue) -> serde_json::Value {
    use bex_project::BexExternalValue as V;
    match value {
        V::Null => serde_json::Value::Null,
        V::Int(i) => serde_json::json!(i),
        // Bigints can exceed JSON number precision; emit as a decimal string.
        V::Bigint(b) => serde_json::json!(b.to_string()),
        V::Float(f) => serde_json::json!(f),
        V::Bool(b) => serde_json::json!(b),
        V::String(s) => serde_json::json!(s.as_str()),
        V::Array { items, .. } => {
            serde_json::Value::Array(items.iter().map(bex_value_to_json).collect())
        }
        V::Map { entries, .. } => serde_json::Value::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), bex_value_to_json(value)))
                .collect(),
        ),
        V::Instance {
            class_name, fields, ..
        } => {
            let mut map = serde_json::Map::new();
            map.insert("$type".to_string(), serde_json::json!(class_name));
            for (key, value) in fields {
                map.insert(key.clone(), bex_value_to_json(value));
            }
            serde_json::Value::Object(map)
        }
        V::Variant {
            enum_name,
            variant_name,
        } => serde_json::json!({ "$enum": enum_name, "value": variant_name }),
        V::Union { value, .. } => bex_value_to_json(value),
        _ => serde_json::Value::Null,
    }
}

/// The workspace root's path and package name, as the playground addresses it.
pub(crate) fn workspace_root(state: &GlobalState) -> Option<(String, String)> {
    read(state, |snap| {
        snap.roots().workspace_roots().next().map(|entry| {
            (
                entry.path.to_string_lossy().into_owned(),
                entry.package.to_string(),
            )
        })
    })
    .flatten()
}

/// The control-flow graph for a function, ready for the graph view.
pub(crate) fn control_flow_graph(
    state: &GlobalState,
    function_name: &str,
) -> Option<serde_json::Value> {
    let function_name = function_name.to_owned();
    let graph = read(state, move |snap| {
        let db = snap.db();
        // Workspace files only: the playground names workspace functions, and
        // the stdlib shares this database — `compiler2_all_files` would let a
        // stdlib function of the same name win the lookup.
        let files = db.workspace_files();
        baml_ide::ast_control_flow_graph(db, &files, &function_name)
    })
    .flatten()?;
    let prepared =
        baml_compiler2_visualization::control_flow::prepare_control_flow_graph_for_visualization(
            &graph,
        );
    serde_json::to_value(prepared).ok()
}

/// What the cursor is inside, for the graph view's follow-along.
///
/// The playground's wire coordinates are fixed zero-based UTF-16, independent
/// of the encoding the LSP session negotiated.
pub(crate) fn cursor_context(
    state: &GlobalState,
    file: &str,
    line: u32,
    column: u32,
) -> Option<serde_json::Value> {
    let file = file.to_owned();
    let context = read(state, move |snap| {
        let db = snap.db();
        let files = db.workspace_files();
        let source_file = baml_ide::find_source_file(db, &files, &file)?;
        let codec = baml_lsp::position_codec::PositionCodec::new(
            source_file.text(db),
            PositionEncoding::UTF16,
        );
        let offset = codec
            .position_to_offset(lsp_types::Position {
                line,
                character: column,
            })
            .map_or(0, u32::from);
        Some(baml_ide::playground_cursor_context(
            db, &files, &file, offset,
        ))
    })
    .flatten()?;
    serde_json::to_value(context).ok()
}
