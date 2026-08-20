//! The playground host's one boundary onto the database and the engines.
//!
//! The host is an axum server on tokio; the database lives on the LSP owner
//! thread and may only be read through a [`baml_lsp::Snapshot`]. Every
//! DB-touching operation the playground needs is therefore an
//! [`baml_lsp::OwnerEvent::Call`] with a `oneshot` reply: the owner either
//! answers from state it holds (the revision, the root table) or mints a
//! snapshot and hands it to an executor, and the answer comes back to the
//! awaiting task. That is what replaces the deleted source gate — reads that
//! must agree with the revision they were validated against are issued inside
//! one owner continuation, so nothing can move underneath them.
//!
//! The seam also owns the **build pipeline**. `baml_lsp` publishes
//! diagnostics to editors and knows nothing about engines; the playground's
//! rebuild is host business, driven off the owner's source-change observer:
//!
//! ```text
//! GlobalState::apply ──observer──▶ [debounce 300 ms, coalescing]
//!   owner Call: snapshot ─▶ diagnostics lane: check + emit ─▶ Program
//!   blocking task: construct the engine candidate (runs `$init`)
//!   owner Call: revision + ProjectRuntime::commit_if_current   (fenced)
//!   ─▶ project update + test collection
//! ```
//!
//! One pipeline task per process makes builds single-flight without a gate,
//! and a superseded candidate simply never commits.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use baml_lsp::{
    Applied, GlobalState, OwnerEvent, Snapshot, SourceRevision,
    diagnostics::collect_root_candidate, executor::spawn_read, position_codec::PositionEncoding,
};
use bex_project::BexEngine;
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};

use crate::{
    engine::{
        CollectionTicket, CommitOutcome, PrepareRunError, ProjectRuntime, RegistryLease,
        RegistryLeaseError, RunSnapshot, RuntimeRegistry, construct_engine_candidate,
        spawn_engine_shutdown,
    },
    lsp_runtime::LspRuntime,
    playground_env::PlaygroundEnvState,
    playground_notify::{PlaygroundNotification, ProjectDiagnostic, TestExpandError},
    playground_sender::NativePlaygroundSender,
};

/// Quiet period before a source change turns into a rebuild. Keystrokes
/// coalesce; the check itself is Salsa-incremental, but `$init` is not.
const REBUILD_DEBOUNCE: Duration = Duration::from_millis(300);

/// A source file as the browser editor sees it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaygroundSourceFile {
    pub path: String,
    pub relative_path: String,
    pub content: String,
}

/// Which executor lane a snapshot read runs on.
#[derive(Debug, Clone, Copy)]
enum Lane {
    /// Interactive playground reads (graphs, cursor context, listings).
    Request,
    /// Whole-project sweeps: the build pipeline's check + emit.
    Build,
}

/// The latest build failure that source diagnostics did not already explain.
///
/// Revision-scoped so a later edit stops surfacing a stale failure while
/// `requestState` can still replay the failure that made the current build
/// unavailable.
#[derive(Default)]
struct BuildFailures {
    latest: Option<(SourceRevision, String)>,
}

impl BuildFailures {
    fn record(&mut self, revision: SourceRevision, message: String) {
        if self
            .latest
            .as_ref()
            .is_some_and(|(latest, _)| *latest > revision)
        {
            return;
        }
        self.latest = Some((revision, message));
    }

    fn clear_through(&mut self, revision: SourceRevision) {
        if self
            .latest
            .as_ref()
            .is_some_and(|(latest, _)| *latest <= revision)
        {
            self.latest = None;
        }
    }

    fn diagnostic_for(&self, revision: SourceRevision) -> Option<ProjectDiagnostic> {
        self.latest
            .as_ref()
            .filter(|(failed, _)| *failed == revision)
            .map(|(_, message)| ProjectDiagnostic {
                severity: "error",
                message: format!("Current build failed: {message}"),
            })
    }
}

pub struct PlaygroundSeam {
    runtime: Arc<LspRuntime>,
    runtimes: Arc<RuntimeRegistry>,
    sender: Arc<NativePlaygroundSender>,
    env_state: Arc<PlaygroundEnvState>,
    /// Built for every engine candidate; the playground intercepts HTTP, env
    /// and IO so runs report through the webview.
    sys_ops: Arc<sys_ops::SysOps>,
    build_failures: Mutex<BuildFailures>,
}

