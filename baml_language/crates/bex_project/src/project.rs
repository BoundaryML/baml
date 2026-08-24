//! Project core: one source gate, owned build candidates, and a coherent
//! runtime state with revision-conditional engine commits.
//!
//! # Concurrency and publication invariants
//!
//! - **`SourceRevision` is authoritative.** [`SourceState`] holds the
//!   Salsa database, a monotonic revision, and the open-document version map
//!   behind one mutex (the *source gate*). Every accepted source-mutation
//!   batch advances the revision in the same critical section that mutates
//!   the database and version map. A lock-free [`BexProject::observed_revision`]
//!   mirror exists for observation (error mapping) only — it never
//!   authorizes a commit.
//! - **Background work produces owned, revision-tagged candidates.**
//!   [`CompilationOutcome`] / [`DiagnosticCandidate`] / [`EngineCandidate`]
//!   are fully owned; no database guard survives candidate creation.
//! - **Engine installation is one atomic conditional commit.**
//!   [`BexProject::commit_engine_if_current`] takes the *same* source gate
//!   used by mutations, compares revisions, and only then installs. A
//!   superseded candidate changes no runtime state and drops quietly
//!   (profiling was never activated).
//! - **Runtime identity is coherent.** [`RuntimeState`] replaces the
//!   old `(bool, Option<Arc<BexEngine>>)` currentness flag and separate
//!   `TestState`. Currentness is *derived* from
//!   `installed.source_revision == source.revision`, so a source mutation
//!   makes the engine non-current by definition, not by flag ordering.
//! - **Run and test entry use coherent snapshots.**
//!   [`BexProject::prepare_function_run`] and [`BexProject::lease_registry`]
//!   validate and capture engine/generation/graph/registry in one
//!   source→runtime transaction.
//! - **Request waits are bounded.**
//!   [`BexProject::read_source_for_request`] retries `try_lock` for up to
//!   [`REQUEST_DB_DEADLINE`] instead of failing instantly (the pre-0.14.2
//!   `-32001` burst) or blocking forever. Poison is terminal
//!   ([`BexProject::mark_broken`]), never treated as recoverable contention.
//!
//! Lock order: source gate → runtime state → caches. The `run_cfgs`
//! cache is a leaf mutex: it may be taken while the source gate is held
//! ([`BexProject::prepare_function_run`]'s cached-graph fast path), and no
//! source or runtime lock is ever acquired while holding it.

use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex, MutexGuard, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use bex_engine::BexEngine;
use bex_external_types::Handle;
use sys_ops::SysOps;

use crate::RuntimeError;

// ---------------------------------------------------------------------------
// Source revision
// ---------------------------------------------------------------------------

/// Monotonic revision of the project's sources.
///
/// Advances exactly once per accepted source-mutation batch: open/change/
/// close, watched-file refresh, full replacement, file add/remove, and
/// playground edit. Captured into every candidate and compared by the
/// conditional engine commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SourceRevision(pub(crate) u64);

impl std::fmt::Display for SourceRevision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "r{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Bounded request wait configuration
// ---------------------------------------------------------------------------

/// Deadline for a request handler waiting on the source gate. Ordinary
/// rebuild holds (measured 156–302ms) fit comfortably; pathological
/// multi-second holds still get a finite escape.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const REQUEST_DB_DEADLINE: std::time::Duration = std::time::Duration::from_secs(1);

/// Poll interval while waiting on the source gate.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const REQUEST_DB_RETRY_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(2);

/// Waits longer than this are logged (observability for the containment
/// scaffolding; the whole wait mechanism is disposable once shared ingress
/// lands).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const REQUEST_DB_LOG_THRESHOLD: std::time::Duration =
    std::time::Duration::from_millis(50);

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// The project observed a poisoned lock: a writer panicked while holding
/// shared mutable state. The state cannot be trusted, so the project enters
/// a terminal broken state: requests are rejected with an internal
/// error, and no path "clears" the poison and continues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectBroken;

/// Typed outcome of a bounded database read on the request lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DbReadError {
    /// The gate stayed contended past the deadline. `revision_changed`
    /// reports whether the source revision advanced while waiting: `true`
    /// maps to `ContentModified` (the request's view genuinely became
    /// stale), `false` to `RequestFailed` (same-revision busy timeout).
    Busy { revision_changed: bool },
    /// Terminal poisoned/broken project.
    Broken,
}

// ---------------------------------------------------------------------------
// Owned candidates
// ---------------------------------------------------------------------------

/// Diagnostics for one file, captured with the exact open-document version
/// the text was checked at (`None` for closed/disk-only files).
#[derive(Debug, Clone)]
pub(crate) struct DocumentDiagnostics {
    pub path: std::path::PathBuf,
    pub version: Option<i32>,
    pub diagnostics: Vec<baml_compiler_diagnostics::Diagnostic>,
}

/// Owned, revision-tagged diagnostics for a whole project.
///
/// Spans stay byte-based (`baml_compiler_diagnostics::Diagnostic`); the LSP
/// boundary converts them with the negotiated position codec at publish
/// time. `file_texts` carries every source file's text so related-info
/// spans in other files can be converted without touching the database.
#[derive(Debug, Clone)]
pub(crate) struct DiagnosticCandidate {
    pub source_revision: SourceRevision,
    pub file_texts: HashMap<baml_base::FileId, (std::path::PathBuf, String)>,
    pub documents: Vec<DocumentDiagnostics>,
    pub has_errors: bool,
}

/// Owned compiled program tagged with the revision it was compiled from.
pub(crate) struct CompiledCandidate {
    pub source_revision: SourceRevision,
    pub program: bex_vm_types::Program,
}

/// Outcome of compiling the current sources into an owned candidate.
/// Invalid source is a first-class outcome — it carries the diagnostics
/// that must still publish, and requires no engine commit.
pub(crate) enum CompilationOutcome {
    /// Boxed: the compiled program dwarfs the other variants.
    Ready(Box<CompiledCandidate>, DiagnosticCandidate),
    BlockedByDiagnostics(DiagnosticCandidate),
    /// Bytecode emission failed without a user-visible diagnostic error
    /// (an internal compiler failure). Diagnostics still publish.
    EmitFailed {
        diagnostics: DiagnosticCandidate,
        message: String,
    },
}

/// An unpublished engine built from a [`CompiledCandidate`]. Its profiling
/// lifecycle is inactive; only a winning conditional commit activates it and
/// wraps it in the installed `Arc<BexEngine>`.
pub(crate) struct EngineCandidate {
    pub source_revision: SourceRevision,
    engine: BexEngine,
}

/// Proof of a winning conditional commit. The installed engine itself
/// lives in [`RuntimeState`]; consumers re-capture it transactionally.
#[derive(Clone)]
pub(crate) struct CommitReceipt {
    pub source_revision: SourceRevision,
    pub generation: u64,
}

/// Result of [`BexProject::commit_engine_if_current`].
pub(crate) enum CommitOutcome {
    Committed(CommitReceipt),
    /// The source revision advanced past the candidate; nothing changed.
    Superseded {
        current_revision: SourceRevision,
    },
}

/// End-to-end outcome of one rebuild attempt ([`BexProject::rebuild_once`]).
pub(crate) enum EngineBuildOutcome {
    Committed(CommitReceipt),
    BlockedByDiagnostics {
        source_revision: SourceRevision,
    },
    Failed {
        source_revision: SourceRevision,
        message: String,
    },
    Superseded {
        current_revision: SourceRevision,
    },
    Broken,
}

/// A rebuild attempt always yields diagnostics to publish (even for invalid
/// source or engine failure) plus the engine outcome.
pub(crate) struct RebuildReport {
    pub diagnostics: Option<DiagnosticCandidate>,
    pub engine: EngineBuildOutcome,
}

