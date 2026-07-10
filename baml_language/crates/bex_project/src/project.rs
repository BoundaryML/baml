use std::{
    collections::{HashMap, HashSet, VecDeque},
    ops::{Deref, DerefMut},
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use bex_engine::BexEngine;
use bex_external_types::Handle;
use sys_ops::SysOps;

use crate::{RuntimeError, fs::FsPath};

/// The authoritative identity of a project's accepted source state.
///
/// The value is advanced while holding the same mutex that protects the
/// mutable Salsa database.  The atomic mirror on [`BexProject`] is only used
/// to classify a request-lock timeout; it never authorizes a commit.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub(crate) struct SourceRevision(pub(crate) u64);

impl SourceRevision {
    pub(crate) const INITIAL: Self = Self(0);
}

/// Identity of every input that can affect engine construction. Source and
/// runtime inputs advance independently, but a current engine must match both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BuildKey {
    source_revision: SourceRevision,
    runtime_input_revision: u64,
}

/// Open-editor identity captured in the same transaction as its source text.
#[derive(Debug, Clone)]
pub(crate) struct OpenDocument {
    pub(crate) client_uri: lsp_types::Url,
    pub(crate) version: i32,
    pub(crate) text: String,
}

/// Fully owned source metadata that may safely outlive the database guard.
#[derive(Debug, Clone)]
pub(crate) struct SourceSnapshot {
    pub(crate) revision: SourceRevision,
}

/// The database and all identity that must change atomically with it.
///
/// `Deref` keeps the existing read-only query call sites concise while the
/// mutation entry points remain private to [`BexProject`].
pub(crate) struct ProjectDatabaseState {
    database: baml_project::ProjectDatabase,
    revision: SourceRevision,
    open_documents: HashMap<FsPath, OpenDocument>,
    diagnostics_dirty: Option<SourceRevision>,
    publication_watermark: SourceRevision,
}

impl Deref for ProjectDatabaseState {
    type Target = baml_project::ProjectDatabase;

    fn deref(&self) -> &Self::Target {
        &self.database
    }
}

impl DerefMut for ProjectDatabaseState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.database
    }
}

impl ProjectDatabaseState {
    pub(crate) fn source_revision(&self) -> SourceRevision {
        self.revision
    }

    pub(crate) fn source_snapshot(&self) -> SourceSnapshot {
        SourceSnapshot {
            revision: self.revision,
        }
    }
}

/// Prepared control-flow graphs pinned for playground runs, keyed by
/// `(engine generation, function name)`.
///
/// A run also retains its graph directly.  This bounded cache is only a
/// convenience for the graph-overlay provider and never establishes run
/// currentness.
const RETAINED_RUN_CFGS: usize = 64;

type PreparedGraph = Arc<baml_compiler2_visualization::control_flow::ControlFlowGraph>;

#[derive(Default)]
struct RunCfgCache {
    order: VecDeque<(u64, String)>,
    graphs: HashMap<(u64, String), PreparedGraph>,
}

impl RunCfgCache {
    fn insert(&mut self, generation: u64, function_name: &str, graph: PreparedGraph) {
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

    fn graph(&self, generation: u64, function_name: &str) -> Option<PreparedGraph> {
        self.graphs
            .get(&(generation, function_name.to_string()))
            .cloned()
    }
}

#[derive(Clone)]
pub(crate) struct InstalledEngine {
    pub(crate) source_revision: SourceRevision,
    runtime_input_revision: u64,
    pub(crate) generation: u64,
    pub(crate) engine: Arc<BexEngine>,
}

#[derive(Clone)]
pub(crate) struct InstalledRegistry {
    pub(crate) source_revision: SourceRevision,
    pub(crate) generation: u64,
    pub(crate) collection_epoch: u64,
    pub(crate) handle: Handle,
    mutation_owner: Arc<futures::lock::Mutex<()>>,
}

/// One coherent snapshot of all engine-derived project state.
pub(crate) struct RuntimeState {
    installed: Option<InstalledEngine>,
    next_generation: u64,
    derived_epoch: u64,
    derived_cancel: sys_types::CancellationToken,
    collection_epoch: u64,
    /// The collection currently allowed to replace `registry`.  Keeping this
    /// distinct from `collection_epoch` lets a failed newer collection retain
    /// the previous registry without making an expansion of that registry
    /// current while the newer collection is still in flight.
    collection_in_flight: Option<u64>,
    /// A registry has been installed with a matching tree envelope but that
    /// envelope has not yet been acknowledged by the transport sequencer.
    collection_publication_pending: Option<u64>,
    registry: Option<InstalledRegistry>,
}

impl RuntimeState {
    fn new() -> Self {
        Self {
            installed: None,
            next_generation: 0,
            derived_epoch: 0,
            derived_cancel: sys_types::CancellationToken::new(),
            collection_epoch: 0,
            collection_in_flight: None,
            collection_publication_pending: None,
            registry: None,
        }
    }

    fn invalidate_derived(&mut self) {
        self.derived_epoch = self.derived_epoch.wrapping_add(1);
        self.collection_epoch = self.collection_epoch.wrapping_add(1);
        self.derived_cancel.cancel();
        self.derived_cancel = sys_types::CancellationToken::new();
        self.collection_in_flight = None;
        self.collection_publication_pending = None;
        self.registry = None;
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RuntimeBuildPhase {
    IdleStale,
    Building,
    Ready,
    BlockedByDiagnostics,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeStatus {
    pub(crate) phase: RuntimeBuildPhase,
    pub(crate) requested_revision: SourceRevision,
    pub(crate) installed_revision: Option<SourceRevision>,
    pub(crate) generation: Option<u64>,
    pub(crate) has_last_known_good: bool,
    pub(crate) error_message: Option<String>,
}

pub(crate) struct CompiledCandidate {
    pub(crate) source_revision: SourceRevision,
    pub(crate) program: bex_vm_types::Program,
}

pub(crate) enum CompilationOutcome {
    Ready(Box<CompiledCandidate>),
    BlockedByDiagnostics(SourceRevision),
    Superseded,
}

pub(crate) struct EngineCandidate {
    source_revision: SourceRevision,
    runtime_input_revision: u64,
    engine: BexEngine,
}

#[derive(Clone)]
pub(crate) struct CommitReceipt {
    pub(crate) source_revision: SourceRevision,
    runtime_input_revision: u64,
    pub(crate) generation: u64,
    pub(crate) engine: Arc<BexEngine>,
}

pub(crate) enum CommitOutcome {
    Committed(CommitReceipt),
    Superseded,
}

#[derive(Clone)]
pub(crate) struct CollectionLease {
    pub(crate) source_revision: SourceRevision,
    pub(crate) generation: u64,
    pub(crate) derived_epoch: u64,
    pub(crate) collection_epoch: u64,
    pub(crate) engine: Arc<BexEngine>,
    pub(crate) cancel: sys_types::CancellationToken,
    /// A newer collection may cancel an in-place expansion of the registry it
    /// is replacing, but it must not expose/retain that registry until the
    /// expansion owner has completed rollback.
    pub(crate) previous_registry_mutation_owner: Option<Arc<futures::lock::Mutex<()>>>,
}

pub(crate) struct RegistryPublicationReceipt {
    pub(crate) batch_id: PublicationBatchId,
    source_revision: SourceRevision,
    generation: u64,
    collection_epoch: u64,
    previous: Option<InstalledRegistry>,
}

#[cfg(test)]
pub(crate) struct ExpansionPublicationReceipt {
    source_revision: SourceRevision,
    generation: u64,
    collection_epoch: u64,
    previous: Handle,
    installed: Handle,
}

#[derive(Clone)]
pub(crate) struct RegistryLease {
    pub(crate) source_revision: SourceRevision,
    pub(crate) generation: u64,
    pub(crate) derived_epoch: u64,
    pub(crate) collection_epoch: u64,
    pub(crate) engine: Arc<BexEngine>,
    pub(crate) registry: Handle,
    pub(crate) cancel: sys_types::CancellationToken,
    pub(crate) mutation_owner: Arc<futures::lock::Mutex<()>>,
}

/// Immutable identity retained for a user run until its terminal outcome.
#[derive(Clone)]
pub(crate) struct RunLease {
    pub(crate) source_revision: SourceRevision,
    pub(crate) generation: u64,
    pub(crate) engine: Arc<BexEngine>,
    pub(crate) test_registry: Option<Handle>,
    function_name: Option<String>,
    graph: Option<PreparedGraph>,
}

#[derive(Clone)]
pub(crate) enum ProjectPublication {
    LspDiagnostics {
        path: FsPath,
        present: bool,
        params: lsp_types::PublishDiagnosticsParams,
    },
    Playground {
        session_epoch: u64,
        notification: crate::bex_lsp::PlaygroundNotification,
    },
}

impl ProjectPublication {
    fn serialized_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        match self {
            Self::LspDiagnostics { params, .. } => serde_json::to_vec(
                &lsp_server::Message::Notification(lsp_server::Notification::new(
                    "textDocument/publishDiagnostics".to_string(),
                    params.clone(),
                )),
            ),
            Self::Playground { notification, .. } => serde_json::to_vec(notification),
        }
    }
}

pub(crate) struct ProjectPublicationEnvelope {
    identity: ProjectPublicationIdentity,
    batch_id: PublicationBatchId,
    batch_len: usize,
    bytes: usize,
    pub(crate) publication: ProjectPublication,
}

impl ProjectPublicationEnvelope {
    pub(crate) fn identity(&self) -> ProjectPublicationIdentity {
        self.identity
    }

    pub(crate) fn batch_id(&self) -> PublicationBatchId {
        self.batch_id
    }

    pub(crate) fn batch_len(&self) -> usize {
        self.batch_len
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PublicationBatchId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublicationEnqueueError {
    Stale,
    Saturated,
    Oversized,
    Serialization,
}

const PUBLICATION_MAILBOX_MAX_ITEMS: usize = 4096;
const PUBLICATION_MAILBOX_MAX_BYTES: usize = 16 * 1024 * 1024;
const PUBLICATION_MAILBOX_MAX_ITEM_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
struct PublicationMailbox {
    queue: VecDeque<ProjectPublicationEnvelope>,
    bytes: usize,
}

impl PublicationMailbox {
    fn pop_front(&mut self) -> Option<ProjectPublicationEnvelope> {
        let envelope = self.queue.pop_front()?;
        self.bytes = self.bytes.saturating_sub(envelope.bytes);
        Some(envelope)
    }

    fn discard_batch(&mut self, batch_id: PublicationBatchId) {
        self.queue.retain(|envelope| {
            if envelope.batch_id == batch_id {
                self.bytes = self.bytes.saturating_sub(envelope.bytes);
                false
            } else {
                true
            }
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ProjectPublicationIdentity {
    pub(crate) source_revision: SourceRevision,
    pub(crate) engine_generation: Option<u64>,
    pub(crate) derived_epoch: u64,
    pub(crate) collection_epoch: Option<u64>,
    /// Present for runtime-status/ProjectUpdate publications so a terminal
    /// build transition invalidates an older same-source `building` payload.
    pub(crate) build_epoch: Option<u64>,
    pub(crate) runtime_fenced: bool,
}

#[derive(Default)]
struct BuildCoordinator {
    active_key: Option<BuildKey>,
    status: Option<(BuildKey, RuntimeBuildPhase)>,
    failure: Option<(BuildKey, String)>,
    status_epoch: u64,
}

pub(crate) struct BexProject {
    pub(crate) db: Arc<Mutex<ProjectDatabaseState>>,
    sys_ops: Arc<SysOps>,
    runtime: Mutex<RuntimeState>,
    run_cfgs: Mutex<RunCfgCache>,
    active_runs: Mutex<HashMap<sys_types::CallId, RunLease>>,
    /// Serializes collection begin through registry/tree delivery
    /// acknowledgement. A later collection must not retain a registry whose
    /// matching tree publication is still unacknowledged.
    collection_owner: Arc<futures::lock::Mutex<()>>,
    publication_mailbox: Mutex<PublicationMailbox>,
    /// Serializes drainers and diagnostics' previous-URI bookkeeping.
    publication_drainer: Mutex<()>,
    /// Linearizes source mutation against the final nonblocking transport
    /// enqueue for one publication envelope.
    publication_barrier: Mutex<()>,
    next_publication_batch: AtomicU64,
    publication_watermark_observer: AtomicU64,
    build: Mutex<BuildCoordinator>,
    build_finished: Condvar,
    source_revision_observer: AtomicU64,
    runtime_input_revision: AtomicU64,
    broken: AtomicBool,
}

impl BexProject {
    pub(crate) fn new(root_path: &vfs::VfsPath, sys_ops: Arc<SysOps>) -> Self {
        let mut database = baml_project::ProjectDatabase::new();
        database.set_project_root(crate::fs::FsPath::from_vfs(root_path).as_path());
        Self {
            db: Arc::new(Mutex::new(ProjectDatabaseState {
                database,
                revision: SourceRevision::INITIAL,
                open_documents: HashMap::new(),
                diagnostics_dirty: Some(SourceRevision::INITIAL),
                publication_watermark: SourceRevision::INITIAL,
            })),
            sys_ops,
            runtime: Mutex::new(RuntimeState::new()),
            run_cfgs: Mutex::new(RunCfgCache::default()),
            active_runs: Mutex::new(HashMap::new()),
            collection_owner: Arc::new(futures::lock::Mutex::new(())),
            publication_mailbox: Mutex::new(PublicationMailbox::default()),
            publication_drainer: Mutex::new(()),
            publication_barrier: Mutex::new(()),
            next_publication_batch: AtomicU64::new(1),
            publication_watermark_observer: AtomicU64::new(SourceRevision::INITIAL.0),
            build: Mutex::new(BuildCoordinator::default()),
            build_finished: Condvar::new(),
            source_revision_observer: AtomicU64::new(SourceRevision::INITIAL.0),
            runtime_input_revision: AtomicU64::new(0),
            broken: AtomicBool::new(false),
        }
    }

    pub(crate) fn is_broken(&self) -> bool {
        self.broken.load(Ordering::Acquire)
    }

    pub(crate) fn mark_broken(&self, context: &str) {
        self.broken.store(true, Ordering::Release);
        tracing::error!(%context, "project entered terminal ProjectBroken state");
    }

    fn poison_error(&self, context: &str) -> crate::LspError {
        self.broken.store(true, Ordering::Release);
        crate::LspError::InternalError(format!(
            "Project database is poisoned while {context}; the language server must restart"
        ))
    }

    /// Native request compatibility lane: wait up to one second for the live
    /// database, checking every 2ms.  The bounded wait is intentionally not a
    /// general background-work primitive.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn try_lock_db_wait_for_request(
        &self,
    ) -> Result<MutexGuard<'_, ProjectDatabaseState>, crate::LspError> {
        self.try_lock_db_wait_for_request_with_timing(
            std::time::Duration::from_secs(1),
            std::time::Duration::from_millis(2),
            std::time::Duration::from_millis(50),
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn try_lock_db_wait_for_request_with_timing(
        &self,
        max_wait: std::time::Duration,
        retry_interval: std::time::Duration,
        wait_log_threshold: std::time::Duration,
    ) -> Result<MutexGuard<'_, ProjectDatabaseState>, crate::LspError> {
        if self.is_broken() {
            return Err(crate::LspError::InternalError(
                "Project is in terminal ProjectBroken state".to_string(),
            ));
        }

        let captured_revision = self.source_revision_observer.load(Ordering::Acquire);
        let started = std::time::Instant::now();
        let deadline = started + max_wait;
        let mut logged = false;
        loop {
            match self.db.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err(self.poison_error("serving a request"));
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    let now = std::time::Instant::now();
                    if !logged && now.duration_since(started) >= wait_log_threshold {
                        logged = true;
                        tracing::warn!(
                            wait_ms = now.duration_since(started).as_millis(),
                            "waiting for project database"
                        );
                    }
                    if now >= deadline {
                        let observed = self.source_revision_observer.load(Ordering::Acquire);
                        return if observed != captured_revision {
                            Err(crate::LspError::ContentModified(
                                "Project source changed while the request waited for the database"
                                    .to_string(),
                            ))
                        } else {
                            Err(crate::LspError::RequestFailed(format!(
                                "Project database remained busy for {}",
                                if max_wait == std::time::Duration::from_secs(1) {
                                    "1s".to_string()
                                } else {
                                    format!("{}ms", max_wait.as_millis())
                                }
                            )))
                        };
                    }
                    std::thread::sleep(retry_interval);
                }
            }
        }
    }

    /// WASM is single-threaded and must never block waiting for the database.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn try_lock_db_wait_for_request(
        &self,
    ) -> Result<MutexGuard<'_, ProjectDatabaseState>, crate::LspError> {
        if self.is_broken() {
            return Err(crate::LspError::InternalError(
                "Project is in terminal ProjectBroken state".to_string(),
            ));
        }
        match self.db.try_lock() {
            Ok(guard) => Ok(guard),
            Err(std::sync::TryLockError::WouldBlock) => Err(crate::LspError::RequestFailed(
                "Project database is busy".to_string(),
            )),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                Err(self.poison_error("serving a WASM request"))
            }
        }
    }

    /// Existing handlers use this name; it now has the bounded request
    /// semantics rather than an immediate, untyped `try_lock` failure.
    pub(crate) fn try_lock_db(
        &self,
    ) -> Result<MutexGuard<'_, ProjectDatabaseState>, crate::LspError> {
        self.try_lock_db_wait_for_request()
    }

    /// Workspace-wide scans must not serially wait one second per project.
    /// `Ok(None)` means ordinary contention; poison remains an explicit error.
    #[cfg(test)]
    pub(crate) fn try_lock_db_nowait(
        &self,
    ) -> Result<Option<MutexGuard<'_, ProjectDatabaseState>>, crate::LspError> {
        if self.is_broken() {
            return Err(crate::LspError::InternalError(
                "Project is in terminal ProjectBroken state".to_string(),
            ));
        }
        match self.db.try_lock() {
            Ok(guard) => Ok(Some(guard)),
            Err(std::sync::TryLockError::WouldBlock) => Ok(None),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                Err(self.poison_error("scanning the workspace"))
            }
        }
    }