impl PlaygroundSeam {
    pub fn new(
        runtime: Arc<LspRuntime>,
        runtimes: Arc<RuntimeRegistry>,
        sender: Arc<NativePlaygroundSender>,
        env_state: Arc<PlaygroundEnvState>,
        sys_ops: Arc<sys_ops::SysOps>,
    ) -> Arc<Self> {
        Arc::new(Self {
            runtime,
            runtimes,
            sender,
            env_state,
            sys_ops,
            build_failures: Mutex::new(BuildFailures::default()),
        })
    }

    // ── Owner primitives ─────────────────────────────────────────────────

    /// Run `f` on the owner thread and await its result. `None` means the
    /// owner loop is gone (shutdown), which every caller treats as "no
    /// answer" rather than an error to report.
    async fn call<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut GlobalState) -> R + Send + 'static,
    ) -> Option<R> {
        let (tx, rx) = oneshot::channel();
        self.runtime
            .owner()
            .post(OwnerEvent::Call(Box::new(move |state| {
                let _ = tx.send(f(state));
            })));
        rx.await.ok()
    }

    /// Mint a snapshot on the owner and run `job` against it on `lane`.
    ///
    /// `None` covers owner shutdown, a mutation cancelling the read, and a
    /// panicking query — the playground degrades (an absent graph, an empty
    /// listing) instead of taking the process with it.
    async fn read<R: Send + 'static>(
        &self,
        lane: Lane,
        job: impl FnOnce(&Snapshot) -> R + Send + 'static,
    ) -> Option<R> {
        let (tx, rx) = oneshot::channel();
        self.runtime
            .owner()
            .post(OwnerEvent::Call(Box::new(move |state| {
                let snapshot = state.snapshot(baml_lsp::RequestCx::default());
                let executor = match lane {
                    Lane::Request => state.request_executor(),
                    Lane::Build => state.diagnostics_executor(),
                };
                spawn_read(
                    executor,
                    snapshot,
                    move |snap| Ok(job(snap)),
                    move |outcome| {
                        let value = match outcome {
                            Ok(Ok(value)) => Some(value),
                            Ok(Err(error)) => {
                                tracing::warn!(%error, "playground read failed");
                                None
                            }
                            Err(failure) => {
                                tracing::debug!(?failure, "playground read did not complete");
                                None
                            }
                        };
                        let _ = tx.send(value);
                    },
                );
            })));
        rx.await.ok().flatten()
    }

    // ── Project surface ──────────────────────────────────────────────────

    /// Absolute paths of the workspace roots, in table order. These are the
    /// "projects" of the playground wire protocol.
    pub async fn workspace_roots(&self) -> Vec<PathBuf> {
        self.call(|state| {
            state
                .roots()
                .workspace_roots()
                .map(|entry| entry.path.clone())
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default()
    }

    /// Source files of one workspace root as the database currently holds
    /// them — editor overlays included, since an overlay *is* the file's text
    /// in the database.
    pub async fn source_files(&self, project: &str) -> Vec<PlaygroundSourceFile> {
        let project = PathBuf::from(project);
        self.read(Lane::Request, move |snap| {
            let db = snap.db();
            let Some(entry) = snap
                .roots()
                .workspace_roots()
                .find(|entry| entry.path == project)
            else {
                return Vec::new();
            };
            let mut files: Vec<PlaygroundSourceFile> = db
                .root_files(entry.root)
                .into_iter()
                .map(|file| {
                    let path = file.path(db);
                    let relative_path = path
                        .strip_prefix(&entry.path)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .into_owned();
                    PlaygroundSourceFile {
                        path: path.to_string_lossy().into_owned(),
                        relative_path,
                        content: file.text(db).clone(),
                    }
                })
                .collect();
            files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
            files
        })
        .await
        .unwrap_or_default()
    }

    /// Env var names referenced from BAML source across every root. Values
    /// are never read here: only the *names* decide which keys are worth
    /// blocking a run to prompt for.
    pub async fn env_var_names(&self) -> Vec<String> {
        self.read(Lane::Request, |snap| baml_ide::all_env_var_names(snap.db()))
            .await
            .unwrap_or_default()
    }

    /// The AST control-flow graph for a function, as of the current sources.
    pub async fn control_flow_graph(
        &self,
        function_name: &str,
    ) -> Option<baml_compiler2_visualization::control_flow::ControlFlowGraph> {
        let function_name = function_name.to_string();
        self.read(Lane::Request, move |snap| {
            let db = snap.db();
            let files = baml_db::baml_compiler2_hir::compiler2_all_files(db);
            baml_ide::ast_control_flow_graph(db, &files, &function_name)
        })
        .await
        .flatten()
    }

    /// Playground cursor context at a zero-based UTF-16 position. The
    /// playground's wire coordinates are fixed UTF-16, independent of the
    /// LSP session's negotiated encoding.
    pub async fn cursor_context(
        &self,
        file_path: &str,
        line: u32,
        column: u32,
    ) -> baml_ide::CursorContext {
        let file_path = file_path.to_string();
        self.read(Lane::Request, move |snap| {
            let db = snap.db();
            let files = baml_db::baml_compiler2_hir::compiler2_all_files(db);
            let Some(source_file) = baml_ide::find_source_file(db, &files, &file_path) else {
                return empty_cursor_context();
            };
            let codec = baml_lsp::position_codec::PositionCodec::new(
                source_file.text(db),
                PositionEncoding::UTF16,
            );
            let byte_offset = codec
                .position_to_offset(lsp_types::Position {
                    line,
                    character: column,
                })
                .map_or(0, u32::from);
            baml_ide::playground_cursor_context(db, &files, &file_path, byte_offset)
        })
        .await
        .unwrap_or_else(empty_cursor_context)
    }

    // ── Runs ─────────────────────────────────────────────────────────────

    /// One coherent launch snapshot: the engine and its generation validated
    /// against the current revision, with the overlay control-flow graph
    /// pinned for that generation so the run's span overlays stay resolvable
    /// after later recompiles retire the engine.
    ///
    /// The validation and the graph's snapshot are minted in the *same* owner
    /// continuation, so the pinned graph is built from exactly the sources the
    /// engine was emitted from.
    pub async fn prepare_function_run(
        &self,
        project: &str,
        overlay_function: Option<&str>,
    ) -> Result<RunSnapshot, PrepareRunError> {
        let root = PathBuf::from(project);
        let overlay_function = overlay_function.map(str::to_string);
        let runtimes = Arc::clone(&self.runtimes);
        let (tx, rx) = oneshot::channel();
        self.runtime
            .owner()
            .post(OwnerEvent::Call(Box::new(move |state| {
                let revision = state.revision();
                let Some(runtime) = runtimes.existing(&root) else {
                    let _ = tx.send(Err(PrepareRunError::NeedsCurrentBuild));
                    return;
                };
                let prepared = match runtime.prepare_function_run(revision) {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        let _ = tx.send(Err(error));
                        return;
                    }
                };
                let generation = prepared.generation;
                let Some(function_name) = overlay_function else {
                    let _ = tx.send(Ok(prepared));
                    return;
                };
                if runtime.overlay_graph(generation, &function_name).is_some() {
                    let _ = tx.send(Ok(prepared));
                    return;
                }
                let snapshot = state.snapshot(baml_lsp::RequestCx::default());
                spawn_read(
                    state.request_executor(),
                    snapshot,
                    move |snap| {
                        let db = snap.db();
                        let files = baml_db::baml_compiler2_hir::compiler2_all_files(db);
                        Ok(baml_ide::ast_control_flow_graph(db, &files, &function_name)
                            .map(|graph| (function_name, graph)))
                    },
                    move |outcome| {
                        if let Ok(Ok(Some((function_name, graph)))) = outcome {
                            runtime.pin_overlay_graph(generation, &function_name, Arc::new(graph));
                        }
                        let _ = tx.send(Ok(prepared));
                    },
                );
            })));
        rx.await.unwrap_or(Err(PrepareRunError::NeedsCurrentBuild))
    }

    /// The engine a run launched on, for cancel targeting. `None` once that
    /// generation has been replaced and released.
    pub fn engine_for_generation(&self, project: &str, generation: u64) -> Option<Arc<BexEngine>> {
        self.runtimes
            .existing(Path::new(project))?
            .engine_for_generation(generation)
    }

    /// Lease the collected test registry for a run of `test_name`, validated
    /// against the current revision on the owner thread.
    pub async fn lease_registry(
        &self,
        project: &str,
        generation: u64,
    ) -> Result<RegistryLease, RegistryLeaseError> {
        let root = PathBuf::from(project);
        let runtimes = Arc::clone(&self.runtimes);
        self.call(move |state| {
            let revision = state.revision();
            let Some(runtime) = runtimes.existing(&root) else {
                return Err(RegistryLeaseError::NeedsCurrentBuild);
            };
            runtime.lease_registry(generation, revision)
        })
        .await
        .unwrap_or(Err(RegistryLeaseError::NeedsCurrentBuild))
    }

    /// Run one collected test against its leased registry.
    pub async fn call_test_function_with_trace(
        &self,
        project: &str,
        generation: u64,
        test_name: &str,
        ctx: bex_project::FunctionCallContext,
    ) -> Result<bex_engine::BexCallResult, bex_engine::EngineError> {
        let lease = self
            .lease_registry(project, generation)
            .await
            .map_err(|e| bex_engine::EngineError::FunctionNotFound {
                name: e.to_string(),
            })?;
        lease
            .engine
            .call_function_with_trace(
                "testing.TestRegistry.run_test",
                vec![
                    bex_project::BexExternalValue::Handle(lease.handle.clone()),
                    bex_project::BexExternalValue::String(test_name.into()),
                ],
                ctx,
                true, // deep copy the TestReport for the wire
            )
            .await
    }
}