/// Collect an owned, revision-tagged diagnostics candidate from a held
/// source guard. Every source file is represented — files with zero
/// diagnostics get an empty entry so stale editor markers clear — and each
/// document records the exact open-document version its text was checked at.
pub(crate) fn collect_diagnostic_candidate(guard: &SourceGuard<'_>) -> DiagnosticCandidate {
    let db = guard.db();
    let source_files = db.get_source_files();

    let mut file_texts: HashMap<baml_base::FileId, (std::path::PathBuf, String)> = HashMap::new();
    let mut documents = Vec::new();
    let mut has_errors = false;

    // Check across worker threads (input order preserved, so per-file zip is
    // sound). This runs under the held source guard, so no mutation can race
    // the cloned worker handles; the guard is the same exclusion the serial
    // loop relied on.
    let per_file = baml_project::check_files_parallel(db, &source_files);
    for (file, diagnostics) in source_files.iter().zip(per_file) {
        let file_id = file.file_id(db);
        let Some(path) = db.file_id_to_path(file_id).cloned() else {
            continue;
        };
        let text = file.text(db).clone();
        file_texts.insert(file_id, (path.clone(), text));

        has_errors |= diagnostics
            .iter()
            .any(|d| d.severity == baml_compiler_diagnostics::Severity::Error);
        documents.push(DocumentDiagnostics {
            version: guard.document_version(&path),
            path,
            diagnostics,
        });
    }

    // Package-level diagnostics (cross-file name conflicts and namespace
    // shadows) come from `package_items`, not `check_file`, so the per-file
    // sweep above misses them: without this the candidate under-reports
    // errors relative to `get_bytecode`'s gate, and cross-file conflicts
    // never reach the editor at all. Bucket each onto its primary-span
    // file's document; only user-file spans count toward `has_errors`
    // (matching the gate's filter).
    let package_diags = baml_project::collect_package_level_diagnostics(db);
    if !package_diags.is_empty() {
        let doc_index: HashMap<std::path::PathBuf, usize> = documents
            .iter()
            .enumerate()
            .map(|(idx, doc)| (doc.path.clone(), idx))
            .collect();
        for diag in package_diags {
            let Some(span) = diag.primary_span() else {
                continue;
            };
            let Some((path, _)) = file_texts.get(&span.file_id) else {
                continue;
            };
            has_errors |= diag.severity == baml_compiler_diagnostics::Severity::Error;
            if let Some(&idx) = doc_index.get(path) {
                documents[idx].diagnostics.push(diag);
            }
        }
    }

    DiagnosticCandidate {
        source_revision: guard.revision(),
        file_texts,
        documents,
        has_errors,
    }
}

// ---------------------------------------------------------------------------
// Source state (the one mutation gate)
// ---------------------------------------------------------------------------

/// Database + revision + open-document versions behind one mutex.
struct SourceState {
    db: baml_project::ProjectDatabase,
    revision: SourceRevision,
    /// LSP versions for currently open editor documents, keyed by the same
    /// path identity the database uses. Updated in the same transaction as
    /// the text they describe.
    open_documents: HashMap<std::path::PathBuf, i32>,
    /// Cold-open per-file cache seeds, loaded from disk at construction and
    /// retained so each full reload can re-evaluate them (whole-project-clean
    /// gated). Dropped to `None` on the first content edit — from then on the
    /// project is "live" and every query recomputes honestly. See
    /// [`crate::seed`].
    seed_source: Option<crate::seed::PerFileSeeds>,
    /// Whether per-file throw-facts / `callable_throws` seeds are currently
    /// installed in the database, so a full reload evicts before re-applying and
    /// an edit evicts before dropping them.
    per_file_seeds_active: bool,
}

impl SourceState {
    /// Evict any installed per-file seeds and clear the active flag. Called
    /// before re-applying seeds on a full reload and before dropping them on an
    /// edit — the seeds hide transitive throws / name-resolution edges, so they
    /// can never survive a content change soundly.
    fn deactivate_per_file_seeds(&mut self) {
        if self.per_file_seeds_active {
            crate::seed::evict_per_file_seeds(&mut self.db);
            self.per_file_seeds_active = false;
        }
    }
}

/// Read guard over the source gate. Exposes the database plus the revision
/// and document versions captured under the same lock, so callers can build
/// revision-tagged owned results without a second racy lookup.
pub(crate) struct SourceGuard<'a> {
    inner: MutexGuard<'a, SourceState>,
    #[cfg(not(target_arch = "wasm32"))]
    acquired_at: std::time::Instant,
    #[cfg(not(target_arch = "wasm32"))]
    label: &'static str,
}

impl SourceGuard<'_> {
    #[cfg(not(target_arch = "wasm32"))]
    fn new<'a>(inner: MutexGuard<'a, SourceState>, label: &'static str) -> SourceGuard<'a> {
        SourceGuard {
            inner,
            acquired_at: std::time::Instant::now(),
            label,
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn new<'a>(inner: MutexGuard<'a, SourceState>, _label: &'static str) -> SourceGuard<'a> {
        SourceGuard { inner }
    }

    pub(crate) fn db(&self) -> &baml_project::ProjectDatabase {
        &self.inner.db
    }

    pub(crate) fn revision(&self) -> SourceRevision {
        self.inner.revision
    }

    pub(crate) fn document_version(&self, path: &std::path::Path) -> Option<i32> {
        self.inner.open_documents.get(path).copied()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for SourceGuard<'_> {
    fn drop(&mut self) {
        let held = self.acquired_at.elapsed();
        if held >= REQUEST_DB_LOG_THRESHOLD {
            tracing::info!("source gate held {}ms [{}]", held.as_millis(), self.label);
        }
    }
}

/// One accepted source-mutation batch: database mutation, revision
/// advance, and open-document version updates commit atomically.
#[derive(Default)]
pub(crate) struct SourceBatch {
    /// `true` removes database files absent from `sources` (full refresh).
    pub replace_all: bool,
    pub sources: HashMap<crate::fs::FsPath, String>,
    /// Open-document version updates: `Some(v)` records the version of an
    /// open document, `None` marks the document closed (published
    /// diagnostics fall back to unversioned).
    pub versions: Vec<(crate::fs::FsPath, Option<i32>)>,
}

// ---------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------

/// The installed engine plus the identity it was built from.
#[derive(Clone)]
pub(crate) struct InstalledEngine {
    pub source_revision: SourceRevision,
    pub generation: u64,
    pub engine: Arc<BexEngine>,
}

/// The installed test registry plus the identity it was collected under.
struct InstalledRegistry {
    generation: u64,
    collection_epoch: u64,
    /// `Some` when the project has tests; `None` when collection completed
    /// and found none (`$init_test` absent). Distinguished from "not yet
    /// collected", which is `RuntimeState::registry == None`.
    handle: Option<Handle>,
    /// Serializes expansion mutations: the registry heap object has exactly
    /// one mutation owner.
    #[cfg(not(target_arch = "wasm32"))]
    expansion_gate: Arc<tokio::sync::Mutex<()>>,
}

/// Coherent runtime identity. All fields swap together under one lock.
struct RuntimeState {
    installed: Option<InstalledEngine>,
    /// Allocator for engine generations; only a winning commit consumes one.
    next_generation: u64,
    /// Epoch for project-derived work (collection/expansion). Bumped by
    /// every source mutation and every commit.
    derived_epoch: u64,
    /// Cancels project-derived work when source moves or an engine commit
    /// supersedes it. Never cancels run-owned function/test tokens.
    derived_cancel: sys_types::CancellationToken,
    /// Fences test-collection installs so two collections on one engine
    /// generation cannot complete out of order.
    collection_epoch: u64,
    registry: Option<InstalledRegistry>,
}

impl RuntimeState {
    fn new() -> Self {
        Self {
            installed: None,
            next_generation: 1,
            derived_epoch: 0,
            derived_cancel: sys_types::CancellationToken::new(),
            collection_epoch: 0,
            registry: None,
        }
    }

    /// Cancel derived work and hand out a fresh token.
    fn supersede_derived(&mut self) {
        self.derived_cancel.cancel();
        self.derived_cancel = sys_types::CancellationToken::new();
        self.derived_epoch += 1;
    }
}

/// Ticket for one test-collection attempt, captured atomically.
pub(crate) struct CollectionTicket {
    pub generation: u64,
    pub collection_epoch: u64,
    pub engine: Arc<BexEngine>,
    pub cancel: sys_types::CancellationToken,
}

/// Coherent lease for test-registry work (test run / expansion), captured in
/// one source→runtime transaction.
pub(crate) struct RegistryLease {
    pub generation: u64,
    /// Epoch the leased registry was collected under. Emission fencing
    /// compares this too: a same-generation re-collection replaces the
    /// registry object, and results computed against the old one are stale.
    pub collection_epoch: u64,
    pub engine: Arc<BexEngine>,
    pub handle: Handle,
    pub cancel: sys_types::CancellationToken,
    /// One mutation owner per installed registry (native only; WASM is
    /// single-threaded and keeps its historical unserialized behavior).
    #[cfg(not(target_arch = "wasm32"))]
    pub expansion_gate: Arc<tokio::sync::Mutex<()>>,
}

/// Why a registry lease could not be produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegistryLeaseError {
    /// Requested generation is not the installed one, or the installed
    /// engine no longer matches current source. Runs never silently use a
    /// last-known-good engine.
    NeedsCurrentBuild,
    /// Collection has not produced a registry for this generation yet.
    NoRegistry,
    /// Collection completed and the project has no tests.
    NoTests,
    Broken,
}

/// Coherent snapshot for launching a function run. The overlay
/// control-flow graph is pinned into the generation-keyed cache as part of
/// preparation; runs resolve it later via
/// [`BexProject::control_flow_graph_for_generation`].
pub(crate) struct RunSnapshot {
    pub generation: u64,
    pub engine: Arc<BexEngine>,
}

/// Why a run snapshot could not be produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrepareRunError {
    /// No engine, or the installed engine predates current source.
    NeedsCurrentBuild,
    Busy,
    Broken,
}