    pub(crate) fn source_revision(&self) -> SourceRevision {
        SourceRevision(self.source_revision_observer.load(Ordering::Acquire))
    }

    fn finish_source_mutation(
        &self,
        state: &mut ProjectDatabaseState,
        runtime: &mut RuntimeState,
    ) -> SourceRevision {
        state.revision = SourceRevision(state.revision.0.wrapping_add(1));
        state.diagnostics_dirty = Some(state.revision);
        state.publication_watermark = state.revision;
        runtime.invalidate_derived();
        self.source_revision_observer
            .store(state.revision.0, Ordering::Release);
        self.publication_watermark_observer
            .store(state.revision.0, Ordering::Release);
        state.revision
    }

    /// Update all disk-visible sources while preserving open-editor overlays.
    /// The database, revision, document identity, and runtime invalidation are
    /// one source/runtime transaction.
    pub(crate) fn apply_all_sources(&self, sources: &HashMap<FsPath, String>) -> SourceRevision {
        let _publication = self
            .publication_barrier
            .lock()
            .expect("publication barrier poisoned");
        let mut state = self.db.lock().expect("project database poisoned");
        let mut runtime = self.runtime.lock().expect("project runtime poisoned");
        let mut effective = sources.clone();
        for (path, document) in &state.open_documents {
            effective.insert(path.clone(), document.text.clone());
        }

        let mut existing_paths: HashSet<_> = state.non_builtin_file_paths().collect();
        for (path, source) in &effective {
            state.add_or_update_file(path.as_path(), source);
            existing_paths.remove(path.as_path());
        }
        for path in existing_paths {
            state.remove_file(&path);
        }
        self.finish_source_mutation(&mut state, &mut runtime)
    }

    pub(crate) fn apply_open_document(
        &self,
        path: FsPath,
        client_uri: lsp_types::Url,
        version: i32,
        text: String,
    ) -> SourceRevision {
        let _publication = self
            .publication_barrier
            .lock()
            .expect("publication barrier poisoned");
        let mut state = self.db.lock().expect("project database poisoned");
        let mut runtime = self.runtime.lock().expect("project runtime poisoned");
        state.add_or_update_file(path.as_path(), &text);
        state.open_documents.insert(
            path,
            OpenDocument {
                client_uri,
                version,
                text,
            },
        );
        self.finish_source_mutation(&mut state, &mut runtime)
    }

    /// Initial open is commonly also the first project load. Apply the disk
    /// snapshot and the editor overlay as one accepted source batch.
    pub(crate) fn apply_all_sources_and_open_document(
        &self,
        sources: &HashMap<FsPath, String>,
        path: FsPath,
        client_uri: lsp_types::Url,
        version: i32,
        text: String,
    ) -> SourceRevision {
        let _publication = self
            .publication_barrier
            .lock()
            .expect("publication barrier poisoned");
        let mut state = self.db.lock().expect("project database poisoned");
        let mut runtime = self.runtime.lock().expect("project runtime poisoned");
        state.open_documents.insert(
            path,
            OpenDocument {
                client_uri,
                version,
                text,
            },
        );
        let mut effective = sources.clone();
        for (path, document) in &state.open_documents {
            effective.insert(path.clone(), document.text.clone());
        }
        let mut existing_paths: HashSet<_> = state.non_builtin_file_paths().collect();
        for (path, source) in &effective {
            state.add_or_update_file(path.as_path(), source);
            existing_paths.remove(path.as_path());
        }
        for path in existing_paths {
            state.remove_file(&path);
        }
        self.finish_source_mutation(&mut state, &mut runtime)
    }

    pub(crate) fn apply_playground_source(
        &self,
        path: &FsPath,
        text: &str,
    ) -> Result<SourceRevision, crate::LspError> {
        let _publication = self
            .publication_barrier
            .lock()
            .expect("publication barrier poisoned");
        let mut state = self.db.lock().expect("project database poisoned");
        if state.open_documents.contains_key(path) {
            return Err(crate::LspError::RequestFailed(format!(
                "Cannot apply an out-of-band playground edit to open document {}",
                path.as_path().display()
            )));
        }
        let mut runtime = self.runtime.lock().expect("project runtime poisoned");
        state.add_or_update_file(path.as_path(), text);
        Ok(self.finish_source_mutation(&mut state, &mut runtime))
    }

    pub(crate) fn close_open_document(
        &self,
        path: &FsPath,
        disk_text: Option<String>,
    ) -> SourceRevision {
        let _publication = self
            .publication_barrier
            .lock()
            .expect("publication barrier poisoned");
        let mut state = self.db.lock().expect("project database poisoned");
        let mut runtime = self.runtime.lock().expect("project runtime poisoned");
        state.open_documents.remove(path);
        match disk_text {
            Some(text) => {
                state.add_or_update_file(path.as_path(), &text);
            }
            None => state.remove_file(path.as_path()),
        }
        self.finish_source_mutation(&mut state, &mut runtime)
    }