fn empty_cursor_context() -> baml_ide::CursorContext {
    baml_ide::CursorContext {
        function_name: None,
        is_workflow: false,
        workflow_memberships: Vec::new(),
        source_expr_id: None,
        source_expr_candidates: Vec::new(),
        source_expr_function_name: None,
        test_name: None,
        cursor_offset: None,
    }
}

// ---------------------------------------------------------------------------
// Project state pushes
// ---------------------------------------------------------------------------

impl PlaygroundSeam {
    /// Push the project list plus one `UpdateProject` per workspace root.
    /// This is what `requestState` answers with, and what every WS page gets
    /// on connect.
    pub async fn push_project_state(&self) {
        self.send_list_projects().await;
        for root in self.workspace_roots().await {
            self.push_project_update(&root).await;
        }
    }

    pub async fn send_list_projects(&self) {
        let projects = self
            .workspace_roots()
            .await
            .into_iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect();
        self.sender
            .send_playground_notification(&PlaygroundNotification::ListProjects { projects });
    }

    /// Compute and push one root's `UpdateProject`, checking the root for
    /// diagnostics as part of it.
    async fn push_project_update(&self, root: &Path) {
        let Some((revision, diagnostics)) = self.check_root(root).await else {
            return;
        };
        self.push_project_update_with(root, revision, diagnostics)
            .await;
    }