// ---------------------------------------------------------------------------
// Run CFG cache
// ---------------------------------------------------------------------------

/// Prepared control-flow graphs pinned for playground runs, keyed by
/// `(engine generation, function name)`.
///
/// A run captures the engine generation at launch and resolves its graph
/// overlay against that generation later, possibly after several recompiles.
/// Graphs are built lazily for the function a run actually executes instead
/// of eagerly for every function on every compile — fully-inlined graphs
/// grow with call-site fan-out, so eager per-compile snapshots dominated LSP
/// memory.
const RETAINED_RUN_CFGS: usize = 64;

#[derive(Default)]
struct RunCfgCache {
    order: VecDeque<(u64, String)>,
    graphs: HashMap<
        (u64, String),
        std::sync::Arc<baml_compiler2_visualization::control_flow::ControlFlowGraph>,
    >,
}

impl RunCfgCache {
    fn insert(
        &mut self,
        generation: u64,
        function_name: &str,
        graph: std::sync::Arc<baml_compiler2_visualization::control_flow::ControlFlowGraph>,
    ) {
        let key = (generation, function_name.to_string());
        if self.graphs.insert(key.clone(), graph).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > RETAINED_RUN_CFGS {
            if let Some(evicted) = self.order.pop_front() {
                self.graphs.remove(&evicted);
            }
        }
    }

    fn graph(
        &self,
        generation: u64,
        function_name: &str,
    ) -> Option<std::sync::Arc<baml_compiler2_visualization::control_flow::ControlFlowGraph>> {
        self.graphs
            .get(&(generation, function_name.to_string()))
            .cloned()
    }
}

// ---------------------------------------------------------------------------
// BexProject
// ---------------------------------------------------------------------------

pub(crate) struct BexProject {
    /// The source gate: database + revision + open-document versions.
    source: Mutex<SourceState>,
    /// Lock-free mirror of `source.revision`, updated inside the mutation
    /// critical section. Observation only (timeout error mapping); never
    /// authorizes commits or publication.
    observed_revision: AtomicU64,
    /// Terminal broken marker (poisoned lock observed). Set once; never
    /// cleared; clearing mutex poison is not recovery.
    broken: OnceLock<String>,
    sys_ops: Arc<SysOps>,
    runtime: Mutex<RuntimeState>,
    run_cfgs: Mutex<RunCfgCache>,
}

impl BexProject {
    pub(crate) fn new(root_path: &vfs::VfsPath, sys_ops: Arc<SysOps>) -> Self {
        let mut db = baml_project::ProjectDatabase::new();
        let root = crate::fs::FsPath::from_vfs(root_path);
        db.set_project_root(root.as_path());

        // Cold-open cache seeding: load the CLI-written blobs now, at
        // construction — all disk I/O happens here, before the database goes
        // behind the source gate (never inside a request). The content-
        // independent stdlib interface is installed immediately; the per-file
        // seeds are retained for the first source population (see
        // `mutate_sources`). Any absence/corruption/opt-out yields no seeds and
        // today's cold build. See [`crate::seed`].
        let seed_source = crate::seed::LspSeedCache::load_for_root(root.as_path())
            .and_then(|seed| seed.install_stdlib(&mut db));

        Self {
            source: Mutex::new(SourceState {
                db,
                revision: SourceRevision(0),
                open_documents: HashMap::new(),
                seed_source,
                per_file_seeds_active: false,
            }),
            observed_revision: AtomicU64::new(0),
            broken: OnceLock::new(),
            sys_ops,
            runtime: Mutex::new(RuntimeState::new()),
            run_cfgs: Mutex::new(RunCfgCache::default()),
        }
    }

    // ── Broken-state handling ────────────────────────────────────────────

    /// Record the terminal broken state (first reason wins) and log it.
    fn mark_broken(&self, reason: &str) -> ProjectBroken {
        if self.broken.set(reason.to_string()).is_ok() {
            log::error!("project entered terminal broken state: {reason}");
        }
        ProjectBroken
    }

    // ── Lock lanes ───────────────────────────────────────────────────────