    pub(crate) fn open_document_sources(&self) -> HashMap<FsPath, String> {
        self.db
            .lock()
            .expect("project database poisoned")
            .open_documents
            .iter()
            .map(|(path, document)| (path.clone(), document.text.clone()))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn diagnostics_dirty_revision(&self) -> Option<SourceRevision> {
        self.db
            .lock()
            .expect("project database poisoned")
            .diagnostics_dirty
    }

    /// Compare-and-clear the diagnostics dirty marker.  A newer mutation can
    /// never be lost by an older diagnostics attempt.
    pub(crate) fn clear_diagnostics_if_current(&self, revision: SourceRevision) -> bool {
        let mut state = self.db.lock().expect("project database poisoned");
        if state.revision == revision && state.diagnostics_dirty == Some(revision) {
            state.diagnostics_dirty = None;
            true
        } else {
            false
        }
    }

    /// Atomically compare project identity and reserve a nonblocking outbound
    /// envelope while holding the source gate. Serialization and transport I/O
    /// happen only after this method returns.
    pub(crate) fn enqueue_publication_batch_if_current(
        &self,
        identity: ProjectPublicationIdentity,
        publications: Vec<ProjectPublication>,
    ) -> Result<PublicationBatchId, PublicationEnqueueError> {
        if publications.is_empty() || publications.len() > PUBLICATION_MAILBOX_MAX_ITEMS {
            return Err(PublicationEnqueueError::Oversized);
        }
        let publication_bytes = publications
            .iter()
            .map(ProjectPublication::serialized_bytes)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| PublicationEnqueueError::Serialization)?
            .into_iter()
            .map(|bytes| bytes.len())
            .collect::<Vec<_>>();
        if publication_bytes
            .iter()
            .any(|bytes| *bytes > PUBLICATION_MAILBOX_MAX_ITEM_BYTES)
        {
            return Err(PublicationEnqueueError::Oversized);
        }
        let batch_bytes = publication_bytes.iter().sum::<usize>();
        if batch_bytes > PUBLICATION_MAILBOX_MAX_BYTES {
            return Err(PublicationEnqueueError::Oversized);
        }
        // Source/runtime/build mutation takes this gate first as well.  Keep
        // the final identity comparison and the all-or-nothing batch insert in
        // the same transaction so a source edit cannot land between them.
        let Ok(_publication) = self.publication_barrier.lock() else {
            return Err(PublicationEnqueueError::Stale);
        };
        let Ok(source) = self.db.lock() else {
            return Err(PublicationEnqueueError::Stale);
        };
        let Ok(runtime) = self.runtime.lock() else {
            return Err(PublicationEnqueueError::Stale);
        };
        if !self.publication_identity_matches(&source, &runtime, identity) {
            return Err(PublicationEnqueueError::Stale);
        }
        if !self.build_identity_matches(identity) {
            return Err(PublicationEnqueueError::Stale);
        }
        let Ok(mut mailbox) = self.publication_mailbox.lock() else {
            return Err(PublicationEnqueueError::Stale);
        };
        if mailbox.queue.len().saturating_add(publications.len()) > PUBLICATION_MAILBOX_MAX_ITEMS
            || mailbox.bytes.saturating_add(batch_bytes) > PUBLICATION_MAILBOX_MAX_BYTES
        {
            return Err(PublicationEnqueueError::Saturated);
        }
        let batch_id =
            PublicationBatchId(self.next_publication_batch.fetch_add(1, Ordering::AcqRel));
        let batch_len = publications.len();
        for (publication, bytes) in publications.into_iter().zip(publication_bytes) {
            mailbox.bytes += bytes;
            mailbox.queue.push_back(ProjectPublicationEnvelope {
                identity,
                batch_id,
                batch_len,
                bytes,
                publication,
            });
        }
        Ok(batch_id)
    }

    #[cfg(test)]
    pub(crate) fn enqueue_publication_if_current(
        &self,
        identity: ProjectPublicationIdentity,
        publication: ProjectPublication,
    ) -> bool {
        self.enqueue_publication_batch_if_current(identity, vec![publication])
            .is_ok()
    }

    fn publication_identity_matches(
        &self,
        source: &ProjectDatabaseState,
        runtime: &RuntimeState,
        identity: ProjectPublicationIdentity,
    ) -> bool {
        if source.revision != identity.source_revision
            || identity.source_revision < source.publication_watermark
        {
            return false;
        }
        if !identity.runtime_fenced {
            return true;
        }
        let generation = runtime.installed.as_ref().and_then(|installed| {
            (installed.source_revision == source.revision
                && installed.runtime_input_revision
                    == self.runtime_input_revision.load(Ordering::Acquire))
            .then_some(installed.generation)
        });
        generation == identity.engine_generation
            && runtime.derived_epoch == identity.derived_epoch
            && identity.collection_epoch.is_none_or(|epoch| {
                runtime.collection_in_flight.is_none()
                    && (runtime.collection_epoch == epoch
                        || runtime.registry.as_ref().is_some_and(|registry| {
                            registry.source_revision == identity.source_revision
                                && Some(registry.generation) == identity.engine_generation
                                && registry.collection_epoch == epoch
                        }))
            })
    }

    pub(crate) fn publication_identity_is_current(
        &self,
        identity: ProjectPublicationIdentity,
    ) -> bool {
        let Ok(source) = self.db.lock() else {
            return false;
        };
        let Ok(runtime) = self.runtime.lock() else {
            return false;
        };
        self.publication_identity_matches(&source, &runtime, identity)
            && self.build_identity_matches(identity)
    }

    fn build_identity_matches(&self, identity: ProjectPublicationIdentity) -> bool {
        identity.build_epoch.is_none_or(|expected| {
            self.build
                .lock()
                .is_ok_and(|build| build.status_epoch == expected)
        })
    }