    /// Push one root's `UpdateProject` from an already-computed diagnostics
    /// list (the build pipeline has one in hand and must not re-derive it).
    async fn push_project_update_with(
        &self,
        root: &Path,
        revision: SourceRevision,
        mut diagnostics: Vec<ProjectDiagnostic>,
    ) {
        // The build-failure read and the push stay in one step: a diagnostics
        // snapshot that read "no failure" must not publish after the build
        // recorded one for the same revision and regress the UI to its
        // transient preparing state.
        if let Some(diagnostic) = self.build_failures.lock().diagnostic_for(revision) {
            diagnostics.push(diagnostic);
        }
        let installed = self
            .runtimes
            .existing(root)
            .and_then(|runtime| runtime.installed());
        let is_bex_current = installed.is_some_and(|receipt| receipt.source_revision == revision);
        let generation = installed.map_or(0, |receipt| receipt.generation);

        let Some(update) = self
            .read(Lane::Request, move |snap| {
                crate::playground_notify::build_project_update(
                    snap.db(),
                    is_bex_current,
                    generation,
                    diagnostics,
                )
            })
            .await
        else {
            return;
        };
        self.sender
            .send_playground_notification(&PlaygroundNotification::UpdateProject {
                project: root.to_string_lossy().into_owned(),
                update,
            });
    }

    /// Check one workspace root and flatten its diagnostics onto the wire
    /// shape. `None` when the root is gone or the read did not complete.
    async fn check_root(&self, root: &Path) -> Option<(SourceRevision, Vec<ProjectDiagnostic>)> {
        let root = root.to_path_buf();
        self.read(Lane::Build, move |snap| {
            let check = check_root_on(snap, &root)?;
            Some((check.revision, check.diagnostics))
        })
        .await
        .flatten()
    }
}