    /// Writer lane: blocking acquisition of the source gate. Used by source
    /// mutations and background candidate builds (never on a path that must
    /// stay responsive under contention).
    fn lock_source_blocking(&self) -> Result<MutexGuard<'_, SourceState>, ProjectBroken> {
        if self.broken.get().is_some() {
            return Err(ProjectBroken);
        }
        match self.source.lock() {
            Ok(guard) => Ok(guard),
            Err(_) => Err(self.mark_broken("source gate poisoned by a panicked writer")),
        }
    }

    fn lock_runtime(&self) -> Result<MutexGuard<'_, RuntimeState>, ProjectBroken> {
        if self.broken.get().is_some() {
            return Err(ProjectBroken);
        }
        match self.runtime.lock() {
            Ok(guard) => Ok(guard),
            Err(_) => Err(self.mark_broken("runtime state poisoned by a panicked writer")),
        }
    }

    /// Request lane: bounded wait on the source gate — 1s
    /// deadline, 2ms retry interval, 50ms wait-log threshold. On WASM the
    /// server is single-threaded, so this is a single non-waiting attempt.
    pub(crate) fn read_source_for_request(&self) -> Result<SourceGuard<'_>, DbReadError> {
        if self.broken.get().is_some() {
            return Err(DbReadError::Broken);
        }

        #[cfg(target_arch = "wasm32")]
        {
            match self.source.try_lock() {
                Ok(inner) => Ok(SourceGuard::new(inner, "request")),
                Err(std::sync::TryLockError::WouldBlock) => Err(DbReadError::Busy {
                    revision_changed: false,
                }),
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    self.mark_broken("source gate poisoned by a panicked writer");
                    Err(DbReadError::Broken)
                }
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let revision_at_start = self.current_revision();
            let started = std::time::Instant::now();
            loop {
                match self.source.try_lock() {
                    Ok(inner) => {
                        let waited = started.elapsed();
                        if waited >= REQUEST_DB_LOG_THRESHOLD {
                            tracing::info!(
                                "request waited {}ms for the source gate",
                                waited.as_millis()
                            );
                        }
                        return Ok(SourceGuard::new(inner, "request"));
                    }
                    Err(std::sync::TryLockError::WouldBlock) => {
                        if started.elapsed() >= REQUEST_DB_DEADLINE {
                            let busy = self.classify_request_timeout(revision_at_start);
                            log::warn!(
                                "request timed out after {}ms waiting for the source gate \
                                 ({busy:?})",
                                started.elapsed().as_millis()
                            );
                            return Err(busy);
                        }
                        std::thread::sleep(REQUEST_DB_RETRY_INTERVAL);
                    }
                    Err(std::sync::TryLockError::Poisoned(_)) => {
                        self.mark_broken("source gate poisoned by a panicked writer");
                        return Err(DbReadError::Broken);
                    }
                }
            }
        }
    }

    /// Classify a request-lane timeout: a revision that moved
    /// while the request waited means the result would have been stale
    /// anyway (`ContentModified` at the LSP boundary); an unchanged revision
    /// is plain congestion (`RequestFailed`).
    #[cfg(not(target_arch = "wasm32"))]
    fn classify_request_timeout(&self, revision_at_start: SourceRevision) -> DbReadError {
        DbReadError::Busy {
            revision_changed: self.current_revision() != revision_at_start,
        }
    }

    /// Loop lane: one non-waiting attempt. `Ok(None)` means "busy, skip me"
    /// (only `WouldBlock`); poison is terminal and never conflated with busy.
    pub(crate) fn read_source_nowait(&self) -> Result<Option<SourceGuard<'_>>, ProjectBroken> {
        if self.broken.get().is_some() {
            return Err(ProjectBroken);
        }
        match self.source.try_lock() {
            Ok(inner) => Ok(Some(SourceGuard::new(inner, "nowait"))),
            Err(std::sync::TryLockError::WouldBlock) => Ok(None),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                Err(self.mark_broken("source gate poisoned by a panicked writer"))
            }
        }
    }

    // ── Source mutation ──────────────────────────────────────────────────

    /// Apply one source-mutation batch. Database mutation, revision advance,
    /// and document-version updates happen under the source gate; runtime
    /// invalidation (derived-work cancellation, epoch bumps) happens in the
    /// same transaction before the gate is released.
    pub(crate) fn mutate_sources(
        &self,
        batch: SourceBatch,
    ) -> Result<SourceRevision, ProjectBroken> {
        #[cfg(not(target_arch = "wasm32"))]
        let mutate_started = std::time::Instant::now();
        let mut source = self.lock_source_blocking()?;

        if batch.replace_all {
            let mut existing_paths: std::collections::HashSet<_> =
                source.db.non_builtin_file_paths().collect();
            for (path, text) in &batch.sources {
                source.db.add_or_update_file(path.as_path(), text);
                existing_paths.remove(path.as_path());
            }
            for path in existing_paths {
                source.db.remove_file(&path);
            }
        } else {
            for (path, text) in &batch.sources {
                source.db.add_or_update_file(path.as_path(), text);
            }
        }

        for (path, version) in batch.versions {
            let key = path.as_path().to_path_buf();
            match version {
                Some(v) => {
                    source.open_documents.insert(key, v);
                }
                None => {
                    source.open_documents.remove(&key);
                }
            }
        }

        // Cold-open cache seeding / eviction (see [`crate::seed`]). No I/O runs
        // here — the blobs were decoded at construction — so the source gate is
        // held no longer than the plain batch apply already required.
        //
        // A full reload (`replace_all`: discovery, didOpen, watched-file refresh)
        // re-evaluates the per-file seeds against the freshly-loaded content,
        // whole-project-clean gated, so they survive the discovery→didOpen
        // reload pair and serve the first diagnostics. A content edit
        // (didChange/didClose) evicts them and drops the seed source — the
        // project is now "live" and every query must recompute honestly (the
        // seeds hide transitive throws / name-resolution edges, so no per-file
        // eviction could keep them sound past an edit).
        if batch.replace_all {
            if let Some(seeds) = source.seed_source.take() {
                source.deactivate_per_file_seeds();
                if let Some(root) = source.db.get_project().map(|p| p.root(&source.db)) {
                    source.per_file_seeds_active = seeds.apply(&mut source.db, &root);
                }
                source.seed_source = Some(seeds);
            }
        } else if source.per_file_seeds_active || source.seed_source.is_some() {
            source.deactivate_per_file_seeds();
            source.seed_source = None;
        }

        source.revision = SourceRevision(source.revision.0 + 1);
        let revision = source.revision;
        self.observed_revision.store(revision.0, Ordering::Release);

        // Same-transaction runtime invalidation: derived work for the
        // previous engine state is superseded immediately; the installed
        // engine becomes non-current by revision comparison. Run-owned
        // function/test tokens are untouched.
        {
            let mut runtime = self.lock_runtime()?;
            runtime.supersede_derived();
            runtime.collection_epoch += 1;
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let held = mutate_started.elapsed();
            if held >= REQUEST_DB_LOG_THRESHOLD {
                tracing::info!("source gate held {}ms [mutate_sources]", held.as_millis());
            }
        }

        Ok(revision)
    }

    /// Latest committed source revision, readable without any lock.
    pub(crate) fn current_revision(&self) -> SourceRevision {
        SourceRevision(self.observed_revision.load(Ordering::Acquire))
    }

    // ── Candidate construction ───────────────────────────────────────────

    /// Capture the current revision and compile an owned
    /// [`CompilationOutcome`]. The source gate is held only while reading
    /// the database; the returned outcome is fully detached.
    pub(crate) fn compile_outcome(&self) -> Result<CompilationOutcome, ProjectBroken> {
        let guard = SourceGuard::new(self.lock_source_blocking()?, "compile_outcome");
        Ok(Self::compile_outcome_locked(&guard))
    }

    fn compile_outcome_locked(guard: &SourceGuard<'_>) -> CompilationOutcome {
        let diagnostics = collect_diagnostic_candidate(guard);
        if diagnostics.has_errors {
            return CompilationOutcome::BlockedByDiagnostics(diagnostics);
        }
        // The candidate above is a full-project check (per-file sweep plus
        // package-level diagnostics), so `get_bytecode`'s error gate would
        // re-derive exactly what `has_errors` just proved — skip it.
        match guard.db().get_bytecode_unchecked() {
            Ok(program) => CompilationOutcome::Ready(
                Box::new(CompiledCandidate {
                    source_revision: guard.revision(),
                    program,
                }),
                diagnostics,
            ),
            Err(e) => CompilationOutcome::EmitFailed {
                diagnostics,
                message: e.to_string(),
            },
        }
    }

    /// Compute an owned diagnostics candidate without waiting (fence lane).
    /// `Ok(None)` is `Busy` — the caller retains its last publication and
    /// schedules a trailing retry. Only the native debounced tail computes
    /// diagnostics off-thread; the WASM tail rides `rebuild_once`.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn diagnostic_candidate_nowait(
        &self,
    ) -> Result<Option<DiagnosticCandidate>, ProjectBroken> {
        let Some(guard) = self.read_source_nowait()? else {
            return Ok(None);
        };
        Ok(Some(collect_diagnostic_candidate(&guard)))
    }

    /// Construct an engine candidate from a compiled program. Runs `$init`
    /// synchronously and candidate-locally, outside the source gate; the
    /// profiling lifecycle stays inactive until the candidate wins a commit.
    pub(crate) fn construct_engine_candidate(
        &self,
        compiled: CompiledCandidate,
    ) -> Result<EngineCandidate, RuntimeError> {
        let engine = BexEngine::new_with_deferred_profiling_and_runtime_compiler(
            compiled.program,
            self.sys_ops.clone(),
            Vec::new(),
            Some(crate::runtime_compiler()),
        )
        .map_err(RuntimeError::Engine)?;
        engine.set_unhandled_spawn_error_handler(Some(Arc::new(|error| {
            let cancelled = error.cancelled;
            let error = error.into_engine_error();
            if cancelled {
                log::warn!("cancelled spawned task failed: {error}");
            } else {
                log::error!("unhandled spawned task failed: {error}");
            }
        })));
        Ok(EngineCandidate {
            source_revision: compiled.source_revision,
            engine,
        })
    }

    // ── Conditional commit ───────────────────────────────────────────────

    /// Atomically install `candidate` iff its revision still matches the
    /// authoritative source revision. Uses the same gate as source
    /// mutations, so commit and mutation are linearized.
    ///
    /// A superseded candidate changes nothing: no generation, no derived
    /// cancellation, no registry/CFG/publication state, no profiling
    /// metadata (the candidate drops quietly).
    pub(crate) fn commit_engine_if_current(
        &self,
        candidate: EngineCandidate,
    ) -> Result<CommitOutcome, ProjectBroken> {
        self.commit_engine_if_current_with_hook(candidate, || {})
    }

    /// Internal seam for proving the commit/source-mutation serialization
    /// boundary. The hook runs after the authoritative source gate is held
    /// but before the revision comparison; production callers use the no-op
    /// wrapper above, tests use the hook to place an edit exactly against
    /// the conditional commit without sleeps or scheduler timing
    /// assumptions.
    fn commit_engine_if_current_with_hook(
        &self,
        candidate: EngineCandidate,
        after_source_lock: impl FnOnce(),
    ) -> Result<CommitOutcome, ProjectBroken> {
        let source = self.lock_source_blocking()?;
        after_source_lock();
        if source.revision != candidate.source_revision {
            return Ok(CommitOutcome::Superseded {
                current_revision: source.revision,
            });
        }

        // The revision comparison won: activate the candidate's profiling
        // lifecycle now — non-awaiting, before the engine becomes reachable.
        candidate.engine.activate_profiling();
        let engine = Arc::new(candidate.engine);

        let (receipt, retired_engine) = {
            let mut runtime = self.lock_runtime()?;
            let generation = runtime.next_generation;
            runtime.next_generation += 1;
            let retired_engine = runtime
                .installed
                .replace(InstalledEngine {
                    source_revision: candidate.source_revision,
                    generation,
                    engine,
                })
                .map(|installed| installed.engine);
            // Derived work bound to the previous engine is superseded; the
            // registry is cleared atomically with the engine swap.
            runtime.supersede_derived();
            runtime.collection_epoch += 1;
            runtime.registry = None;
            (
                CommitReceipt {
                    source_revision: candidate.source_revision,
                    generation,
                },
                retired_engine,
            )
        };
        drop(source);
        if let Some(engine) = retired_engine {
            crate::BackgroundSpawner::new().spawn(async move {
                engine.shutdown().await;
            });
        }
        Ok(CommitOutcome::Committed(receipt))
    }

    /// One full rebuild attempt: compile owned outcome → drop the guard →
    /// construct the engine candidate (`$init`) → conditional commit.
    /// Always returns the diagnostics candidate for fence publication.
    pub(crate) fn rebuild_once(&self) -> RebuildReport {
        let outcome = match self.compile_outcome() {
            Ok(outcome) => outcome,
            Err(ProjectBroken) => {
                return RebuildReport {
                    diagnostics: None,
                    engine: EngineBuildOutcome::Broken,
                };
            }
        };

        match outcome {
            CompilationOutcome::BlockedByDiagnostics(diagnostics) => {
                let source_revision = diagnostics.source_revision;
                RebuildReport {
                    diagnostics: Some(diagnostics),
                    engine: EngineBuildOutcome::BlockedByDiagnostics { source_revision },
                }
            }
            CompilationOutcome::EmitFailed {
                diagnostics,
                message,
            } => {
                let source_revision = diagnostics.source_revision;
                log::warn!("rebuild: bytecode emission failed at {source_revision}: {message}");
                RebuildReport {
                    diagnostics: Some(diagnostics),
                    engine: EngineBuildOutcome::Failed {
                        source_revision,
                        message,
                    },
                }
            }
            CompilationOutcome::Ready(compiled, diagnostics) => {
                let source_revision = compiled.source_revision;
                let engine = match self.construct_engine_candidate(*compiled) {
                    Ok(candidate) => match self.commit_engine_if_current(candidate) {
                        Ok(CommitOutcome::Committed(receipt)) => {
                            log::info!(
                                "rebuild: committed engine generation {} at {source_revision}",
                                receipt.generation
                            );
                            EngineBuildOutcome::Committed(receipt)
                        }
                        Ok(CommitOutcome::Superseded { current_revision }) => {
                            log::info!(
                                "rebuild: candidate at {source_revision} superseded by \
                                 {current_revision}; dropping quietly"
                            );
                            EngineBuildOutcome::Superseded { current_revision }
                        }
                        Err(ProjectBroken) => EngineBuildOutcome::Broken,
                    },
                    Err(e) => {
                        log::warn!("rebuild: engine construction failed: {e}");
                        EngineBuildOutcome::Failed {
                            source_revision,
                            message: e.to_string(),
                        }
                    }
                };
                RebuildReport {
                    diagnostics: Some(diagnostics),
                    engine,
                }
            }
        }
    }

    // ── Status / compat accessors ────────────────────────────────────────

    /// Derived currentness: `installed.source_revision == current
    /// revision`, read under both gates. For reporting; work capture uses
    /// the transactional APIs below.
    pub(crate) fn is_bex_current(&self) -> bool {
        let Ok(source) = self.lock_source_blocking() else {
            return false;
        };
        let source_revision = source.revision;
        let Ok(runtime) = self.lock_runtime() else {
            return false;
        };
        runtime
            .installed
            .as_ref()
            .is_some_and(|i| i.source_revision == source_revision)
    }

    /// Generation of the installed engine (0 when none) — playground compat.
    pub(crate) fn current_generation(&self) -> u64 {
        self.lock_runtime()
            .ok()
            .and_then(|rt| rt.installed.as_ref().map(|i| i.generation))
            .unwrap_or(0)
    }

    /// Runtime status for a source revision already captured under a caller's
    /// source lease. This avoids re-locking the source gate while that lease
    /// is held.
    pub(crate) fn runtime_status_for_source(&self, source_revision: SourceRevision) -> (bool, u64) {
        let Ok(runtime) = self.lock_runtime() else {
            return (false, 0);
        };
        let Some(installed) = runtime.installed.as_ref() else {
            return (false, 0);
        };
        (
            installed.source_revision == source_revision,
            installed.generation,
        )
    }

    /// Latest installed engine regardless of currentness (playground render
    /// paths that tolerate a stale engine; run launch uses
    /// [`Self::prepare_function_run`] instead).
    pub(crate) fn get_bex(&self) -> Result<Arc<BexEngine>, RuntimeError> {
        let runtime = self
            .lock_runtime()
            .map_err(|ProjectBroken| RuntimeError::Compilation {
                message: "project is in a broken state".to_string(),
            })?;
        runtime
            .installed
            .as_ref()
            .map(|i| i.engine.clone())
            .ok_or(RuntimeError::Compilation {
                message: "No bex has been created yet".to_string(),
            })
    }

    /// Consume the project, returning the engine iff it matches current
    /// source (embedding API used by [`crate::new`]).
    pub(crate) fn take(self) -> Result<Arc<BexEngine>, RuntimeError> {
        if !self.is_bex_current() {
            return Err(RuntimeError::Compilation {
                message: "Bex is outdated".to_string(),
            });
        }
        self.get_bex()
    }

    /// Full replacement + synchronous rebuild (embedding API and the WASM
    /// LSP path, which has no background scheduler).
    pub(crate) fn update_all_sources(&self, sources: &HashMap<crate::fs::FsPath, String>) {
        let batch = SourceBatch {
            replace_all: true,
            sources: sources.clone(),
            versions: Vec::new(),
        };
        if self.mutate_sources(batch).is_err() {
            return;
        }
        let _ = self.rebuild_once();
    }

    // ── Test collection ──────────────────────────────────────────────────

    /// Begin a test-collection attempt against the installed engine.
    /// Returns `None` when there is no current engine to collect against.
    /// Captures engine, generation, fresh derived token, and a new
    /// collection epoch atomically, superseding any in-flight collection.
    pub(crate) fn begin_test_collection(&self) -> Result<Option<CollectionTicket>, ProjectBroken> {
        let source = self.lock_source_blocking()?;
        let source_revision = source.revision;
        let mut runtime = self.lock_runtime()?;
        let Some(installed) = runtime.installed.clone() else {
            return Ok(None);
        };
        if installed.source_revision != source_revision {
            // Stale engine: collection would be derived work for a dead
            // revision; source changes cancel stale tree maintenance.
            return Ok(None);
        }
        runtime.supersede_derived();
        runtime.collection_epoch += 1;
        let ticket = CollectionTicket {
            generation: installed.generation,
            collection_epoch: runtime.collection_epoch,
            engine: installed.engine,
            cancel: runtime.derived_cancel.clone(),
        };
        drop(runtime);
        drop(source);
        Ok(Some(ticket))
    }

    /// Install a collected registry iff the ticket still matches the
    /// installed generation and collection epoch. Returns `false` (emit
    /// nothing) for stale results — the ABA fence.
    pub(crate) fn install_collected_registry(
        &self,
        ticket: &CollectionTicket,
        handle: Option<Handle>,
    ) -> Result<bool, ProjectBroken> {
        let mut runtime = self.lock_runtime()?;
        let matches = runtime
            .installed
            .as_ref()
            .is_some_and(|i| i.generation == ticket.generation)
            && runtime.collection_epoch == ticket.collection_epoch;
        if !matches {
            return Ok(false);
        }
        runtime.registry = Some(InstalledRegistry {
            generation: ticket.generation,
            collection_epoch: ticket.collection_epoch,
            handle,
            #[cfg(not(target_arch = "wasm32"))]
            expansion_gate: Arc::new(tokio::sync::Mutex::new(())),
        });
        Ok(true)
    }

    /// Emission fence for collection results: `true` while the
    /// ticket's engine generation and collection epoch are still installed.
    pub(crate) fn collection_ticket_is_current(&self, ticket: &CollectionTicket) -> bool {
        let Ok(runtime) = self.lock_runtime() else {
            return false;
        };
        runtime
            .installed
            .as_ref()
            .is_some_and(|i| i.generation == ticket.generation)
            && runtime.collection_epoch == ticket.collection_epoch
    }

    /// Emission fence for expansion results: `true` while the lease's
    /// engine generation is still installed *and* the leased registry object
    /// is still the installed one (a same-generation re-collection replaces
    /// the registry, making results against the old object stale).
    pub(crate) fn registry_lease_is_current(&self, lease: &RegistryLease) -> bool {
        let Ok(runtime) = self.lock_runtime() else {
            return false;
        };
        runtime
            .installed
            .as_ref()
            .is_some_and(|i| i.generation == lease.generation)
            && runtime.registry.as_ref().is_some_and(|r| {
                r.generation == lease.generation && r.collection_epoch == lease.collection_epoch
            })
    }

    /// The installed engine iff its generation matches (no currency
    /// requirement), used to target cancellation for already-launched runs.
    /// Runs pin their own engine handle at launch; this covers adapters that
    /// only kept the generation.
    pub(crate) fn engine_for_generation(&self, generation: u64) -> Option<Arc<BexEngine>> {
        let runtime = self.lock_runtime().ok()?;
        runtime
            .installed
            .as_ref()
            .filter(|i| i.generation == generation)
            .map(|i| i.engine.clone())
    }

    /// Coherent lease for registry work (test run / expansion): validates
    /// `generation` against the installed engine, requires the engine to
    /// match current source, and captures the registry handle plus its
    /// expansion gate in one transaction.
    pub(crate) fn lease_registry(
        &self,
        generation: u64,
    ) -> Result<RegistryLease, RegistryLeaseError> {
        let source = match self.lock_source_blocking() {
            Ok(g) => g,
            Err(ProjectBroken) => return Err(RegistryLeaseError::Broken),
        };
        let source_revision = source.revision;
        let runtime = match self.lock_runtime() {
            Ok(g) => g,
            Err(ProjectBroken) => return Err(RegistryLeaseError::Broken),
        };
        let Some(installed) = runtime.installed.as_ref() else {
            return Err(RegistryLeaseError::NeedsCurrentBuild);
        };
        if installed.generation != generation || installed.source_revision != source_revision {
            return Err(RegistryLeaseError::NeedsCurrentBuild);
        }
        let Some(registry) = runtime.registry.as_ref() else {
            return Err(RegistryLeaseError::NoRegistry);
        };
        debug_assert_eq!(registry.generation, generation);
        let Some(handle) = registry.handle.clone() else {
            return Err(RegistryLeaseError::NoTests);
        };
        let lease = RegistryLease {
            generation,
            collection_epoch: registry.collection_epoch,
            engine: installed.engine.clone(),
            handle,
            cancel: runtime.derived_cancel.clone(),
            #[cfg(not(target_arch = "wasm32"))]
            expansion_gate: registry.expansion_gate.clone(),
        };
        drop(runtime);
        drop(source);
        Ok(lease)
    }

    // ── Run preparation ──────────────────────────────────────────────────

    /// Prepare a function run: one transaction that validates the installed
    /// engine against current source, captures the engine + generation,
    /// and — when `overlay_function` is set — obtains the pinned
    /// control-flow graph for that generation.
    ///
    /// Graph building holds only the serialized source gate (never runtime
    /// or active-run locks); the cached-graph fast path briefly takes the
    /// leaf `run_cfgs` mutex while the source gate is held, and the
    /// generation-keyed CFG cache is populated only after all guards are
    /// released.
    pub(crate) fn prepare_function_run(
        &self,
        overlay_function: Option<&str>,
    ) -> Result<RunSnapshot, PrepareRunError> {
        // Fast path: cached graph + coherent validation without building.
        let cached_graph = |generation: u64| -> Option<
            std::sync::Arc<baml_compiler2_visualization::control_flow::ControlFlowGraph>,
        > {
            let name = overlay_function?;
            self.run_cfgs.lock().ok()?.graph(generation, name)
        };

        let source = match self.read_source_for_request() {
            Ok(g) => g,
            Err(DbReadError::Busy { .. }) => return Err(PrepareRunError::Busy),
            Err(DbReadError::Broken) => return Err(PrepareRunError::Broken),
        };
        let source_revision = source.revision();
        let installed = {
            let runtime = match self.lock_runtime() {
                Ok(g) => g,
                Err(ProjectBroken) => return Err(PrepareRunError::Broken),
            };
            let Some(installed) = runtime.installed.clone() else {
                return Err(PrepareRunError::NeedsCurrentBuild);
            };
            installed
        };
        if installed.source_revision != source_revision {
            return Err(PrepareRunError::NeedsCurrentBuild);
        }

        // While the source gate is held, commits cannot interleave, so a
        // graph built here reflects exactly `installed`'s revision.
        let mut built_graph = None;
        if let Some(name) = overlay_function {
            if cached_graph(installed.generation).is_none() {
                built_graph = source.db().ast_control_flow_graph(name).map(|g| {
                    (
                        installed.generation,
                        name.to_string(),
                        std::sync::Arc::new(
                            baml_compiler2_visualization::control_flow::prepare_control_flow_graph_for_visualization(&g),
                        ),
                    )
                });
            }
        }
        drop(source);

        // Populate the generation-keyed cache only after the source/runtime
        // guards are released.
        if let Some((generation, name, graph)) = built_graph {
            if let Ok(mut cache) = self.run_cfgs.lock() {
                cache.insert(generation, &name, graph);
            }
        }

        Ok(RunSnapshot {
            generation: installed.generation,
            engine: installed.engine,
        })
    }

    /// Return the prepared control-flow graph for `function_name` as of the
    /// given engine generation.
    ///
    /// A cache hit serves any generation still retained; a miss can only be
    /// built while `generation` is still the installed, current engine (the
    /// database has moved on otherwise). Active-run overlays resolve from
    /// graphs pinned at launch by [`Self::prepare_function_run`]; this is
    /// the fallback for overlays whose run outlived the cache.
    pub(crate) fn control_flow_graph_for_generation(
        &self,
        generation: u64,
        function_name: &str,
    ) -> Option<std::sync::Arc<baml_compiler2_visualization::control_flow::ControlFlowGraph>> {
        if let Ok(cache) = self.run_cfgs.lock() {
            if let Some(graph) = cache.graph(generation, function_name) {
                return Some(graph);
            }
        }

        let source = self.read_source_for_request().ok()?;
        {
            let runtime = self.lock_runtime().ok()?;
            let installed = runtime.installed.as_ref()?;
            if installed.generation != generation || installed.source_revision != source.revision()
            {
                return None;
            }
        }
        let graph = source.db().ast_control_flow_graph(function_name)?;
        let graph = std::sync::Arc::new(
            baml_compiler2_visualization::control_flow::prepare_control_flow_graph_for_visualization(
                &graph,
            ),
        );
        drop(source);

        if let Ok(mut cache) = self.run_cfgs.lock() {
            cache.insert(generation, function_name, graph.clone());
        }
        Some(graph)
    }
}