    /// Serialize mailbox draining and transport enqueue for this project.
    /// Source/runtime guards are never held while this guard is held by a
    /// sender, so transport backpressure cannot block source mutation.
    pub(crate) fn lock_publication_drainer(&self) -> Option<MutexGuard<'_, ()>> {
        self.publication_drainer.lock().ok()
    }

    pub(crate) fn lock_publication_barrier(&self) -> Option<MutexGuard<'_, ()>> {
        self.publication_barrier.lock().ok()
    }

    pub(crate) fn publication_identity(&self) -> Option<ProjectPublicationIdentity> {
        let source = self.db.lock().ok()?;
        let runtime = self.runtime.lock().ok()?;
        let runtime_input_revision = self.runtime_input_revision.load(Ordering::Acquire);
        let installed = runtime.installed.as_ref().filter(|installed| {
            installed.source_revision == source.revision
                && installed.runtime_input_revision == runtime_input_revision
        })?;
        Some(ProjectPublicationIdentity {
            source_revision: source.revision,
            engine_generation: Some(installed.generation),
            derived_epoch: runtime.derived_epoch,
            collection_epoch: None,
            build_epoch: None,
            runtime_fenced: true,
        })
    }

    pub(crate) fn collection_publication_identity(
        lease: &CollectionLease,
    ) -> ProjectPublicationIdentity {
        ProjectPublicationIdentity {
            source_revision: lease.source_revision,
            engine_generation: Some(lease.generation),
            derived_epoch: lease.derived_epoch,
            collection_epoch: Some(lease.collection_epoch),
            build_epoch: None,
            runtime_fenced: true,
        }
    }

    pub(crate) fn registry_publication_identity(
        lease: &RegistryLease,
    ) -> ProjectPublicationIdentity {
        ProjectPublicationIdentity {
            source_revision: lease.source_revision,
            engine_generation: Some(lease.generation),
            derived_epoch: lease.derived_epoch,
            collection_epoch: Some(lease.collection_epoch),
            build_epoch: None,
            runtime_fenced: true,
        }
    }

    pub(crate) fn source_publication_identity(
        source_revision: SourceRevision,
    ) -> ProjectPublicationIdentity {
        ProjectPublicationIdentity {
            source_revision,
            engine_generation: None,
            derived_epoch: 0,
            collection_epoch: None,
            build_epoch: None,
            runtime_fenced: false,
        }
    }

    /// Drain in sequencer order. Source mutation advances the watermark while
    /// holding the source gate; envelopes queued for an older revision are
    /// discarded even if transport draining was delayed.
    pub(crate) fn pop_next_publication(&self) -> Option<ProjectPublicationEnvelope> {
        self.publication_mailbox.lock().ok()?.pop_front()
    }

    pub(crate) fn discard_publication_batch(&self, batch_id: PublicationBatchId) {
        if let Ok(mut mailbox) = self.publication_mailbox.lock() {
            mailbox.discard_batch(batch_id);
        }
    }

    pub(crate) fn discard_all_publications(&self) {
        if let Ok(mut mailbox) = self.publication_mailbox.lock() {
            mailbox.queue.clear();
            mailbox.bytes = 0;
        }
    }

    #[cfg(test)]
    pub(crate) fn take_next_publication(&self) -> Option<ProjectPublicationEnvelope> {
        loop {
            let envelope = self.pop_next_publication()?;
            if self.publication_identity_is_current(envelope.identity) {
                return Some(envelope);
            }
            self.discard_publication_batch(envelope.batch_id);
        }
    }

    #[cfg(test)]
    fn publication_mailbox_usage(&self) -> (usize, usize) {
        self.publication_mailbox
            .lock()
            .map(|mailbox| (mailbox.queue.len(), mailbox.bytes))
            .unwrap_or_default()
    }

    /// Update all sources and synchronously produce a current engine.  This is
    /// retained for the non-LSP construction API; LSP callers use the same
    /// single-flight build from background work.
    pub(crate) fn update_all_sources(&self, sources: &HashMap<FsPath, String>) {
        self.apply_all_sources(sources);
        let _ = self.ensure_engine_current();
    }

    fn compile_candidate(
        &self,
        revision: SourceRevision,
    ) -> Result<CompilationOutcome, RuntimeError> {
        let state = self.db.lock().map_err(|_| RuntimeError::Compilation {
            message: "Project database is poisoned".to_string(),
        })?;
        if state.revision != revision {
            return Ok(CompilationOutcome::Superseded);
        }

        let diagnostics = baml_project::collect_compiler2_diagnostics(&state);
        if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == baml_compiler_diagnostics::Severity::Error)
        {
            return Ok(CompilationOutcome::BlockedByDiagnostics(revision));
        }

        let program = state
            .get_bytecode()
            .map_err(|error| RuntimeError::Compilation {
                message: error.to_string(),
            })?;
        Ok(CompilationOutcome::Ready(Box::new(CompiledCandidate {
            source_revision: revision,
            program,
        })))
    }

    fn construct_engine_candidate(
        &self,
        compiled: CompiledCandidate,
        runtime_input_revision: u64,
    ) -> Result<EngineCandidate, RuntimeError> {
        let engine = BexEngine::new_inactive(compiled.program, self.sys_ops.clone(), Vec::new())?;
        Ok(EngineCandidate {
            source_revision: compiled.source_revision,
            runtime_input_revision,
            engine,
        })
    }

    pub(crate) fn commit_engine_if_current(&self, candidate: EngineCandidate) -> CommitOutcome {
        self.commit_engine_if_current_with_hook(candidate, || {})
    }

    /// Internal seam for proving the commit/source-mutation serialization
    /// boundary. The hook runs after the authoritative source gate is held,
    /// but before the revision comparison. Production callers use the no-op
    /// wrapper above; tests use the hook to place an edit exactly against the
    /// conditional commit without sleeps or scheduler timing assumptions.
    fn commit_engine_if_current_with_hook(
        &self,
        mut candidate: EngineCandidate,
        after_source_lock: impl FnOnce(),
    ) -> CommitOutcome {
        let _publication = self
            .publication_barrier
            .lock()
            .expect("publication barrier poisoned");
        let source = self.db.lock().expect("project database poisoned");
        after_source_lock();
        if source.revision != candidate.source_revision
            || self.runtime_input_revision.load(Ordering::Acquire)
                != candidate.runtime_input_revision
        {
            return CommitOutcome::Superseded;
        }
        let mut runtime = self.runtime.lock().expect("project runtime poisoned");

        // Activation is non-awaiting and remains inside the same source/runtime
        // critical section as the revision comparison and install.
        candidate.engine.activate_profiling();
        runtime.next_generation = runtime.next_generation.wrapping_add(1);
        let generation = runtime.next_generation;
        runtime.invalidate_derived();
        let engine = Arc::new(candidate.engine);
        runtime.installed = Some(InstalledEngine {
            source_revision: candidate.source_revision,
            runtime_input_revision: candidate.runtime_input_revision,
            generation,
            engine: engine.clone(),
        });

        CommitOutcome::Committed(CommitReceipt {
            source_revision: candidate.source_revision,
            runtime_input_revision: candidate.runtime_input_revision,
            generation,
            engine,
        })
    }

    fn set_build_phase(&self, key: BuildKey, phase: RuntimeBuildPhase) {
        let _publication = self
            .publication_barrier
            .lock()
            .expect("publication barrier poisoned");
        if self.current_build_key() != key {
            return;
        }
        let mut build = self.build.lock().expect("build coordinator poisoned");
        build.status = Some((key, phase));
        build.status_epoch = build.status_epoch.wrapping_add(1);
        if phase != RuntimeBuildPhase::Failed {
            build.failure = None;
        }
    }

    fn current_build_key(&self) -> BuildKey {
        BuildKey {
            source_revision: self.source_revision(),
            runtime_input_revision: self.runtime_input_revision.load(Ordering::Acquire),
        }
    }

    /// Advance configuration/environment identity. This is serialized with
    /// source/runtime publication so an old engine, registry, or derived
    /// payload becomes non-current immediately, while already-started D8 runs
    /// keep their pinned engine.
    pub(crate) fn advance_runtime_inputs(&self) -> u64 {
        let _publication = self
            .publication_barrier
            .lock()
            .expect("publication barrier poisoned");
        let mut runtime = self.runtime.lock().expect("project runtime poisoned");
        let revision = self
            .runtime_input_revision
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        runtime.invalidate_derived();
        drop(runtime);

        let mut build = self.build.lock().expect("build coordinator poisoned");
        build.status = None;
        build.failure = None;
        build.status_epoch = build.status_epoch.wrapping_add(1);
        revision
    }

    fn request_engine_build_inner(&self, retry_terminal: bool) -> bool {
        let Ok(_publication) = self.publication_barrier.lock() else {
            self.mark_broken("locking publication barrier for engine demand");
            return false;
        };
        let requested_key = self.current_build_key();
        if self.current_receipt().is_some_and(|receipt| {
            receipt.source_revision == requested_key.source_revision
                && receipt.runtime_input_revision == requested_key.runtime_input_revision
        }) {
            return false;
        }
        let Ok(mut build) = self.build.lock() else {
            self.mark_broken("requesting an engine build");
            return false;
        };
        if let Some((key, phase)) = &build.status
            && *key == requested_key
        {
            match phase {
                RuntimeBuildPhase::Failed if retry_terminal => {}
                // Invalid source is never retryable without a source change.
                RuntimeBuildPhase::BlockedByDiagnostics
                | RuntimeBuildPhase::Failed
                | RuntimeBuildPhase::Building
                | RuntimeBuildPhase::Ready
                | RuntimeBuildPhase::IdleStale => return false,
            }
        }
        // Reserve this revision's single flight before releasing the
        // coordinator lock. The spawned worker will perform or join the
        // actual build; subsequent Ensure requests return `building`.
        build.status = Some((requested_key, RuntimeBuildPhase::Building));
        build.failure = None;
        build.status_epoch = build.status_epoch.wrapping_add(1);
        true
    }

    pub(crate) fn request_engine_build(&self) -> bool {
        self.request_engine_build_inner(false)
    }

    pub(crate) fn request_engine_retry(&self) -> bool {
        self.request_engine_build_inner(true)
    }

    /// Join or perform the one build flight for the latest project revision.
    ///
    /// A newer source revision supersedes an older candidate and causes all
    /// waiters to follow the latest revision.  The method is synchronous so it
    /// can serve native and WASM call sites; native LSP scheduling invokes it
    /// from `spawn_blocking`.
    pub(crate) fn ensure_engine_current(&self) -> Result<CommitReceipt, RuntimeError> {
        self.ensure_engine_current_with_constructor(|compiled, runtime_input_revision| {
            self.construct_engine_candidate(compiled, runtime_input_revision)
        })
    }

    fn ensure_engine_current_with_constructor(
        &self,
        mut construct: impl FnMut(CompiledCandidate, u64) -> Result<EngineCandidate, RuntimeError>,
    ) -> Result<CommitReceipt, RuntimeError> {
        loop {
            let requested_key = self.current_build_key();
            if let Some(receipt) = self.current_receipt() {
                if receipt.source_revision == requested_key.source_revision
                    && receipt.runtime_input_revision == requested_key.runtime_input_revision
                {
                    return Ok(receipt);
                }
            }

            let mut build = self.build.lock().map_err(|_| RuntimeError::Compilation {
                message: "Build coordinator is poisoned".to_string(),
            })?;
            if let Some((key, phase)) = &build.status
                && *key == requested_key
                && matches!(
                    phase,
                    RuntimeBuildPhase::BlockedByDiagnostics | RuntimeBuildPhase::Failed
                )
            {
                return Err(RuntimeError::Compilation {
                    message: match phase {
                        RuntimeBuildPhase::BlockedByDiagnostics => {
                            "Current source is blocked by diagnostics".to_string()
                        }
                        RuntimeBuildPhase::Failed => {
                            "Current source build previously failed; explicit retry required"
                                .to_string()
                        }
                        _ => unreachable!(),
                    },
                });
            }
            if build.active_key.is_some() {
                build = self
                    .build_finished
                    .wait(build)
                    .map_err(|_| RuntimeError::Compilation {
                        message: "Build coordinator is poisoned".to_string(),
                    })?;
                drop(build);
                continue;
            }
            build.active_key = Some(requested_key);
            drop(build);

            self.set_build_phase(requested_key, RuntimeBuildPhase::Building);
            let outcome = self
                .compile_candidate(requested_key.source_revision)
                .and_then(|compiled| match compiled {
                    CompilationOutcome::Ready(compiled) => {
                        construct(*compiled, requested_key.runtime_input_revision)
                            .map(|candidate| Some(self.commit_engine_if_current(candidate)))
                    }
                    CompilationOutcome::BlockedByDiagnostics(revision) => {
                        debug_assert_eq!(revision, requested_key.source_revision);
                        self.set_build_phase(
                            requested_key,
                            RuntimeBuildPhase::BlockedByDiagnostics,
                        );
                        Ok(None)
                    }
                    CompilationOutcome::Superseded => Ok(Some(CommitOutcome::Superseded)),
                });

            {
                let _publication =
                    self.publication_barrier
                        .lock()
                        .map_err(|_| RuntimeError::Compilation {
                            message: "Publication barrier is poisoned".to_string(),
                        })?;
                let mut build = self.build.lock().map_err(|_| RuntimeError::Compilation {
                    message: "Build coordinator is poisoned".to_string(),
                })?;
                // Candidate construction (including `$init`) runs without the
                // source gate. An edit may therefore supersede this flight
                // before a constructor error or blocked-diagnostics outcome
                // returns. Such a stale outcome is not the result of the
                // current source: wake joiners and let this caller follow the
                // newer revision instead of surfacing or memoizing it.
                let superseded = self.current_build_key() != requested_key;
                if !superseded && let Err(error) = &outcome {
                    let message = error.to_string();
                    build.status = Some((requested_key, RuntimeBuildPhase::Failed));
                    build.failure = Some((requested_key, message));
                    build.status_epoch = build.status_epoch.wrapping_add(1);
                }
                build.active_key = None;
                self.build_finished.notify_all();
                if superseded {
                    drop(build);
                    continue;
                }
            }

            match outcome {
                Ok(Some(CommitOutcome::Committed(receipt))) => {
                    self.set_build_phase(
                        BuildKey {
                            source_revision: receipt.source_revision,
                            runtime_input_revision: receipt.runtime_input_revision,
                        },
                        RuntimeBuildPhase::Ready,
                    );
                    return Ok(receipt);
                }
                Ok(Some(CommitOutcome::Superseded)) => continue,
                Ok(None) => {
                    return Err(RuntimeError::Compilation {
                        message: "Cannot generate bytecode: diagnostic errors present".to_string(),
                    });
                }
                Err(error) => {
                    return Err(error);
                }
            }
        }
    }

    fn current_receipt(&self) -> Option<CommitReceipt> {
        let source = self.db.lock().ok()?;
        let runtime = self.runtime.lock().ok()?;
        let installed = runtime.installed.as_ref()?;
        let runtime_input_revision = self.runtime_input_revision.load(Ordering::Acquire);
        (installed.source_revision == source.revision
            && installed.runtime_input_revision == runtime_input_revision)
            .then(|| CommitReceipt {
                source_revision: installed.source_revision,
                runtime_input_revision: installed.runtime_input_revision,
                generation: installed.generation,
                engine: installed.engine.clone(),
            })
    }

    pub(crate) fn runtime_status(&self) -> RuntimeStatus {
        let source = self.db.lock().expect("project database poisoned");
        self.runtime_status_with_source(&source)
    }

    pub(crate) fn runtime_status_with_source(
        &self,
        source: &ProjectDatabaseState,
    ) -> RuntimeStatus {
        self.runtime_status_and_identity_with_source(source).0
    }

    pub(crate) fn runtime_status_and_identity_with_source(
        &self,
        source: &ProjectDatabaseState,
    ) -> (RuntimeStatus, ProjectPublicationIdentity) {
        let requested_revision = source.revision;
        // Runtime-input identity advances while holding `runtime`. Capture it
        // under that same guard so a concurrent environment/configuration
        // change cannot make an old installed engine appear current in a
        // direct status response (mailbox publication has a second fence, but
        // command responses use this snapshot directly).
        let runtime = self.runtime.lock().expect("project runtime poisoned");
        let requested_key = BuildKey {
            source_revision: requested_revision,
            runtime_input_revision: self.runtime_input_revision.load(Ordering::Acquire),
        };
        let installed_revision = runtime
            .installed
            .as_ref()
            .map(|engine| engine.source_revision);
        let generation = runtime.installed.as_ref().map(|engine| engine.generation);
        let has_last_known_good = runtime.installed.is_some();
        let is_current = runtime.installed.as_ref().is_some_and(|installed| {
            installed.source_revision == requested_revision
                && installed.runtime_input_revision == requested_key.runtime_input_revision
        });
        let derived_epoch = runtime.derived_epoch;
        drop(runtime);
        let build = self.build.lock().expect("build coordinator poisoned");
        let phase = if is_current {
            RuntimeBuildPhase::Ready
        } else {
            build
                .status
                .as_ref()
                .filter(|(key, _)| *key == requested_key)
                .map(|(_, phase)| *phase)
                .unwrap_or(RuntimeBuildPhase::IdleStale)
        };
        let error_message = (phase == RuntimeBuildPhase::Failed)
            .then(|| build.failure.clone())
            .flatten()
            .and_then(|(key, message)| (key == requested_key).then_some(message));
        let build_epoch = build.status_epoch;
        drop(build);
        let identity = ProjectPublicationIdentity {
            source_revision: requested_revision,
            engine_generation: is_current.then_some(generation).flatten(),
            derived_epoch,
            collection_epoch: None,
            build_epoch: Some(build_epoch),
            runtime_fenced: true,
        };
        (
            RuntimeStatus {
                phase,
                requested_revision,
                installed_revision,
                generation,
                has_last_known_good,
                error_message,
            },
            identity,
        )
    }

    pub(crate) fn current_generation(&self) -> u64 {
        self.current_receipt()
            .map_or(0, |receipt| receipt.generation)
    }

    /// New callers may only obtain an engine matching current source.  Pinned
    /// older runs resolve their engine through the active-run registry.
    pub(crate) fn get_bex(&self) -> Result<Arc<BexEngine>, RuntimeError> {
        self.current_receipt()
            .map(|receipt| receipt.engine)
            .ok_or(RuntimeError::Compilation {
                message: "No engine for the current source revision".to_string(),
            })
    }

    pub(crate) fn take(self) -> Result<Arc<BexEngine>, RuntimeError> {
        let revision = SourceRevision(self.source_revision_observer.load(Ordering::Acquire));
        let runtime_input_revision = self.runtime_input_revision.load(Ordering::Acquire);
        let runtime = self
            .runtime
            .into_inner()
            .map_err(|_| RuntimeError::Compilation {
                message: "Project runtime is poisoned".to_string(),
            })?;
        runtime
            .installed
            .filter(|installed| {
                installed.source_revision == revision
                    && installed.runtime_input_revision == runtime_input_revision
            })
            .map(|installed| installed.engine)
            .ok_or(RuntimeError::Compilation {
                message: "Bex is outdated".to_string(),
            })
    }

    pub(crate) fn begin_test_collection(&self) -> Option<CollectionLease> {
        let _publication = self.publication_barrier.lock().ok()?;
        let source = self.db.lock().ok()?;
        let mut runtime = self.runtime.lock().ok()?;
        let installed = runtime.installed.clone()?;
        if installed.source_revision != source.revision
            || installed.runtime_input_revision
                != self.runtime_input_revision.load(Ordering::Acquire)
        {
            return None;
        }
        if runtime.collection_publication_pending.is_some() {
            return None;
        }
        runtime.collection_epoch = runtime.collection_epoch.wrapping_add(1);
        runtime.collection_in_flight = Some(runtime.collection_epoch);
        runtime.derived_cancel.cancel();
        runtime.derived_cancel = sys_types::CancellationToken::new();
        let previous_registry_mutation_owner = runtime
            .registry
            .as_ref()
            .map(|registry| registry.mutation_owner.clone());
        Some(CollectionLease {
            source_revision: installed.source_revision,
            generation: installed.generation,
            derived_epoch: runtime.derived_epoch,
            collection_epoch: runtime.collection_epoch,
            engine: installed.engine,
            cancel: runtime.derived_cancel.clone(),
            previous_registry_mutation_owner,
        })
    }

    pub(crate) fn test_collection_owner(&self) -> Arc<futures::lock::Mutex<()>> {
        self.collection_owner.clone()
    }

    /// Install a freshly collected registry and reserve its matching tree
    /// publication as one conditional transaction. If transport delivery
    /// later fails, the caller rolls this receipt back so the backend registry
    /// remains aligned with the tree retained by the client.
    pub(crate) fn install_test_registry_and_enqueue(
        &self,
        lease: &CollectionLease,
        registry: Option<Handle>,
        publications: Vec<ProjectPublication>,
    ) -> Result<RegistryPublicationReceipt, PublicationEnqueueError> {
        if publications.is_empty() || publications.len() > PUBLICATION_MAILBOX_MAX_ITEMS {
            return Err(PublicationEnqueueError::Oversized);
        }
        let publication_bytes = publications
            .iter()
            .map(ProjectPublication::serialized_bytes)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| PublicationEnqueueError::Serialization)?
            .into_iter()
            .map(|bytes| bytes.len())
            .collect::<Vec<_>>();
        if publication_bytes
            .iter()
            .any(|bytes| *bytes > PUBLICATION_MAILBOX_MAX_ITEM_BYTES)
        {
            return Err(PublicationEnqueueError::Oversized);
        }
        let batch_bytes = publication_bytes.iter().sum::<usize>();
        if batch_bytes > PUBLICATION_MAILBOX_MAX_BYTES {
            return Err(PublicationEnqueueError::Oversized);
        }
        let Ok(_publication) = self.publication_barrier.lock() else {
            return Err(PublicationEnqueueError::Stale);
        };
        let Ok(source) = self.db.lock() else {
            return Err(PublicationEnqueueError::Stale);
        };
        let Ok(mut runtime) = self.runtime.lock() else {
            return Err(PublicationEnqueueError::Stale);
        };
        let current = source.revision == lease.source_revision
            && runtime.collection_epoch == lease.collection_epoch
            && runtime.collection_in_flight == Some(lease.collection_epoch)
            && runtime.installed.as_ref().is_some_and(|installed| {
                installed.source_revision == lease.source_revision
                    && installed.generation == lease.generation
            });
        if !current {
            return Err(PublicationEnqueueError::Stale);
        }
        let Ok(mut mailbox) = self.publication_mailbox.lock() else {
            return Err(PublicationEnqueueError::Stale);
        };
        if mailbox.queue.len().saturating_add(publications.len()) > PUBLICATION_MAILBOX_MAX_ITEMS
            || mailbox.bytes.saturating_add(batch_bytes) > PUBLICATION_MAILBOX_MAX_BYTES
        {
            return Err(PublicationEnqueueError::Saturated);
        }
        let batch_id =
            PublicationBatchId(self.next_publication_batch.fetch_add(1, Ordering::AcqRel));
        let batch_len = publications.len();
        for (publication, bytes) in publications.into_iter().zip(publication_bytes) {
            mailbox.bytes += bytes;
            mailbox.queue.push_back(ProjectPublicationEnvelope {
                identity: Self::collection_publication_identity(lease),
                batch_id,
                batch_len,
                bytes,
                publication,
            });
        }
        let previous = std::mem::replace(
            &mut runtime.registry,
            registry.map(|handle| InstalledRegistry {
                source_revision: lease.source_revision,
                generation: lease.generation,
                collection_epoch: lease.collection_epoch,
                handle,
                mutation_owner: Arc::new(futures::lock::Mutex::new(())),
            }),
        );
        runtime.collection_in_flight = None;
        runtime.collection_publication_pending = Some(lease.collection_epoch);
        Ok(RegistryPublicationReceipt {
            batch_id,
            source_revision: lease.source_revision,
            generation: lease.generation,
            collection_epoch: lease.collection_epoch,
            previous,
        })
    }

    pub(crate) fn rollback_test_registry_publication(&self, receipt: RegistryPublicationReceipt) {
        let Ok(_publication) = self.publication_barrier.lock() else {
            return;
        };
        let Ok(source) = self.db.lock() else {
            return;
        };
        let Ok(mut runtime) = self.runtime.lock() else {
            return;
        };
        let installed_matches = runtime.installed.as_ref().is_some_and(|installed| {
            installed.source_revision == receipt.source_revision
                && installed.generation == receipt.generation
        });
        let registry_matches = runtime.registry.as_ref().is_none_or(|registry| {
            registry.source_revision == receipt.source_revision
                && registry.generation == receipt.generation
                && registry.collection_epoch == receipt.collection_epoch
        });
        if source.revision == receipt.source_revision
            && runtime.collection_epoch == receipt.collection_epoch
            && installed_matches
            && registry_matches
        {
            runtime.registry = receipt.previous.map(|mut previous| {
                // The retained tree is now the terminal registry for this
                // (failed-delivery) collection epoch. Rebasing its fence keeps
                // old canceled expansion leases stale while allowing a fresh
                // expansion lease to serialize the retained tree later.
                previous.collection_epoch = receipt.collection_epoch;
                previous
            });
            runtime.collection_publication_pending = None;
        }
    }

    pub(crate) fn acknowledge_test_registry_publication(
        &self,
        receipt: &RegistryPublicationReceipt,
    ) -> bool {
        let Ok(_publication) = self.publication_barrier.lock() else {
            return false;
        };
        let Ok(source) = self.db.lock() else {
            return false;
        };
        let Ok(mut runtime) = self.runtime.lock() else {
            return false;
        };
        let registry_matches = runtime.registry.as_ref().is_none_or(|registry| {
            registry.source_revision == receipt.source_revision
                && registry.generation == receipt.generation
                && registry.collection_epoch == receipt.collection_epoch
        });
        if source.revision == receipt.source_revision
            && runtime.collection_epoch == receipt.collection_epoch
            && runtime.collection_publication_pending == Some(receipt.collection_epoch)
            && registry_matches
        {
            runtime.collection_publication_pending = None;
            true
        } else {
            false
        }
    }

    /// Finish a collection that did not install a registry. Only the exact
    /// in-flight collection may clear the marker; a stale completion can
    /// never make a newer collection's retained registry launchable.
    pub(crate) fn finish_test_collection(&self, lease: &CollectionLease) -> bool {
        let Ok(_publication) = self.publication_barrier.lock() else {
            return false;
        };
        let Ok(source) = self.db.lock() else {
            return false;
        };
        let Ok(mut runtime) = self.runtime.lock() else {
            return false;
        };
        let installed_matches = runtime.installed.as_ref().is_some_and(|installed| {
            installed.source_revision == lease.source_revision
                && installed.generation == lease.generation
        });
        if source.revision == lease.source_revision
            && installed_matches
            && runtime.collection_in_flight == Some(lease.collection_epoch)
        {
            if let Some(registry) = runtime.registry.as_mut()
                && registry.source_revision == lease.source_revision
                && registry.generation == lease.generation
            {
                registry.collection_epoch = lease.collection_epoch;
            }
            runtime.collection_in_flight = None;
            true
        } else {
            false
        }
    }

    /// If rollback of an in-place registry mutation itself fails, make the
    /// exact registry non-launchable. The client may retain its prior tree,
    /// but the backend must never run against a tree it could not restore.
    pub(crate) fn discard_registry_if_current(&self, lease: &RegistryLease) -> bool {
        let Ok(_publication) = self.publication_barrier.lock() else {
            return false;
        };
        let Ok(source) = self.db.lock() else {
            return false;
        };
        let Ok(mut runtime) = self.runtime.lock() else {
            return false;
        };
        let installed_matches = runtime.installed.as_ref().is_some_and(|installed| {
            installed.source_revision == lease.source_revision
                && installed.generation == lease.generation
        });
        let registry_matches = runtime.registry.as_ref().is_some_and(|registry| {
            registry.source_revision == lease.source_revision
                && registry.generation == lease.generation
                && registry.collection_epoch == lease.collection_epoch
                && registry.handle == lease.registry
        });
        if source.revision == lease.source_revision && installed_matches && registry_matches {
            runtime.registry = None;
            true
        } else {
            false
        }
    }

    /// Swap a copy-expanded registry and reserve its matching serialized tree
    /// under the same project-derived publication fence.
    #[cfg(test)]
    pub(crate) fn install_expanded_registry_and_enqueue(
        &self,
        lease: &RegistryLease,
        expanded_registry: Handle,
        publications: Vec<ProjectPublication>,
    ) -> Result<ExpansionPublicationReceipt, PublicationEnqueueError> {
        if publications.is_empty() || publications.len() > PUBLICATION_MAILBOX_MAX_ITEMS {
            return Err(PublicationEnqueueError::Oversized);
        }
        let publication_bytes = publications
            .iter()
            .map(ProjectPublication::serialized_bytes)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| PublicationEnqueueError::Serialization)?
            .into_iter()
            .map(|bytes| bytes.len())
            .collect::<Vec<_>>();
        if publication_bytes
            .iter()
            .any(|bytes| *bytes > PUBLICATION_MAILBOX_MAX_ITEM_BYTES)
        {
            return Err(PublicationEnqueueError::Oversized);
        }
        let batch_bytes = publication_bytes.iter().sum::<usize>();
        if batch_bytes > PUBLICATION_MAILBOX_MAX_BYTES {
            return Err(PublicationEnqueueError::Oversized);
        }
        let Ok(_publication) = self.publication_barrier.lock() else {
            return Err(PublicationEnqueueError::Stale);
        };
        let Ok(source) = self.db.lock() else {
            return Err(PublicationEnqueueError::Stale);
        };
        let Ok(mut runtime) = self.runtime.lock() else {
            return Err(PublicationEnqueueError::Stale);
        };
        let installed_matches = runtime.installed.as_ref().is_some_and(|installed| {
            installed.source_revision == lease.source_revision
                && installed.generation == lease.generation
        });
        let Some(registry) = runtime.registry.as_mut() else {
            return Err(PublicationEnqueueError::Stale);
        };
        if source.revision != lease.source_revision
            || !installed_matches
            || registry.source_revision != lease.source_revision
            || registry.generation != lease.generation
            || registry.collection_epoch != lease.collection_epoch
            || registry.handle != lease.registry
        {
            return Err(PublicationEnqueueError::Stale);
        }
        let Ok(mut mailbox) = self.publication_mailbox.lock() else {
            return Err(PublicationEnqueueError::Stale);
        };
        if mailbox.queue.len().saturating_add(publications.len()) > PUBLICATION_MAILBOX_MAX_ITEMS
            || mailbox.bytes.saturating_add(batch_bytes) > PUBLICATION_MAILBOX_MAX_BYTES
        {
            return Err(PublicationEnqueueError::Saturated);
        }
        let batch_id =
            PublicationBatchId(self.next_publication_batch.fetch_add(1, Ordering::AcqRel));
        let batch_len = publications.len();
        for (publication, bytes) in publications.into_iter().zip(publication_bytes) {
            mailbox.bytes += bytes;
            mailbox.queue.push_back(ProjectPublicationEnvelope {
                identity: Self::registry_publication_identity(lease),
                batch_id,
                batch_len,
                bytes,
                publication,
            });
        }
        let previous = std::mem::replace(&mut registry.handle, expanded_registry.clone());
        Ok(ExpansionPublicationReceipt {
            source_revision: lease.source_revision,
            generation: lease.generation,
            collection_epoch: lease.collection_epoch,
            previous,
            installed: expanded_registry,
        })
    }

    #[cfg(test)]
    pub(crate) fn rollback_expanded_registry_publication(
        &self,
        receipt: ExpansionPublicationReceipt,
    ) {
        let Ok(_publication) = self.publication_barrier.lock() else {
            return;
        };
        let Ok(source) = self.db.lock() else {
            return;
        };
        let Ok(mut runtime) = self.runtime.lock() else {
            return;
        };
        let installed_matches = runtime.installed.as_ref().is_some_and(|installed| {
            installed.source_revision == receipt.source_revision
                && installed.generation == receipt.generation
        });
        let Some(registry) = runtime.registry.as_mut() else {
            return;
        };
        if source.revision == receipt.source_revision
            && installed_matches
            && registry.source_revision == receipt.source_revision
            && registry.generation == receipt.generation
            && registry.collection_epoch == receipt.collection_epoch
            && registry.handle == receipt.installed
        {
            registry.handle = receipt.previous;
        }
    }

    pub(crate) fn registry_lease(&self, generation: u64) -> Option<RegistryLease> {
        let source = self.db.lock().ok()?;
        let runtime = self.runtime.lock().ok()?;
        if runtime.collection_in_flight.is_some()
            || runtime.collection_publication_pending.is_some()
        {
            return None;
        }
        let installed = runtime.installed.as_ref()?;
        let registry = runtime.registry.as_ref()?;
        if installed.source_revision != source.revision
            || installed.generation != generation
            || registry.source_revision != source.revision
            || registry.generation != generation
            || registry.collection_epoch != runtime.collection_epoch
        {
            return None;
        }
        Some(RegistryLease {
            source_revision: source.revision,
            generation,
            derived_epoch: runtime.derived_epoch,
            collection_epoch: registry.collection_epoch,
            engine: installed.engine.clone(),
            registry: registry.handle.clone(),
            cancel: runtime.derived_cancel.clone(),
            mutation_owner: registry.mutation_owner.clone(),
        })
    }

    pub(crate) fn registry_lease_is_current(&self, lease: &RegistryLease) -> bool {
        let Ok(source) = self.db.lock() else {
            return false;
        };
        let Ok(runtime) = self.runtime.lock() else {
            return false;
        };
        source.revision == lease.source_revision
            && runtime.collection_in_flight.is_none()
            && runtime.collection_publication_pending.is_none()
            && runtime.collection_epoch == lease.collection_epoch
            && !lease.cancel.is_cancelled()
            && runtime.installed.as_ref().is_some_and(|installed| {
                installed.source_revision == lease.source_revision
                    && installed.generation == lease.generation
            })
            && runtime.registry.as_ref().is_some_and(|registry| {
                registry.source_revision == lease.source_revision
                    && registry.generation == lease.generation
                    && registry.collection_epoch == lease.collection_epoch
            })
    }

    /// Build and register a run against one coherent source/runtime snapshot.
    pub(crate) fn prepare_and_register_run(
        &self,
        call_id: sys_types::CallId,
        function_name: Option<&str>,
        required_generation: Option<u64>,
        require_test_registry: bool,
    ) -> Result<RunLease, RuntimeError> {
        let (source_revision, installed, graph) = {
            let source = self.db.lock().map_err(|_| RuntimeError::Compilation {
                message: "Project database is poisoned".to_string(),
            })?;
            let runtime = self.runtime.lock().map_err(|_| RuntimeError::Compilation {
                message: "Project runtime is poisoned".to_string(),
            })?;
            let runtime_input_revision = self.runtime_input_revision.load(Ordering::Acquire);
            let installed = runtime
                .installed
                .clone()
                .filter(|installed| {
                    installed.source_revision == source.revision
                        && installed.runtime_input_revision == runtime_input_revision
                })
                .ok_or(RuntimeError::Compilation {
                    message: "Current source must be built before starting a run".to_string(),
                })?;
            if required_generation.is_some_and(|generation| generation != installed.generation) {
                return Err(RuntimeError::Compilation {
                    message: "Requested run generation is stale".to_string(),
                });
            }
            drop(runtime);
            let graph = function_name.and_then(|function_name| {
                source.ast_control_flow_graph(function_name).map(|graph| {
                    Arc::new(
                        baml_compiler2_visualization::control_flow::prepare_control_flow_graph_for_visualization(
                            &graph,
                        ),
                    )
                })
            });
            (source.revision, installed, graph)
        };

        // Re-enter the source/runtime registration transaction after the
        // owned CFG candidate has been produced and the database guard dropped.
        let source = self.db.lock().map_err(|_| RuntimeError::Compilation {
            message: "Project database is poisoned".to_string(),
        })?;
        let runtime = self.runtime.lock().map_err(|_| RuntimeError::Compilation {
            message: "Project runtime is poisoned".to_string(),
        })?;
        let runtime_input_revision = self.runtime_input_revision.load(Ordering::Acquire);
        let current = runtime.installed.as_ref().is_some_and(|current| {
            source.revision == source_revision
                && current.source_revision == source_revision
                && current.runtime_input_revision == runtime_input_revision
                && current.generation == installed.generation
        });
        if !current {
            return Err(RuntimeError::Compilation {
                message: "Source changed while preparing the run; build the latest revision"
                    .to_string(),
            });
        }
        let registry = if require_test_registry {
            if runtime.collection_in_flight.is_some()
                || runtime.collection_publication_pending.is_some()
            {
                return Err(RuntimeError::Compilation {
                    message: "Test registry collection is not yet acknowledged".to_string(),
                });
            }
            let registry = runtime.registry.as_ref().filter(|registry| {
                registry.source_revision == source_revision
                    && registry.generation == installed.generation
                    && registry.collection_epoch == runtime.collection_epoch
            });
            Some(
                registry
                    .ok_or(RuntimeError::Compilation {
                        message: "No current test registry".to_string(),
                    })?
                    .handle
                    .clone(),
            )
        } else {
            None
        };

        let lease = RunLease {
            source_revision,
            generation: installed.generation,
            engine: installed.engine,
            test_registry: registry,
            function_name: function_name.map(ToOwned::to_owned),
            graph: graph.clone(),
        };
        self.active_runs
            .lock()
            .map_err(|_| RuntimeError::Compilation {
                message: "Active-run registry is poisoned".to_string(),
            })?
            .insert(call_id, lease.clone());
        drop(runtime);
        drop(source);

        if let (Some(function_name), Some(graph)) = (function_name, graph) {
            self.run_cfgs
                .lock()
                .expect("run CFG cache poisoned")
                .insert(lease.generation, function_name, graph);
        }
        Ok(lease)
    }

    pub(crate) fn finish_run(&self, call_id: sys_types::CallId) -> bool {
        self.active_runs
            .lock()
            .is_ok_and(|mut runs| runs.remove(&call_id).is_some())
    }

    pub(crate) fn active_run(&self, call_id: sys_types::CallId) -> Option<RunLease> {
        self.active_runs.lock().ok()?.get(&call_id).cloned()
    }

    pub(crate) fn cancel_run(
        &self,
        call_id: sys_types::CallId,
    ) -> Result<(), bex_engine::EngineError> {
        let engine = self
            .active_runs
            .lock()
            .ok()
            .and_then(|runs| runs.get(&call_id).map(|lease| lease.engine.clone()))
            .ok_or_else(|| bex_engine::EngineError::FunctionNotFound {
                name: format!("active run {call_id} not found"),
            })?;
        engine.cancel_function_call(call_id)
    }

    pub(crate) fn control_flow_graph_for_generation(
        &self,
        generation: u64,
        function_name: &str,
    ) -> Option<PreparedGraph> {
        if let Ok(runs) = self.active_runs.lock()
            && let Some(graph) = runs.values().find_map(|lease| {
                (lease.generation == generation
                    && lease.function_name.as_deref() == Some(function_name))
                .then(|| lease.graph.clone())
                .flatten()
            })
        {
            return Some(graph);
        }
        if let Some(graph) = self.run_cfgs.lock().ok()?.graph(generation, function_name) {
            return Some(graph);
        }

        let source = self.db.lock().ok()?;
        let runtime = self.runtime.lock().ok()?;
        let installed = runtime.installed.as_ref()?;
        if installed.source_revision != source.revision || installed.generation != generation {
            return None;
        }
        drop(runtime);
        let graph = Arc::new(
            baml_compiler2_visualization::control_flow::prepare_control_flow_graph_for_visualization(
                &source.ast_control_flow_graph(function_name)?,
            ),
        );
        drop(source);

        let mut cache = self.run_cfgs.lock().ok()?;
        // Revalidate before caching a graph made from the live database.
        if self.current_generation() != generation {
            return None;
        }
        cache.insert(generation, function_name, graph.clone());
        Some(graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHeap;

    impl bex_external_types::WeakHeapRef for TestHeap {
        fn release_handle(&self, _slab_key: usize) {}

        fn resolve_handle_ptr(&self, _slab_key: usize) -> Option<bex_vm_types::HeapPtr> {
            None
        }
    }

    fn test_handle(slab_key: usize) -> Handle {
        Handle::new(slab_key, Arc::new(TestHeap))
    }

    fn collection_publication(context: serde_json::Value) -> ProjectPublication {
        ProjectPublication::Playground {
            session_epoch: 1,
            notification: crate::bex_lsp::PlaygroundNotification::CursorContext { context },
        }
    }

    fn project() -> Arc<BexProject> {
        let root = vfs::VfsPath::new(vfs::MemoryFS::new());
        Arc::new(BexProject::new(
            &root,
            Arc::new(sys_ops::SysOpsBuilder::new().build()),
        ))
    }

    fn sources(text: &str) -> HashMap<FsPath, String> {
        HashMap::from([(FsPath::from_str("/main.baml".to_string()), text.to_string())])
    }

    fn candidate_for_current(project: &BexProject) -> EngineCandidate {
        static PROGRAM: std::sync::OnceLock<bex_vm_types::Program> = std::sync::OnceLock::new();
        let revision = project.source_revision();
        let program = PROGRAM
            .get_or_init(|| {
                let CompilationOutcome::Ready(compiled) = project
                    .compile_candidate(revision)
                    .expect("valid source should compile")
                else {
                    panic!("valid source should produce a compiled candidate");
                };
                compiled.program
            })
            .clone();
        EngineCandidate {
            source_revision: revision,
            runtime_input_revision: project.runtime_input_revision.load(Ordering::Acquire),
            engine: BexEngine::new_inactive(program, project.sys_ops.clone(), Vec::new())
                .expect("compiled candidate should construct an engine"),
        }
    }

    #[test]
    fn run_cfg_cache_evicts_oldest_entries() {
        let mut cache = RunCfgCache::default();
        for generation in 1..=(RETAINED_RUN_CFGS as u64 + 1) {
            cache.insert(
                generation,
                "Workflow",
                Arc::new(baml_compiler2_visualization::control_flow::ControlFlowGraph::default()),
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
        let graph =
            || Arc::new(baml_compiler2_visualization::control_flow::ControlFlowGraph::default());
        for _ in 0..(RETAINED_RUN_CFGS * 2) {
            cache.insert(7, "Workflow", graph());
        }
        cache.insert(8, "Workflow", graph());
        assert!(cache.graph(7, "Workflow").is_some());
        assert!(cache.graph(8, "Workflow").is_some());
        assert_eq!(cache.order.len(), 2);
    }

    #[test]
    fn active_run_retains_cfg_after_bounded_cache_evicts_it() {
        let project = project();
        project.apply_all_sources(&sources("function Workflow() -> int { 1 }"));
        let receipt = project
            .ensure_engine_current()
            .expect("valid source should build");
        let call_id = sys_types::CallId::next();
        let lease = project
            .prepare_and_register_run(call_id, Some("Workflow"), None, false)
            .expect("current run should register");
        let retained = lease
            .graph
            .as_ref()
            .expect("function run should pin its CFG");

        let mut cache = project.run_cfgs.lock().expect("run CFG cache");
        for generation in 100..=(100 + RETAINED_RUN_CFGS as u64) {
            cache.insert(
                generation,
                "Other",
                Arc::new(baml_compiler2_visualization::control_flow::ControlFlowGraph::default()),
            );
        }
        assert!(cache.graph(receipt.generation, "Workflow").is_none());
        drop(cache);

        let resolved = project
            .control_flow_graph_for_generation(receipt.generation, "Workflow")
            .expect("active run graph must not depend on the bounded cache");
        assert!(Arc::ptr_eq(retained, &resolved));
        assert!(project.finish_run(call_id));
    }

    #[test]
    fn playground_edit_rejects_an_open_document_overlay() {
        let project = project();
        let path = FsPath::from_str("/main.baml".to_string());
        let uri = lsp_types::Url::parse("file:///main.baml").unwrap();
        let revision = project.apply_open_document(
            path.clone(),
            uri,
            7,
            "function Main() -> int { 1 }".to_string(),
        );

        let error = project
            .apply_playground_source(&path, "function Main() -> int { 2 }")
            .expect_err("out-of-band edits must not rewrite an open overlay");

        assert!(matches!(error, crate::LspError::RequestFailed(_)));
        assert_eq!(project.source_revision(), revision);
        assert_eq!(
            project
                .open_document_sources()
                .get(&path)
                .map(String::as_str),
            Some("function Main() -> int { 1 }")
        );
    }

    #[test]
    fn source_mutation_invalidates_runtime_identity_transactionally() {
        let project = project();
        let sources = sources("function f() -> int { 1 }");

        let first = project.apply_all_sources(&sources);
        let second = project.apply_all_sources(&sources);

        assert_eq!(first, SourceRevision(1));
        assert_eq!(second, SourceRevision(2));
        assert_eq!(project.diagnostics_dirty_revision(), Some(second));
        assert!(!project.clear_diagnostics_if_current(first));
        assert!(project.clear_diagnostics_if_current(second));
    }

    #[test]
    fn queued_project_publication_is_dropped_after_source_advances() {
        let project = project();
        let sources = sources("function f() -> int { 1 }");
        let revision = project.apply_all_sources(&sources);
        assert!(project.enqueue_publication_if_current(
            BexProject::source_publication_identity(revision),
            ProjectPublication::Playground {
                session_epoch: 1,
                notification: crate::bex_lsp::PlaygroundNotification::CursorContext {
                    context: serde_json::Value::Null,
                },
            },
        ));

        project.apply_all_sources(&sources);

        assert!(project.take_next_publication().is_none());
    }

    #[test]
    fn publication_mailbox_rejects_overflow_atomically() {
        let project = project();
        let revision = project.apply_all_sources(&sources("function f() -> int { 1 }"));
        let identity = BexProject::source_publication_identity(revision);
        let publication = || ProjectPublication::Playground {
            session_epoch: 1,
            notification: crate::bex_lsp::PlaygroundNotification::CursorContext {
                context: serde_json::Value::Null,
            },
        };

        let initial = (0..PUBLICATION_MAILBOX_MAX_ITEMS - 1)
            .map(|_| publication())
            .collect();
        assert!(
            project
                .enqueue_publication_batch_if_current(identity, initial)
                .is_ok()
        );
        let usage_before = project.publication_mailbox_usage();
        assert_eq!(usage_before.0, PUBLICATION_MAILBOX_MAX_ITEMS - 1);

        assert!(
            project
                .enqueue_publication_batch_if_current(identity, vec![publication(), publication()],)
                .is_err(),
            "a batch that does not fit must not be partially inserted"
        );
        assert_eq!(project.publication_mailbox_usage(), usage_before);

        assert!(
            project
                .enqueue_publication_batch_if_current(identity, vec![publication()])
                .is_ok()
        );
        assert_eq!(
            project.publication_mailbox_usage().0,
            PUBLICATION_MAILBOX_MAX_ITEMS
        );
        assert!(
            project
                .enqueue_publication_batch_if_current(identity, vec![publication()])
                .is_err()
        );
    }

    #[test]
    fn publication_mailbox_rejects_oversized_envelope_without_growth() {
        let project = project();
        let revision = project.apply_all_sources(&sources("function f() -> int { 1 }"));
        let identity = BexProject::source_publication_identity(revision);
        let oversized = ProjectPublication::Playground {
            session_epoch: 1,
            notification: crate::bex_lsp::PlaygroundNotification::CursorContext {
                context: serde_json::Value::String(
                    "x".repeat(PUBLICATION_MAILBOX_MAX_ITEM_BYTES + 1),
                ),
            },
        };

        assert!(
            project
                .enqueue_publication_batch_if_current(identity, vec![oversized])
                .is_err()
        );
        assert_eq!(project.publication_mailbox_usage(), (0, 0));
    }

    #[test]
    fn registry_install_and_tree_reservation_are_atomic_and_rollbackable() {
        let project = project();
        project.apply_all_sources(&sources("function f() -> int { 1 }"));
        project
            .ensure_engine_current()
            .expect("valid source should build");

        let first = project
            .begin_test_collection()
            .expect("current engine should collect tests");
        let first_receipt = project
            .install_test_registry_and_enqueue(
                &first,
                Some(test_handle(1)),
                vec![collection_publication(serde_json::Value::Null)],
            )
            .expect("first registry and tree should reserve together");
        assert!(
            project.begin_test_collection().is_none(),
            "a newer collection must wait until the installed registry's tree is acknowledged"
        );
        assert!(project.take_next_publication().is_some());
        assert!(project.acknowledge_test_registry_publication(&first_receipt));
        assert_eq!(
            project
                .runtime
                .lock()
                .expect("runtime")
                .registry
                .as_ref()
                .map(|registry| registry.handle.slab_key()),
            Some(1)
        );
        let old_expansion_lease = project
            .registry_lease(project.current_generation())
            .expect("the first installed registry should be expandable");

        let second = project
            .begin_test_collection()
            .expect("a newer collection should retain the prior registry until commit");
        assert!(!project.registry_lease_is_current(&old_expansion_lease));
        assert!(
            project
                .registry_lease(project.current_generation())
                .is_none()
        );
        let oversized = vec![collection_publication(serde_json::Value::String(
            "x".repeat(PUBLICATION_MAILBOX_MAX_ITEM_BYTES + 1),
        ))];
        assert!(matches!(
            project.install_test_registry_and_enqueue(&second, Some(test_handle(2)), oversized,),
            Err(PublicationEnqueueError::Oversized)
        ));
        assert_eq!(
            project
                .runtime
                .lock()
                .expect("runtime")
                .registry
                .as_ref()
                .map(|registry| registry.handle.slab_key()),
            Some(1),
            "an undeliverable tree must retain the previous registry"
        );

        let receipt = project
            .install_test_registry_and_enqueue(
                &second,
                Some(test_handle(2)),
                vec![collection_publication(serde_json::Value::Null)],
            )
            .expect("new registry and tree should reserve together");
        assert_eq!(
            project
                .runtime
                .lock()
                .expect("runtime")
                .registry
                .as_ref()
                .map(|registry| registry.handle.slab_key()),
            Some(2)
        );
        assert!(project.pop_next_publication().is_some());
        project.rollback_test_registry_publication(receipt);
        assert!(
            !project.registry_lease_is_current(&old_expansion_lease),
            "a canceled pre-collection expansion lease must not become current again"
        );
        assert_eq!(
            project
                .runtime
                .lock()
                .expect("runtime")
                .registry
                .as_ref()
                .map(|registry| registry.handle.slab_key()),
            Some(1),
            "failed transport delivery must restore the tree-aligned registry"
        );

        let expansion_lease = project
            .registry_lease(project.current_generation())
            .expect("restored registry should be expandable");
        assert_eq!(expansion_lease.collection_epoch, second.collection_epoch);
        let oversized = vec![collection_publication(serde_json::Value::String(
            "x".repeat(PUBLICATION_MAILBOX_MAX_ITEM_BYTES + 1),
        ))];
        assert!(matches!(
            project.install_expanded_registry_and_enqueue(
                &expansion_lease,
                test_handle(3),
                oversized,
            ),
            Err(PublicationEnqueueError::Oversized)
        ));
        assert_eq!(
            project
                .runtime
                .lock()
                .expect("runtime")
                .registry
                .as_ref()
                .map(|registry| registry.handle.slab_key()),
            Some(1)
        );

        let expansion_receipt = project
            .install_expanded_registry_and_enqueue(
                &expansion_lease,
                test_handle(3),
                vec![collection_publication(serde_json::Value::Null)],
            )
            .expect("expanded registry and tree should reserve together");
        assert_eq!(
            project
                .runtime
                .lock()
                .expect("runtime")
                .registry
                .as_ref()
                .map(|registry| registry.handle.slab_key()),
            Some(3)
        );
        assert!(project.pop_next_publication().is_some());
        project.rollback_expanded_registry_publication(expansion_receipt);
        assert_eq!(
            project
                .runtime
                .lock()
                .expect("runtime")
                .registry
                .as_ref()
                .map(|registry| registry.handle.slab_key()),
            Some(1)
        );
    }

    #[test]
    fn failed_newer_collection_rebases_retained_registry_without_lease_aba() {
        let project = project();
        project.apply_all_sources(&sources("function f() -> int { 1 }"));
        project
            .ensure_engine_current()
            .expect("valid source should build");
        let first = project.begin_test_collection().expect("first collection");
        let first_receipt = project
            .install_test_registry_and_enqueue(
                &first,
                Some(test_handle(1)),
                vec![collection_publication(serde_json::Value::Null)],
            )
            .expect("first registry install");
        let _ = project.pop_next_publication();
        assert!(project.acknowledge_test_registry_publication(&first_receipt));
        let old = project
            .registry_lease(project.current_generation())
            .expect("first registry lease");

        let failed = project.begin_test_collection().expect("newer collection");
        assert!(!project.registry_lease_is_current(&old));
        assert!(project.finish_test_collection(&failed));
        assert!(!project.registry_lease_is_current(&old));

        let retained = project
            .registry_lease(project.current_generation())
            .expect("failed collection should retain a freshly fenced registry");
        assert_eq!(retained.registry.slab_key(), old.registry.slab_key());
        assert_eq!(retained.collection_epoch, failed.collection_epoch);
        assert!(project.registry_lease_is_current(&retained));
    }

    #[test]
    fn queued_collection_publication_is_dropped_after_epoch_advances() {
        let project = project();
        let revision = project.apply_all_sources(&sources("function f() -> int { 1 }"));
        let identity = {
            let runtime = project.runtime.lock().expect("runtime lock");
            ProjectPublicationIdentity {
                source_revision: revision,
                engine_generation: None,
                derived_epoch: runtime.derived_epoch,
                collection_epoch: Some(runtime.collection_epoch),
                build_epoch: None,
                runtime_fenced: true,
            }
        };
        assert!(project.enqueue_publication_if_current(
            identity,
            ProjectPublication::Playground {
                session_epoch: 1,
                notification: crate::bex_lsp::PlaygroundNotification::CursorContext {
                    context: serde_json::Value::Null,
                },
            },
        ));

        project
            .runtime
            .lock()
            .expect("runtime lock")
            .collection_epoch += 1;

        assert!(project.take_next_publication().is_none());
    }

    #[test]
    fn queued_same_source_runtime_update_is_dropped_after_build_phase_advances() {
        let project = project();
        let revision = project.apply_all_sources(&sources("function f() -> int { 1 }"));
        assert!(project.request_engine_build());
        let identity = {
            let source = project.db.lock().expect("source lock");
            let (status, identity) = project.runtime_status_and_identity_with_source(&source);
            assert_eq!(status.phase, RuntimeBuildPhase::Building);
            identity
        };
        assert!(project.enqueue_publication_if_current(
            identity,
            ProjectPublication::Playground {
                session_epoch: 1,
                notification: crate::bex_lsp::PlaygroundNotification::CursorContext {
                    context: serde_json::Value::Null,
                },
            },
        ));

        project.set_build_phase(
            BuildKey {
                source_revision: revision,
                runtime_input_revision: project.runtime_input_revision.load(Ordering::Acquire),
            },
            RuntimeBuildPhase::Failed,
        );

        assert!(project.take_next_publication().is_none());
    }

    #[test]
    fn newest_candidate_wins_in_both_completion_orders() {
        // Old candidate completes first: it is rejected, then R2 installs.
        let old_first = project();
        let r1 = old_first.apply_all_sources(&sources("function f() -> int { 1 }"));
        let old_candidate = candidate_for_current(&old_first);
        let r2 = old_first.apply_all_sources(&sources("function f() -> int { 2 }"));
        let new_candidate = candidate_for_current(&old_first);

        assert!(matches!(
            old_first.commit_engine_if_current(old_candidate),
            CommitOutcome::Superseded
        ));
        let CommitOutcome::Committed(receipt) = old_first.commit_engine_if_current(new_candidate)
        else {
            panic!("R2 should install after the stale R1 candidate is rejected");
        };
        assert_eq!(r1, SourceRevision(1));
        assert_eq!(r2, SourceRevision(2));
        assert_eq!(receipt.source_revision, r2);
        assert_eq!(receipt.generation, 1);

        // New candidate completes first: a late R1 completion must be a true
        // no-op, including generation and derived-cancellation identity.
        let new_first = project();
        new_first.apply_all_sources(&sources("function f() -> int { 1 }"));
        let old_candidate = candidate_for_current(&new_first);
        let r2 = new_first.apply_all_sources(&sources("function f() -> int { 2 }"));
        let new_candidate = candidate_for_current(&new_first);
        let CommitOutcome::Committed(receipt) = new_first.commit_engine_if_current(new_candidate)
        else {
            panic!("R2 should install");
        };
        let (generation_before, epoch_before, cancellation) = {
            let runtime = new_first.runtime.lock().expect("runtime lock");
            (
                runtime.next_generation,
                runtime.derived_epoch,
                runtime.derived_cancel.clone(),
            )
        };

        assert!(matches!(
            new_first.commit_engine_if_current(old_candidate),
            CommitOutcome::Superseded
        ));
        let runtime = new_first.runtime.lock().expect("runtime lock");
        assert_eq!(receipt.source_revision, r2);
        assert_eq!(runtime.next_generation, generation_before);
        assert_eq!(runtime.derived_epoch, epoch_before);
        assert!(!cancellation.is_cancelled());
        assert_eq!(
            runtime.installed.as_ref().map(|engine| engine.generation),
            Some(receipt.generation)
        );
    }

    #[test]
    fn stale_constructor_failure_follows_the_newest_source_revision() {
        let project = project();
        let r1 = project.apply_all_sources(&sources("function f() -> int { 1 }"));
        let mut latest_revision = None;
        let mut first_construction = true;

        let receipt = project
            .ensure_engine_current_with_constructor(|compiled, runtime_input_revision| {
                if first_construction {
                    first_construction = false;
                    assert_eq!(compiled.source_revision, r1);
                    latest_revision =
                        Some(project.apply_all_sources(&sources("function f() -> int { 2 }")));
                    return Err(RuntimeError::Compilation {
                        message: "stale R1 constructor failure".to_string(),
                    });
                }
                project.construct_engine_candidate(compiled, runtime_input_revision)
            })
            .expect("a stale constructor error must follow and build the latest source");

        let r2 = latest_revision.expect("the first construction should install a newer source");
        assert_eq!(r2, SourceRevision(r1.0 + 1));
        assert_eq!(receipt.source_revision, r2);
        assert_eq!(project.runtime_status().phase, RuntimeBuildPhase::Ready);
        assert_eq!(
            project
                .current_receipt()
                .map(|receipt| receipt.source_revision),
            Some(r2)
        );
    }

    #[test]
    fn runtime_input_change_invalidates_failure_and_allows_one_new_flight() {
        let project = project();
        let source_revision = project.apply_all_sources(&sources("function f() -> int { 1 }"));

        let error = project.ensure_engine_current_with_constructor(|_, _| {
            Err(RuntimeError::Compilation {
                message: "missing runtime input".to_string(),
            })
        });
        assert!(error.is_err());
        assert_eq!(project.runtime_status().phase, RuntimeBuildPhase::Failed);
        assert!(
            !project.request_engine_build(),
            "an unchanged failed build key must stay terminal"
        );

        let runtime_input_revision = project.advance_runtime_inputs();
        assert_eq!(runtime_input_revision, 1);
        assert_eq!(project.runtime_status().phase, RuntimeBuildPhase::IdleStale);
        assert!(
            project.request_engine_build(),
            "a new runtime-input key must reserve one fresh flight"
        );
        assert!(
            !project.request_engine_build(),
            "the same runtime-input key must still be single-flight"
        );

        let receipt = project
            .ensure_engine_current()
            .expect("the corrected runtime input should build without a source edit");
        assert_eq!(receipt.source_revision, source_revision);
        assert_eq!(receipt.runtime_input_revision, runtime_input_revision);
        assert_eq!(project.runtime_status().phase, RuntimeBuildPhase::Ready);

        let derived_cancel = project
            .runtime
            .lock()
            .expect("runtime lock")
            .derived_cancel
            .clone();
        project.advance_runtime_inputs();
        assert!(derived_cancel.is_cancelled());
        assert!(project.current_receipt().is_none());
        let stale = project.runtime_status();
        assert_eq!(stale.phase, RuntimeBuildPhase::IdleStale);
        assert!(stale.has_last_known_good);
    }

    #[test]
    fn edit_at_commit_boundary_never_leaves_a_false_current_engine() {
        let project = project();
        let r1 = project.apply_all_sources(&sources("function f() -> int { 1 }"));
        let candidate = candidate_for_current(&project);
        let rendezvous = Arc::new(std::sync::Barrier::new(2));
        let (lock_observation_tx, lock_observation_rx) = std::sync::mpsc::sync_channel(1);
        let edit_project = project.clone();
        let edit_rendezvous = rendezvous.clone();
        let edit = std::thread::spawn(move || {
            edit_rendezvous.wait();
            let commit_holds_source_gate = matches!(
                edit_project.db.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            );
            lock_observation_tx
                .send(commit_holds_source_gate)
                .expect("commit test receiver should stay alive");
            edit_project.apply_all_sources(&sources("function f() -> int { 2 }"))
        });

        let outcome = project.commit_engine_if_current_with_hook(candidate, || {
            rendezvous.wait();
            assert!(
                lock_observation_rx
                    .recv()
                    .expect("edit thread should report lock state"),
                "commit must hold the same source gate required by edits"
            );
        });
        let CommitOutcome::Committed(receipt) = outcome else {
            panic!("commit holding the gate should finish before the queued edit");
        };
        let r2 = edit.join().expect("edit thread should finish");

        assert_eq!(receipt.source_revision, r1);
        assert_eq!(r2, SourceRevision(r1.0 + 1));
        assert!(project.current_receipt().is_none());
        let runtime = project.runtime.lock().expect("runtime lock");
        assert_eq!(
            runtime
                .installed
                .as_ref()
                .map(|installed| installed.source_revision),
            Some(r1),
            "the old engine may remain last-known-good but cannot be current"
        );
        assert_eq!(runtime.derived_epoch, 3);
    }

    #[test]
    fn final_invalid_edit_blocks_stale_engine_install() {
        let project = project();
        let r1 = project.apply_all_sources(&sources("function f() -> int { 1 }"));
        let candidate = candidate_for_current(&project);
        let r2 = project.apply_all_sources(&sources("function f() -> int {"));

        assert!(matches!(
            project
                .compile_candidate(r2)
                .expect("invalid source is a compilation outcome, not an engine failure"),
            CompilationOutcome::BlockedByDiagnostics(revision) if revision == r2
        ));
        assert!(matches!(
            project.commit_engine_if_current(candidate),
            CommitOutcome::Superseded
        ));
        assert_eq!(r1, SourceRevision(1));
        assert_eq!(r2, SourceRevision(2));
        assert!(project.current_receipt().is_none());
        let runtime = project.runtime.lock().expect("runtime lock");
        assert_eq!(runtime.next_generation, 0);
        assert!(runtime.installed.is_none());
    }

    #[test]
    fn runtime_demand_reserves_one_async_build_and_invalid_source_is_not_retryable() {
        let project = project();
        project.apply_all_sources(&sources("function Main() -> int { 1 }"));

        assert!(project.request_engine_build());
        assert_eq!(project.runtime_status().phase, RuntimeBuildPhase::Building);
        assert!(
            !project.request_engine_build(),
            "one revision has one flight"
        );
        project
            .ensure_engine_current()
            .expect("valid source builds");
        assert_eq!(project.runtime_status().phase, RuntimeBuildPhase::Ready);

        project.apply_all_sources(&sources("function Main( -> int {"));
        assert!(project.request_engine_build());
        assert!(project.ensure_engine_current().is_err());
        assert_eq!(
            project.runtime_status().phase,
            RuntimeBuildPhase::BlockedByDiagnostics
        );
        assert!(!project.request_engine_build());
        assert!(
            !project.request_engine_retry(),
            "explicit Retry cannot rebuild unchanged invalid source"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn bounded_request_wait_classifies_busy_by_revision_change() {
        let project = project();
        let source_guard = project.db.lock().expect("source lock");

        let Err(same_revision) = project.try_lock_db_wait_for_request_with_timing(
            std::time::Duration::from_millis(10),
            std::time::Duration::from_millis(1),
            std::time::Duration::from_secs(1),
        ) else {
            panic!("same-revision contention must time out");
        };
        assert!(matches!(same_revision, crate::LspError::RequestFailed(_)));

        // Change the mirror after request capture to exercise the other
        // classification without mutating the source state outside its gate.
        let observer_project = project.clone();
        let observer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2));
            observer_project
                .source_revision_observer
                .fetch_add(1, Ordering::AcqRel);
        });
        let Err(changed_while_waiting) = project.try_lock_db_wait_for_request_with_timing(
            std::time::Duration::from_millis(20),
            std::time::Duration::from_millis(1),
            std::time::Duration::from_secs(1),
        ) else {
            panic!("revision-changing contention must time out");
        };
        observer.join().expect("revision observer thread");
        assert!(matches!(
            changed_while_waiting,
            crate::LspError::ContentModified(_)
        ));
        drop(source_guard);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(panic = "unwind")]
    #[test]
    fn nowait_distinguishes_contention_from_poison() {
        let busy_project = project();
        let source_guard = busy_project.db.lock().expect("source lock");
        assert!(matches!(busy_project.try_lock_db_nowait(), Ok(None)));
        drop(source_guard);

        let poisoned_project = project();
        let poisoner = poisoned_project.clone();
        let poison = std::thread::spawn(move || {
            let _guard = poisoner.db.lock().expect("source lock");
            panic!("poison the project database for the unwind-enabled unit test");
        });
        assert!(poison.join().is_err());
        assert!(matches!(
            poisoned_project.try_lock_db_nowait(),
            Err(crate::LspError::InternalError(_))
        ));
        assert!(poisoned_project.is_broken());
    }
}