/// One workspace root's check, as the playground needs it.
struct RootCheck {
    revision: SourceRevision,
    diagnostics: Vec<ProjectDiagnostic>,
    has_errors: bool,
}

/// Check `root` against `snap` — the shared half of "show me the diagnostics"
/// and "build me an engine". `None` when `root` is not (or is no longer) a
/// workspace root of this snapshot.
fn check_root_on(snap: &Snapshot, root: &Path) -> Option<RootCheck> {
    let entry = snap
        .roots()
        .workspace_roots()
        .find(|entry| entry.path == root)?;
    let candidate = collect_root_candidate(snap, entry.root).ok()?;
    let documents = baml_lsp::diagnostics::candidate_to_publishable(
        &candidate,
        PositionEncoding::UTF16,
        snap.roots(),
    );
    Some(RootCheck {
        revision: snap.revision(),
        diagnostics: crate::playground_notify::flatten_diagnostics(&documents),
        has_errors: candidate.has_errors(),
    })
}

// ---------------------------------------------------------------------------
// Build pipeline
// ---------------------------------------------------------------------------

/// What one workspace root's check produced for the build pipeline.
struct BuildInput {
    revision: SourceRevision,
    diagnostics: Vec<ProjectDiagnostic>,
    /// `None` when the check found errors: emit is not attempted, and the
    /// diagnostics are the explanation the UI shows.
    program: Option<Box<bex_vm_types::Program>>,
    /// Emit failed on a clean check — a compiler defect worth surfacing as a
    /// project-level failure rather than silently leaving a stale engine.
    emit_error: Option<String>,
}

/// The facts the owner hands the pipeline for one applied batch.
#[derive(Debug, Clone, Copy)]
struct SourceChange {
    roots_changed: bool,
}

impl PlaygroundSeam {
    /// Install the owner's source-change observer and start the single
    /// pipeline task. Call once, on the tokio runtime that owns the host.
    pub fn spawn_source_pipeline(self: &Arc<Self>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let observer: Arc<dyn Fn(&Applied) + Send + Sync> = Arc::new(move |applied: &Applied| {
            // Runs on the owner thread: hand the facts off and return.
            let _ = tx.send(SourceChange {
                roots_changed: applied.roots_changed,
            });
        });
        self.runtime
            .owner()
            .post(OwnerEvent::Call(Box::new(move |state| {
                state.set_source_observer(observer);
            })));

        let seam = Arc::clone(self);
        tokio::spawn(async move { seam.source_pipeline(rx).await });
    }

    /// Coalesce source changes into one rebuild per quiet period. The task is
    /// sequential, which is what makes builds single-flight.
    async fn source_pipeline(self: Arc<Self>, mut rx: mpsc::UnboundedReceiver<SourceChange>) {
        loop {
            let Some(mut change) = rx.recv().await else {
                return; // the owner is gone
            };
            loop {
                self.on_source_changed(change).await;
                match tokio::time::timeout(REBUILD_DEBOUNCE, rx.recv()).await {
                    Ok(Some(next)) => change = next,
                    Ok(None) => return,
                    Err(_) => break, // quiet period elapsed
                }
            }
            self.rebuild_all().await;
        }
    }

    /// Immediate (undebounced) reactions to a source change: stale derived
    /// work is cancelled now, and the project list is only pushed when the
    /// root set actually moved.
    async fn on_source_changed(&self, change: SourceChange) {
        self.runtimes.for_each(ProjectRuntime::on_source_changed);
        if !change.roots_changed {
            return;
        }
        // A root left the workspace: its engine has nothing left to serve.
        for engine in self.runtimes.retain(&self.workspace_roots().await) {
            spawn_engine_shutdown(engine);
        }
        self.send_list_projects().await;
    }

    /// One rebuild attempt per workspace root.
    pub async fn rebuild_all(self: &Arc<Self>) {
        // The declared set decides which env keys are worth blocking a run to
        // prompt for; an edit can add or remove an `env.FOO` reference.
        self.env_state
            .set_declared_keys(&self.env_var_names().await);
        for root in self.workspace_roots().await {
            self.rebuild_root(&root).await;
        }
    }