#[cfg(test)]
mod tests {
    use sys_native::SysOpsExt as _;

    use super::*;

    #[test]
    fn run_cfg_cache_evicts_oldest_entries() {
        let mut cache = RunCfgCache::default();
        for generation in 1..=(RETAINED_RUN_CFGS as u64 + 1) {
            cache.insert(
                generation,
                "Workflow",
                std::sync::Arc::new(
                    baml_compiler2_visualization::control_flow::ControlFlowGraph::default(),
                ),
            );
        }

        assert!(cache.graph(1, "Workflow").is_none());
        assert!(
            cache
                .graph(RETAINED_RUN_CFGS as u64 + 1, "Workflow")
                .is_some()
        );
    }

    #[test]
    fn run_cfg_cache_reinsert_does_not_duplicate_order() {
        let mut cache = RunCfgCache::default();
        let graph = || {
            std::sync::Arc::new(
                baml_compiler2_visualization::control_flow::ControlFlowGraph::default(),
            )
        };
        for _ in 0..(RETAINED_RUN_CFGS * 2) {
            cache.insert(7, "Workflow", graph());
        }
        cache.insert(8, "Workflow", graph());
        assert!(cache.graph(7, "Workflow").is_some());
        assert!(cache.graph(8, "Workflow").is_some());
        assert_eq!(cache.order.len(), 2);
    }

    // ── Revision / commit-race tests ─────────────────────────────────────

    fn test_project() -> BexProject {
        let root = vfs::VfsPath::new(vfs::MemoryFS::new());
        BexProject::new(&root, Arc::new(sys_ops::SysOps::native()))
    }

    fn batch(files: &[(&str, &str)], versions: &[(&str, Option<i32>)]) -> SourceBatch {
        SourceBatch {
            replace_all: false,
            sources: files
                .iter()
                .map(|(p, t)| (crate::fs::FsPath::from_str(p.to_string()), (*t).to_string()))
                .collect(),
            versions: versions
                .iter()
                .map(|(p, v)| (crate::fs::FsPath::from_str(p.to_string()), *v))
                .collect(),
        }
    }

    const VALID_SOURCE: &str = "function main() -> int {\n    1\n}\n";

    #[test]
    fn mutation_advances_revision_and_tracks_versions_atomically() {
        let project = test_project();

        let r1 = project
            .mutate_sources(batch(
                &[("/p/a.baml", VALID_SOURCE)],
                &[("/p/a.baml", Some(3))],
            ))
            .unwrap();
        assert_eq!(r1, SourceRevision(1));

        {
            let guard = project.read_source_nowait().unwrap().unwrap();
            assert_eq!(guard.revision(), r1);
            assert_eq!(
                guard.document_version(std::path::Path::new("/p/a.baml")),
                Some(3)
            );
        }

        // Closing drops the version in the same batch that advances the
        // revision (didClose semantics).
        let r2 = project
            .mutate_sources(batch(&[], &[("/p/a.baml", None)]))
            .unwrap();
        assert_eq!(r2, SourceRevision(2));
        let guard = project.read_source_nowait().unwrap().unwrap();
        assert_eq!(
            guard.document_version(std::path::Path::new("/p/a.baml")),
            None
        );
    }