    async fn rebuild_root(self: &Arc<Self>, root: &Path) {
        let Some(input) = self.build_input(root).await else {
            return;
        };
        let revision = input.revision;
        match &input.emit_error {
            Some(message) => {
                tracing::warn!(%message, "playground build: emit failed at {revision}");
                self.build_failures.lock().record(revision, message.clone());
            }
            // Whatever failure is showing belongs to an older revision and no
            // longer explains anything about this one.
            None => self.build_failures.lock().clear_through(revision),
        }

        // Push now that the check is done: diagnostics land in the UI without
        // waiting for `$init`.
        self.push_project_update_with(root, revision, input.diagnostics.clone())
            .await;

        let Some(program) = input.program else {
            return; // blocked by diagnostics, or emit failed
        };

        let sys_ops = Arc::clone(&self.sys_ops);
        let candidate = tokio::task::spawn_blocking(move || {
            construct_engine_candidate(*program, sys_ops, revision)
        })
        .await;
        let candidate = match candidate {
            Ok(Ok(candidate)) => candidate,
            Ok(Err(error)) => {
                let message = error.to_string();
                tracing::warn!(%message, "playground build: engine construction failed");
                self.build_failures.lock().record(revision, message);
                self.push_project_update_with(root, revision, input.diagnostics)
                    .await;
                return;
            }
            Err(error) => {
                // A panicking `$init` unwinds into the blocking task instead
                // of killing the process; nothing commits and the next edit
                // retries.
                tracing::error!(%error, "playground build: engine construction panicked");
                self.build_failures
                    .lock()
                    .record(revision, "engine initialization panicked".to_string());
                self.push_project_update_with(root, revision, input.diagnostics)
                    .await;
                return;
            }
        };

        let runtime = self.runtimes.runtime(root);
        let commit_runtime = Arc::clone(&runtime);
        let outcome = self
            .call(move |state| commit_runtime.commit_if_current(candidate, state.revision()))
            .await;
        match outcome {
            Some(CommitOutcome::Committed { receipt, retired }) => {
                // Draining is spawned from here, not from the commit itself:
                // commits run on the owner thread, which is not a tokio
                // worker and so cannot spawn the drain.
                if let Some(engine) = retired {
                    spawn_engine_shutdown(engine);
                }
                self.build_failures
                    .lock()
                    .clear_through(receipt.source_revision);
                tracing::info!(
                    "playground build: generation {} committed at {}",
                    receipt.generation,
                    receipt.source_revision
                );
                self.push_project_update_with(root, revision, input.diagnostics)
                    .await;
                self.collect_tests(root).await;
            }
            Some(CommitOutcome::Superseded { current_revision }) => {
                tracing::debug!(
                    "playground build: superseded by {current_revision}; candidate dropped"
                );
            }
            None => {}
        }
    }

    /// Check the root and, when it is clean, emit its program — one snapshot,
    /// on the build lane.
    async fn build_input(&self, root: &Path) -> Option<BuildInput> {
        let root = root.to_path_buf();
        self.read(Lane::Build, move |snap| {
            let check = check_root_on(snap, &root)?;
            let mut input = BuildInput {
                revision: check.revision,
                diagnostics: check.diagnostics,
                program: None,
                emit_error: None,
            };
            if check.has_errors {
                return Some(input);
            }
            // The check above is a full sweep of the root, so
            // `get_bytecode`'s own error gate would re-derive exactly what
            // `has_errors` just proved — skip it.
            match snap.db().get_bytecode_unchecked() {
                Ok(program) => input.program = Some(Box::new(program)),
                Err(error) => input.emit_error = Some(error.to_string()),
            }
            Some(input)
        })
        .await
        .flatten()
    }
}

// ---------------------------------------------------------------------------
// Test collection and expansion
// ---------------------------------------------------------------------------