    #[test]
    fn superseded_candidate_commits_nothing_and_current_candidate_wins() {
        let project = test_project();
        project
            .mutate_sources(batch(&[("/p/a.baml", VALID_SOURCE)], &[]))
            .unwrap();

        // Compile an owned candidate at r1…
        let CompilationOutcome::Ready(compiled, _) = project.compile_outcome().unwrap() else {
            panic!("trivial source should compile");
        };
        assert_eq!(compiled.source_revision, SourceRevision(1));

        // …then a mutation advances to r2 before the commit.
        project
            .mutate_sources(batch(
                &[("/p/a.baml", "function main() -> int {\n    2\n}\n")],
                &[],
            ))
            .unwrap();

        let candidate = project.construct_engine_candidate(*compiled).unwrap();
        match project.commit_engine_if_current(candidate).unwrap() {
            CommitOutcome::Superseded { current_revision } => {
                assert_eq!(current_revision, SourceRevision(2));
            }
            CommitOutcome::Committed(_) => panic!("stale candidate must not install"),
        }
        // Nothing changed: no engine, no generation, not current.
        assert_eq!(project.current_generation(), 0);
        assert!(!project.is_bex_current());
        assert!(project.get_bex().is_err());

        // A candidate built from the current revision commits.
        let CompilationOutcome::Ready(compiled, _) = project.compile_outcome().unwrap() else {
            panic!("trivial source should compile");
        };
        let candidate = project.construct_engine_candidate(*compiled).unwrap();
        match project.commit_engine_if_current(candidate).unwrap() {
            CommitOutcome::Committed(receipt) => {
                assert_eq!(receipt.source_revision, SourceRevision(2));
                assert_eq!(receipt.generation, 1);
            }
            CommitOutcome::Superseded { .. } => panic!("current candidate must install"),
        }
        assert!(project.is_bex_current());
        assert_eq!(project.current_generation(), 1);

        // The next mutation makes the engine non-current by definition
        // and the next winning commit consumes the next generation.
        project
            .mutate_sources(batch(&[("/p/a.baml", VALID_SOURCE)], &[]))
            .unwrap();
        assert!(!project.is_bex_current());
        let report = project.rebuild_once();
        match report.engine {
            EngineBuildOutcome::Committed(receipt) => assert_eq!(receipt.generation, 2),
            _ => panic!("rebuild of valid source should commit"),
        }
        assert!(project.is_bex_current());
    }

    /// An edit that lands while the commit holds the source gate is
    /// serialized after it: the commit installs its (immediately
    /// last-known-good) engine, and the queued edit makes it non-current.
    /// The rendezvous hook places the edit exactly at the commit boundary —
    /// no sleeps or scheduler timing assumptions.
    #[test]
    fn edit_at_commit_boundary_never_leaves_a_false_current_engine() {
        let project = test_project();
        project
            .mutate_sources(batch(&[("/p/a.baml", VALID_SOURCE)], &[]))
            .unwrap();
        let CompilationOutcome::Ready(compiled, _) = project.compile_outcome().unwrap() else {
            panic!("trivial source should compile");
        };
        let candidate = project.construct_engine_candidate(*compiled).unwrap();

        let rendezvous = std::sync::Barrier::new(2);
        let (gate_observation_tx, gate_observation_rx) = std::sync::mpsc::sync_channel(1);

        std::thread::scope(|scope| {
            let edit = scope.spawn(|| {
                rendezvous.wait();
                let commit_holds_source_gate = matches!(
                    project.source.try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                );
                gate_observation_tx
                    .send(commit_holds_source_gate)
                    .expect("commit test receiver should stay alive");
                project
                    .mutate_sources(batch(
                        &[("/p/a.baml", "function main() -> int {\n    2\n}\n")],
                        &[],
                    ))
                    .unwrap()
            });

            let outcome = project
                .commit_engine_if_current_with_hook(candidate, || {
                    rendezvous.wait();
                    assert!(
                        gate_observation_rx
                            .recv()
                            .expect("edit thread should report the gate state"),
                        "commit must hold the same source gate required by edits"
                    );
                })
                .unwrap();

            let CommitOutcome::Committed(receipt) = outcome else {
                panic!("commit holding the gate must finish before the queued edit");
            };
            let r2 = edit.join().expect("edit thread should finish");
            assert_eq!(receipt.source_revision, SourceRevision(1));
            assert_eq!(r2, SourceRevision(2));
        });

        // The installed engine remains last-known-good but is not current,
        // so run admission refuses it.
        assert_eq!(project.current_generation(), 1);
        assert!(!project.is_bex_current());
        assert!(matches!(
            project.prepare_function_run(None),
            Err(PrepareRunError::NeedsCurrentBuild)
        ));
    }

    /// The request lane's bounded wait times out with a typed busy error
    /// whose classification depends on whether the source revision moved
    /// while waiting. The writer releases the gate only after the timeout
    /// result is asserted, so the test never races the deadline.
    #[test]
    fn bounded_request_wait_classifies_timeout_by_revision_change() {
        let project = test_project();
        project
            .mutate_sources(batch(&[("/p/a.baml", VALID_SOURCE)], &[]))
            .unwrap();

        // Gate held with no mutation: the timeout is plain congestion
        // (RequestFailed at the LSP boundary, never -32001).
        std::thread::scope(|scope| {
            let (held_tx, held_rx) = std::sync::mpsc::sync_channel::<()>(0);
            let (release_tx, release_rx) = std::sync::mpsc::sync_channel::<()>(0);
            let project = &project;
            scope.spawn(move || {
                let _guard = project.read_source_nowait().unwrap().unwrap();
                held_tx.send(()).expect("test thread should be waiting");
                release_rx.recv().expect("main thread should release us");
            });
            held_rx.recv().expect("holder should signal");
            assert!(matches!(
                project.read_source_for_request(),
                Err(DbReadError::Busy {
                    revision_changed: false
                })
            ));
            release_tx.send(()).expect("holder should be waiting");
        });

        // A revision that moved during the wait marks the timeout stale
        // (ContentModified) rather than congested.
        let revision_at_start = project.current_revision();
        project
            .mutate_sources(batch(
                &[("/p/a.baml", "function main() -> int {\n    2\n}\n")],
                &[],
            ))
            .unwrap();
        assert!(matches!(
            project.classify_request_timeout(revision_at_start),
            DbReadError::Busy {
                revision_changed: true
            }
        ));
        assert!(matches!(
            project.classify_request_timeout(project.current_revision()),
            DbReadError::Busy {
                revision_changed: false
            }
        ));
    }

    #[test]
    fn rebuild_with_errors_publishes_diagnostics_without_engine() {
        let project = test_project();
        project
            .mutate_sources(batch(&[("/p/a.baml", "function broken( {")], &[]))
            .unwrap();

        let report = project.rebuild_once();
        let diagnostics = report.diagnostics.expect("diagnostics always publish");
        assert!(diagnostics.has_errors);
        assert!(matches!(
            report.engine,
            EngineBuildOutcome::BlockedByDiagnostics { .. }
        ));
        assert_eq!(project.current_generation(), 0);
    }

    #[test]
    fn collection_ticket_is_fenced_by_epoch_and_generation() {
        let project = test_project();
        project
            .mutate_sources(batch(&[("/p/a.baml", VALID_SOURCE)], &[]))
            .unwrap();
        let _ = project.rebuild_once();

        let ticket = project
            .begin_test_collection()
            .unwrap()
            .expect("current engine yields a ticket");
        assert!(project.collection_ticket_is_current(&ticket));

        // A source mutation supersedes the collection: the ticket is stale,
        // its cancel token fires, and installation is refused.
        project
            .mutate_sources(batch(&[("/p/a.baml", VALID_SOURCE)], &[]))
            .unwrap();
        assert!(!project.collection_ticket_is_current(&ticket));
        assert!(ticket.cancel.is_cancelled());
        assert!(!project.install_collected_registry(&ticket, None).unwrap());
    }

    #[test]
    fn stale_engine_yields_no_collection_ticket() {
        let project = test_project();
        project
            .mutate_sources(batch(&[("/p/a.baml", VALID_SOURCE)], &[]))
            .unwrap();
        let _ = project.rebuild_once();
        project
            .mutate_sources(batch(&[("/p/a.baml", VALID_SOURCE)], &[]))
            .unwrap();

        assert!(project.begin_test_collection().unwrap().is_none());
    }

    #[test]
    fn registry_lease_requires_current_engine_and_installed_registry() {
        fn lease_error(project: &BexProject, generation: u64) -> RegistryLeaseError {
            match project.lease_registry(generation) {
                Ok(_) => panic!("lease unexpectedly granted"),
                Err(e) => e,
            }
        }

        let project = test_project();
        project
            .mutate_sources(batch(&[("/p/a.baml", VALID_SOURCE)], &[]))
            .unwrap();

        // No engine at all.
        assert_eq!(
            lease_error(&project, 1),
            RegistryLeaseError::NeedsCurrentBuild
        );

        let _ = project.rebuild_once();
        let generation = project.current_generation();

        // Engine current, but no collection installed yet.
        assert_eq!(
            lease_error(&project, generation),
            RegistryLeaseError::NoRegistry
        );

        // Install a "no tests" registry through the ticket path.
        let ticket = project.begin_test_collection().unwrap().unwrap();
        assert!(project.install_collected_registry(&ticket, None).unwrap());
        assert_eq!(
            lease_error(&project, generation),
            RegistryLeaseError::NoTests
        );

        // A wrong generation is rejected regardless.
        assert_eq!(
            lease_error(&project, generation + 1),
            RegistryLeaseError::NeedsCurrentBuild
        );

        // A mutation invalidates the lease path; there is no last-known-good
        // fallback.
        project
            .mutate_sources(batch(&[("/p/a.baml", VALID_SOURCE)], &[]))
            .unwrap();
        assert_eq!(
            lease_error(&project, generation),
            RegistryLeaseError::NeedsCurrentBuild
        );
    }

    #[test]
    fn prepare_function_run_validates_and_pins_generation() {
        let project = test_project();
        project
            .mutate_sources(batch(&[("/p/a.baml", VALID_SOURCE)], &[]))
            .unwrap();

        assert!(matches!(
            project.prepare_function_run(None),
            Err(PrepareRunError::NeedsCurrentBuild)
        ));

        let _ = project.rebuild_once();
        let snapshot = project.prepare_function_run(None).unwrap();
        assert_eq!(snapshot.generation, 1);

        // The engine of a retained generation stays addressable for cancel
        // targeting; a superseded generation stops matching once a new
        // engine installs.
        assert!(project.engine_for_generation(1).is_some());
        project
            .mutate_sources(batch(&[("/p/a.baml", VALID_SOURCE)], &[]))
            .unwrap();
        assert!(matches!(
            project.prepare_function_run(None),
            Err(PrepareRunError::NeedsCurrentBuild)
        ));
        let _ = project.rebuild_once();
        assert!(project.engine_for_generation(1).is_none());
        assert!(project.engine_for_generation(2).is_some());
    }

    #[test]
    fn nowait_read_reports_busy_while_gate_is_held() {
        let project = test_project();
        project
            .mutate_sources(batch(&[("/p/a.baml", VALID_SOURCE)], &[]))
            .unwrap();

        let held = project.read_source_nowait().unwrap().unwrap();
        assert!(matches!(project.read_source_nowait(), Ok(None)));
        assert!(matches!(project.diagnostic_candidate_nowait(), Ok(None)));
        drop(held);
        assert!(project.read_source_nowait().unwrap().is_some());
    }

    #[test]
    fn poisoned_source_gate_is_terminal_broken_state() {
        let project = std::sync::Arc::new(test_project());
        project
            .mutate_sources(batch(&[("/p/a.baml", VALID_SOURCE)], &[]))
            .unwrap();

        // Panic while holding the source gate on another thread.
        let poisoner = project.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.source.lock().unwrap();
            panic!("poison the source gate");
        })
        .join();

        // Every lane observes terminal Broken — never Busy, never a clear.
        assert!(matches!(
            project.read_source_for_request(),
            Err(DbReadError::Broken)
        ));
        assert!(project.read_source_nowait().is_err());
        assert!(project.mutate_sources(batch(&[], &[])).is_err());
        assert!(matches!(
            project.prepare_function_run(None),
            Err(PrepareRunError::Broken)
        ));
        assert!(matches!(
            project.rebuild_once().engine,
            EngineBuildOutcome::Broken
        ));
        assert!(!project.is_bex_current());
    }

    // ── Scale / pipeline timings (local dev; prints to stderr) ────────────

    fn load_disk_project(root: &std::path::Path) -> Option<(BexProject, usize, usize)> {
        if !root.join("baml.toml").exists() {
            return None;
        }
        let vfs_root = vfs::VfsPath::new(vfs::MemoryFS::new());
        let project = BexProject::new(&vfs_root, Arc::new(sys_ops::SysOps::native()));

        let mut sources = HashMap::new();
        let baml_src = root.join("baml_src");
        let scan_root = if baml_src.is_dir() {
            baml_src
        } else {
            root.to_path_buf()
        };
        let mut file_count = 0usize;
        let mut line_count = 0usize;
        for entry in walkdir::WalkDir::new(&scan_root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "baml"))
        {
            let text = std::fs::read_to_string(entry.path()).ok()?;
            line_count += text.lines().count();
            file_count += 1;
            let rel = entry.path().strip_prefix(root).ok()?;
            let key = format!("/{}", rel.to_string_lossy().replace('\\', "/"));
            sources.insert(crate::fs::FsPath::from_str(key), text);
        }

        project
            .mutate_sources(SourceBatch {
                replace_all: true,
                sources,
                versions: vec![],
            })
            .ok()?;
        Some((project, file_count, line_count))
    }

    #[test]
    #[ignore = "local scale benchmark; run: cargo test -p bex_project bench_sandbox_scale_timings -- --ignored --nocapture"]
    #[allow(clippy::print_stdout)]
    fn bench_sandbox_scale_timings() {
        let root = std::path::Path::new("/Users/rossir/dev/sandbox");
        let Some((project, files, lines)) = load_disk_project(root) else {
            println!("bench_sandbox_scale_timings: skipped (no project at {root:?})");
            return;
        };
        println!("bench_sandbox_scale_timings: files={files} lines={lines}");

        let t0 = std::time::Instant::now();
        let guard = project.read_source_nowait().unwrap().unwrap();
        collect_diagnostic_candidate(&guard);
        let diag_cold_ms = t0.elapsed().as_millis();
        drop(guard);

        let t1 = std::time::Instant::now();
        let guard = project.read_source_nowait().unwrap().unwrap();
        collect_diagnostic_candidate(&guard);
        let diag_warm_ms = t1.elapsed().as_millis();
        drop(guard);

        let t2 = std::time::Instant::now();
        let _ = project.compile_outcome().unwrap();
        let compile_ms = t2.elapsed().as_millis();

        let main_rel = "baml_src/main.baml";
        let main_text = std::fs::read_to_string(root.join(main_rel)).unwrap();
        let edited = format!("{main_text}\n// bench-touch\n");
        project
            .mutate_sources(batch(&[(&format!("/{main_rel}"), &edited)], &[]))
            .unwrap();
        let t3 = std::time::Instant::now();
        let guard = project.read_source_nowait().unwrap().unwrap();
        collect_diagnostic_candidate(&guard);
        let diag_incremental_ms = t3.elapsed().as_millis();

        println!(
            "  diagnostics_cold={diag_cold_ms}ms diagnostics_warm={diag_warm_ms}ms \
             compile_outcome={compile_ms}ms diagnostics_after_one_file_edit={diag_incremental_ms}ms"
        );
    }
}