impl PlaygroundSeam {
    /// Start one test-collection attempt for a root and push the resulting
    /// tree. Every emission is fenced by the ticket identity, so a collection
    /// that a newer build (or a newer collection) superseded emits nothing.
    pub async fn collect_tests(self: &Arc<Self>, root: &Path) {
        let root_path = root.to_path_buf();
        let runtimes = Arc::clone(&self.runtimes);
        // The ticket and the root's package name come from one owner
        // continuation: `collect_tests` addresses the engine by package, and
        // that must be the package the engine was emitted for.
        let begun = self
            .call(move |state| {
                let revision = state.revision();
                let package = state
                    .roots()
                    .workspace_roots()
                    .find(|entry| entry.path == root_path)?
                    .package
                    .to_string();
                let ticket = runtimes
                    .existing(&root_path)?
                    .begin_test_collection(revision)?;
                Some((ticket, package))
            })
            .await
            .flatten();
        let Some((ticket, package)) = begun else {
            tracing::debug!("collect_tests: no current engine for {}", root.display());
            return;
        };

        let seam = Arc::clone(self);
        let runtime = self.runtimes.runtime(root);
        let project = root.to_string_lossy().into_owned();
        tokio::spawn(async move {
            seam.run_test_collection(&runtime, ticket, project, package)
                .await;
        });
    }

    async fn run_test_collection(
        &self,
        runtime: &ProjectRuntime,
        ticket: CollectionTicket,
        project: String,
        package: String,
    ) {
        let call_id = sys_types::CallId::next();
        let generation = ticket.generation;
        let engine = Arc::clone(&ticket.engine);
        let cancel = ticket.cancel.clone();

        let registry = match engine
            .collect_tests(&package, call_id, cancel.clone())
            .await
        {
            Ok(registry) => registry,
            Err(error) => {
                // A stale or cancelled collection emits nothing; a failure for
                // the still-current build unblocks the frontend with an empty
                // tree rather than a spinner that never resolves.
                tracing::error!(%error, "collect_tests failed");
                if runtime.collection_ticket_is_current(&ticket) {
                    self.send_test_tree(project, generation, call_id, empty_tree(), None);
                }
                return;
            }
        };

        // Null means the project has no tests (`$init_test` absent), which is
        // a different state from "not collected yet".
        let handle = match &registry {
            bex_project::BexExternalValue::Handle(handle) => Some(handle.clone()),
            bex_project::BexExternalValue::Null => None,
            other => {
                tracing::error!("collect_tests returned an unexpected value: {other:?}");
                return;
            }
        };

        // ABA fence: install only while the ticket's generation and collection
        // epoch are still the installed ones.
        if !runtime.install_collected_registry(&ticket, handle) {
            tracing::debug!("collect_tests: discarding a stale result (gen {generation})");
            return;
        }
        if matches!(registry, bex_project::BexExternalValue::Null) {
            self.send_test_tree(project, generation, call_id, empty_tree(), None);
            return;
        }

        let ctx = bex_project::FunctionCallContextBuilder::new(call_id)
            .with_cancel_token(cancel)
            .with_profile_enabled(false)
            .build();
        let data = match engine
            .call_function("testing.TestRegistry.serialize", vec![registry], ctx, true)
            .await
        {
            Ok(serialized) => serialize_tree(&serialized),
            Err(error) => {
                tracing::error!(%error, "collect_tests: serializing the tree failed");
                empty_tree()
            }
        };
        // Emission is fenced too: if a newer engine or collection superseded
        // us during serialization, stay silent — the newer attempt owns the
        // tree.
        if runtime.collection_ticket_is_current(&ticket) {
            self.send_test_tree(project, generation, call_id, data, None);
        }
    }

    /// Expand one lazy test set in place and re-push the tree. Fire-and-forget
    /// from the wire's perspective: the result arrives as a
    /// `TestCollectionResult` notification.
    pub async fn expand_test_set(self: &Arc<Self>, project: &str, generation: u64, name: &str) {
        let lease = match self.lease_registry(project, generation).await {
            Ok(lease) => lease,
            Err(error) => {
                tracing::info!("not expanding '{name}': {error}");
                return;
            }
        };
        let Some(runtime) = self.runtimes.existing(Path::new(project)) else {
            return;
        };
        let seam = Arc::clone(self);
        let project = project.to_string();
        let name = name.to_string();
        tokio::spawn(async move {
            seam.run_expansion(&runtime, lease, project, generation, name)
                .await;
        });
    }

    async fn run_expansion(
        &self,
        runtime: &ProjectRuntime,
        lease: RegistryLease,
        project: String,
        generation: u64,
        name: String,
    ) {
        // One mutation owner per installed registry: expansions mutate the
        // registry heap object in place, so they serialize here.
        let _mutation_owner = lease.expansion_gate.lock().await;

        let call_id = sys_types::CallId::next();
        let engine = Arc::clone(&lease.engine);
        let registry_value = bex_project::BexExternalValue::Handle(lease.handle.clone());
        let cancel = lease.cancel.clone();

        let ctx = bex_project::FunctionCallContextBuilder::new(call_id)
            .with_cancel_token(cancel.clone())
            .with_profile_enabled(false)
            .build();
        let expand_error = match engine
            .call_function(
                "testing.TestRegistry.expand_set",
                vec![registry_value.clone(), name.as_str().into()],
                ctx,
                true,
            )
            .await
        {
            Ok(_) => None,
            Err(error) => {
                tracing::error!(%error, "expanding testset '{name}' failed");
                if cancel.is_cancelled() {
                    return; // superseded mid-expansion: emit nothing
                }
                Some(TestExpandError {
                    testset_name: name.clone(),
                    message: error.to_string(),
                })
            }
        };

        // Re-serialize either way: on failure the pre-expansion tree unblocks
        // the UI from its loading state instead of spinning forever.
        let ctx = bex_project::FunctionCallContextBuilder::new(sys_types::CallId::next())
            .with_cancel_token(cancel)
            .with_profile_enabled(false)
            .build();
        let data = match engine
            .call_function(
                "testing.TestRegistry.serialize",
                vec![registry_value],
                ctx,
                true,
            )
            .await
        {
            Ok(serialized) => serialize_tree(&serialized),
            Err(error) => {
                tracing::error!(%error, "serializing after expanding '{name}' failed");
                empty_tree()
            }
        };
        if runtime.registry_lease_is_current(&lease) {
            self.send_test_tree(project, generation, call_id, data, expand_error);
        }
    }

    fn send_test_tree(
        &self,
        project: String,
        generation: u64,
        call_id: sys_types::CallId,
        data: Vec<u8>,
        expand_error: Option<TestExpandError>,
    ) {
        self.sender
            .send_playground_notification(&PlaygroundNotification::TestCollectionResult {
                project,
                generation,
                call_id: call_id.0,
                data,
                expand_error,
            });
    }
}

fn empty_tree() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!([])).unwrap_or_default()
}

fn serialize_tree(value: &bex_project::BexExternalValue) -> Vec<u8> {
    serde_json::to_vec(&bex_value_to_json(value)).unwrap_or_default()
}

/// Convert a `BexExternalValue` to JSON for the test tree.
///
/// Only the primitive/structural variants that appear in test reports are
/// represented; handles, ADTs, and function refs become `null`.
fn bex_value_to_json(value: &bex_project::BexExternalValue) -> serde_json::Value {
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
        V::Map { entries, .. } => {
            let mut map = serde_json::Map::new();
            for (key, value) in entries {
                map.insert(key.clone(), bex_value_to_json(value));
            }
            serde_json::Value::Object(map)
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The failure a `requestState` replays must belong to the revision the
    /// UI is looking at: an edit stops surfacing it, a late stale rebuild
    /// cannot overwrite a newer one, and a commit clears it.
    #[test]
    fn build_failures_are_revision_fenced_for_project_update_replay() {
        let mut failures = BuildFailures::default();

        failures.record(SourceRevision(4), "failed to emit bytecode".to_string());
        let rendered = failures
            .diagnostic_for(SourceRevision(4))
            .expect("the current revision's build failure reaches the project update");
        assert_eq!(rendered.severity, "error");
        assert_eq!(
            rendered.message,
            "Current build failed: failed to emit bytecode"
        );
        assert!(
            failures.diagnostic_for(SourceRevision(5)).is_none(),
            "an edit must not replay the previous revision's failure"
        );

        failures.record(SourceRevision(3), "older failure".to_string());
        assert_eq!(
            failures
                .diagnostic_for(SourceRevision(4))
                .map(|diagnostic| diagnostic.message),
            Some("Current build failed: failed to emit bytecode".to_string()),
            "a late stale rebuild must not replace a newer failure"
        );

        failures.clear_through(SourceRevision(3));
        assert!(
            failures.diagnostic_for(SourceRevision(4)).is_some(),
            "clearing through an older revision leaves the newer failure"
        );
        failures.clear_through(SourceRevision(4));
        assert!(failures.diagnostic_for(SourceRevision(4)).is_none());
    }
}
