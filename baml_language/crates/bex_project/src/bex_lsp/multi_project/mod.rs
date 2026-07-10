mod commands;
mod diagnostics;
mod notification;
mod request;
mod wasm_helpers;

use std::{collections::HashMap, io::Read};

use ::std::sync::{Arc, OnceLock};
use baml_project::position::PositionEncoding;
use baml_workspace::{BAML_SRC_DIR, BAML_TOML, find_baml_project_root_from_ancestors};
pub use wasm_helpers::BackgroundSpawner;

/// Factory that creates [`sys_ops::SysOps`] for a given project root.
type SysOpFactory =
    std::sync::Arc<dyn Fn(&vfs::VfsPath) -> std::sync::Arc<sys_ops::SysOps> + Send + Sync>;
type ActiveRunRegistry = std::sync::Arc<
    std::sync::Mutex<HashMap<sys_types::CallId, (String, std::sync::Arc<LiveProject>)>>,
>;
type DiagnosticProgress = Arc<
    std::sync::Mutex<
        HashMap<DiagnosticProgressKey, std::collections::HashSet<(crate::fs::FsPath, bool)>>,
    >,
>;
type OpenDocuments =
    Arc<std::sync::Mutex<HashMap<crate::fs::FsPath, crate::project::OpenDocument>>>;
type PublishedDiagnostics =
    Arc<std::sync::Mutex<HashMap<PublishedProjectKey, HashMap<crate::fs::FsPath, lsp_types::Url>>>>;

use crate::{
    RuntimeError,
    bex_lsp::{
        LspError,
        multi_project::diagnostics::{DiagnosticRead, WithDiagnostics},
    },
};

struct LiveProject {
    project: crate::project::BexProject,
    /// Debounce epoch for scheduled engine rebuilds: every refresh bumps it,
    /// and a scheduled rebuild only runs if its captured epoch is still
    /// current after the debounce delay.
    #[cfg(not(target_arch = "wasm32"))]
    rebuild_epoch: std::sync::atomic::AtomicU64,
    #[cfg(not(target_arch = "wasm32"))]
    rebuild_task: std::sync::Mutex<Option<tokio::task::AbortHandle>>,
    /// One replaceable, reserved project tail. Normal request admission never
    /// owns this slot, so the final edit cannot lose diagnostics under load.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    tail_epoch: std::sync::atomic::AtomicU64,
    #[cfg(not(target_arch = "wasm32"))]
    tail_task: std::sync::Mutex<Option<tokio::task::AbortHandle>>,
    pending_full_diagnostic_refresh_revision: std::sync::atomic::AtomicU64,
    diagnostic_retry_count: std::sync::atomic::AtomicU32,
    /// Runtime work is demand-gated per project. Editor diagnostics and the
    /// catalog remain active at zero.
    runtime_demand: std::sync::atomic::AtomicUsize,
    incarnation: u64,
    last_project_update:
        std::sync::Mutex<std::collections::HashMap<u64, crate::bex_lsp::ProjectUpdate>>,
}

struct RuntimeDemandGuard {
    project: Arc<LiveProject>,
}

impl RuntimeDemandGuard {
    fn acquire(project: Arc<LiveProject>) -> Self {
        project
            .runtime_demand
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        Self { project }
    }

    /// Retain an already-demanded operation across an asynchronous tail. This
    /// closes the check-then-start race without allowing automatic work to
    /// create first demand for an otherwise cold project.
    fn try_acquire_demanded(project: Arc<LiveProject>) -> Option<Self> {
        project
            .runtime_demand
            .fetch_update(
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
                |demand| (demand > 0).then(|| demand.saturating_add(1)),
            )
            .ok()?;
        Some(Self { project })
    }
}

impl Drop for RuntimeDemandGuard {
    fn drop(&mut self) {
        let _ = self.project.runtime_demand.fetch_update(
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
            |demand| demand.checked_sub(1),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SessionConfig {
    position_encoding: PositionEncoding,
}

fn read_session_config(config: &OnceLock<SessionConfig>) -> Result<SessionConfig, LspError> {
    config.get().copied().ok_or_else(|| {
        LspError::ServerNotInitialized(
            "position encoding is unavailable before initialize".to_string(),
        )
    })
}

#[derive(Clone)]
struct BexMulitProject {
    projects:
        std::sync::Arc<std::sync::Mutex<HashMap<crate::fs::FsPath, std::sync::Arc<LiveProject>>>>,
    project_incarnations: std::sync::Arc<std::sync::Mutex<HashMap<crate::fs::FsPath, u64>>>,
    playground_session_epoch: std::sync::Arc<std::sync::atomic::AtomicU64>,
    active_playground_sessions: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<u64>>>,
    /// Set only on the lightweight dispatcher clone owned by one `/api/ws`
    /// endpoint. Root/LSP clones fan project-wide updates out to every active
    /// playground session.
    playground_session_context: Option<u64>,
    /// Run IDs resolve to the exact project incarnation that registered them.
    /// This keeps D8 completion/cancel valid after a remove/re-add at the same
    /// path and owns the extra runtime-demand reference until terminal state.
    active_runs: ActiveRunRegistry,
    /// Process/dispatcher-local dedupe for the infallible playground catalog.
    /// LSP catalog delivery has separate connection-scoped state because its
    /// bounded writer can reject an enqueue.
    last_catalog:
        std::sync::Arc<std::sync::Mutex<Option<Vec<crate::bex_lsp::ProjectCatalogEntry>>>>,
    catalog_delivery: Arc<std::sync::Mutex<CatalogDelivery>>,
    sys_op_factory: SysOpFactory,
    sender: std::sync::Arc<dyn LspClientSenderTrait + Send + Sync>,
    active_lsp_outputs: Arc<std::sync::Mutex<Vec<std::sync::Weak<LspSessionOutput>>>>,
    lsp_output_context: Option<Arc<LspSessionOutput>>,
    playground_sender: std::sync::Arc<dyn crate::bex_lsp::PlaygroundSender>,

    /// Connection-scoped immutable configuration. Every dispatcher clone for
    /// this connection observes the same value after initialize.
    session_config: Arc<OnceLock<SessionConfig>>,

    /// Workspace root directories provided by the LSP client during
    /// `initialize`. Used by `on_notification_initialized` to scope
    /// project discovery instead of walking the entire filesystem.
    workspace_roots: std::sync::Arc<std::sync::Mutex<Vec<vfs::VfsPath>>>,

    /// Empty diagnostic tombstones outlive the project incarnation they
    /// retire. This queue is connection-scoped (reset by `new_lsp_session`),
    /// coalesces per URI, and retries bounded-writer saturation until delivery
    /// or deterministic session closure.
    retired_diagnostics: Arc<std::sync::Mutex<RetiredDiagnostics>>,
    /// Connection-scoped checkpoint for multi-URI diagnostic revisions.
    /// Successful prefixes survive bounded-writer retries.
    diagnostic_progress: DiagnosticProgress,
    open_documents: OpenDocuments,
    published_diagnostics: PublishedDiagnostics,

    /// The VFS path to the project root.
    fs: crate::fs::BamlVFS,

    spawner: BackgroundSpawner,
}

pub trait LspClientSenderTrait {
    fn send_notification(&self, msg: lsp_server::Notification) -> Result<(), LspError>;
    fn send_response_impl(&self, msg: lsp_server::Response) -> Result<(), LspError>;
    fn send_response(
        &self,
        id: lsp_server::RequestId,
        msg: Result<serde_json::Value, LspError>,
    ) -> Result<(), LspError> {
        let (result, error) = match msg {
            Err(error) => (None, Some(error)),
            Ok(result) => (Some(result), None),
        };
        let response = lsp_server::Response {
            id,
            result,
            error: error.map(|e| lsp_server::ResponseError {
                code: e.json_rpc_code(),
                message: e.to_string(),
                data: None,
            }),
        };
        self.send_response_impl(response)
    }
    fn make_request(&self, msg: lsp_server::Request) -> Result<(), LspError>;
    /// True only after the owning transport has been permanently tombstoned.
    /// Temporary bounded-queue saturation must return an error while keeping
    /// this false so diagnostics can retry with backoff.
    fn is_closed(&self) -> bool {
        false
    }

    /// Deterministically tombstone a transport that cannot preserve durable
    /// protocol state within its configured memory bound.
    fn close_on_overload(&self) {}
}

#[derive(Clone, Copy)]
enum ProjectRefreshMode {
    Full,
    /// The notification/playground handler already applied one authoritative
    /// source mutation; only schedule the trailing work.
    Applied {
        full_diagnostic_refresh: bool,
    },
}

#[derive(Default)]
struct PublicationDrainReport {
    completed_batches: std::collections::HashSet<crate::project::PublicationBatchId>,
}

#[derive(Default)]
struct RetiredDiagnostics {
    pending: std::collections::VecDeque<RetiredDiagnostic>,
    pending_bytes: usize,
    retry_scheduled: bool,
    retry_count: u32,
}

struct RetiredDiagnostic {
    uri: lsp_types::Url,
    notification: lsp_server::Notification,
    bytes: usize,
    in_flight: bool,
}

const MAX_RETIRED_DIAGNOSTIC_ITEMS: usize = 1_024;
const MAX_RETIRED_DIAGNOSTIC_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, PartialEq, Eq, Hash)]
struct DiagnosticProgressKey {
    project_root: crate::fs::FsPath,
    incarnation: u64,
    source_revision: crate::project::SourceRevision,
}

#[derive(Default)]
struct CatalogDelivery {
    /// The last catalog accepted by this connection's bounded writer.
    acknowledged: Option<Vec<crate::bex_lsp::ProjectCatalogEntry>>,
    /// One coalescing slot containing the newest catalog that still needs to
    /// be accepted. This bounds retained outbound catalog work per connection.
    pending: Option<Vec<crate::bex_lsp::ProjectCatalogEntry>>,
    in_flight: bool,
    retry_scheduled: bool,
    retry_count: u32,
    /// A non-retryable failure for an exact catalog identity. Routine refresh
    /// calls suppress that identity without pretending it was acknowledged;
    /// an explicit forced refresh may try it again.
    terminal_failure: Option<Vec<crate::bex_lsp::ProjectCatalogEntry>>,
}

struct LspSessionOutput {
    sender: Arc<dyn LspClientSenderTrait + Send + Sync>,
    session_config: Arc<OnceLock<SessionConfig>>,
    catalog_delivery: Arc<std::sync::Mutex<CatalogDelivery>>,
    retired_diagnostics: Arc<std::sync::Mutex<RetiredDiagnostics>>,
    diagnostic_progress: DiagnosticProgress,
    /// Client-owned document identity keyed by canonical physical path. The
    /// shared project database owns the latest source, while each output
    /// retains the URI/version/text tuple needed to tag its own diagnostics.
    open_documents: OpenDocuments,
    /// Diagnostics acknowledged by this output, isolated by project
    /// incarnation so remove/re-add retirement cannot consume another
    /// connection's URI history.
    published_diagnostics: PublishedDiagnostics,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct PublishedProjectKey {
    project_root: crate::fs::FsPath,
    incarnation: u64,
}

impl BexMulitProject {
    fn new(
        sys_op_factory: SysOpFactory,
        sender: std::sync::Arc<dyn LspClientSenderTrait + Send + Sync>,
        playground_sender: std::sync::Arc<dyn crate::bex_lsp::PlaygroundSender>,
        fs: crate::fs::BamlVFS,
        spawner: BackgroundSpawner,
    ) -> Self {
        let catalog_delivery = Arc::new(std::sync::Mutex::new(CatalogDelivery::default()));
        Self {
            projects: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            project_incarnations: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            playground_session_epoch: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1)),
            active_playground_sessions: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashSet::new(),
            )),
            playground_session_context: None,
            active_runs: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            last_catalog: std::sync::Arc::new(std::sync::Mutex::new(None)),
            catalog_delivery,
            sys_op_factory,
            sender,
            active_lsp_outputs: Arc::new(std::sync::Mutex::new(Vec::new())),
            lsp_output_context: None,
            playground_sender,
            session_config: Arc::new(OnceLock::new()),
            workspace_roots: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            retired_diagnostics: Arc::new(std::sync::Mutex::new(RetiredDiagnostics::default())),
            diagnostic_progress: Arc::new(std::sync::Mutex::new(HashMap::new())),
            open_documents: Arc::new(std::sync::Mutex::new(HashMap::new())),
            published_diagnostics: Arc::new(std::sync::Mutex::new(HashMap::new())),
            fs,
            spawner,
        }
    }

    fn session_config(&self) -> Result<SessionConfig, LspError> {
        read_session_config(&self.session_config)
    }

    fn connection_scoped_lsp_session(
        &self,
        sender: Arc<dyn LspClientSenderTrait + Send + Sync>,
    ) -> Self {
        let mut session = self.clone();
        let session_config = Arc::new(OnceLock::new());
        let retired_diagnostics = Arc::new(std::sync::Mutex::new(RetiredDiagnostics::default()));
        let diagnostic_progress = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let catalog_delivery = Arc::new(std::sync::Mutex::new(CatalogDelivery::default()));
        let open_documents = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let published_diagnostics = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let output = Arc::new(LspSessionOutput {
            sender: sender.clone(),
            session_config: session_config.clone(),
            catalog_delivery: catalog_delivery.clone(),
            retired_diagnostics: retired_diagnostics.clone(),
            diagnostic_progress: diagnostic_progress.clone(),
            open_documents: open_documents.clone(),
            published_diagnostics: published_diagnostics.clone(),
        });
        if let Ok(mut outputs) = session.active_lsp_outputs.lock() {
            outputs.retain(|output| output.strong_count() > 0);
            outputs.push(Arc::downgrade(&output));
        }
        session.sender = sender;
        session.lsp_output_context = Some(output);
        session.session_config = session_config;
        session.workspace_roots = Arc::new(std::sync::Mutex::new(Vec::new()));
        session.last_catalog = Arc::new(std::sync::Mutex::new(None));
        session.catalog_delivery = catalog_delivery;
        session.retired_diagnostics = retired_diagnostics;
        session.diagnostic_progress = diagnostic_progress;
        session.open_documents = open_documents;
        session.published_diagnostics = published_diagnostics;
        session
    }

    fn position_encoding(&self) -> Result<PositionEncoding, LspError> {
        Ok(self.session_config()?.position_encoding)
    }

    fn current_playground_session_epoch(&self) -> u64 {
        self.playground_session_context.unwrap_or_else(|| {
            self.playground_session_epoch
                .load(std::sync::atomic::Ordering::Acquire)
        })
    }

    fn playground_session_targets(&self) -> Vec<u64> {
        if let Some(session_epoch) = self.playground_session_context {
            return self
                .playground_session_is_current_internal(session_epoch)
                .then_some(vec![session_epoch])
                .unwrap_or_default();
        }
        self.all_playground_session_targets()
    }

    fn all_playground_session_targets(&self) -> Vec<u64> {
        let mut sessions = self
            .active_playground_sessions
            .lock()
            .map(|sessions| sessions.iter().copied().collect::<Vec<_>>())
            .unwrap_or_default();
        sessions.sort_unstable();
        if sessions.is_empty() {
            // WASM and editor-only operation use one stable implicit context.
            sessions.push(self.current_playground_session_epoch());
        }
        sessions
    }

    fn playground_targets_for_origin_locked(
        &self,
        sessions: &std::collections::HashSet<u64>,
        origin_session_epoch: u64,
    ) -> Option<Vec<u64>> {
        let origin_is_current = if let Some(bound_session) = self.playground_session_context {
            bound_session == origin_session_epoch && sessions.contains(&origin_session_epoch)
        } else {
            sessions.contains(&origin_session_epoch)
                || (sessions.is_empty()
                    && origin_session_epoch
                        == self
                            .playground_session_epoch
                            .load(std::sync::atomic::Ordering::Acquire))
        };
        if !origin_is_current {
            return None;
        }
        let mut targets = if sessions.is_empty() {
            vec![origin_session_epoch]
        } else {
            sessions.iter().copied().collect::<Vec<_>>()
        };
        targets.sort_unstable();
        Some(targets)
    }

    fn playground_session_is_current_internal(&self, session_epoch: u64) -> bool {
        self.active_playground_sessions
            .lock()
            .is_ok_and(|sessions| {
                if let Some(bound_session) = self.playground_session_context {
                    return bound_session == session_epoch && sessions.contains(&session_epoch);
                }
                sessions.contains(&session_epoch)
                    || (sessions.is_empty()
                        && session_epoch
                            == self
                                .playground_session_epoch
                                .load(std::sync::atomic::Ordering::Acquire))
            })
    }

    fn retarget_playground_notification(
        mut notification: crate::bex_lsp::PlaygroundNotification,
        target_session_epoch: u64,
    ) -> Option<crate::bex_lsp::PlaygroundNotification> {
        match &mut notification {
            crate::bex_lsp::PlaygroundNotification::ListProjects { session_epoch, .. }
            | crate::bex_lsp::PlaygroundNotification::UpdateProject { session_epoch, .. }
            | crate::bex_lsp::PlaygroundNotification::ControlFlowGraphResult {
                session_epoch,
                ..
            }
            | crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                session_epoch, ..
            } => *session_epoch = target_session_epoch,
            crate::bex_lsp::PlaygroundNotification::OpenPlayground { .. }
            | crate::bex_lsp::PlaygroundNotification::CursorContext { .. } => return None,
        }
        Some(notification)
    }

    fn get_path_from_uri_unchecked(&self, uri: &lsp_types::Url) -> Result<vfs::VfsPath, LspError> {
        let path = wasm_helpers::to_file_path(uri)
            .map_err(|()| LspError::InvalidParams("Failed to convert URI to path".to_string()))?;
        let identity = canonical_path_identity(&path);
        self.fs.get_path_from_path(&identity, "get_path_from_uri")
    }

    fn get_path_from_uri(&self, uri: &lsp_types::Url) -> Result<vfs::VfsPath, LspError> {
        let path = self.get_path_from_uri_unchecked(uri)?;
        self.validate_owned_path(&path)?;
        Ok(path)
    }

    fn validate_owned_path(&self, path: &vfs::VfsPath) -> Result<(), LspError> {
        let roots = self.workspace_roots.lock().unwrap().clone();
        // Initialize uses this helper to establish the first roots. Lifecycle
        // ingress prevents other URI-bearing traffic before that handshake.
        if roots.is_empty() {
            return Ok(());
        }
        let candidate = canonical_path_identity(std::path::Path::new(path.as_str()));
        if roots.iter().any(|root| {
            let root = canonical_path_identity(std::path::Path::new(root.as_str()));
            candidate == root || candidate.starts_with(&root)
        }) {
            Ok(())
        } else {
            Err(LspError::RequestFailed(format!(
                "Path is outside the initialized ownership roots: {}",
                path.as_str()
            )))
        }
    }

    fn path_overlaps_ownership_roots(path: &std::path::Path, roots: &[vfs::VfsPath]) -> bool {
        let candidate = canonical_path_identity(path);
        roots.iter().any(|root| {
            let root = canonical_path_identity(std::path::Path::new(root.as_str()));
            // Normally the semantic project is nested below its outer owner.
            // Keep the reverse relation for clients that open `baml_src/` (or
            // another subdirectory) inside an already-marked project.
            candidate == root || candidate.starts_with(&root) || root.starts_with(&candidate)
        })
    }

    fn get_or_create_project(
        &self,
        root_path: vfs::VfsPath,
    ) -> Result<std::sync::Arc<LiveProject>, LspError> {
        let mut projects = self.projects.lock().unwrap();
        if !root_path.exists().unwrap_or(false) {
            projects.remove(&crate::fs::FsPath::from_vfs(&root_path));
            return Err(LspError::ProjectNotFound(root_path));
        }

        if let Some(project) = projects.get(&crate::fs::FsPath::from_vfs(&root_path)) {
            return Ok(project.clone());
        }

        let sys_ops = (self.sys_op_factory)(&root_path);
        let project_key = crate::fs::FsPath::from_vfs(&root_path);
        let incarnation = {
            let mut incarnations = self.project_incarnations.lock().unwrap();
            let incarnation = incarnations.entry(project_key.clone()).or_default();
            *incarnation = incarnation.wrapping_add(1).max(1);
            *incarnation
        };
        let project = crate::project::BexProject::new(&root_path, sys_ops);
        let project = std::sync::Arc::new(LiveProject {
            project,
            #[cfg(not(target_arch = "wasm32"))]
            rebuild_epoch: std::sync::atomic::AtomicU64::new(0),
            #[cfg(not(target_arch = "wasm32"))]
            rebuild_task: std::sync::Mutex::new(None),
            tail_epoch: std::sync::atomic::AtomicU64::new(0),
            #[cfg(not(target_arch = "wasm32"))]
            tail_task: std::sync::Mutex::new(None),
            pending_full_diagnostic_refresh_revision: std::sync::atomic::AtomicU64::new(0),
            diagnostic_retry_count: std::sync::atomic::AtomicU32::new(0),
            runtime_demand: std::sync::atomic::AtomicUsize::new(0),
            incarnation,
            last_project_update: std::sync::Mutex::new(std::collections::HashMap::new()),
        });
        projects.insert(project_key, project.clone());
        Ok(project)
    }

    fn get_bex_for_project(
        &self,
        project_root: &crate::fs::FsPath,
    ) -> Result<Arc<dyn crate::Bex>, RuntimeError> {
        let project = {
            let projects = self.projects.lock().unwrap();
            projects
                .get(project_root)
                .ok_or(RuntimeError::Compilation {
                    message: format!("Project not found: {}", project_root.as_path().display()),
                })?
                .clone()
        };
        let bex = project.project.get_bex()?;
        Ok(bex)
    }

    fn project_for_root_str(&self, project_root: &str) -> Option<Arc<LiveProject>> {
        self.projects
            .lock()
            .ok()?
            .iter()
            .find(|(path, _)| path.as_path().to_string_lossy() == project_root)
            .map(|(_, project)| project.clone())
    }

    fn project_entry_for_root_str(
        &self,
        project_root: &str,
    ) -> Option<(crate::fs::FsPath, Arc<LiveProject>)> {
        self.projects
            .lock()
            .ok()?
            .iter()
            .find(|(path, _)| path.as_path().to_string_lossy() == project_root)
            .map(|(path, project)| (path.clone(), project.clone()))
    }

    fn project_incarnation_is_current(
        &self,
        project_root: &crate::fs::FsPath,
        project: &Arc<LiveProject>,
    ) -> bool {
        self.projects
            .lock()
            .ok()
            .and_then(|projects| projects.get(project_root).cloned())
            .is_some_and(|current| Arc::ptr_eq(&current, project))
    }

    fn enqueue_project_playground_publication(
        &self,
        project_root: &crate::fs::FsPath,
        project: &Arc<LiveProject>,
        identity: crate::project::ProjectPublicationIdentity,
        session_epoch: u64,
        notification: crate::bex_lsp::PlaygroundNotification,
    ) -> bool {
        self.enqueue_project_playground_publication_batch(
            project_root,
            project,
            identity,
            session_epoch,
            notification,
        )
        .is_some()
    }

    fn enqueue_project_playground_publication_batch(
        &self,
        project_root: &crate::fs::FsPath,
        project: &Arc<LiveProject>,
        identity: crate::project::ProjectPublicationIdentity,
        session_epoch: u64,
        notification: crate::bex_lsp::PlaygroundNotification,
    ) -> Option<crate::project::PublicationBatchId> {
        self.enqueue_project_playground_publication_batch_result(
            project_root,
            project,
            identity,
            session_epoch,
            notification,
        )
        .ok()
    }

    fn enqueue_project_playground_publication_batch_result(
        &self,
        project_root: &crate::fs::FsPath,
        project: &Arc<LiveProject>,
        identity: crate::project::ProjectPublicationIdentity,
        session_epoch: u64,
        notification: crate::bex_lsp::PlaygroundNotification,
    ) -> Result<crate::project::PublicationBatchId, crate::project::PublicationEnqueueError> {
        // Registry membership is the project-incarnation fence. The source,
        // generation, and operation epochs are checked atomically by the
        // project's reserved publication mailbox.
        if !self.project_incarnation_is_current(project_root, project)
            || !self.playground_session_is_current_internal(session_epoch)
        {
            return Err(crate::project::PublicationEnqueueError::Stale);
        }
        project.project.enqueue_publication_batch_if_current(
            identity,
            vec![crate::project::ProjectPublication::Playground {
                session_epoch,
                notification,
            }],
        )
    }

    async fn ensure_project_engine(
        project: Arc<LiveProject>,
    ) -> Result<crate::project::CommitReceipt, LspError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::task::spawn_blocking(move || project.project.ensure_engine_current())
                .await
                .map_err(|error| {
                    LspError::InternalError(format!("Engine build task failed: {error}"))
                })?
                .map_err(LspError::Runtime)
        }
        #[cfg(target_arch = "wasm32")]
        {
            project
                .project
                .ensure_engine_current()
                .map_err(LspError::Runtime)
        }
    }

    fn spawn_engine_build_and_publish(
        &self,
        project_root: vfs::VfsPath,
        project: Arc<LiveProject>,
    ) {
        let this = self.clone();
        let session_epochs = self.playground_session_targets();
        self.spawner.spawn(async move {
            let built = Self::ensure_project_engine(project.clone()).await.is_ok();
            if !this.playground_sender.has_runtime_subscribers()
                || project
                    .runtime_demand
                    .load(std::sync::atomic::Ordering::Acquire)
                    == 0
            {
                return;
            }
            let (source_revision, diagnostics) =
                match project.project.diagnostics_by_file(PositionEncoding::Utf16) {
                    DiagnosticRead::Ready(candidate)
                        if candidate.source_revision == project.project.source_revision() =>
                    {
                        (
                            candidate.source_revision,
                            Self::flatten_diagnostics(&candidate.documents),
                        )
                    }
                    DiagnosticRead::Ready(_) | DiagnosticRead::Busy | DiagnosticRead::Poisoned => {
                        return;
                    }
                };
            for session_epoch in session_epochs.iter().copied() {
                this.send_update_project(
                    &project_root,
                    &project,
                    source_revision,
                    session_epoch,
                    diagnostics.clone(),
                    false,
                );
            }
            if built
                && let Some(session_epoch) = session_epochs
                    .iter()
                    .copied()
                    .find(|epoch| this.playground_session_is_current_internal(*epoch))
                && let Some(collection_demand) =
                    RuntimeDemandGuard::try_acquire_demanded(project.clone())
            {
                let project_key = crate::fs::FsPath::from_vfs(&project_root);
                this.request_collect_tests_for_project(
                    project_key,
                    project.clone(),
                    project_root.as_str().to_string(),
                    session_epoch,
                    Some(collection_demand),
                );
            }
        });
    }

    fn release_runtime_demand(project: &LiveProject) {
        let _ = project.runtime_demand.fetch_update(
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
            |demand| demand.checked_sub(1),
        );
    }

    fn runtime_status_wire(project: &LiveProject) -> crate::bex_lsp::ProjectRuntimeStatus {
        let status = project.project.runtime_status();
        let state = match status.phase {
            crate::project::RuntimeBuildPhase::IdleStale => "idleStale",
            crate::project::RuntimeBuildPhase::Building => "building",
            crate::project::RuntimeBuildPhase::Ready => "ready",
            crate::project::RuntimeBuildPhase::BlockedByDiagnostics => "blockedByDiagnostics",
            crate::project::RuntimeBuildPhase::Failed => "failed",
        };
        crate::bex_lsp::ProjectRuntimeStatus {
            state: state.to_string(),
            requested_revision: status.requested_revision.0,
            installed_revision: status.installed_revision.map(|revision| revision.0),
            generation: status.generation,
            has_last_known_good: status.has_last_known_good,
            error_message: status.error_message,
        }
    }

    /// Resolve the project root using real project markers only: the closest
    /// ancestor with a `baml.toml` or a `baml_src/` directory. This is the
    /// resolver used during workspace discovery, where a lenient fallback
    /// would promote stray `.baml` files into full projects.
    fn get_marked_baml_project_root(path: &vfs::VfsPath) -> Result<vfs::VfsPath, LspError> {
        let start = Self::project_search_start(path);
        find_baml_project_root_from_ancestors(
            vfs_ancestors(start),
            Self::has_baml_toml,
            Self::has_baml_src_dir,
        )
        .ok_or_else(|| {
            LspError::ProjectRootNotFound(path.clone(), "Not a BAML project".to_string())
        })
    }

    fn get_baml_project_root(path: &vfs::VfsPath) -> Result<vfs::VfsPath, LspError> {
        if let Ok(root) = Self::get_marked_baml_project_root(path) {
            return Ok(root);
        }

        // In some special cases, .baml files are treated as their own projects
        // This is only for internal development
        let allow_standalone_baml_file = path.as_str().split('/').any(|p| p == "baml_language");

        if allow_standalone_baml_file
            && path.extension().is_some_and(|e| e.as_str() == "baml")
            && path.is_file().map_err(|e| LspError::InvalidVFSPath {
                path: path.clone(),
                message: format!("Failed to check if path is a file: {e}"),
            })?
        {
            return Ok(path.clone());
        }

        Err(LspError::ProjectRootNotFound(
            path.clone(),
            "Not a BAML project".to_string(),
        ))
    }

    fn load_project_sources(
        &self,
        project_root: &vfs::VfsPath,
    ) -> Result<HashMap<crate::fs::FsPath, String>, LspError> {
        if project_root
            .is_file()
            .map_err(|e| LspError::InvalidVFSPath {
                path: project_root.clone(),
                message: format!("Failed to check if path is a file: {e}"),
            })?
        {
            if project_root
                .extension()
                .is_some_and(|e| e.as_str() == "baml")
            {
                let mut reader =
                    project_root
                        .open_file()
                        .map_err(|e| LspError::InvalidVFSPath {
                            path: project_root.clone(),
                            message: format!("Failed to open file: {e}"),
                        })?;
                let mut bytes = Vec::new();
                reader
                    .read_to_end(&mut bytes)
                    .map_err(|e| LspError::InvalidVFSPath {
                        path: project_root.clone(),
                        message: format!("Failed to read file: {e}"),
                    })?;
                let mut files = HashMap::new();
                files.insert(
                    crate::fs::FsPath::from_vfs(project_root),
                    String::from_utf8(bytes).unwrap_or_default(),
                );
                return Ok(files);
            }
        }

        let source_root = Self::project_source_root(project_root)?;
        let glob = format!("{}/**/*.baml", source_root.as_str());
        let entries = self
            .fs
            .read_many(&glob)
            .map_err(|e| LspError::InvalidVFSPath {
                path: project_root.clone(),
                message: e.to_string(),
            })?;
        let files = entries
            .into_iter()
            .map(|(path, bytes)| {
                let content = String::from_utf8(bytes).unwrap_or_default();
                (crate::fs::FsPath::from_str(path), content)
            })
            .collect();
        Ok(files)
    }

    fn refresh_project(&self, project_root: &vfs::VfsPath, refresh_mode: ProjectRefreshMode) {
        self.refresh_project_async(project_root, refresh_mode);
    }

    fn has_baml_toml(path: &vfs::VfsPath) -> bool {
        path.join(BAML_TOML)
            .ok()
            .and_then(|path| path.is_file().ok())
            .unwrap_or(false)
    }

    fn has_baml_src_dir(path: &vfs::VfsPath) -> bool {
        path.join(BAML_SRC_DIR)
            .ok()
            .and_then(|path| path.is_dir().ok())
            .unwrap_or(false)
    }

    fn project_source_root(project_root: &vfs::VfsPath) -> Result<vfs::VfsPath, LspError> {
        let baml_src = project_root
            .join(BAML_SRC_DIR)
            .map_err(|e| LspError::InvalidVFSPath {
                path: project_root.clone(),
                message: format!("Failed to join path: {e}"),
            })?;
        if baml_src.is_dir().unwrap_or(false) {
            Ok(baml_src)
        } else {
            Ok(project_root.clone())
        }
    }

    fn project_search_start(path: &vfs::VfsPath) -> vfs::VfsPath {
        if path.filename().as_str() == BAML_TOML
            || path.extension().is_some_and(|ext| ext.as_str() == "baml")
            || path.is_file().unwrap_or(false)
        {
            path.parent()
        } else {
            path.clone()
        }
    }

    fn discover_workspace_projects(&self, workspace_roots: &[vfs::VfsPath]) -> Vec<vfs::VfsPath> {
        workspace_roots.clone_into(&mut self.workspace_roots.lock().unwrap());

        if workspace_roots.is_empty() {
            tracing::warn!(
                "No workspace roots provided during initialize — skipping project discovery"
            );
            return Vec::new();
        }

        let mut project_roots = Vec::new();
        for root in workspace_roots {
            if root.is_file().unwrap_or(false)
                && root.extension().is_some_and(|e| e.as_str() == "baml")
            {
                project_roots.push(root.clone());
                continue;
            }

            // The workspace folder itself may live inside a project
            // (e.g. the user opened `baml_src/` or a subdirectory).
            if let Ok(pr) = Self::get_marked_baml_project_root(root) {
                project_roots.push(pr);
            }

            #[cfg(not(target_arch = "wasm32"))]
            project_roots.extend(self.collect_marked_project_roots(root));
            #[cfg(target_arch = "wasm32")]
            project_roots.extend(Self::collect_marked_project_roots(root));
        }

        project_roots.sort_by_key(|path| path.as_str().to_string());
        project_roots.dedup_by(|a, b| a.as_str() == b.as_str());
        let manifest_roots = project_roots
            .iter()
            .filter(|path| Self::has_baml_toml(path))
            .map(|path| path.as_str().trim_end_matches('/').to_string())
            .collect::<Vec<_>>();
        project_roots.retain(|candidate| {
            if Self::has_baml_toml(candidate) {
                return true;
            }
            let candidate = candidate.as_str().trim_end_matches('/');
            !manifest_roots.iter().any(|manifest_root| {
                candidate != manifest_root
                    && candidate
                        .strip_prefix(manifest_root)
                        .is_some_and(|suffix| suffix.starts_with('/'))
            })
        });

        tracing::info!("Discovered {} BAML project(s)", project_roots.len());

        // Discovery is also the removal transaction. Keep the monotonically
        // increasing incarnation counter, but evict the old LiveProject and
        // all of its warm dedupe/runtime state when a root disappears.
        let discovered: std::collections::HashSet<_> = project_roots
            .iter()
            .map(crate::fs::FsPath::from_vfs)
            .collect();
        let removed = {
            let mut projects = self.projects.lock().unwrap();
            let removed = projects
                .iter()
                .filter(|(project_root, _)| {
                    !discovered.contains(*project_root)
                        && Self::path_overlaps_ownership_roots(
                            project_root.as_path(),
                            workspace_roots,
                        )
                })
                .map(|(project_root, project)| (project_root.clone(), project.clone()))
                .collect::<Vec<_>>();
            // Remove first. A tail waiting on the per-project drainer will now
            // fail its incarnation fence; a tail already holding it completes
            // before retirement sends any tombstone.
            for (project_root, _) in &removed {
                projects.remove(project_root);
            }
            removed
        };
        for (project_root, project) in &removed {
            self.retire_project_diagnostics(project_root, project);
        }

        for project_root in &project_roots {
            let Ok(_) = self.get_or_create_project(project_root.clone()) else {
                continue;
            };
            self.refresh_project(project_root, ProjectRefreshMode::Full);
        }
        self.send_list_projects(false);

        project_roots
    }

    fn retire_project_diagnostics(
        &self,
        project_root: &crate::fs::FsPath,
        project: &Arc<LiveProject>,
    ) {
        let Some(drain_guard) = project.project.lock_publication_drainer() else {
            return;
        };
        project.project.discard_all_publications();
        drop(drain_guard);

        for output in self.active_initialized_lsp_outputs() {
            if let Ok(mut progress) = output.diagnostic_progress.lock() {
                progress.retain(|key, _| key.project_root != *project_root);
            }
            let project_key = PublishedProjectKey {
                project_root: project_root.clone(),
                incarnation: project.incarnation,
            };
            let uris = output
                .published_diagnostics
                .lock()
                .ok()
                .and_then(|mut published| published.remove(&project_key))
                .map(|published| published.into_values().collect::<Vec<_>>())
                .unwrap_or_default();
            let mut overflowed = false;
            for uri in uris {
                if !Self::enqueue_retired_diagnostic(&output, uri) {
                    overflowed = true;
                    break;
                }
            }
            if overflowed {
                if let Ok(mut retired) = output.retired_diagnostics.lock() {
                    retired.pending.clear();
                    retired.pending_bytes = 0;
                    retired.retry_scheduled = false;
                }
                output.sender.close_on_overload();
                continue;
            }
            self.flush_retired_diagnostics_for(output);
        }
    }

    fn enqueue_retired_diagnostic(output: &LspSessionOutput, uri: lsp_types::Url) -> bool {
        let notification = lsp_server::Notification::new(
            "textDocument/publishDiagnostics".to_string(),
            lsp_types::PublishDiagnosticsParams::new(uri.clone(), Vec::new(), None),
        );
        let bytes = serde_json::to_vec(&lsp_server::Message::Notification(notification.clone()))
            .map_or(usize::MAX, |message| message.len());
        let Ok(mut retired) = output.retired_diagnostics.lock() else {
            return false;
        };
        if let Some(position) = retired
            .pending
            .iter()
            .position(|pending| pending.uri == uri)
        {
            if retired.pending[position].in_flight {
                return true;
            }
            if let Some(replaced) = retired.pending.remove(position) {
                retired.pending_bytes = retired.pending_bytes.saturating_sub(replaced.bytes);
            }
        }
        if retired.pending.len() >= MAX_RETIRED_DIAGNOSTIC_ITEMS
            || retired.pending_bytes.saturating_add(bytes) > MAX_RETIRED_DIAGNOSTIC_BYTES
        {
            return false;
        }
        retired.pending_bytes += bytes;
        retired.pending.push_back(RetiredDiagnostic {
            uri,
            notification,
            bytes,
            in_flight: false,
        });
        true
    }

    fn active_initialized_lsp_outputs(&self) -> Vec<Arc<LspSessionOutput>> {
        let Ok(mut outputs) = self.active_lsp_outputs.lock() else {
            return Vec::new();
        };
        let mut active = outputs
            .iter()
            .filter_map(std::sync::Weak::upgrade)
            .filter(|output| output.session_config.get().is_some() && !output.sender.is_closed())
            .collect::<Vec<_>>();
        outputs.retain(|output| output.strong_count() > 0);
        if active.is_empty() && self.session_config.get().is_some() && !self.sender.is_closed() {
            active.push(self.lsp_output_context.clone().unwrap_or_else(|| {
                Arc::new(LspSessionOutput {
                    sender: self.sender.clone(),
                    session_config: self.session_config.clone(),
                    catalog_delivery: self.catalog_delivery.clone(),
                    retired_diagnostics: self.retired_diagnostics.clone(),
                    diagnostic_progress: self.diagnostic_progress.clone(),
                    open_documents: self.open_documents.clone(),
                    published_diagnostics: self.published_diagnostics.clone(),
                })
            }));
        }
        active
    }

    fn retargeted_lsp_output(&self, output: Arc<LspSessionOutput>) -> Self {
        let mut session = self.clone();
        session.sender = output.sender.clone();
        session.session_config = output.session_config.clone();
        session.catalog_delivery = output.catalog_delivery.clone();
        session.retired_diagnostics = output.retired_diagnostics.clone();
        session.diagnostic_progress = output.diagnostic_progress.clone();
        session.open_documents = output.open_documents.clone();
        session.published_diagnostics = output.published_diagnostics.clone();
        session.lsp_output_context = Some(output);
        session
    }

    fn catalog_lsp_outputs(&self, force: bool) -> Vec<Arc<LspSessionOutput>> {
        // Initialization is a connection-local subscriber boundary. Routine
        // root/poller updates, and forced process/playground state requests,
        // fan out to every initialized connection.
        if force
            && let Some(output) = self.lsp_output_context.as_ref()
            && output.session_config.get().is_some()
            && !output.sender.is_closed()
        {
            return vec![output.clone()];
        }
        self.active_initialized_lsp_outputs()
    }

    fn enqueue_lsp_catalog(
        &self,
        output: Arc<LspSessionOutput>,
        entries: Vec<crate::bex_lsp::ProjectCatalogEntry>,
        force: bool,
    ) {
        let should_flush = {
            let Ok(mut delivery) = output.catalog_delivery.lock() else {
                return;
            };
            if output.sender.is_closed() {
                delivery.pending = None;
                delivery.retry_scheduled = false;
                return;
            }
            if !force && delivery.acknowledged.as_ref() == Some(&entries) {
                // If a different catalog was waiting only on writer capacity,
                // cancel it: the connection already has the current identity.
                // An actual in-flight send cannot be recalled, so queue the
                // current identity behind it to restore final ordering.
                if delivery.in_flight {
                    delivery.pending = Some(entries);
                } else {
                    delivery.pending = None;
                }
                return;
            }
            if !force && delivery.terminal_failure.as_ref() == Some(&entries) {
                if delivery.in_flight {
                    delivery.pending = Some(entries);
                } else {
                    delivery.pending = None;
                }
                return;
            }
            if force {
                delivery.terminal_failure = None;
            }
            if delivery.pending.as_ref() != Some(&entries) {
                delivery.pending = Some(entries);
            }
            !delivery.in_flight && (force || !delivery.retry_scheduled)
        };
        if should_flush {
            self.flush_lsp_catalog(output);
        }
    }

    fn flush_lsp_catalog(&self, output: Arc<LspSessionOutput>) {
        loop {
            let entries = {
                let Ok(mut delivery) = output.catalog_delivery.lock() else {
                    return;
                };
                if output.sender.is_closed() {
                    delivery.pending = None;
                    delivery.in_flight = false;
                    delivery.retry_scheduled = false;
                    return;
                }
                let Some(entries) = delivery.pending.clone() else {
                    return;
                };
                if delivery.in_flight {
                    return;
                }
                delivery.in_flight = true;
                entries
            };

            let roots = entries
                .iter()
                .map(|entry| entry.project.clone())
                .collect::<Vec<_>>();
            let notification = lsp_server::Notification::new(
                "baml/listProjects".to_string(),
                serde_json::json!({ "projects": roots }),
            );
            match output.sender.send_notification(notification) {
                Ok(()) => {
                    let Ok(mut delivery) = output.catalog_delivery.lock() else {
                        return;
                    };
                    // The bounded sender accepting the complete frame is the
                    // catalog's acknowledgement point. A newer coalesced
                    // catalog remains pending and is sent by the next loop.
                    delivery.acknowledged = Some(entries.clone());
                    if delivery.pending.as_ref() == Some(&entries) {
                        delivery.pending = None;
                    }
                    if delivery.terminal_failure.as_ref() == Some(&entries) {
                        delivery.terminal_failure = None;
                    }
                    delivery.in_flight = false;
                    delivery.retry_count = 0;
                }
                Err(LspError::OutboundSaturated) => {
                    if let Ok(mut delivery) = output.catalog_delivery.lock() {
                        delivery.in_flight = false;
                    }
                    self.schedule_lsp_catalog_retry(output);
                    return;
                }
                Err(LspError::ClientClosed) => {
                    if let Ok(mut delivery) = output.catalog_delivery.lock() {
                        delivery.pending = None;
                        delivery.in_flight = false;
                        delivery.retry_scheduled = false;
                        delivery.retry_count = 0;
                        delivery.terminal_failure = Some(entries);
                    }
                    return;
                }
                Err(error) => {
                    let oversized = matches!(&error, LspError::OutboundOversized);
                    let has_newer = if let Ok(mut delivery) = output.catalog_delivery.lock() {
                        delivery.in_flight = false;
                        let has_newer = delivery.pending.as_ref() != Some(&entries);
                        if !has_newer {
                            delivery.pending = None;
                        }
                        delivery.terminal_failure = Some(entries.clone());
                        has_newer
                    } else {
                        false
                    };
                    if oversized {
                        // A complete catalog cannot be split without changing
                        // protocol semantics. Browser transports close their
                        // session here; transports without a close hook retain
                        // the exact terminal identity without retrying it.
                        output.sender.close_on_overload();
                    }
                    match error {
                        LspError::OutboundOversized => tracing::warn!(
                            projects = entries.len(),
                            "LSP catalog exceeds the bounded outbound frame limit"
                        ),
                        _ => tracing::warn!(%error, "LSP catalog delivery failed"),
                    }
                    if !has_newer {
                        return;
                    }
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn schedule_lsp_catalog_retry(&self, output: Arc<LspSessionOutput>) {
        let delay_ms = {
            let Ok(mut delivery) = output.catalog_delivery.lock() else {
                return;
            };
            if delivery.pending.is_none() || delivery.retry_scheduled {
                return;
            }
            delivery.retry_scheduled = true;
            let attempt = delivery.retry_count.min(6);
            delivery.retry_count = delivery.retry_count.saturating_add(1);
            (5_u64 << attempt).min(250)
        };
        let this = self.clone();
        self.spawner.spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            if let Ok(mut delivery) = output.catalog_delivery.lock() {
                delivery.retry_scheduled = false;
            }
            this.flush_lsp_catalog(output);
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn schedule_lsp_catalog_retry(&self, _output: Arc<LspSessionOutput>) {
        // The in-process WASM sender is not backed by the native bounded
        // writer. Retaining the single coalesced slot lets a later explicit
        // state request retry an embedding-level transient failure.
    }

    fn flush_retired_diagnostics_for(&self, output: Arc<LspSessionOutput>) {
        loop {
            let next = {
                let Ok(mut retired) = output.retired_diagnostics.lock() else {
                    return;
                };
                let Some(pending) = retired.pending.front_mut() else {
                    retired.retry_count = 0;
                    return;
                };
                if pending.in_flight {
                    return;
                }
                pending.in_flight = true;
                (pending.uri.clone(), pending.notification.clone())
            };
            let (uri, notification) = next;
            match output.sender.send_notification(notification) {
                Ok(()) => {
                    if let Ok(mut retired) = output.retired_diagnostics.lock() {
                        if retired
                            .pending
                            .front()
                            .is_some_and(|pending| pending.uri == uri && pending.in_flight)
                        {
                            if let Some(delivered) = retired.pending.pop_front() {
                                retired.pending_bytes =
                                    retired.pending_bytes.saturating_sub(delivered.bytes);
                            }
                        }
                        retired.retry_count = 0;
                    }
                }
                Err(LspError::ClientClosed) => {
                    if let Ok(mut retired) = output.retired_diagnostics.lock() {
                        retired.pending.clear();
                        retired.pending_bytes = 0;
                        retired.retry_scheduled = false;
                        retired.retry_count = 0;
                    }
                    return;
                }
                Err(error) => {
                    if let Ok(mut retired) = output.retired_diagnostics.lock()
                        && let Some(pending) = retired
                            .pending
                            .iter_mut()
                            .find(|pending| pending.uri == uri)
                    {
                        pending.in_flight = false;
                    }
                    tracing::debug!(%error, %uri, "retired diagnostic tombstone is waiting for LSP writer capacity");
                    self.schedule_retired_diagnostics_retry(output);
                    return;
                }
            }
        }
    }

    /// Establish per-URI transport order between a retired incarnation's
    /// tombstone and a replacement incarnation's diagnostics. If the clear is
    /// already in flight, the new project tail retries instead of allowing a
    /// later clear to erase the replacement diagnostics. If it has not left
    /// the queue yet, the replacement supersedes it atomically.
    fn supersede_retired_diagnostic(&self, uri: &lsp_types::Url) -> bool {
        let Ok(mut retired) = self.retired_diagnostics.lock() else {
            return false;
        };
        let Some(position) = retired
            .pending
            .iter()
            .position(|pending| &pending.uri == uri)
        else {
            return true;
        };
        if retired.pending[position].in_flight {
            return false;
        }
        if let Some(removed) = retired.pending.remove(position) {
            retired.pending_bytes = retired.pending_bytes.saturating_sub(removed.bytes);
        }
        true
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn schedule_retired_diagnostics_retry(&self, output: Arc<LspSessionOutput>) {
        let delay_ms = {
            let Ok(mut retired) = output.retired_diagnostics.lock() else {
                return;
            };
            if retired.pending.is_empty() || retired.retry_scheduled {
                return;
            }
            retired.retry_scheduled = true;
            let attempt = retired.retry_count.min(6);
            retired.retry_count = retired.retry_count.saturating_add(1);
            (5_u64 << attempt).min(250)
        };
        let this = self.clone();
        self.spawner.spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            if let Ok(mut retired) = output.retired_diagnostics.lock() {
                retired.retry_scheduled = false;
            }
            this.flush_retired_diagnostics_for(output);
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn schedule_retired_diagnostics_retry(&self, _output: Arc<LspSessionOutput>) {
        // The in-process WASM sender has no bounded native writer. A failure
        // here is a terminal embedding error; retaining the coalesced clears
        // is sufficient for the next explicit state flush.
    }

    /// Directories that are never descended into during workspace discovery,
    /// even when a workspace has no `.gitignore` to prune them. Mirrors
    /// `should_skip_poll_dir` in `baml_lsp_server`.
    fn should_skip_discovery_dir(name: &str) -> bool {
        name.starts_with('.') || matches!(name, "node_modules" | "target" | "dist")
    }

    /// Recursively find directories that are project roots by real markers
    /// (`baml.toml` file or `baml_src/` child).
    ///
    /// On native this walks with the `ignore` crate (like ruff), so
    /// `.gitignore`d directories (`target/`, `node_modules/`, build output)
    /// are pruned before descending; `should_skip_discovery_dir` is a
    /// backstop for workspaces that are not git repositories.
    #[cfg(not(target_arch = "wasm32"))]
    fn collect_marked_project_roots(&self, root: &vfs::VfsPath) -> Vec<vfs::VfsPath> {
        // Native VfsPaths are OS paths joined onto the filesystem root, so
        // the OS-level walker applies whenever the path really exists on
        // disk. Fall back to the VFS walk otherwise (e.g. in-memory
        // filesystems in tests).
        let os_root = std::path::Path::new(root.as_str());
        if os_root.is_dir() {
            return self.collect_marked_project_roots_native(os_root);
        }
        let mut found = Vec::new();
        Self::collect_marked_project_roots_vfs(root, &mut found);
        found
    }

    #[cfg(target_arch = "wasm32")]
    fn collect_marked_project_roots(root: &vfs::VfsPath) -> Vec<vfs::VfsPath> {
        let mut found = Vec::new();
        Self::collect_marked_project_roots_vfs(root, &mut found);
        found
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn collect_marked_project_roots_native(&self, os_root: &std::path::Path) -> Vec<vfs::VfsPath> {
        let mut found = Vec::new();
        for dir in Self::scan_marked_project_roots_native(os_root) {
            match self
                .fs
                .get_path_from_path(&dir, "discover_workspace_projects")
            {
                Ok(vfs_path) => found.push(vfs_path),
                Err(e) => tracing::warn!("Skipping discovered project root: {e}"),
            }
        }
        found
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn scan_marked_project_roots_native(os_root: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut found = Vec::new();
        let walker = ignore::WalkBuilder::new(os_root)
            .standard_filters(true)
            .follow_links(false)
            .filter_entry(|entry| {
                let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
                !(is_dir
                    && entry
                        .file_name()
                        .to_str()
                        .is_some_and(Self::should_skip_discovery_dir))
            })
            .build();
        for entry in walker.filter_map(Result::ok) {
            if !entry.file_type().is_some_and(|t| t.is_dir()) {
                continue;
            }
            let dir = entry.path();
            if dir.join(BAML_TOML).is_file() || dir.join(BAML_SRC_DIR).is_dir() {
                found.push(dir.to_path_buf());
            }
        }
        found
    }

    fn collect_marked_project_roots_vfs(root: &vfs::VfsPath, found: &mut Vec<vfs::VfsPath>) {
        if Self::has_baml_toml(root) || Self::has_baml_src_dir(root) {
            found.push(root.clone());
        }
        let Ok(entries) = root.read_dir() else {
            return;
        };
        for entry in entries {
            if Self::should_skip_discovery_dir(&entry.filename()) {
                continue;
            }
            if entry.is_dir().unwrap_or(false) {
                Self::collect_marked_project_roots_vfs(&entry, found);
            }
        }
    }

    fn refresh_project_async(&self, project_root: &vfs::VfsPath, refresh_mode: ProjectRefreshMode) {
        use crate::bex_lsp::notification::BexLspNotification;
        let Ok(project) = self.get_or_create_project(project_root.to_owned()) else {
            return;
        };

        let full_diagnostic_refresh = match refresh_mode {
            ProjectRefreshMode::Full => {
                let sources = match self.load_project_sources(project_root) {
                    Ok(sources) => sources,
                    Err(e) => {
                        if self.session_config.get().is_some() {
                            let _ =
                                self.send_notification_show_message(lsp_types::ShowMessageParams {
                                    typ: lsp_types::MessageType::ERROR,
                                    message: format!(
                                        "Failed to read project files for {project_root:?}: {e}"
                                    ),
                                });
                        }
                        return;
                    }
                };
                project.project.apply_all_sources(&sources);
                true
            }
            ProjectRefreshMode::Applied {
                full_diagnostic_refresh,
            } => full_diagnostic_refresh,
        };

        self.schedule_project_tail(project_root.clone(), &project, full_diagnostic_refresh);

        if project
            .runtime_demand
            .load(std::sync::atomic::Ordering::Acquire)
            > 0
        {
            #[cfg(not(target_arch = "wasm32"))]
            self.schedule_engine_rebuild(project_root, &project);
            #[cfg(target_arch = "wasm32")]
            {
                let _ = project.project.request_engine_build();
                self.spawn_engine_build_and_publish(project_root.clone(), project);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn schedule_project_tail(
        &self,
        project_root: vfs::VfsPath,
        project: &Arc<LiveProject>,
        full_diagnostic_refresh: bool,
    ) {
        use std::sync::atomic::Ordering;
        let source_revision = project.project.source_revision();
        if full_diagnostic_refresh {
            project
                .pending_full_diagnostic_refresh_revision
                .fetch_max(source_revision.0, Ordering::AcqRel);
        }
        project.diagnostic_retry_count.store(0, Ordering::Release);
        let epoch = project.tail_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        let this = self.clone();
        let scheduled_project = project.clone();
        let task = self.spawner.spawn_abortable(async move {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            if scheduled_project.tail_epoch.load(Ordering::SeqCst) != epoch
                || scheduled_project.project.source_revision() != source_revision
            {
                return;
            }
            this.run_project_tail(&project_root, &scheduled_project, source_revision);
        });
        if let Some(previous) = project.tail_task.lock().unwrap().replace(task) {
            previous.abort();
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn schedule_project_tail(
        &self,
        project_root: vfs::VfsPath,
        project: &Arc<LiveProject>,
        full_diagnostic_refresh: bool,
    ) {
        let source_revision = project.project.source_revision();
        if full_diagnostic_refresh {
            project
                .pending_full_diagnostic_refresh_revision
                .fetch_max(source_revision.0, std::sync::atomic::Ordering::AcqRel);
        }
        let this = self.clone();
        let project = project.clone();
        self.spawner.spawn(async move {
            this.run_project_tail(&project_root, &project, source_revision);
        });
    }

    fn run_project_tail(
        &self,
        project_root: &vfs::VfsPath,
        project: &Arc<LiveProject>,
        source_revision: crate::project::SourceRevision,
    ) {
        if project.project.source_revision() != source_revision {
            return;
        }
        let playground_candidate = match project
            .project
            .diagnostics_by_file(PositionEncoding::Utf16)
        {
            DiagnosticRead::Ready(candidate) if candidate.source_revision == source_revision => {
                candidate
            }
            DiagnosticRead::Ready(_) => return,
            DiagnosticRead::Busy => {
                self.schedule_diagnostics_retry(project_root, source_revision);
                return;
            }
            DiagnosticRead::Poisoned => {
                project
                    .project
                    .mark_broken("collecting project diagnostics");
                return;
            }
        };
        let full_refresh_ticket = project
            .pending_full_diagnostic_refresh_revision
            .load(std::sync::atomic::Ordering::Acquire);
        let full_diagnostic_refresh =
            full_refresh_ticket != 0 && full_refresh_ticket <= source_revision.0;
        let mut delivered_to_every_output = true;
        for output in self.active_initialized_lsp_outputs() {
            let output_session = self.retargeted_lsp_output(output);
            if !output_session.publish_diagnostics_for_output(
                project_root,
                project,
                source_revision,
                &playground_candidate,
                full_diagnostic_refresh,
            ) {
                delivered_to_every_output = false;
            }
        }
        if !delivered_to_every_output {
            // The bounded writer was saturated. Keep the revision dirty and
            // recompute/requeue it; a final invalid edit must not require a
            // later keystroke to become visible.
            self.schedule_diagnostics_retry(project_root, source_revision);
            return;
        }
        project
            .diagnostic_retry_count
            .store(0, std::sync::atomic::Ordering::Release);
        if !project
            .project
            .clear_diagnostics_if_current(source_revision)
        {
            return;
        }
        if full_diagnostic_refresh {
            let _ = project
                .pending_full_diagnostic_refresh_revision
                .compare_exchange(
                    full_refresh_ticket,
                    0,
                    std::sync::atomic::Ordering::AcqRel,
                    std::sync::atomic::Ordering::Acquire,
                );
        }
        self.send_list_projects(false);
        if self.playground_sender.has_runtime_subscribers()
            && project
                .runtime_demand
                .load(std::sync::atomic::Ordering::Acquire)
                > 0
        {
            let flat = Self::flatten_diagnostics(&playground_candidate.documents);
            for session_epoch in self.playground_session_targets() {
                self.send_update_project(
                    project_root,
                    project,
                    source_revision,
                    session_epoch,
                    flat.clone(),
                    false,
                );
            }
        }
    }

    fn publish_diagnostics_for_output(
        &self,
        project_root: &vfs::VfsPath,
        project: &Arc<LiveProject>,
        source_revision: crate::project::SourceRevision,
        utf16_candidate: &diagnostics::DiagnosticCandidate,
        full_diagnostic_refresh: bool,
    ) -> bool {
        let Some(config) = self.session_config.get().copied() else {
            return true;
        };
        let negotiated_candidate = if config.position_encoding == PositionEncoding::Utf16 {
            None
        } else {
            match project
                .project
                .diagnostics_by_file(config.position_encoding)
            {
                DiagnosticRead::Ready(candidate)
                    if candidate.source_revision == source_revision =>
                {
                    Some(candidate)
                }
                DiagnosticRead::Ready(_) | DiagnosticRead::Busy => return false,
                DiagnosticRead::Poisoned => {
                    project
                        .project
                        .mark_broken("collecting negotiated diagnostics");
                    return false;
                }
            }
        };
        let candidate = negotiated_candidate.as_ref().unwrap_or(utf16_candidate);
        let output_documents = self
            .open_documents
            .lock()
            .unwrap()
            .iter()
            .map(|(path, document)| (canonical_fs_path_identity(path), document.clone()))
            .collect::<HashMap<_, _>>();
        let current_files = candidate
            .documents
            .keys()
            .filter_map(|path| {
                let path = canonical_fs_path_identity(&crate::fs::FsPath::from_str(
                    path.to_string_lossy().into_owned(),
                ));
                let uri = output_documents
                    .get(&path)
                    .map(|document| document.client_uri.clone())
                    .or_else(|| wasm_helpers::from_file_path(path.as_path()).ok())?;
                Some((path, uri))
            })
            .collect::<HashMap<_, _>>();
        let mut publications = Vec::with_capacity(current_files.len());
        for (path, uri) in &current_files {
            let candidate_diagnostics = candidate
                .documents
                .get(path.as_path())
                .cloned()
                .unwrap_or_default();
            let output_document = output_documents.get(path);
            let output_text_matches = output_document.is_none_or(|document| {
                candidate
                    .source_texts
                    .get(path)
                    .is_some_and(|text| text == &document.text)
            });
            // Concurrent clients can use different version sequences for
            // identical text, so tag matching text with each client's own
            // version. If the texts diverge, publish an exact-version clear:
            // diagnostics computed from the shared latest source may have
            // position ranges that are invalid for this output's document.
            let file_diagnostics = if output_text_matches {
                candidate_diagnostics
            } else {
                Vec::new()
            };
            let version = output_document.map(|document| document.version);
            publications.push(crate::project::ProjectPublication::LspDiagnostics {
                path: path.clone(),
                present: true,
                params: lsp_types::PublishDiagnosticsParams::new(
                    uri.clone(),
                    file_diagnostics,
                    version,
                ),
            });
        }
        let project_key = PublishedProjectKey {
            project_root: crate::fs::FsPath::from_vfs(project_root),
            incarnation: project.incarnation,
        };
        if full_diagnostic_refresh
            && let Ok(published) = self.published_diagnostics.lock()
            && let Some(previous) = published.get(&project_key)
        {
            for (deleted, uri) in previous {
                if current_files.contains_key(deleted) {
                    continue;
                }
                publications.push(crate::project::ProjectPublication::LspDiagnostics {
                    path: deleted.clone(),
                    present: false,
                    params: lsp_types::PublishDiagnosticsParams::new(uri.clone(), Vec::new(), None),
                });
            }
        }

        // Serialize this output's acknowledged-URI read, mailbox drain, and
        // history update. Other outputs use the same project sequencer but
        // retain independent transport checkpoints.
        let Some(_publication_drain) = project.project.lock_publication_drainer() else {
            project
                .project
                .mark_broken("locking the project publication sequencer");
            return false;
        };
        let delivered = self.enqueue_and_drain_diagnostics_locked(
            &project_key.project_root,
            project,
            source_revision,
            publications,
        );
        delivered || self.sender.is_closed()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn schedule_diagnostics_retry(
        &self,
        project_root: &vfs::VfsPath,
        source_revision: crate::project::SourceRevision,
    ) {
        let Some(project) = self.project_for_root_str(project_root.as_str()) else {
            return;
        };
        if project.project.source_revision() != source_revision {
            return;
        }
        let epoch = project
            .tail_epoch
            .load(std::sync::atomic::Ordering::Acquire);
        let attempt = project
            .diagnostic_retry_count
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            .min(6);
        let delay_ms = (5_u64 << attempt).min(250);
        let this = self.clone();
        let project_root = project_root.clone();
        let scheduled_project = project.clone();
        let task = self.spawner.spawn_abortable(async move {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            if scheduled_project.project.source_revision() == source_revision
                && scheduled_project
                    .tail_epoch
                    .load(std::sync::atomic::Ordering::Acquire)
                    == epoch
            {
                this.run_project_tail(&project_root, &scheduled_project, source_revision);
            }
        });
        let mut tail_task = project.tail_task.lock().unwrap();
        if project.project.source_revision() != source_revision
            || project
                .tail_epoch
                .load(std::sync::atomic::Ordering::Acquire)
                != epoch
        {
            task.abort();
            return;
        }
        if let Some(previous) = tail_task.replace(task) {
            previous.abort();
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn schedule_diagnostics_retry(
        &self,
        _project_root: &vfs::VfsPath,
        _source_revision: crate::project::SourceRevision,
    ) {
        // The WASM runtime is single-threaded; observing WouldBlock here would
        // indicate re-entrant project work rather than ordinary contention.
    }

    fn drain_project_publications(
        &self,
        project_root: &crate::fs::FsPath,
        project: &Arc<LiveProject>,
    ) -> Option<PublicationDrainReport> {
        let _drain = project.project.lock_publication_drainer()?;
        self.drain_project_publications_locked(project_root, project)
    }

    /// Diagnostics are replacement notifications per URI, so one oversized
    /// aggregate batch can be split by URI without changing LSP semantics. A
    /// single URI that exceeds the bounded writer limit is replaced with one
    /// explicit diagnostic instead of retrying forever or silently clearing
    /// the previous publication.
    fn enqueue_and_drain_diagnostics_locked(
        &self,
        project_root: &crate::fs::FsPath,
        project: &Arc<LiveProject>,
        source_revision: crate::project::SourceRevision,
        publications: Vec<crate::project::ProjectPublication>,
    ) -> bool {
        let identity = crate::project::BexProject::source_publication_identity(source_revision);
        let progress_key = DiagnosticProgressKey {
            project_root: project_root.clone(),
            incarnation: project.incarnation,
            source_revision,
        };
        let publications = {
            let Ok(mut progress) = self.diagnostic_progress.lock() else {
                return false;
            };
            progress.retain(|key, _| {
                key.project_root != *project_root
                    || (key.incarnation == project.incarnation
                        && key.source_revision == source_revision)
            });
            let delivered = progress.entry(progress_key.clone()).or_default();
            publications
                .into_iter()
                .filter(|publication| {
                    let crate::project::ProjectPublication::LspDiagnostics {
                        path, present, ..
                    } = publication
                    else {
                        return true;
                    };
                    !delivered.contains(&(path.clone(), *present))
                })
                .collect::<Vec<_>>()
        };
        if publications.is_empty() {
            let delivered = self
                .drain_project_publications_locked(project_root, project)
                .is_some();
            if delivered && let Ok(mut progress) = self.diagnostic_progress.lock() {
                progress.remove(&progress_key);
            }
            return delivered;
        }

        for mut publication in publications {
            let publication_key = match &publication {
                crate::project::ProjectPublication::LspDiagnostics { path, present, .. } => {
                    (path.clone(), *present)
                }
                crate::project::ProjectPublication::Playground { .. } => return false,
            };
            let mut drained_for_capacity = false;
            let mut replaced_oversized_payload = false;
            loop {
                match project
                    .project
                    .enqueue_publication_batch_if_current(identity, vec![publication.clone()])
                {
                    Ok(batch_id) => {
                        let Some(report) =
                            self.drain_project_publications_locked(project_root, project)
                        else {
                            return false;
                        };
                        if !report.completed_batches.contains(&batch_id) {
                            return false;
                        }
                        if let Ok(mut progress) = self.diagnostic_progress.lock() {
                            progress
                                .entry(progress_key.clone())
                                .or_default()
                                .insert(publication_key.clone());
                        }
                        break;
                    }
                    Err(crate::project::PublicationEnqueueError::Saturated)
                        if !drained_for_capacity =>
                    {
                        if self
                            .drain_project_publications_locked(project_root, project)
                            .is_none()
                        {
                            return false;
                        }
                        drained_for_capacity = true;
                    }
                    Err(crate::project::PublicationEnqueueError::Oversized)
                        if !replaced_oversized_payload =>
                    {
                        let crate::project::ProjectPublication::LspDiagnostics { params, .. } =
                            &mut publication
                        else {
                            return false;
                        };
                        params.diagnostics = vec![lsp_types::Diagnostic::new_simple(
                            lsp_types::Range::new(
                                lsp_types::Position::new(0, 0),
                                lsp_types::Position::new(0, 0),
                            ),
                            "BAML produced more diagnostics for this file than the bounded LSP writer can deliver"
                                .to_string(),
                        )];
                        replaced_oversized_payload = true;
                    }
                    Err(crate::project::PublicationEnqueueError::Stale) => return false,
                    Err(crate::project::PublicationEnqueueError::Serialization) => {
                        project
                            .project
                            .mark_broken("serializing a diagnostics publication");
                        return false;
                    }
                    Err(
                        crate::project::PublicationEnqueueError::Saturated
                        | crate::project::PublicationEnqueueError::Oversized,
                    ) => return false,
                }
            }
        }
        if let Ok(mut progress) = self.diagnostic_progress.lock() {
            progress.remove(&progress_key);
        }
        true
    }

    /// Drain while the caller owns the project's sequencer guard.
    fn drain_project_publications_locked(
        &self,
        project_root: &crate::fs::FsPath,
        project: &Arc<LiveProject>,
    ) -> Option<PublicationDrainReport> {
        use crate::bex_lsp::notification::BexLspNotification;
        let mut report = PublicationDrainReport::default();
        let mut delivered_by_batch = std::collections::HashMap::new();
        while self.project_incarnation_is_current(project_root, project) {
            let Some(envelope) = project.project.pop_next_publication() else {
                break;
            };
            let batch_id = envelope.batch_id();
            let batch_len = envelope.batch_len();
            let identity = envelope.identity();
            {
                let Some(_barrier) = project.project.lock_publication_barrier() else {
                    project.project.discard_publication_batch(batch_id);
                    return None;
                };
                // Removal/re-add may win while the old mailbox is being
                // popped. The barrier makes this final currentness decision
                // atomic with source/runtime mutation. Transport encoding and
                // delivery happen only after every shared guard is released;
                // document versions and session/revision fences reject a
                // frame superseded after this reservation point.
                if !self.project_incarnation_is_current(project_root, project) {
                    project.project.discard_publication_batch(batch_id);
                    return None;
                }
                if !project.project.publication_identity_is_current(identity) {
                    project.project.discard_publication_batch(batch_id);
                    continue;
                }
            }
            match envelope.publication {
                crate::project::ProjectPublication::LspDiagnostics {
                    path,
                    present,
                    params,
                } => {
                    let uri = params.uri.clone();
                    let version = params.version;
                    if !self.supersede_retired_diagnostic(&uri) {
                        project.project.discard_publication_batch(batch_id);
                        return None;
                    }
                    match self.send_notification_publish_diagnostics(params) {
                        Ok(()) => {}
                        Err(LspError::OutboundOversized) => {
                            let fallback = lsp_types::PublishDiagnosticsParams::new(
                                uri.clone(),
                                vec![lsp_types::Diagnostic::new_simple(
                                    lsp_types::Range::new(
                                        lsp_types::Position::new(0, 0),
                                        lsp_types::Position::new(0, 0),
                                    ),
                                    "BAML produced more diagnostics for this file than the LSP transport can deliver"
                                        .to_string(),
                                )],
                                version,
                            );
                            if self
                                .send_notification_publish_diagnostics(fallback)
                                .is_err()
                            {
                                project.project.discard_publication_batch(batch_id);
                                return None;
                            }
                        }
                        Err(_) => {
                            project.project.discard_publication_batch(batch_id);
                            return None;
                        }
                    }
                    let project_key = PublishedProjectKey {
                        project_root: project_root.clone(),
                        incarnation: project.incarnation,
                    };
                    let mut histories = self.published_diagnostics.lock().unwrap();
                    let published = histories.entry(project_key).or_default();
                    if present {
                        published.insert(path, uri);
                    } else {
                        published.remove(&path);
                    }
                }
                crate::project::ProjectPublication::Playground {
                    session_epoch,
                    notification,
                } => {
                    if !self.playground_session_is_current_internal(session_epoch) {
                        // A closed endpoint no longer requires delivery. Count
                        // this envelope as retired so one disconnect cannot
                        // roll back a shared registry/tree update destined for
                        // the remaining active sessions.
                    } else {
                        self.playground_sender
                            .send_playground_notification(notification);
                    }
                }
            }
            let delivered = delivered_by_batch.entry(batch_id).or_insert(0usize);
            *delivered += 1;
            if *delivered == batch_len {
                report.completed_batches.insert(batch_id);
                delivered_by_batch.remove(&batch_id);
            }
        }
        self.project_incarnation_is_current(project_root, project)
            .then_some(report)
    }

    /// Debounced engine rebuild: bytecode generation, `BexEngine::new` (which
    /// executes `$init`), and test collection are the heavy tail of a refresh.
    /// Running them per keystroke burned CPU and heap on engines that were
    /// discarded milliseconds later, so they run on a background task after
    /// the project has been quiet for the debounce window.
    #[cfg(not(target_arch = "wasm32"))]
    fn schedule_engine_rebuild(
        &self,
        project_root: &vfs::VfsPath,
        project: &std::sync::Arc<LiveProject>,
    ) {
        use std::sync::atomic::Ordering;
        const ENGINE_REBUILD_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(300);

        let epoch = project.rebuild_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        let this = self.clone();
        let project_root = project_root.clone();
        let scheduled_project = project.clone();
        let task = self.spawner.spawn_abortable(async move {
            tokio::time::sleep(ENGINE_REBUILD_DEBOUNCE).await;
            if scheduled_project.rebuild_epoch.load(Ordering::SeqCst) != epoch {
                // A newer refresh superseded this one; its own rebuild is scheduled.
                return;
            }
            if scheduled_project.runtime_demand.load(Ordering::Acquire) == 0 {
                return;
            }
            let _ = scheduled_project.project.request_engine_build();
            this.spawn_engine_build_and_publish(project_root, scheduled_project);
        });
        if let Some(previous) = project.rebuild_task.lock().unwrap().replace(task) {
            previous.abort();
        }
    }

    fn flatten_diagnostics(
        diagnostics: &std::collections::HashMap<std::path::PathBuf, Vec<lsp_types::Diagnostic>>,
    ) -> Vec<crate::bex_lsp::ProjectDiagnostic> {
        let mut out = Vec::new();
        for (path, diags) in diagnostics {
            let filename = path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            for d in diags {
                let severity = match d.severity {
                    Some(lsp_types::DiagnosticSeverity::ERROR) => "error",
                    Some(lsp_types::DiagnosticSeverity::WARNING) => "warning",
                    _ => "info",
                };
                let line = d.range.start.line + 1;
                out.push(crate::bex_lsp::ProjectDiagnostic {
                    severity,
                    message: format!("{filename}:{line}: {}", d.message),
                });
            }
        }
        out.sort_by(|a, b| a.message.cmp(&b.message));
        out
    }

    fn build_project_update(
        project: &LiveProject,
        expected_revision: crate::project::SourceRevision,
        diagnostics: Vec<crate::bex_lsp::ProjectDiagnostic>,
    ) -> Option<(
        crate::project::ProjectPublicationIdentity,
        crate::bex_lsp::ProjectUpdate,
    )> {
        let db_guard = project.project.db.lock().ok()?;
        if db_guard.source_revision() != expected_revision {
            return None;
        }
        let (status, identity) = project
            .project
            .runtime_status_and_identity_with_source(&db_guard);
        let is_bex_current = status.phase == crate::project::RuntimeBuildPhase::Ready;
        let runtime_state = match status.phase {
            crate::project::RuntimeBuildPhase::IdleStale => "idleStale",
            crate::project::RuntimeBuildPhase::Building => "building",
            crate::project::RuntimeBuildPhase::Ready => "ready",
            crate::project::RuntimeBuildPhase::BlockedByDiagnostics => "blockedByDiagnostics",
            crate::project::RuntimeBuildPhase::Failed => "failed",
        };
        let db = db_guard.db();
        let listing = baml_project::list_functions_with_metadata(db);
        let functions = listing
            .functions
            .into_iter()
            .map(|f| crate::bex_lsp::FunctionInfo {
                name: f.name,
                kind: if f.is_llm {
                    crate::bex_lsp::FunctionKind::Llm
                } else {
                    crate::bex_lsp::FunctionKind::Expr
                },
                origin: f.origin.into(),
                capabilities: if f.is_llm {
                    Some(crate::bex_lsp::LlmCapabilities {
                        render_prompt: true,
                        build_request: true,
                        client_name: f.client_name,
                    })
                } else {
                    None
                },
                params: f.params,
            })
            .collect();

        let update = crate::bex_lsp::ProjectUpdate {
            source_revision: status.requested_revision.0,
            project_incarnation: project.incarnation,
            runtime: crate::bex_lsp::ProjectRuntimeStatus {
                state: runtime_state.to_string(),
                requested_revision: status.requested_revision.0,
                installed_revision: status.installed_revision.map(|revision| revision.0),
                generation: status.generation,
                has_last_known_good: status.has_last_known_good,
                error_message: status.error_message,
            },
            is_bex_current,
            functions,
            types: Some(listing.types),
            diagnostics,
        };
        Some((identity, update))
    }

    fn send_list_projects(&self, force: bool) {
        let mut entries: Vec<_> = self
            .projects
            .lock()
            .unwrap()
            .iter()
            .map(|(path, project)| crate::bex_lsp::ProjectCatalogEntry {
                project: path.as_path().to_string_lossy().into_owned(),
                incarnation: project.incarnation,
                source_revision: project.project.source_revision().0,
            })
            .collect();
        entries.sort_by(|left, right| left.project.cmp(&right.project));
        let roots: Vec<_> = entries.iter().map(|entry| entry.project.clone()).collect();
        let send_playground = {
            let mut last_catalog = self.last_catalog.lock().unwrap();
            if !force && last_catalog.as_ref() == Some(&entries) {
                false
            } else {
                *last_catalog = Some(entries.clone());
                true
            }
        };
        if send_playground {
            for session_epoch in self.playground_session_targets() {
                self.playground_sender.send_playground_notification(
                    crate::bex_lsp::PlaygroundNotification::ListProjects {
                        session_epoch,
                        projects: roots.clone(),
                        entries: entries.clone(),
                    },
                );
            }
        }

        // Root/poller changes are process-owned and fan out to every active
        // initialized LSP connection. Each connection retains only its newest
        // pending catalog until its bounded writer accepts the frame.
        for output in self.catalog_lsp_outputs(force) {
            self.enqueue_lsp_catalog(output, entries.clone(), force);
        }
    }

    fn send_update_project(
        &self,
        project_root: &vfs::VfsPath,
        project: &Arc<LiveProject>,
        expected_revision: crate::project::SourceRevision,
        session_epoch: u64,
        diagnostics: Vec<crate::bex_lsp::ProjectDiagnostic>,
        force: bool,
    ) -> bool {
        let Some((identity, update)) =
            Self::build_project_update(project, expected_revision, diagnostics)
        else {
            return false;
        };
        {
            let previous = project.last_project_update.lock().unwrap();
            if !force && previous.get(&session_epoch) == Some(&update) {
                return true;
            }
        }
        let notification = crate::bex_lsp::PlaygroundNotification::UpdateProject {
            session_epoch,
            project: project_root.as_str().to_string(),
            update: update.clone(),
        };
        let project_key = crate::fs::FsPath::from_vfs(project_root);
        let Some(batch_id) = self.enqueue_project_playground_publication_batch(
            &project_key,
            project,
            identity,
            session_epoch,
            notification,
        ) else {
            return false;
        };
        let Some(delivered) = self.drain_project_publications(&project_key, project) else {
            return false;
        };
        if !delivered.completed_batches.contains(&batch_id)
            || !project.project.publication_identity_is_current(identity)
            || !self.project_incarnation_is_current(&project_key, project)
        {
            return false;
        }
        project
            .last_project_update
            .lock()
            .unwrap()
            .insert(session_epoch, update);
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn publish_test_collection_error(
        &self,
        project_root: &crate::fs::FsPath,
        project_handle: &Arc<LiveProject>,
        identity: crate::project::ProjectPublicationIdentity,
        session_epoch: u64,
        project: String,
        source_revision: u64,
        generation: u64,
        collection_epoch: u64,
        call_id: sys_types::CallId,
        message: String,
    ) {
        let notification = crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
            session_epoch,
            project,
            project_incarnation: project_handle.incarnation,
            source_revision,
            generation,
            collection_epoch,
            call_id: call_id.0,
            data: Vec::new(),
            expand_error: None,
            collection_error: Some(message),
        };
        self.publish_test_collection_notification(
            project_root,
            project_handle,
            identity,
            session_epoch,
            notification,
        );
    }

    fn publish_test_collection_notification(
        &self,
        project_root: &crate::fs::FsPath,
        project_handle: &Arc<LiveProject>,
        identity: crate::project::ProjectPublicationIdentity,
        session_epoch: u64,
        mut notification: crate::bex_lsp::PlaygroundNotification,
    ) -> bool {
        let mut drained_for_capacity = false;
        let mut replaced_oversized_payload = false;
        loop {
            match self.enqueue_project_playground_publication_batch_result(
                project_root,
                project_handle,
                identity,
                session_epoch,
                notification.clone(),
            ) {
                Ok(batch_id) => {
                    let Some(report) =
                        self.drain_project_publications(project_root, project_handle)
                    else {
                        return false;
                    };
                    let delivered = report.completed_batches.contains(&batch_id);
                    if !delivered {
                        tracing::warn!(
                            "test collection publication was not fully delivered before invalidation"
                        );
                    }
                    return delivered && !replaced_oversized_payload;
                }
                Err(crate::project::PublicationEnqueueError::Saturated)
                    if !drained_for_capacity =>
                {
                    if self
                        .drain_project_publications(project_root, project_handle)
                        .is_none()
                    {
                        return false;
                    }
                    drained_for_capacity = true;
                }
                Err(crate::project::PublicationEnqueueError::Oversized)
                    if !replaced_oversized_payload =>
                {
                    let crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                        data,
                        collection_error,
                        ..
                    } = &mut notification
                    else {
                        return false;
                    };
                    data.clear();
                    *collection_error = Some(
                        "The serialized test tree exceeds the bounded playground transport limit; the previous tree was retained"
                            .to_string(),
                    );
                    replaced_oversized_payload = true;
                }
                Err(crate::project::PublicationEnqueueError::Serialization) => {
                    project_handle
                        .project
                        .mark_broken("serializing a test collection publication");
                    return false;
                }
                Err(
                    crate::project::PublicationEnqueueError::Stale
                    | crate::project::PublicationEnqueueError::Saturated
                    | crate::project::PublicationEnqueueError::Oversized,
                ) => return false,
            }
        }
    }

    fn publish_test_collection_notification_to_all(
        &self,
        project_root: &crate::fs::FsPath,
        project_handle: &Arc<LiveProject>,
        identity: crate::project::ProjectPublicationIdentity,
        origin_session_epoch: u64,
        notification: crate::bex_lsp::PlaygroundNotification,
    ) -> bool {
        let mut drained_for_capacity = false;
        loop {
            let enqueue_result = {
                let Ok(sessions) = self.active_playground_sessions.lock() else {
                    return false;
                };
                if !self.project_incarnation_is_current(project_root, project_handle) {
                    return false;
                }
                let Some(targets) =
                    self.playground_targets_for_origin_locked(&sessions, origin_session_epoch)
                else {
                    return false;
                };
                let publications = targets
                    .into_iter()
                    .filter_map(|target| {
                        Self::retarget_playground_notification(notification.clone(), target).map(
                            |notification| crate::project::ProjectPublication::Playground {
                                session_epoch: target,
                                notification,
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                project_handle
                    .project
                    .enqueue_publication_batch_if_current(identity, publications)
            };
            match enqueue_result {
                Ok(batch_id) => {
                    return self
                        .drain_project_publications(project_root, project_handle)
                        .is_some_and(|report| report.completed_batches.contains(&batch_id));
                }
                Err(crate::project::PublicationEnqueueError::Saturated)
                    if !drained_for_capacity =>
                {
                    if self
                        .drain_project_publications(project_root, project_handle)
                        .is_none()
                    {
                        return false;
                    }
                    drained_for_capacity = true;
                }
                Err(crate::project::PublicationEnqueueError::Oversized) => {
                    self.publish_test_collection_notification(
                        project_root,
                        project_handle,
                        identity,
                        origin_session_epoch,
                        notification,
                    );
                    return false;
                }
                Err(crate::project::PublicationEnqueueError::Serialization) => {
                    project_handle
                        .project
                        .mark_broken("serializing a shared test tree publication");
                    return false;
                }
                Err(
                    crate::project::PublicationEnqueueError::Stale
                    | crate::project::PublicationEnqueueError::Saturated,
                ) => return false,
            }
        }
    }

    fn install_and_publish_test_collection(
        &self,
        project_root: &crate::fs::FsPath,
        project_handle: &Arc<LiveProject>,
        lease: &crate::project::CollectionLease,
        session_epoch: u64,
        registry: Option<&bex_external_types::Handle>,
        notification: crate::bex_lsp::PlaygroundNotification,
    ) -> bool {
        let mut drained_for_capacity = false;
        loop {
            let install_result = {
                let Ok(sessions) = self.active_playground_sessions.lock() else {
                    return false;
                };
                if !self.project_incarnation_is_current(project_root, project_handle) {
                    return false;
                }
                let Some(targets) =
                    self.playground_targets_for_origin_locked(&sessions, session_epoch)
                else {
                    return false;
                };
                let publications = targets
                    .into_iter()
                    .filter_map(|target| {
                        Self::retarget_playground_notification(notification.clone(), target).map(
                            |notification| crate::project::ProjectPublication::Playground {
                                session_epoch: target,
                                notification,
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                project_handle.project.install_test_registry_and_enqueue(
                    lease,
                    registry.cloned(),
                    publications,
                )
            };
            match install_result {
                Ok(receipt) => {
                    let batch_id = receipt.batch_id;
                    let delivered = self
                        .drain_project_publications(project_root, project_handle)
                        .is_some_and(|report| report.completed_batches.contains(&batch_id));
                    if delivered
                        && project_handle
                            .project
                            .acknowledge_test_registry_publication(&receipt)
                    {
                        return true;
                    }
                    project_handle
                        .project
                        .rollback_test_registry_publication(receipt);
                    return false;
                }
                Err(crate::project::PublicationEnqueueError::Saturated)
                    if !drained_for_capacity =>
                {
                    if self
                        .drain_project_publications(project_root, project_handle)
                        .is_none()
                    {
                        return false;
                    }
                    drained_for_capacity = true;
                }
                Err(crate::project::PublicationEnqueueError::Oversized) => {
                    // Keep the previous registry and tree aligned, while still
                    // delivering a small terminal error to release UI loading.
                    project_handle.project.finish_test_collection(lease);
                    self.publish_test_collection_notification(
                        project_root,
                        project_handle,
                        crate::project::BexProject::collection_publication_identity(lease),
                        session_epoch,
                        notification,
                    );
                    return false;
                }
                Err(crate::project::PublicationEnqueueError::Serialization) => {
                    project_handle.project.finish_test_collection(lease);
                    project_handle
                        .project
                        .mark_broken("serializing a test registry publication");
                    return false;
                }
                Err(
                    crate::project::PublicationEnqueueError::Stale
                    | crate::project::PublicationEnqueueError::Saturated,
                ) => return false,
            }
        }
    }

    fn request_collect_tests_with_temporary_demand(
        &self,
        project_root_str: &str,
        session_epoch: u64,
    ) {
        let Some((project_key, project)) = self.project_entry_for_root_str(project_root_str) else {
            return;
        };
        let demand = RuntimeDemandGuard::acquire(project.clone());
        let this = self.clone();
        let project_root = project_root_str.to_string();
        self.spawner.spawn(async move {
            if Self::ensure_project_engine(project.clone()).await.is_err()
                || !this.project_incarnation_is_current(&project_key, &project)
            {
                return;
            }
            this.request_collect_tests_for_project(
                project_key,
                project,
                project_root,
                session_epoch,
                Some(demand),
            );
        });
    }

    fn request_collect_tests_for_project(
        &self,
        project_root: crate::fs::FsPath,
        project_handle: Arc<LiveProject>,
        project: String,
        session_epoch: u64,
        terminal_demand: Option<RuntimeDemandGuard>,
    ) {
        log::info!("[request_collect_tests_impl] project={project}");
        let this = self.clone();
        self.spawner.spawn(async move {
            let _terminal_demand = terminal_demand;
            // Serialize collection begin through registry/tree transport
            // acknowledgement. A later collection may not observe or retain a
            // registry until its matching tree has either delivered or rolled
            // back to the previously acknowledged registry.
            let collection_owner = project_handle.project.test_collection_owner();
            let _collection_owner = collection_owner.lock().await;
            if !this.playground_session_is_current_internal(session_epoch)
                || !this.project_incarnation_is_current(&project_root, &project_handle)
            {
                return;
            }
            let Some(lease) = project_handle.project.begin_test_collection() else {
                return;
            };
            let package = "user".to_string();
            let call_id = sys_types::CallId::next();
            let engine = lease.engine.clone();
            let cancel = lease.cancel.clone();
            let generation = lease.generation;
            let source_revision = lease.source_revision.0;
            let collection_epoch = lease.collection_epoch;
            let project_incarnation = project_handle.incarnation;
            let publication_identity =
                crate::project::BexProject::collection_publication_identity(&lease);

            // `begin_test_collection` cancels an older expansion. Wait for
            // that expansion's mutation owner to finish rollback before this
            // collection may retain or replace the prior registry.
            let previous_registry_owner = lease.previous_registry_mutation_owner.clone();
            let _previous_registry_mutation = match previous_registry_owner.as_ref() {
                Some(owner) => Some(owner.lock().await),
                None => None,
            };
            match engine
                .collect_tests(&package, call_id, cancel.clone())
                .await
            {
                Ok(registry) => {
                    let handle = match &registry {
                        bex_engine::BexExternalValue::Handle(handle) => Some(handle.clone()),
                        bex_engine::BexExternalValue::Null => None,
                        _ => {
                            log::error!("[collect_tests] unexpected result type");
                            project_handle.project.finish_test_collection(&lease);
                            this.publish_test_collection_error(
                                &project_root,
                                &project_handle,
                                publication_identity,
                                session_epoch,
                                project.clone(),
                                source_revision,
                                generation,
                                collection_epoch,
                                call_id,
                                "Test collection returned an unexpected value".to_string(),
                            );
                            return;
                        }
                    };
                    // If the project has no tests, send an empty test tree.
                    if matches!(registry, bex_engine::BexExternalValue::Null) {
                        let notification =
                            crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                                session_epoch,
                                project,
                                project_incarnation,
                                source_revision,
                                generation,
                                collection_epoch,
                                call_id: call_id.0,
                                data: serde_json::to_vec(&serde_json::json!([]))
                                    .unwrap_or_default(),
                                expand_error: None,
                                collection_error: None,
                            };
                        if !this.install_and_publish_test_collection(
                            &project_root,
                            &project_handle,
                            &lease,
                            session_epoch,
                            None,
                            notification,
                        ) {
                            project_handle.project.finish_test_collection(&lease);
                            log::info!(
                                "[collect_tests] retained prior registry after empty collection publication failed"
                            );
                        }
                        return;
                    }

                    // Serialize the full test tree via TestRegistry.serialize
                    let ctx = bex_engine::FunctionCallContextBuilder::new(call_id)
                        .with_cancel_token(cancel)
                        .with_profile_enabled(false)
                        .build();
                    match engine
                        .call_function(
                            "testing.TestRegistry.serialize",
                            vec![registry],
                            ctx,
                            true, // deep copy for wire
                        )
                        .await
                    {
                        Ok(serialized) => {
                            let data = serde_json::to_vec(&bex_value_to_json(&serialized))
                                .unwrap_or_default();
                            let notification =
                                crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                                    session_epoch,
                                    project,
                                    project_incarnation,
                                    source_revision,
                                    generation,
                                    collection_epoch,
                                    call_id: call_id.0,
                                    data,
                                    expand_error: None,
                                    collection_error: None,
                                };
                            if !this.install_and_publish_test_collection(
                                &project_root,
                                &project_handle,
                                &lease,
                                session_epoch,
                                handle.as_ref(),
                                notification,
                            ) {
                                project_handle.project.finish_test_collection(&lease);
                                log::info!(
                                    "[collect_tests] retained prior registry after serialized collection publication failed"
                                );
                            }
                        }
                        Err(e) => {
                            // Failure is not an empty test tree, and stale errors
                            // are not project state.  Keep the prior tree visible.
                            log::error!("[collect_tests] serialize failed: {e}");
                            project_handle.project.finish_test_collection(&lease);
                            this.publish_test_collection_error(
                                &project_root,
                                &project_handle,
                                publication_identity,
                                session_epoch,
                                project,
                                source_revision,
                                generation,
                                collection_epoch,
                                call_id,
                                format!("Failed to serialize the test collection: {e}"),
                            );
                        }
                    }
                }
                Err(e) => {
                    // Collection failure retains the prior tree; an empty tree
                    // is reserved for a successful zero-test collection.
                    log::error!("[collect_tests] collect_tests failed: {e}");
                    project_handle.project.finish_test_collection(&lease);
                    this.publish_test_collection_error(
                        &project_root,
                        &project_handle,
                        publication_identity,
                        session_epoch,
                        project,
                        source_revision,
                        generation,
                        collection_epoch,
                        call_id,
                        format!("Failed to collect tests: {e}"),
                    );
                }
            }
        });
    }

    async fn call_test_function_impl(
        &self,
        project_root_str: &str,
        generation: u64,
        test_name: &str,
        call_id: sys_types::CallId,
        ctx: bex_engine::FunctionCallContext,
    ) -> Result<bex_engine::BexCallResult, bex_engine::EngineError> {
        let prepared_project = self
            .active_runs
            .lock()
            .ok()
            .and_then(|runs| runs.get(&call_id).map(|(_, project)| project.clone()));
        let was_prepared = prepared_project.is_some();
        let project = prepared_project
            .or_else(|| self.project_for_root_str(project_root_str))
            .ok_or_else(|| bex_engine::EngineError::FunctionNotFound {
                name: format!("project not found: {project_root_str}"),
            })?;
        let lease = if let Some(lease) = project.project.active_run(call_id) {
            // A transport that emitted RunStarted already captured this exact
            // D8 lease. Do not re-check current source after a later edit.
            if lease.generation != generation || lease.test_registry.is_none() {
                <Self as crate::bex_lsp::BexLsp>::finish_project_run(
                    self,
                    project_root_str,
                    call_id,
                );
                return Err(bex_engine::EngineError::FunctionNotFound {
                    name: "prepared test run identity does not match the request".to_string(),
                });
            }
            lease
        } else {
            // Compatibility path for non-RunStore callers. New transports use
            // prepare_test_run before exposing the run.
            project
                .runtime_demand
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            if let Err(error) = Self::ensure_project_engine(project.clone()).await {
                Self::release_runtime_demand(&project);
                return Err(bex_engine::EngineError::FunctionNotFound {
                    name: format!("engine not ready: {error}"),
                });
            }
            match project
                .project
                .prepare_and_register_run(call_id, None, Some(generation), true)
            {
                Ok(lease) => lease,
                Err(error) => {
                    Self::release_runtime_demand(&project);
                    return Err(bex_engine::EngineError::FunctionNotFound {
                        name: error.to_string(),
                    });
                }
            }
        };
        let engine = lease.engine.clone();
        let registry_value = bex_engine::BexExternalValue::Handle(
            lease
                .test_registry
                .clone()
                .expect("test run lease must retain a registry"),
        );

        log::info!("[call_test_function] test_name={test_name} generation={generation}");

        let result = engine
            .call_function_with_trace(
                "testing.TestRegistry.run_test",
                vec![
                    registry_value,
                    bex_engine::BexExternalValue::String(test_name.into()),
                ],
                ctx,
                true, // deep copy TestReport for wire
            )
            .await;

        if !was_prepared && project.project.finish_run(call_id) {
            Self::release_runtime_demand(&project);
        }
        match &result {
            Ok(_) => log::info!("[call_test_function] test_name={test_name} succeeded"),
            Err(e) => log::error!("[call_test_function] test_name={test_name} failed: {e}"),
        }

        result
    }

    fn expand_test_set_impl(&self, project_root_str: &str, generation: u64, testset_name: &str) {
        let Some((project_key, project)) = self.project_entry_for_root_str(project_root_str) else {
            return;
        };
        let demand = RuntimeDemandGuard::acquire(project.clone());
        let this = self.clone();
        let project_root = project_root_str.to_string();
        let testset_name = testset_name.to_string();
        self.spawner.spawn(async move {
            if Self::ensure_project_engine(project.clone()).await.is_err()
                || !this.project_incarnation_is_current(&project_key, &project)
            {
                return;
            }
            this.expand_test_set_current_for_project(
                project_key,
                project,
                project_root,
                generation,
                testset_name,
                Some(demand),
            );
        });
    }

    fn expand_test_set_current_for_project(
        &self,
        project_root: crate::fs::FsPath,
        project_handle: Arc<LiveProject>,
        project: String,
        generation: u64,
        testset_name: String,
        terminal_demand: Option<RuntimeDemandGuard>,
    ) {
        let session_epoch = self.current_playground_session_epoch();
        if !self.playground_session_is_current_internal(session_epoch)
            || !self.project_incarnation_is_current(&project_root, &project_handle)
        {
            return;
        }
        let Some(lease) = project_handle.project.registry_lease(generation) else {
            return;
        };

        let call_id = sys_types::CallId::next();
        let this = self.clone();
        let name = testset_name;
        let source_revision = lease.source_revision.0;
        let collection_epoch = lease.collection_epoch;
        let project_incarnation = project_handle.incarnation;
        let publication_identity =
            crate::project::BexProject::registry_publication_identity(&lease);

        self.spawner.spawn(async move {
            let _terminal_demand = terminal_demand;
            let collection_owner = project_handle.project.test_collection_owner();
            let _collection_owner = collection_owner.lock().await;
            // A registry is mutable heap state.  Exactly one expansion owns it
            // at a time, while source/runtime guards remain released.
            let _mutation_owner = lease.mutation_owner.lock().await;
            if !this.playground_session_is_current_internal(session_epoch)
                || !this.project_incarnation_is_current(&project_root, &project_handle)
                || !project_handle.project.registry_lease_is_current(&lease)
            {
                return;
            }
            let engine = lease.engine.clone();
            let registry_value = bex_engine::BexExternalValue::Handle(lease.registry.clone());
            let cancel = lease.cancel.clone();
            let ctx = bex_engine::FunctionCallContextBuilder::new(call_id)
                .with_cancel_token(cancel.clone())
                .with_profile_enabled(false)
                .build();

            // Record whether this request owns a newly-created expansion. If
            // serialization or bounded publication fails, the same mutation
            // owner rolls it back before releasing the registry.
            log::info!("[expand_test_set] expanding testset: {name}");
            let was_expanded = match test_registry_is_expanded(
                &engine,
                lease.registry.clone(),
                &name,
                cancel.clone(),
            )
            .await
            {
                Ok(expanded) => expanded,
                Err(error) => {
                    let notification =
                        crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                            session_epoch,
                            project,
                            project_incarnation,
                            source_revision,
                            generation,
                            collection_epoch,
                            call_id: call_id.0,
                            data: Vec::new(),
                            expand_error: Some(crate::bex_lsp::TestExpandError {
                                testset_name: name,
                                message: error.to_string(),
                            }),
                            collection_error: Some(
                                "Test expansion could not start; the previous tree was retained"
                                    .to_string(),
                            ),
                        };
                    this.publish_test_collection_notification(
                        &project_root,
                        &project_handle,
                        publication_identity,
                        session_epoch,
                        notification,
                    );
                    return;
                }
            };
            let expanded = engine
                .call_function(
                    "testing.TestRegistry.expand_set_in_place",
                    vec![
                        registry_value.clone(),
                        bex_engine::BexExternalValue::String(name.as_str().into()),
                    ],
                    ctx,
                    true,
                )
                .await;
            if let Err(error) = expanded {
                if !was_expanded
                    && !rollback_test_registry_expansion_or_discard(
                        &project_handle,
                        &lease,
                        &name,
                    )
                    .await
                {
                    return;
                }
                if !project_handle.project.registry_lease_is_current(&lease) {
                    return;
                }
                let data = serialize_test_registry_for_wire(
                    &engine,
                    lease.registry.clone(),
                    sys_types::CancellationToken::new(),
                )
                .await;
                let (data, collection_error) = match data {
                    Ok(data) => (data, None),
                    Err(serialize_error) => (
                        Vec::new(),
                        Some(format!(
                            "Test expansion failed and the retained tree could not be serialized: {serialize_error}"
                        )),
                    ),
                };
                let notification =
                    crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                        session_epoch,
                        project,
                        project_incarnation,
                        source_revision,
                        generation,
                        collection_epoch,
                        call_id: call_id.0,
                        data,
                        expand_error: Some(crate::bex_lsp::TestExpandError {
                            testset_name: name,
                            message: error.to_string(),
                        }),
                        collection_error,
                    };
                this.publish_test_collection_notification_to_all(
                    &project_root,
                    &project_handle,
                    publication_identity,
                    session_epoch,
                    notification,
                );
                return;
            }
            if !project_handle.project.registry_lease_is_current(&lease) {
                if !was_expanded {
                    rollback_test_registry_expansion_or_discard(
                        &project_handle,
                        &lease,
                        &name,
                    )
                    .await;
                }
                return;
            }
            let data = match serialize_test_registry_for_wire(
                &engine,
                lease.registry.clone(),
                cancel.clone(),
            )
            .await
            {
                Ok(data) => data,
                Err(error) => {
                    if !was_expanded
                        && !rollback_test_registry_expansion_or_discard(
                            &project_handle,
                            &lease,
                            &name,
                        )
                        .await
                    {
                        return;
                    }
                    if !project_handle.project.registry_lease_is_current(&lease) {
                        return;
                    }
                    let notification =
                        crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                            session_epoch,
                            project,
                            project_incarnation,
                            source_revision,
                            generation,
                            collection_epoch,
                            call_id: call_id.0,
                            data: Vec::new(),
                            expand_error: Some(crate::bex_lsp::TestExpandError {
                                testset_name: name,
                                message: format!("Expanded tree could not be serialized: {error}"),
                            }),
                            collection_error: Some(
                                "The previous test tree was retained".to_string(),
                            ),
                        };
                    this.publish_test_collection_notification(
                        &project_root,
                        &project_handle,
                        publication_identity,
                        session_epoch,
                        notification,
                    );
                    return;
                }
            };
            if !project_handle.project.registry_lease_is_current(&lease) {
                if !was_expanded {
                    rollback_test_registry_expansion_or_discard(
                        &project_handle,
                        &lease,
                        &name,
                    )
                    .await;
                }
                return;
            }
            let notification = crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                session_epoch,
                project,
                project_incarnation,
                source_revision,
                generation,
                collection_epoch,
                call_id: call_id.0,
                data,
                expand_error: None,
                collection_error: None,
            };
            if !this.publish_test_collection_notification_to_all(
                &project_root,
                &project_handle,
                publication_identity,
                session_epoch,
                notification,
            ) && !was_expanded
            {
                rollback_test_registry_expansion_or_discard(
                    &project_handle,
                    &lease,
                    &name,
                )
                .await;
            }
        });
    }
}

async fn serialize_test_registry_for_wire(
    engine: &Arc<bex_engine::BexEngine>,
    registry: bex_external_types::Handle,
    cancel: sys_types::CancellationToken,
) -> Result<Vec<u8>, bex_engine::EngineError> {
    let ctx = bex_engine::FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_cancel_token(cancel)
        .with_profile_enabled(false)
        .build();
    let serialized = engine
        .call_function(
            "testing.TestRegistry.serialize",
            vec![bex_engine::BexExternalValue::Handle(registry)],
            ctx,
            true,
        )
        .await?;
    Ok(serde_json::to_vec(&bex_value_to_json(&serialized)).unwrap_or_default())
}

async fn test_registry_is_expanded(
    engine: &Arc<bex_engine::BexEngine>,
    registry: bex_external_types::Handle,
    name: &str,
    cancel: sys_types::CancellationToken,
) -> Result<bool, bex_engine::EngineError> {
    let ctx = bex_engine::FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_cancel_token(cancel)
        .with_profile_enabled(false)
        .build();
    match engine
        .call_function(
            "testing.TestRegistry.is_expanded",
            vec![
                bex_engine::BexExternalValue::Handle(registry),
                bex_engine::BexExternalValue::String(name.into()),
            ],
            ctx,
            true,
        )
        .await?
    {
        bex_engine::BexExternalValue::Bool(expanded) => Ok(expanded),
        _ => Err(bex_engine::EngineError::FunctionNotFound {
            name: "testing.TestRegistry.is_expanded returned a non-bool value".to_string(),
        }),
    }
}

async fn rollback_test_registry_expansion_or_discard(
    project: &Arc<LiveProject>,
    lease: &crate::project::RegistryLease,
    name: &str,
) -> bool {
    // Rollback is compensating cleanup, not derived work. A superseding
    // collection cancels `lease.cancel`, so cleanup must use a fresh token.
    let ctx = bex_engine::FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_cancel_token(sys_types::CancellationToken::new())
        .with_profile_enabled(false)
        .build();
    match lease
        .engine
        .call_function(
            "testing.TestRegistry.rollback_expand_set",
            vec![
                bex_engine::BexExternalValue::Handle(lease.registry.clone()),
                bex_engine::BexExternalValue::String(name.into()),
            ],
            ctx,
            true,
        )
        .await
    {
        Ok(_) => true,
        Err(error) => {
            log::error!(
                "[expand_test_set] failed to roll back testset {name}: {error}; discarding registry"
            );
            project.project.discard_registry_if_current(lease);
            false
        }
    }
}

/// Convert a `BexExternalValue` to a `serde_json::Value` for serialization.
///
/// Only handles the primitive/structural variants that appear in test reports.
/// Handles, ADTs, and function refs are serialized as null.
fn bex_value_to_json(v: &bex_engine::BexExternalValue) -> serde_json::Value {
    match v {
        bex_engine::BexExternalValue::Null => serde_json::Value::Null,
        bex_engine::BexExternalValue::Int(i) => serde_json::json!(i),
        // Bigints can exceed JSON number precision; emit as a decimal string.
        bex_engine::BexExternalValue::Bigint(b) => serde_json::json!(b.to_string()),
        bex_engine::BexExternalValue::Float(f) => serde_json::json!(f),
        bex_engine::BexExternalValue::Bool(b) => serde_json::json!(b),
        bex_engine::BexExternalValue::String(s) => serde_json::json!(s.as_str()),
        bex_engine::BexExternalValue::Array { items, .. } => {
            serde_json::Value::Array(items.iter().map(bex_value_to_json).collect())
        }
        bex_engine::BexExternalValue::Map { entries, .. } => {
            let mut map = serde_json::Map::new();
            for (k, v) in entries {
                map.insert(k.clone(), bex_value_to_json(v));
            }
            serde_json::Value::Object(map)
        }
        bex_engine::BexExternalValue::Instance {
            class_name, fields, ..
        } => {
            let mut map = serde_json::Map::new();
            map.insert("$type".to_string(), serde_json::json!(class_name));
            for (k, v) in fields {
                map.insert(k.clone(), bex_value_to_json(v));
            }
            serde_json::Value::Object(map)
        }
        bex_engine::BexExternalValue::Variant {
            enum_name,
            variant_name,
        } => {
            serde_json::json!({ "$enum": enum_name, "value": variant_name })
        }
        bex_engine::BexExternalValue::Union { value, .. } => bex_value_to_json(value),
        _ => serde_json::Value::Null,
    }
}

fn relative_source_path(project_root: &vfs::VfsPath, path: &crate::fs::FsPath) -> String {
    let root_path = std::path::Path::new(project_root.as_str());
    let path = path.as_path();
    if path == root_path {
        return path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
    }
    path.strip_prefix(root_path)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn canonical_path_identity(path: &std::path::Path) -> std::path::PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut normalized = std::path::PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    let mut existing = normalized.as_path();
    let mut missing = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name() else {
            break;
        };
        missing.push(name.to_os_string());
        let Some(parent) = existing.parent() else {
            break;
        };
        existing = parent;
    }
    let mut canonical = std::fs::canonicalize(existing).unwrap_or_else(|_| existing.to_path_buf());
    for component in missing.into_iter().rev() {
        canonical.push(component);
    }
    #[cfg(windows)]
    {
        canonical = std::path::PathBuf::from(canonical.to_string_lossy().to_lowercase());
    }
    canonical
}

fn canonical_fs_path_identity(path: &crate::fs::FsPath) -> crate::fs::FsPath {
    crate::fs::FsPath::from_str(
        canonical_path_identity(path.as_path())
            .to_string_lossy()
            .into_owned(),
    )
}

fn vfs_ancestors(start: vfs::VfsPath) -> impl Iterator<Item = vfs::VfsPath> {
    let mut ancestors = Vec::new();
    let mut current = start;
    loop {
        ancestors.push(current.clone());
        if current.is_root() {
            break;
        }
        let parent = current.parent();
        if parent.as_str() == current.as_str() {
            break;
        }
        current = parent;
    }
    ancestors.into_iter()
}

fn resolve_source_path_for_project(
    project_root: &vfs::VfsPath,
    path: &str,
) -> Result<vfs::VfsPath, LspError> {
    let raw = std::path::Path::new(path);
    if raw.is_absolute() {
        return Err(LspError::InvalidVFSPath {
            path: project_root.clone(),
            message: format!("Expected a project-relative source path, got {path}"),
        });
    }

    if raw
        .components()
        .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(LspError::InvalidVFSPath {
            path: project_root.clone(),
            message: format!("Unsafe relative source path: {path}"),
        });
    }

    if project_root.is_file().unwrap_or(false) {
        return Ok(project_root.clone());
    }

    project_root
        .join(path)
        .map_err(|e| LspError::InvalidVFSPath {
            path: project_root.clone(),
            message: format!("Failed to join path: {e}"),
        })
}

fn ensure_source_belongs_to_project(
    project_root: &vfs::VfsPath,
    source_path: &vfs::VfsPath,
) -> Result<(), LspError> {
    let expected_root;
    if project_root.is_file().unwrap_or(false) {
        if source_path.as_str() == project_root.as_str() {
            return Ok(());
        }
        expected_root = project_root.as_str().to_string();
    } else {
        let source_root = BexMulitProject::project_source_root(project_root)?;
        expected_root = source_root.as_str().to_string();
        let root = source_root.as_str().trim_end_matches('/');
        let source = source_path.as_str();
        if source == root
            || source
                .strip_prefix(root)
                .is_some_and(|suffix| suffix.starts_with('/'))
        {
            return Ok(());
        }
    }

    Err(LspError::InvalidVFSPath {
        path: source_path.clone(),
        message: format!("Source file is outside project source root {expected_root}"),
    })
}

impl BexMulitProject {
    fn request_control_flow_graph_current(
        &self,
        project_key: &crate::fs::FsPath,
        project: &Arc<LiveProject>,
        project_root: String,
        function_name: String,
        _terminal_demand: RuntimeDemandGuard,
    ) {
        let session_epoch = self.current_playground_session_epoch();
        if !self.playground_session_is_current_internal(session_epoch)
            || !self.project_incarnation_is_current(project_key, project)
        {
            return;
        }
        let Some(identity) = project.project.publication_identity() else {
            return;
        };
        let graph = {
            let Ok(source) = project.project.db.lock() else {
                return;
            };
            if source.source_revision() != identity.source_revision {
                return;
            }
            source.ast_control_flow_graph(&function_name).map(|graph| {
                baml_compiler2_visualization::control_flow::prepare_control_flow_graph_for_visualization(&graph)
            })
        };
        let graph_json = graph
            .as_ref()
            .and_then(|graph| serde_json::to_value(graph).ok());
        let notification = crate::bex_lsp::PlaygroundNotification::ControlFlowGraphResult {
            session_epoch,
            project: project_root,
            project_incarnation: project.incarnation,
            source_revision: identity.source_revision.0,
            generation: identity
                .engine_generation
                .expect("current runtime publication must have a generation"),
            derived_epoch: identity.derived_epoch,
            function_name,
            graph: graph_json,
        };
        if self.enqueue_project_playground_publication(
            project_key,
            project,
            identity,
            session_epoch,
            notification,
        ) {
            let _ = self.drain_project_publications(project_key, project);
        }
    }
}

#[async_trait::async_trait]
impl super::BexLsp for BexMulitProject {
    fn new_lsp_session(
        &self,
        sender: Arc<dyn LspClientSenderTrait + Send + Sync>,
    ) -> Arc<dyn super::BexLsp> {
        Arc::new(self.connection_scoped_lsp_session(sender))
    }

    fn get_bex_for_project(
        &self,
        project_root: &crate::fs::FsPath,
    ) -> Result<Arc<dyn crate::Bex>, crate::RuntimeError> {
        self.get_bex_for_project(project_root)
    }

    fn all_env_var_names(&self) -> Vec<String> {
        let projects = self.projects.lock().unwrap();
        let mut names = std::collections::BTreeSet::new();
        for project in projects.values() {
            let db_guard = project.project.db.lock().unwrap();
            let db = db_guard.db();
            for name in baml_lsp2_actions::all_env_var_names(db) {
                names.insert(name);
            }
        }
        names.into_iter().collect()
    }

    fn playground_source_files(
        &self,
        project: &str,
    ) -> Result<Vec<crate::bex_lsp::PlaygroundSourceFile>, LspError> {
        let project_root = self
            .fs
            .get_path_from_path(std::path::Path::new(project), "playground source files")?;
        self.validate_owned_path(&project_root)?;
        let project_handle = self.get_or_create_project(project_root.clone())?;
        let mut sources = self.load_project_sources(&project_root)?;
        for (path, source) in project_handle.project.open_document_sources() {
            sources.insert(path, source);
        }

        let mut files = sources
            .into_iter()
            .map(|(path, content)| {
                let relative_path = relative_source_path(&project_root, &path);
                crate::bex_lsp::PlaygroundSourceFile {
                    path: path.as_path().to_string_lossy().into_owned(),
                    relative_path,
                    content,
                }
            })
            .collect::<Vec<_>>();
        files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        Ok(files)
    }

    fn playground_update_source_file(
        &self,
        project: &str,
        path: &str,
        content: String,
    ) -> Result<(), LspError> {
        let project_root = self.fs.get_path_from_path(
            std::path::Path::new(project),
            "playground update source file",
        )?;
        self.validate_owned_path(&project_root)?;
        let raw_path = std::path::Path::new(path);
        let source_path = if raw_path.is_absolute() {
            self.fs
                .get_path_from_path(raw_path, "playground update source file path")?
        } else {
            resolve_source_path_for_project(&project_root, path)?
        };
        if source_path.extension().is_none_or(|e| e.as_str() != "baml") {
            return Err(LspError::InvalidVFSPath {
                path: source_path,
                message: "Only .baml files can be edited from the playground".to_string(),
            });
        }
        ensure_source_belongs_to_project(&project_root, &source_path)?;

        let project_handle = self.get_or_create_project(project_root.clone())?;
        let source_path = crate::fs::FsPath::from_vfs(&source_path);
        project_handle
            .project
            .apply_playground_source(&source_path, &content)?;

        self.refresh_project(
            &project_root,
            ProjectRefreshMode::Applied {
                full_diagnostic_refresh: false,
            },
        );
        Ok(())
    }

    fn initialize_workspace_roots(
        &self,
        roots: Vec<std::path::PathBuf>,
    ) -> Result<Vec<String>, LspError> {
        let roots = roots
            .into_iter()
            .map(|root| self.fs.get_path_from_path(&root, "lsp --workspace"))
            .collect::<Result<Vec<_>, _>>()?;
        let projects = self.discover_workspace_projects(&roots);
        Ok(projects
            .into_iter()
            .map(|project| project.as_str().to_string())
            .collect())
    }

    fn request_playground_state(&self) {
        self.send_list_projects(true);
        let projects: Vec<_> = self
            .projects
            .lock()
            .unwrap()
            .iter()
            .map(|(path, project)| (path.clone(), project.clone()))
            .collect();
        for (fs_path, project) in projects {
            if !self.playground_sender.has_runtime_subscribers()
                || project
                    .runtime_demand
                    .load(std::sync::atomic::Ordering::Acquire)
                    == 0
            {
                continue;
            }
            let (source_revision, diags_by_file) =
                match project.project.diagnostics_by_file(PositionEncoding::Utf16) {
                    DiagnosticRead::Ready(candidate) => {
                        (candidate.source_revision, candidate.documents)
                    }
                    DiagnosticRead::Busy | DiagnosticRead::Poisoned => continue,
                };
            let flat_diags = Self::flatten_diagnostics(&diags_by_file);
            let Ok(root) = self
                .fs
                .get_path_from_str(&fs_path, "request playground state")
            else {
                continue;
            };
            for session_epoch in self.playground_session_targets() {
                self.send_update_project(
                    &root,
                    &project,
                    source_revision,
                    session_epoch,
                    flat_diags.clone(),
                    true,
                );
            }
        }
    }

    fn runtime_inputs_changed(&self) {
        // Environment/configuration inputs are process-wide. Fan the state
        // transition and any demanded rebuild to every active playground
        // session even when the mutation originated from one bound endpoint.
        let mut process = self.clone();
        process.playground_session_context = None;
        let projects = process
            .projects
            .lock()
            .map(|projects| {
                projects
                    .iter()
                    .map(|(path, project)| (path.clone(), project.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let mut builds = Vec::new();
        for (project_key, project) in projects {
            project.project.advance_runtime_inputs();
            if project
                .runtime_demand
                .load(std::sync::atomic::Ordering::Acquire)
                == 0
                || !project.project.request_engine_build()
            {
                continue;
            }
            let Ok(project_root) = process
                .fs
                .get_path_from_str(&project_key, "runtime input change")
            else {
                continue;
            };
            builds.push((project_root, project));
        }

        // Publish `building`/`idleStale` before a native build can take the DB
        // lane. Terminal continuations publish the eventual ready/blocked/
        // failed state.
        process.request_playground_state();
        for (project_root, project) in builds {
            process.spawn_engine_build_and_publish(project_root, project);
        }
    }

    fn begin_playground_session(&self) -> u64 {
        let session_epoch = self
            .playground_session_epoch
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            .wrapping_add(1);
        if let Ok(mut sessions) = self.active_playground_sessions.lock() {
            sessions.insert(session_epoch);
        }
        session_epoch
    }

    fn bind_playground_session(&self, session_epoch: u64) -> Arc<dyn super::BexLsp> {
        let mut session = self.clone();
        session.playground_session_context = Some(session_epoch);
        Arc::new(session)
    }

    fn end_playground_session(&self, session_epoch: u64) {
        if let Ok(mut sessions) = self.active_playground_sessions.lock() {
            sessions.remove(&session_epoch);
        }
        if let Ok(projects) = self.projects.lock() {
            for project in projects.values() {
                if let Ok(mut updates) = project.last_project_update.lock() {
                    updates.remove(&session_epoch);
                }
            }
        }
    }

    fn playground_session_is_current(&self, session_epoch: u64) -> bool {
        self.playground_session_is_current_internal(session_epoch)
    }

    async fn ensure_project_runtime(
        &self,
        project_root: &str,
        expected_incarnation: Option<u64>,
    ) -> Result<crate::bex_lsp::ProjectRuntimeStatus, LspError> {
        let (project_key, project) = self
            .project_entry_for_root_str(project_root)
            .ok_or_else(|| LspError::RequestFailed(format!("Project not found: {project_root}")))?;
        if expected_incarnation != Some(project.incarnation) {
            return Err(LspError::RequestFailed(format!(
                "Project incarnation is missing or stale; current incarnation is {}",
                project.incarnation
            )));
        }
        let project_root = self
            .fs
            .get_path_from_str(&project_key, "ensure project runtime")?;
        project
            .runtime_demand
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let _ = project.project.request_engine_build();
        // Every lease acquisition joins the shared flight and owns a terminal
        // publication continuation, even when another caller started it.
        self.spawn_engine_build_and_publish(project_root, project.clone());
        Ok(Self::runtime_status_wire(&project))
    }

    async fn retry_project_runtime(
        &self,
        project_root: &str,
        expected_incarnation: Option<u64>,
    ) -> Result<crate::bex_lsp::ProjectRuntimeStatus, LspError> {
        let (project_key, project) = self
            .project_entry_for_root_str(project_root)
            .ok_or_else(|| LspError::RequestFailed(format!("Project not found: {project_root}")))?;
        if expected_incarnation != Some(project.incarnation) {
            return Err(LspError::RequestFailed(format!(
                "Project incarnation is missing or stale; current incarnation is {}",
                project.incarnation
            )));
        }
        if project
            .runtime_demand
            .load(std::sync::atomic::Ordering::Acquire)
            == 0
        {
            return Err(LspError::RequestFailed(
                "Cannot retry a project without an active runtime-demand lease".to_string(),
            ));
        }
        let project_root = self
            .fs
            .get_path_from_str(&project_key, "retry project runtime")?;
        let _ = project.project.request_engine_retry();
        self.spawn_engine_build_and_publish(project_root, project.clone());
        Ok(Self::runtime_status_wire(&project))
    }

    fn release_project_runtime(&self, project_root: &str, expected_incarnation: Option<u64>) {
        if let Some(project) = self.project_for_root_str(project_root)
            && expected_incarnation == Some(project.incarnation)
        {
            Self::release_runtime_demand(&project);
        }
    }

    fn project_incarnation(&self, project_root: &str) -> Option<u64> {
        self.project_for_root_str(project_root)
            .map(|project| project.incarnation)
    }

    async fn prepare_function_run(
        &self,
        project_root: &str,
        call_id: sys_types::CallId,
        function_name: &str,
    ) -> Result<crate::bex_lsp::PreparedFunctionRun, LspError> {
        let project = self
            .project_for_root_str(project_root)
            .ok_or_else(|| LspError::RequestFailed(format!("Project not found: {project_root}")))?;
        project
            .runtime_demand
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let lease = loop {
            let receipt = match Self::ensure_project_engine(project.clone()).await {
                Ok(receipt) => receipt,
                Err(error) => {
                    Self::release_runtime_demand(&project);
                    return Err(error);
                }
            };
            match project.project.prepare_and_register_run(
                call_id,
                Some(function_name),
                None,
                false,
            ) {
                Ok(lease) => break lease,
                Err(_) if project.project.source_revision() != receipt.source_revision => continue,
                Err(error) => {
                    Self::release_runtime_demand(&project);
                    return Err(LspError::Runtime(error));
                }
            }
        };
        if self
            .active_runs
            .lock()
            .map(|mut runs| runs.insert(call_id, (project_root.to_string(), project.clone())))
            .is_err()
        {
            project.project.finish_run(call_id);
            Self::release_runtime_demand(&project);
            return Err(LspError::InternalError(
                "Global active-run registry is poisoned".to_string(),
            ));
        }
        let engine: Arc<dyn crate::Bex> = lease.engine;
        Ok(crate::bex_lsp::PreparedFunctionRun {
            source_revision: lease.source_revision.0,
            generation: lease.generation,
            engine,
        })
    }

    async fn prepare_test_run(
        &self,
        project_root: &str,
        call_id: sys_types::CallId,
        generation: u64,
    ) -> Result<crate::bex_lsp::PreparedTestRun, LspError> {
        let project = self
            .project_for_root_str(project_root)
            .ok_or_else(|| LspError::RequestFailed(format!("Project not found: {project_root}")))?;
        project
            .runtime_demand
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let lease = loop {
            let receipt = match Self::ensure_project_engine(project.clone()).await {
                Ok(receipt) => receipt,
                Err(error) => {
                    Self::release_runtime_demand(&project);
                    return Err(error);
                }
            };
            match project
                .project
                .prepare_and_register_run(call_id, None, Some(generation), true)
            {
                Ok(lease) => break lease,
                Err(_) if project.project.source_revision() != receipt.source_revision => continue,
                Err(error) => {
                    Self::release_runtime_demand(&project);
                    return Err(LspError::Runtime(error));
                }
            }
        };
        if self
            .active_runs
            .lock()
            .map(|mut runs| runs.insert(call_id, (project_root.to_string(), project.clone())))
            .is_err()
        {
            project.project.finish_run(call_id);
            Self::release_runtime_demand(&project);
            return Err(LspError::InternalError(
                "Global active-run registry is poisoned".to_string(),
            ));
        }
        Ok(crate::bex_lsp::PreparedTestRun {
            source_revision: lease.source_revision.0,
            generation: lease.generation,
        })
    }

    fn finish_project_run(&self, _project_root: &str, call_id: sys_types::CallId) {
        let project = self
            .active_runs
            .lock()
            .ok()
            .and_then(|mut runs| runs.remove(&call_id).map(|(_, project)| project));
        if let Some(project) = project
            && project.project.finish_run(call_id)
        {
            Self::release_runtime_demand(&project);
        }
    }

    fn cancel_project_run(
        &self,
        _project_root: &str,
        call_id: sys_types::CallId,
    ) -> Result<(), RuntimeError> {
        let project = self
            .active_runs
            .lock()
            .ok()
            .and_then(|runs| runs.get(&call_id).map(|(_, project)| project.clone()))
            .ok_or_else(|| RuntimeError::Compilation {
                message: format!("Active run {call_id} not found"),
            })?;
        project
            .project
            .cancel_run(call_id)
            .map_err(RuntimeError::from)
    }

    fn ast_control_flow_graph(
        &self,
        function_name: &str,
    ) -> Option<baml_compiler2_visualization::control_flow::ControlFlowGraph> {
        let projects = self.projects.lock().ok()?;
        for project in projects.values() {
            let db = project.project.db.lock().ok()?;
            if let Some(graph) = db.ast_control_flow_graph(function_name) {
                return Some(graph);
            }
        }
        None
    }

    fn project_generation(&self, project_root: &str) -> Option<u64> {
        let projects = self.projects.lock().ok()?;
        projects
            .iter()
            .find(|(path, _)| path.as_path().to_string_lossy() == project_root)
            .map(|(_, project)| project.project.current_generation())
    }

    fn control_flow_graph_for_generation(
        &self,
        project_root: &str,
        generation: u64,
        function_name: &str,
        call_id: Option<sys_types::CallId>,
    ) -> Option<std::sync::Arc<baml_compiler2_visualization::control_flow::ControlFlowGraph>> {
        if let Some(call_id) = call_id {
            let (registered_root, project) =
                self.active_runs.lock().ok()?.get(&call_id).cloned()?;
            if registered_root != project_root {
                return None;
            }
            return project
                .project
                .control_flow_graph_for_generation(generation, function_name);
        }

        // Clone the project handle out of the registry lock: building a
        // missing graph takes the project's database lock, which must not be
        // held while the registry lock is.
        let current_project = {
            let projects = self.projects.lock().ok()?;
            projects
                .iter()
                .find(|(path, _)| path.as_path().to_string_lossy() == project_root)
                .map(|(_, project)| project.clone())
        };
        current_project.and_then(|project| {
            project
                .project
                .control_flow_graph_for_generation(generation, function_name)
        })
    }

    fn request_control_flow_graph(&self, project_root: &str, function_name: &str) {
        let Some((project_key, project)) = self.project_entry_for_root_str(project_root) else {
            return;
        };
        let demand = RuntimeDemandGuard::acquire(project.clone());
        let this = self.clone();
        let project_root = project_root.to_string();
        let function_name = function_name.to_string();
        self.spawner.spawn(async move {
            if Self::ensure_project_engine(project.clone()).await.is_err()
                || !this.project_incarnation_is_current(&project_key, &project)
            {
                return;
            }
            this.request_control_flow_graph_current(
                &project_key,
                &project,
                project_root,
                function_name,
                demand,
            );
        });
    }

    fn playground_cursor_context(
        &self,
        file_path: &str,
        line: u32,
        column: u32,
    ) -> baml_project::CursorContext {
        let empty = baml_project::CursorContext {
            function_name: None,
            is_workflow: false,
            workflow_memberships: vec![],
            source_expr_id: None,
            source_expr_candidates: vec![],
            source_expr_function_name: None,
            test_name: None,
            cursor_offset: None,
        };

        let Ok(projects) = self.projects.lock() else {
            return empty;
        };

        for project in projects.values() {
            let Ok(db) = project.project.db.lock() else {
                continue;
            };

            // Convert line/column to byte offset using the source file text.
            // The file_path from Monaco may be relative — find matching file.
            let Some(source_file) = db.find_source_file(file_path) else {
                continue;
            };

            let text: &str = source_file.text(&**db);
            let codec =
                baml_project::position::LspPositionCodec::new(text, PositionEncoding::Utf16);
            let Ok(byte_offset) = codec.position_to_offset(lsp_types::Position::new(line, column))
            else {
                return empty;
            };

            return db.playground_cursor_context(file_path, byte_offset);
        }

        empty
    }

    fn request_cursor_context(&self, file_path: &str, line: u32, column: u32) {
        let ctx = self.playground_cursor_context(file_path, line, column);
        let ctx_json = serde_json::to_value(&ctx).unwrap_or(serde_json::Value::Null);
        self.playground_sender.send_playground_notification(
            crate::bex_lsp::PlaygroundNotification::CursorContext { context: ctx_json },
        );
    }

    fn request_collect_tests(&self, project: &str) {
        self.request_collect_tests_with_temporary_demand(
            project,
            self.current_playground_session_epoch(),
        );
    }

    async fn call_test_function(
        &self,
        project: &str,
        generation: u64,
        test_name: &str,
        call_id: sys_types::CallId,
        ctx: bex_engine::FunctionCallContext,
    ) -> Result<bex_engine::BexExternalValue, bex_engine::EngineError> {
        self.call_test_function_impl(project, generation, test_name, call_id, ctx)
            .await
            .and_then(|result| result.value)
    }

    async fn call_test_function_with_trace(
        &self,
        project: &str,
        generation: u64,
        test_name: &str,
        call_id: sys_types::CallId,
        ctx: bex_engine::FunctionCallContext,
    ) -> Result<bex_engine::BexCallResult, bex_engine::EngineError> {
        self.call_test_function_impl(project, generation, test_name, call_id, ctx)
            .await
    }

    fn expand_test_set(&self, project: &str, generation: u64, testset_name: &str) {
        self.expand_test_set_impl(project, generation, testset_name);
    }

    fn resolve_file_id(&self, file_id: u32) -> Option<String> {
        let projects = self.projects.lock().unwrap();
        for project in projects.values() {
            let db = project.project.db.lock().unwrap();
            if let Some(path) = db.file_id_to_path(baml_base::FileId::new(file_id)) {
                return Some(path.to_string_lossy().to_string());
            }
        }
        None
    }
}

pub fn new_lsp(
    sys_op_factory: SysOpFactory,
    sender: std::sync::Arc<dyn LspClientSenderTrait + Send + Sync>,
    playground_sender: std::sync::Arc<dyn crate::bex_lsp::PlaygroundSender>,
    fs: crate::fs::BamlVFS,
    spawner: BackgroundSpawner,
) -> impl crate::bex_lsp::BexLsp {
    BexMulitProject::new(sys_op_factory, sender, playground_sender, fs, spawner)
}

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;

    struct NoopLspSender;

    impl LspClientSenderTrait for NoopLspSender {
        fn send_notification(&self, _msg: lsp_server::Notification) -> Result<(), LspError> {
            Ok(())
        }

        fn send_response_impl(&self, _msg: lsp_server::Response) -> Result<(), LspError> {
            Ok(())
        }

        fn make_request(&self, _msg: lsp_server::Request) -> Result<(), LspError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct CapturingLspSender {
        messages: std::sync::Mutex<Vec<lsp_server::Message>>,
    }

    impl LspClientSenderTrait for CapturingLspSender {
        fn send_notification(&self, msg: lsp_server::Notification) -> Result<(), LspError> {
            self.messages
                .lock()
                .unwrap()
                .push(lsp_server::Message::Notification(msg));
            Ok(())
        }

        fn send_response_impl(&self, msg: lsp_server::Response) -> Result<(), LspError> {
            self.messages
                .lock()
                .unwrap()
                .push(lsp_server::Message::Response(msg));
            Ok(())
        }

        fn make_request(&self, msg: lsp_server::Request) -> Result<(), LspError> {
            self.messages
                .lock()
                .unwrap()
                .push(lsp_server::Message::Request(msg));
            Ok(())
        }
    }

    #[derive(Default)]
    struct GatedCatalogSender {
        allow_catalog: std::sync::atomic::AtomicBool,
        catalog_attempts: std::sync::atomic::AtomicUsize,
        messages: std::sync::Mutex<Vec<lsp_server::Message>>,
    }

    impl LspClientSenderTrait for GatedCatalogSender {
        fn send_notification(&self, msg: lsp_server::Notification) -> Result<(), LspError> {
            if msg.method == "baml/listProjects" {
                self.catalog_attempts
                    .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                if !self
                    .allow_catalog
                    .load(std::sync::atomic::Ordering::Acquire)
                {
                    return Err(LspError::OutboundSaturated);
                }
            }
            self.messages
                .lock()
                .unwrap()
                .push(lsp_server::Message::Notification(msg));
            Ok(())
        }

        fn send_response_impl(&self, msg: lsp_server::Response) -> Result<(), LspError> {
            self.messages
                .lock()
                .unwrap()
                .push(lsp_server::Message::Response(msg));
            Ok(())
        }

        fn make_request(&self, msg: lsp_server::Request) -> Result<(), LspError> {
            self.messages
                .lock()
                .unwrap()
                .push(lsp_server::Message::Request(msg));
            Ok(())
        }
    }

    struct FailingOpenPanelSender;

    impl LspClientSenderTrait for FailingOpenPanelSender {
        fn send_notification(&self, msg: lsp_server::Notification) -> Result<(), LspError> {
            if msg.method == "baml/openPlayground" {
                Err(LspError::OutboundSaturated)
            } else {
                Ok(())
            }
        }

        fn send_response_impl(&self, _msg: lsp_server::Response) -> Result<(), LspError> {
            Ok(())
        }

        fn make_request(&self, _msg: lsp_server::Request) -> Result<(), LspError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct SaturateSecondDiagnosticSender {
        diagnostic_attempts: std::sync::atomic::AtomicUsize,
        messages: std::sync::Mutex<Vec<lsp_server::Message>>,
    }

    impl LspClientSenderTrait for SaturateSecondDiagnosticSender {
        fn send_notification(&self, msg: lsp_server::Notification) -> Result<(), LspError> {
            if msg.method == "textDocument/publishDiagnostics" {
                let attempt = self
                    .diagnostic_attempts
                    .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                if attempt == 1 {
                    return Err(LspError::RequestFailed(
                        "test writer is temporarily saturated".to_string(),
                    ));
                }
            }
            self.messages
                .lock()
                .unwrap()
                .push(lsp_server::Message::Notification(msg));
            Ok(())
        }

        fn send_response_impl(&self, msg: lsp_server::Response) -> Result<(), LspError> {
            self.messages
                .lock()
                .unwrap()
                .push(lsp_server::Message::Response(msg));
            Ok(())
        }

        fn make_request(&self, msg: lsp_server::Request) -> Result<(), LspError> {
            self.messages
                .lock()
                .unwrap()
                .push(lsp_server::Message::Request(msg));
            Ok(())
        }
    }

    #[derive(Default)]
    struct BlockingDiagnosticSender {
        entered: std::sync::atomic::AtomicBool,
        released: std::sync::Mutex<bool>,
        released_cv: std::sync::Condvar,
    }

    impl BlockingDiagnosticSender {
        fn wait_until_entered(&self) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while !self.entered.load(std::sync::atomic::Ordering::Acquire) {
                assert!(
                    std::time::Instant::now() < deadline,
                    "diagnostic sender was never entered"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }

        fn release(&self) {
            *self.released.lock().unwrap() = true;
            self.released_cv.notify_all();
        }
    }

    impl LspClientSenderTrait for BlockingDiagnosticSender {
        fn send_notification(&self, msg: lsp_server::Notification) -> Result<(), LspError> {
            if msg.method == "textDocument/publishDiagnostics" {
                self.entered
                    .store(true, std::sync::atomic::Ordering::Release);
                let mut released = self.released.lock().unwrap();
                while !*released {
                    released = self.released_cv.wait(released).unwrap();
                }
            }
            Ok(())
        }

        fn send_response_impl(&self, _msg: lsp_server::Response) -> Result<(), LspError> {
            Ok(())
        }

        fn make_request(&self, _msg: lsp_server::Request) -> Result<(), LspError> {
            Ok(())
        }
    }

    struct TimeGatedDiagnosticSender {
        next_ready: std::sync::Mutex<std::time::Instant>,
        messages: std::sync::Mutex<Vec<lsp_server::Message>>,
    }

    impl Default for TimeGatedDiagnosticSender {
        fn default() -> Self {
            Self {
                next_ready: std::sync::Mutex::new(std::time::Instant::now()),
                messages: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl LspClientSenderTrait for TimeGatedDiagnosticSender {
        fn send_notification(&self, msg: lsp_server::Notification) -> Result<(), LspError> {
            if msg.method == "textDocument/publishDiagnostics" {
                let now = std::time::Instant::now();
                let mut next_ready = self.next_ready.lock().unwrap();
                if now < *next_ready {
                    return Err(LspError::OutboundSaturated);
                }
                *next_ready = now + std::time::Duration::from_millis(25);
            }
            self.messages
                .lock()
                .unwrap()
                .push(lsp_server::Message::Notification(msg));
            Ok(())
        }

        fn send_response_impl(&self, _msg: lsp_server::Response) -> Result<(), LspError> {
            Ok(())
        }

        fn make_request(&self, _msg: lsp_server::Request) -> Result<(), LspError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct HeldTombstoneSender {
        allow_tombstones: std::sync::atomic::AtomicBool,
        messages: std::sync::Mutex<Vec<lsp_server::Message>>,
    }

    impl LspClientSenderTrait for HeldTombstoneSender {
        fn send_notification(&self, msg: lsp_server::Notification) -> Result<(), LspError> {
            if msg.method == "textDocument/publishDiagnostics"
                && serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(msg.params.clone())
                    .is_ok_and(|params| params.diagnostics.is_empty())
                && !self
                    .allow_tombstones
                    .load(std::sync::atomic::Ordering::Acquire)
            {
                return Err(LspError::OutboundSaturated);
            }
            self.messages
                .lock()
                .unwrap()
                .push(lsp_server::Message::Notification(msg));
            Ok(())
        }

        fn send_response_impl(&self, _msg: lsp_server::Response) -> Result<(), LspError> {
            Ok(())
        }

        fn make_request(&self, _msg: lsp_server::Request) -> Result<(), LspError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct SlowRetirementSender {
        closed: std::sync::atomic::AtomicBool,
    }

    impl LspClientSenderTrait for SlowRetirementSender {
        fn send_notification(&self, msg: lsp_server::Notification) -> Result<(), LspError> {
            if msg.method == "textDocument/publishDiagnostics" {
                return Err(LspError::OutboundSaturated);
            }
            Ok(())
        }

        fn send_response_impl(&self, _msg: lsp_server::Response) -> Result<(), LspError> {
            Ok(())
        }

        fn make_request(&self, _msg: lsp_server::Request) -> Result<(), LspError> {
            Ok(())
        }

        fn is_closed(&self) -> bool {
            self.closed.load(std::sync::atomic::Ordering::Acquire)
        }

        fn close_on_overload(&self) {
            self.closed
                .store(true, std::sync::atomic::Ordering::Release);
        }
    }

    #[derive(Default)]
    struct CapturingPlaygroundSender {
        notifications: std::sync::Mutex<Vec<serde_json::Value>>,
    }

    impl crate::bex_lsp::PlaygroundSender for CapturingPlaygroundSender {
        fn send_playground_notification(
            &self,
            notification: crate::bex_lsp::PlaygroundNotification,
        ) {
            self.notifications
                .lock()
                .unwrap()
                .push(serde_json::to_value(notification).unwrap());
        }
    }

    #[derive(Default)]
    struct PortPlaygroundSender {
        notifications: std::sync::Mutex<Vec<serde_json::Value>>,
    }

    impl crate::bex_lsp::PlaygroundSender for PortPlaygroundSender {
        fn send_playground_notification(
            &self,
            notification: crate::bex_lsp::PlaygroundNotification,
        ) {
            self.notifications
                .lock()
                .unwrap()
                .push(serde_json::to_value(notification).unwrap());
        }

        fn lsp_playground_port(&self) -> Option<u16> {
            Some(3030)
        }
    }

    fn test_multi_project(playground_sender: Arc<CapturingPlaygroundSender>) -> BexMulitProject {
        let fs = crate::fs::BamlVFS::new(Arc::new(Box::new(vfs::PhysicalFS::new("/"))));
        BexMulitProject::new(
            Arc::new(|_| Arc::new(sys_ops::SysOpsBuilder::new().build())),
            Arc::new(NoopLspSender),
            playground_sender,
            fs,
            BackgroundSpawner::new(),
        )
    }

    impl crate::fs::BulkReadFileSystem for vfs::PhysicalFS {
        fn read_many(&self, _glob: &str) -> vfs::VfsResult<Vec<(String, Vec<u8>)>> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn lsp_session_clones_bind_only_their_connection_sender() {
        let multi = test_multi_project(Arc::new(CapturingPlaygroundSender::default()));
        let first_sender = Arc::new(CapturingLspSender::default());
        let second_sender = Arc::new(CapturingLspSender::default());
        let first = <BexMulitProject as crate::bex_lsp::BexLsp>::new_lsp_session(
            &multi,
            first_sender.clone(),
        );
        let second = <BexMulitProject as crate::bex_lsp::BexLsp>::new_lsp_session(
            &multi,
            second_sender.clone(),
        );

        (first.notification_sender())(lsp_server::Notification::new(
            "test/first".to_string(),
            serde_json::Value::Null,
        ))
        .unwrap();
        (second.notification_sender())(lsp_server::Notification::new(
            "test/second".to_string(),
            serde_json::Value::Null,
        ))
        .unwrap();

        let first_messages = first_sender.messages.lock().unwrap();
        let second_messages = second_sender.messages.lock().unwrap();
        assert_eq!(first_messages.len(), 1);
        assert_eq!(second_messages.len(), 1);
        assert!(matches!(
            &first_messages[0],
            lsp_server::Message::Notification(notification)
                if notification.method == "test/first"
        ));
        assert!(matches!(
            &second_messages[0],
            lsp_server::Message::Notification(notification)
                if notification.method == "test/second"
        ));
    }

    #[test]
    fn root_catalog_fans_out_and_retries_saturation_before_acknowledging() {
        let multi = test_multi_project(Arc::new(CapturingPlaygroundSender::default()));
        let gated_sender = Arc::new(GatedCatalogSender::default());
        let other_sender = Arc::new(CapturingLspSender::default());
        let gated_session = multi.connection_scoped_lsp_session(gated_sender.clone());
        let other_session = multi.connection_scoped_lsp_session(other_sender.clone());
        gated_session
            .session_config
            .set(SessionConfig {
                position_encoding: PositionEncoding::Utf16,
            })
            .unwrap();
        other_session
            .session_config
            .set(SessionConfig {
                position_encoding: PositionEncoding::Utf16,
            })
            .unwrap();

        // The process/root dispatcher is intentionally uninitialized. Its
        // catalog must still fan out through the registered session outputs.
        multi.send_list_projects(false);

        {
            let delivery = gated_session.catalog_delivery.lock().unwrap();
            assert!(delivery.acknowledged.is_none());
            assert_eq!(delivery.pending.as_deref(), Some([].as_slice()));
            assert!(delivery.retry_scheduled);
        }
        assert!(other_sender.messages.lock().unwrap().iter().any(|message| {
            matches!(message, lsp_server::Message::Notification(notification)
                if notification.method == "baml/listProjects")
        }));

        let newest = vec![crate::bex_lsp::ProjectCatalogEntry {
            project: "/workspace/project".to_string(),
            incarnation: 7,
            source_revision: 11,
        }];
        multi.enqueue_lsp_catalog(
            gated_session.lsp_output_context.clone().unwrap(),
            newest.clone(),
            false,
        );
        gated_sender
            .allow_catalog
            .store(true, std::sync::atomic::Ordering::Release);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !gated_sender.messages.lock().unwrap().iter().any(|message| {
            matches!(message, lsp_server::Message::Notification(notification)
                if notification.method == "baml/listProjects")
        }) {
            assert!(
                std::time::Instant::now() < deadline,
                "catalog saturation retry never reached the writer"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let attempts_after_success = gated_sender
            .catalog_attempts
            .load(std::sync::atomic::Ordering::Acquire);
        assert!(attempts_after_success >= 2);
        {
            let delivery = gated_session.catalog_delivery.lock().unwrap();
            assert_eq!(delivery.acknowledged.as_ref(), Some(&newest));
            assert!(delivery.pending.is_none());
        }
        let delivered_catalog = gated_sender
            .messages
            .lock()
            .unwrap()
            .iter()
            .find_map(|message| match message {
                lsp_server::Message::Notification(notification)
                    if notification.method == "baml/listProjects" =>
                {
                    Some(notification.params.clone())
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(
            delivered_catalog,
            serde_json::json!({ "projects": ["/workspace/project"] })
        );

        multi.enqueue_lsp_catalog(gated_session.lsp_output_context.unwrap(), newest, false);
        assert_eq!(
            gated_sender
                .catalog_attempts
                .load(std::sync::atomic::Ordering::Acquire),
            attempts_after_success,
            "routine dedupe must use only a successfully enqueued catalog"
        );
    }

    #[test]
    fn lsp_session_clone_resets_connection_state_and_preserves_process_state() {
        let multi = test_multi_project(Arc::new(CapturingPlaygroundSender::default()));
        multi
            .session_config
            .set(SessionConfig {
                position_encoding: PositionEncoding::Utf8,
            })
            .unwrap();
        multi.workspace_roots.lock().unwrap().push(
            multi
                .fs
                .get_path_from_path(std::path::Path::new("/workspace"), "test root")
                .unwrap(),
        );
        *multi.last_catalog.lock().unwrap() = Some(Vec::new());

        let session = multi.connection_scoped_lsp_session(Arc::new(CapturingLspSender::default()));

        assert!(Arc::ptr_eq(&multi.projects, &session.projects));
        assert!(Arc::ptr_eq(
            &multi.project_incarnations,
            &session.project_incarnations
        ));
        assert!(Arc::ptr_eq(
            &multi.active_playground_sessions,
            &session.active_playground_sessions
        ));
        assert!(Arc::ptr_eq(&multi.active_runs, &session.active_runs));
        assert!(session.session_config.get().is_none());
        assert!(session.workspace_roots.lock().unwrap().is_empty());
        assert!(session.last_catalog.lock().unwrap().is_none());
        assert!(!Arc::ptr_eq(&multi.session_config, &session.session_config));
        assert!(!Arc::ptr_eq(
            &multi.workspace_roots,
            &session.workspace_roots
        ));
        assert!(!Arc::ptr_eq(&multi.last_catalog, &session.last_catalog));
        assert!(!Arc::ptr_eq(
            &multi.catalog_delivery,
            &session.catalog_delivery
        ));
        assert!(!Arc::ptr_eq(
            &multi.retired_diagnostics,
            &session.retired_diagnostics
        ));
    }

    struct TempWorkspace {
        root: std::path::PathBuf,
    }

    impl TempWorkspace {
        fn new(tag: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("bex_discovery_{}_{}", tag, std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn file(&self, rel: &str, contents: &str) {
            let path = self.root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }

        fn dir(&self, rel: &str) {
            std::fs::create_dir_all(self.root.join(rel)).unwrap();
        }

        fn vfs_path(&self, rel: &str) -> vfs::VfsPath {
            let abs = self.root.join(rel);
            crate::fs::BamlVFS::new(std::sync::Arc::new(Box::new(vfs::PhysicalFS::new("/"))))
                .get_path_from_path(&abs, "test workspace path")
                .unwrap()
        }

        fn root_vfs_path(&self) -> vfs::VfsPath {
            crate::fs::BamlVFS::new(std::sync::Arc::new(Box::new(vfs::PhysicalFS::new("/"))))
                .get_path_from_path(&self.root, "test workspace root")
                .unwrap()
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn multi_with_senders(
        lsp_sender: Arc<dyn LspClientSenderTrait + Send + Sync>,
        playground_sender: Arc<dyn crate::bex_lsp::PlaygroundSender>,
    ) -> BexMulitProject {
        let fs = crate::fs::BamlVFS::new(Arc::new(Box::new(vfs::PhysicalFS::new("/"))));
        BexMulitProject::new(
            Arc::new(|_| Arc::new(sys_ops::SysOpsBuilder::new().build())),
            lsp_sender,
            playground_sender,
            fs,
            BackgroundSpawner::new(),
        )
    }

    fn seed_published_diagnostic(
        session: &BexMulitProject,
        project_root: &vfs::VfsPath,
        project: &Arc<LiveProject>,
        path: crate::fs::FsPath,
        uri: lsp_types::Url,
    ) {
        session
            .published_diagnostics
            .lock()
            .unwrap()
            .entry(PublishedProjectKey {
                project_root: crate::fs::FsPath::from_vfs(project_root),
                incarnation: project.incarnation,
            })
            .or_default()
            .insert(path, uri);
    }

    fn initialize_params(workspace: &TempWorkspace) -> lsp_types::InitializeParams {
        lsp_types::InitializeParams {
            workspace_folders: Some(vec![lsp_types::WorkspaceFolder {
                uri: lsp_types::Url::from_directory_path(&workspace.root).unwrap(),
                name: workspace.root.display().to_string(),
            }]),
            ..lsp_types::InitializeParams::default()
        }
    }

    #[test]
    fn lsp_session_config_and_workspace_roots_are_connection_scoped() {
        let first_workspace = TempWorkspace::new("session_roots_first");
        first_workspace.file("proj/baml_src/main.baml", "function first() -> int { 1 }");
        let second_workspace = TempWorkspace::new("session_roots_second");
        second_workspace.file("proj/baml_src/main.baml", "function second() -> int { 2 }");

        let root_sender = Arc::new(CapturingLspSender::default());
        let multi = multi_with_senders(root_sender, Arc::new(CapturingPlaygroundSender::default()));
        let first_root = first_workspace.root_vfs_path();
        multi.discover_workspace_projects(std::slice::from_ref(&first_root));
        multi
            .session_config
            .set(SessionConfig {
                position_encoding: PositionEncoding::Utf8,
            })
            .unwrap();

        let session_sender = Arc::new(CapturingLspSender::default());
        let session = <BexMulitProject as crate::bex_lsp::BexLsp>::new_lsp_session(
            &multi,
            session_sender.clone(),
        );
        session.handle_request(lsp_server::Request::new(
            lsp_server::RequestId::from(1),
            "initialize".to_string(),
            initialize_params(&second_workspace),
        ));

        let initialize_result = session_sender
            .messages
            .lock()
            .unwrap()
            .iter()
            .find_map(|message| match message {
                lsp_server::Message::Response(response) => response.result.clone(),
                _ => None,
            })
            .and_then(|result| serde_json::from_value::<lsp_types::InitializeResult>(result).ok())
            .expect("a fresh session must accept initialize");
        assert_eq!(
            initialize_result.capabilities.position_encoding,
            Some(lsp_types::PositionEncodingKind::UTF16)
        );
        assert_eq!(
            multi.session_config().unwrap().position_encoding,
            PositionEncoding::Utf8,
            "session negotiation must not overwrite the process/root dispatcher"
        );
        assert_eq!(
            multi.workspace_roots.lock().unwrap().as_slice(),
            std::slice::from_ref(&first_root),
            "session initialize must not overwrite preload/poller roots"
        );

        session
            .initialize_workspace_roots(vec![second_workspace.root.clone()])
            .unwrap();
        assert!(
            multi
                .project_for_root_str(first_workspace.vfs_path("proj").as_str())
                .is_some(),
            "discovering a second session must preserve projects outside its roots"
        );
        assert!(
            multi
                .project_for_root_str(second_workspace.vfs_path("proj").as_str())
                .is_some()
        );
    }

    #[test]
    fn root_discovery_publishes_catalog_only_to_playground_before_initialize() {
        let workspace = TempWorkspace::new("preinitialize_catalog");
        workspace.file("proj/baml_src/main.baml", "function f() -> int { 1 }");
        let lsp_sender = Arc::new(CapturingLspSender::default());
        let playground_sender = Arc::new(CapturingPlaygroundSender::default());
        let multi = multi_with_senders(lsp_sender.clone(), playground_sender.clone());

        multi.discover_workspace_projects(&[workspace.root_vfs_path()]);
        assert!(
            lsp_sender.messages.lock().unwrap().iter().all(|message| {
                !matches!(message, lsp_server::Message::Notification(notification)
                    if notification.method == "baml/listProjects")
            }),
            "preload/poller clones must not bypass the connection-scoped LSP sink"
        );
        assert!(
            playground_sender
                .notifications
                .lock()
                .unwrap()
                .iter()
                .any(|notification| notification["type"] == "listProjects"),
            "pre-initialize discovery must still seed the shared playground"
        );

        multi
            .session_config
            .set(SessionConfig {
                position_encoding: PositionEncoding::Utf16,
            })
            .unwrap();
        multi.send_list_projects(true);
        assert!(lsp_sender.messages.lock().unwrap().iter().any(|message| {
            matches!(message, lsp_server::Message::Notification(notification)
                    if notification.method == "baml/listProjects")
        }));
    }

    #[test]
    fn shutdown_and_exit_preserve_process_owned_projects() {
        let workspace = TempWorkspace::new("session_lifecycle_preserves_projects");
        workspace.file("proj/baml_src/main.baml", "function f() -> int { 1 }");
        let multi = test_multi_project(Arc::new(CapturingPlaygroundSender::default()));
        let workspace_root = workspace.root_vfs_path();
        let project_root = workspace.vfs_path("proj");
        multi.discover_workspace_projects(std::slice::from_ref(&workspace_root));

        <BexMulitProject as crate::bex_lsp::request::BexLspRequest>::on_request_shutdown(
            &multi,
            (),
        )
        .unwrap();
        assert!(multi.project_for_root_str(project_root.as_str()).is_some());

        <BexMulitProject as crate::bex_lsp::notification::BexLspNotification>::on_notification_exit(
            &multi,
            (),
        )
        .unwrap();
        assert!(multi.project_for_root_str(project_root.as_str()).is_some());
    }

    #[test]
    fn open_baml_panel_propagates_lsp_notification_delivery_failure() {
        let workspace = TempWorkspace::new("open_panel_delivery_failure");
        workspace.file("proj/baml_src/main.baml", "function f() -> int { 1 }");
        let playground_sender = Arc::new(PortPlaygroundSender::default());
        let multi = multi_with_senders(Arc::new(FailingOpenPanelSender), playground_sender.clone());
        let project_root = workspace.vfs_path("proj");
        multi.discover_workspace_projects(&[workspace.root_vfs_path()]);

        let params = serde_json::from_value::<lsp_types::ExecuteCommandParams>(serde_json::json!({
            "command": "baml.openBamlPanel",
            "arguments": [{ "projectPath": project_root.as_str() }]
        }))
        .unwrap();
        let result = <BexMulitProject as crate::bex_lsp::request::BexLspRequest>::on_request_workspace_execute_command(
            &multi,
            params,
        );

        assert!(matches!(result, Err(LspError::OutboundSaturated)));
        assert!(
            playground_sender
                .notifications
                .lock()
                .unwrap()
                .iter()
                .all(|notification| notification["type"] != "openPlayground")
        );
    }

    #[test]
    fn scoped_rediscovery_evicts_only_projects_owned_by_that_session() {
        let first_workspace = TempWorkspace::new("scoped_eviction_first");
        first_workspace.file("proj/baml_src/main.baml", "function first() -> int { 1 }");
        let second_workspace = TempWorkspace::new("scoped_eviction_second");
        second_workspace.file("proj/baml_src/main.baml", "function second() -> int { 2 }");
        let multi = test_multi_project(Arc::new(CapturingPlaygroundSender::default()));
        let first_root = first_workspace.root_vfs_path();
        let second_root = second_workspace.root_vfs_path();
        let first_project = first_workspace.vfs_path("proj");
        let second_project = second_workspace.vfs_path("proj");

        multi.discover_workspace_projects(std::slice::from_ref(&first_root));
        multi.discover_workspace_projects(std::slice::from_ref(&second_root));
        assert!(multi.project_for_root_str(first_project.as_str()).is_some());
        assert!(
            multi
                .project_for_root_str(second_project.as_str())
                .is_some()
        );

        std::fs::remove_dir_all(first_workspace.root.join("proj")).unwrap();
        multi.discover_workspace_projects(std::slice::from_ref(&first_root));
        assert!(multi.project_for_root_str(first_project.as_str()).is_none());
        assert!(
            multi
                .project_for_root_str(second_project.as_str())
                .is_some(),
            "reconciling one ownership domain must not evict another domain"
        );
    }

    #[test]
    fn standalone_baml_file_is_not_promoted_by_strict_resolver() {
        let ws = TempWorkspace::new("standalone_baml_language");
        // Path contains a `baml_language` segment, triggering the lenient
        // internal-dev fallback.
        ws.file("baml_language/case.baml", "// standalone");
        let file = ws.vfs_path("baml_language/case.baml");

        let lenient = BexMulitProject::get_baml_project_root(&file).unwrap();
        assert_eq!(lenient.as_str(), file.as_str());

        let strict = BexMulitProject::get_marked_baml_project_root(&file);
        assert!(matches!(strict, Err(LspError::ProjectRootNotFound(..))));
    }

    #[test]
    fn strict_resolver_finds_marked_project_root() {
        let ws = TempWorkspace::new("marked_root");
        ws.file("proj/baml_src/main.baml", "// main");
        let file = ws.vfs_path("proj/baml_src/main.baml");

        let root = BexMulitProject::get_marked_baml_project_root(&file).unwrap();
        assert_eq!(root.as_str(), ws.vfs_path("proj").as_str());
    }

    #[test]
    fn native_scan_skips_generated_and_hidden_dirs() {
        let ws = TempWorkspace::new("scan_skips");
        ws.file("proj/baml_src/main.baml", "// main");
        ws.dir("target/junk/baml_src");
        ws.dir("node_modules/pkg/baml_src");
        ws.dir(".hidden/baml_src");

        let found = BexMulitProject::scan_marked_project_roots_native(&ws.root);
        assert_eq!(
            found,
            vec![ws.root.join("proj")],
            "only the real project should be discovered"
        );
    }

    #[test]
    fn native_scan_respects_gitignore() {
        let ws = TempWorkspace::new("scan_gitignore");
        // A `.git` dir marks the workspace as a git repo for the `ignore` crate.
        ws.dir(".git");
        ws.file(".gitignore", "generated/\n");
        ws.dir("generated/baml_src");
        ws.file("app/baml_src/main.baml", "// main");

        let found = BexMulitProject::scan_marked_project_roots_native(&ws.root);
        assert_eq!(
            found,
            vec![ws.root.join("app")],
            "gitignored directories must not be discovered"
        );
    }

    #[test]
    fn vfs_walk_skips_generated_dirs_and_finds_markers() {
        let root = vfs::VfsPath::new(vfs::MemoryFS::new());
        for dir in [
            "proj/baml_src",
            "manifest_proj",
            "node_modules/pkg/baml_src",
            "target/junk/baml_src",
        ] {
            root.join(dir).unwrap().create_dir_all().unwrap();
        }
        root.join("manifest_proj/baml.toml")
            .unwrap()
            .create_file()
            .unwrap();

        let mut found = Vec::new();
        BexMulitProject::collect_marked_project_roots_vfs(&root, &mut found);
        let mut names: Vec<_> = found.iter().map(vfs::VfsPath::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, vec!["/manifest_proj", "/proj"]);
    }

    #[test]
    fn remove_then_readd_allocates_new_incarnation_and_catalog_entry() {
        let ws = TempWorkspace::new("remove_readd_incarnation");
        ws.file("proj/baml_src/main.baml", "function f() -> int { 1 }");
        let sender = Arc::new(CapturingPlaygroundSender::default());
        let multi = test_multi_project(sender.clone());
        let workspace_root = ws.root_vfs_path();
        let project_root = ws.vfs_path("proj");

        multi.discover_workspace_projects(std::slice::from_ref(&workspace_root));
        let first = multi
            .project_for_root_str(project_root.as_str())
            .expect("project should be discovered");
        let first_key = crate::fs::FsPath::from_vfs(&project_root);
        let stale_identity = crate::project::BexProject::source_publication_identity(
            first.project.source_revision(),
        );
        assert!(first.project.enqueue_publication_if_current(
            stale_identity,
            crate::project::ProjectPublication::Playground {
                session_epoch: multi.current_playground_session_epoch(),
                notification: crate::bex_lsp::PlaygroundNotification::CursorContext {
                    context: serde_json::json!({ "staleIncarnation": true }),
                },
            },
        ));

        std::fs::remove_dir_all(ws.root.join("proj")).unwrap();
        multi.discover_workspace_projects(std::slice::from_ref(&workspace_root));
        assert!(multi.project_for_root_str(project_root.as_str()).is_none());

        ws.file("proj/baml_src/main.baml", "function f() -> int { 2 }");
        multi.discover_workspace_projects(std::slice::from_ref(&workspace_root));
        let second = multi
            .project_for_root_str(project_root.as_str())
            .expect("re-added project should be discovered");

        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(second.incarnation, first.incarnation + 1);
        let _ = multi.drain_project_publications(&first_key, &first);

        let notifications = sender.notifications.lock().unwrap();
        assert!(
            notifications
                .iter()
                .all(|notification| notification["context"]["staleIncarnation"] != true)
        );
        let last_catalog = notifications
            .iter()
            .rev()
            .find(|notification| notification["type"] == "listProjects")
            .expect("catalog should be sent after re-add");
        assert_eq!(last_catalog["entries"][0]["incarnation"], 2);
    }

    #[test]
    fn removing_project_clears_every_previously_published_diagnostic_uri() {
        let ws = TempWorkspace::new("remove_clears_diagnostics");
        ws.file("proj/baml_src/main.baml", "function f() -> int { 1 }");
        let lsp_sender = Arc::new(CapturingLspSender::default());
        let playground_sender = Arc::new(CapturingPlaygroundSender::default());
        let fs = crate::fs::BamlVFS::new(Arc::new(Box::new(vfs::PhysicalFS::new("/"))));
        let multi = BexMulitProject::new(
            Arc::new(|_| Arc::new(sys_ops::SysOpsBuilder::new().build())),
            lsp_sender.clone(),
            playground_sender,
            fs,
            BackgroundSpawner::new(),
        );
        multi
            .session_config
            .set(SessionConfig {
                position_encoding: PositionEncoding::Utf16,
            })
            .unwrap();
        let workspace_root = ws.root_vfs_path();
        let project_root = ws.vfs_path("proj");
        multi.discover_workspace_projects(std::slice::from_ref(&workspace_root));
        let project = multi
            .project_for_root_str(project_root.as_str())
            .expect("project should be discovered");
        let source_path = crate::fs::FsPath::from_vfs(&ws.vfs_path("proj/baml_src/main.baml"));
        let source_uri = lsp_types::Url::from_file_path(source_path.as_path()).unwrap();
        seed_published_diagnostic(
            &multi,
            &project_root,
            &project,
            source_path,
            source_uri.clone(),
        );

        std::fs::remove_dir_all(ws.root.join("proj")).unwrap();
        multi.discover_workspace_projects(std::slice::from_ref(&workspace_root));

        let messages = lsp_sender.messages.lock().unwrap();
        assert!(messages.iter().any(|message| {
            let lsp_server::Message::Notification(notification) = message else {
                return false;
            };
            if notification.method != "textDocument/publishDiagnostics" {
                return false;
            }
            serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(
                notification.params.clone(),
            )
            .is_ok_and(|params| params.uri == source_uri && params.diagnostics.is_empty())
        }));
    }

    #[test]
    fn source_diagnostics_fan_out_with_each_outputs_uri_and_version() {
        let ws = TempWorkspace::new("diagnostic_output_identity");
        let invalid = "function f() -> int {";
        ws.file("proj/baml_src/main.baml", invalid);
        let root = test_multi_project(Arc::new(CapturingPlaygroundSender::default()));
        let first_sender = Arc::new(CapturingLspSender::default());
        let second_sender = Arc::new(CapturingLspSender::default());
        let divergent_sender = Arc::new(CapturingLspSender::default());
        let closed_sender = Arc::new(CapturingLspSender::default());
        let first = root.connection_scoped_lsp_session(first_sender.clone());
        let second = root.connection_scoped_lsp_session(second_sender.clone());
        let divergent = root.connection_scoped_lsp_session(divergent_sender.clone());
        let closed = root.connection_scoped_lsp_session(closed_sender.clone());
        first
            .session_config
            .set(SessionConfig {
                position_encoding: PositionEncoding::Utf16,
            })
            .unwrap();
        second
            .session_config
            .set(SessionConfig {
                position_encoding: PositionEncoding::Utf16,
            })
            .unwrap();
        divergent
            .session_config
            .set(SessionConfig {
                position_encoding: PositionEncoding::Utf16,
            })
            .unwrap();
        closed
            .session_config
            .set(SessionConfig {
                position_encoding: PositionEncoding::Utf16,
            })
            .unwrap();

        let project_root = ws.vfs_path("proj");
        let client_path = crate::fs::FsPath::from_vfs(&ws.vfs_path("proj/baml_src/main.baml"));
        let first_uri = lsp_types::Url::from_file_path(client_path.as_path()).unwrap();
        let path = crate::fs::FsPath::from_str(
            canonical_path_identity(client_path.as_path())
                .to_string_lossy()
                .into_owned(),
        );
        let mut second_uri = first_uri.clone();
        second_uri.set_query(Some("second-client-uri"));
        let mut divergent_uri = first_uri.clone();
        divergent_uri.set_query(Some("divergent-client-uri"));
        let project = root.get_or_create_project(project_root.clone()).unwrap();
        let sources = root.load_project_sources(&project_root).unwrap();
        let revision = project.project.apply_all_sources_and_open_document(
            &sources,
            path.clone(),
            first_uri.clone(),
            3,
            invalid.to_string(),
        );
        first.open_documents.lock().unwrap().insert(
            path.clone(),
            crate::project::OpenDocument {
                client_uri: first_uri.clone(),
                version: 3,
                text: invalid.to_string(),
            },
        );
        second.open_documents.lock().unwrap().insert(
            path.clone(),
            crate::project::OpenDocument {
                client_uri: second_uri.clone(),
                version: 91,
                text: invalid.to_string(),
            },
        );
        divergent.open_documents.lock().unwrap().insert(
            path.clone(),
            crate::project::OpenDocument {
                client_uri: divergent_uri.clone(),
                version: 44,
                text: "function f() -> int { 1 }".to_string(),
            },
        );

        root.run_project_tail(&project_root, &project, revision);

        let diagnostic_for_uri = |sender: &CapturingLspSender, expected_uri: &lsp_types::Url| {
            sender
                .messages
                .lock()
                .unwrap()
                .iter()
                .filter_map(|message| {
                    let lsp_server::Message::Notification(notification) = message else {
                        return None;
                    };
                    (notification.method == "textDocument/publishDiagnostics")
                        .then(|| {
                            serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(
                                notification.params.clone(),
                            )
                            .ok()
                        })
                        .flatten()
                })
                .find(|params| &params.uri == expected_uri)
        };
        let first_diagnostics =
            diagnostic_for_uri(&first_sender, &first_uri).expect("first output diagnostics");
        let second_diagnostics =
            diagnostic_for_uri(&second_sender, &second_uri).expect("second output diagnostics");
        let divergent_diagnostics =
            diagnostic_for_uri(&divergent_sender, &divergent_uri).expect("divergent output clear");
        let canonical_uri = wasm_helpers::from_file_path(path.as_path()).unwrap();
        let closed_diagnostics = diagnostic_for_uri(&closed_sender, &canonical_uri)
            .expect("closed-document output diagnostics");
        assert_eq!(first_diagnostics.uri, first_uri);
        assert_eq!(first_diagnostics.version, Some(3));
        assert!(!first_diagnostics.diagnostics.is_empty());
        assert_eq!(second_diagnostics.uri, second_uri);
        assert_eq!(second_diagnostics.version, Some(91));
        assert!(!second_diagnostics.diagnostics.is_empty());
        assert_eq!(divergent_diagnostics.uri, divergent_uri);
        assert_eq!(divergent_diagnostics.version, Some(44));
        assert!(
            divergent_diagnostics.diagnostics.is_empty(),
            "shared-source ranges must not be applied to different retained text"
        );
        assert_eq!(closed_diagnostics.version, None);
        assert!(!closed_diagnostics.diagnostics.is_empty());
    }

    #[test]
    fn project_retirement_clears_each_outputs_acknowledged_uri() {
        let ws = TempWorkspace::new("retirement_output_identity");
        ws.file("proj/baml_src/main.baml", "function f() -> int { 1 }");
        let root = test_multi_project(Arc::new(CapturingPlaygroundSender::default()));
        let first_sender = Arc::new(CapturingLspSender::default());
        let second_sender = Arc::new(CapturingLspSender::default());
        let first = root.connection_scoped_lsp_session(first_sender.clone());
        let second = root.connection_scoped_lsp_session(second_sender.clone());
        for session in [&first, &second] {
            session
                .session_config
                .set(SessionConfig {
                    position_encoding: PositionEncoding::Utf16,
                })
                .unwrap();
        }
        let project_root = ws.vfs_path("proj");
        let project = root.get_or_create_project(project_root.clone()).unwrap();
        let path = crate::fs::FsPath::from_vfs(&ws.vfs_path("proj/baml_src/main.baml"));
        let first_uri = lsp_types::Url::parse("file:///first-client/main.baml").unwrap();
        let second_uri = lsp_types::Url::parse("file:///second-client/main.baml").unwrap();
        seed_published_diagnostic(
            &first,
            &project_root,
            &project,
            path.clone(),
            first_uri.clone(),
        );
        seed_published_diagnostic(&second, &project_root, &project, path, second_uri.clone());

        root.retire_project_diagnostics(&crate::fs::FsPath::from_vfs(&project_root), &project);

        let cleared_uris = |sender: &CapturingLspSender| {
            sender
                .messages
                .lock()
                .unwrap()
                .iter()
                .filter_map(|message| {
                    let lsp_server::Message::Notification(notification) = message else {
                        return None;
                    };
                    (notification.method == "textDocument/publishDiagnostics")
                        .then(|| {
                            serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(
                                notification.params.clone(),
                            )
                            .ok()
                        })
                        .flatten()
                })
                .filter(|params| params.diagnostics.is_empty())
                .map(|params| params.uri)
                .collect::<Vec<_>>()
        };
        assert_eq!(cleared_uris(&first_sender), vec![first_uri]);
        assert_eq!(cleared_uris(&second_sender), vec![second_uri]);
    }

    #[test]
    fn slow_retirement_churn_closes_output_at_durable_queue_bound() {
        let ws = TempWorkspace::new("bounded_retirement_churn");
        ws.file("proj/baml_src/main.baml", "function f() -> int { 1 }");
        let sender = Arc::new(SlowRetirementSender::default());
        let session = multi_with_senders(
            sender.clone(),
            Arc::new(CapturingPlaygroundSender::default()),
        );
        session
            .session_config
            .set(SessionConfig {
                position_encoding: PositionEncoding::Utf16,
            })
            .unwrap();
        let project_root = ws.vfs_path("proj");
        let project = session.get_or_create_project(project_root.clone()).unwrap();
        let project_key = PublishedProjectKey {
            project_root: crate::fs::FsPath::from_vfs(&project_root),
            incarnation: project.incarnation,
        };
        let history = (0..=MAX_RETIRED_DIAGNOSTIC_ITEMS)
            .map(|index| {
                (
                    crate::fs::FsPath::from_str(format!("/retired/{index}.baml")),
                    lsp_types::Url::parse(&format!("file:///retired/{index}.baml")).unwrap(),
                )
            })
            .collect();
        session
            .published_diagnostics
            .lock()
            .unwrap()
            .insert(project_key, history);

        session.retire_project_diagnostics(&crate::fs::FsPath::from_vfs(&project_root), &project);

        assert!(
            sender.is_closed(),
            "overflow must deterministically close output"
        );
        let retired = session.retired_diagnostics.lock().unwrap();
        assert!(retired.pending.is_empty());
        assert_eq!(retired.pending_bytes, 0);
    }

    #[test]
    fn root_poller_removal_routes_tombstones_to_initialized_session_output() {
        let ws = TempWorkspace::new("root_poller_routes_diagnostic_clears");
        ws.file("proj/baml_src/main.baml", "function f() -> int { 1 }");
        let session_sender = Arc::new(CapturingLspSender::default());
        let root = test_multi_project(Arc::new(CapturingPlaygroundSender::default()));
        let session = root.connection_scoped_lsp_session(session_sender.clone());
        session
            .session_config
            .set(SessionConfig {
                position_encoding: PositionEncoding::Utf16,
            })
            .unwrap();
        let workspace_root = ws.root_vfs_path();
        let project_root = ws.vfs_path("proj");
        session.discover_workspace_projects(std::slice::from_ref(&workspace_root));
        let project = session
            .project_for_root_str(project_root.as_str())
            .expect("project should be discovered");
        let source_path = crate::fs::FsPath::from_vfs(&ws.vfs_path("proj/baml_src/main.baml"));
        let source_uri = lsp_types::Url::from_file_path(source_path.as_path()).unwrap();
        seed_published_diagnostic(
            &session,
            &project_root,
            &project,
            source_path,
            source_uri.clone(),
        );

        std::fs::remove_dir_all(ws.root.join("proj")).unwrap();
        root.discover_workspace_projects(std::slice::from_ref(&workspace_root));

        assert!(
            session_sender
                .messages
                .lock()
                .unwrap()
                .iter()
                .any(|message| {
                    let lsp_server::Message::Notification(notification) = message else {
                        return false;
                    };
                    notification.method == "textDocument/publishDiagnostics"
                        && serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(
                            notification.params.clone(),
                        )
                        .is_ok_and(|params| {
                            params.uri == source_uri && params.diagnostics.is_empty()
                        })
                })
        );
    }

    #[test]
    fn removed_project_diagnostic_tombstones_survive_writer_saturation() {
        let ws = TempWorkspace::new("remove_retries_diagnostic_clears");
        ws.file("proj/baml_src/main.baml", "function f() -> int { 1 }");
        let lsp_sender = Arc::new(SaturateSecondDiagnosticSender::default());
        let multi = multi_with_senders(
            lsp_sender.clone(),
            Arc::new(CapturingPlaygroundSender::default()),
        );
        multi
            .session_config
            .set(SessionConfig {
                position_encoding: PositionEncoding::Utf16,
            })
            .unwrap();
        let workspace_root = ws.root_vfs_path();
        let project_root = ws.vfs_path("proj");
        multi.discover_workspace_projects(std::slice::from_ref(&workspace_root));
        let project = multi
            .project_for_root_str(project_root.as_str())
            .expect("project should be discovered");
        let expected = (0..3)
            .map(|index| {
                let path = crate::fs::FsPath::from_str(format!("/retired/{index}.baml"));
                let uri = lsp_types::Url::parse(&format!("file:///retired/{index}.baml"))
                    .expect("test URI");
                seed_published_diagnostic(&multi, &project_root, &project, path, uri.clone());
                uri
            })
            .collect::<std::collections::HashSet<_>>();

        std::fs::remove_dir_all(ws.root.join("proj")).unwrap();
        multi.discover_workspace_projects(std::slice::from_ref(&workspace_root));

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let pending = multi.retired_diagnostics.lock().unwrap().pending.len();
            if pending == 0 || std::time::Instant::now() >= deadline {
                assert_eq!(pending, 0, "every tombstone should eventually drain");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let delivered = lsp_sender
            .messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|message| {
                let lsp_server::Message::Notification(notification) = message else {
                    return None;
                };
                (notification.method == "textDocument/publishDiagnostics")
                    .then(|| {
                        serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(
                            notification.params.clone(),
                        )
                        .ok()
                    })
                    .flatten()
            })
            .filter(|params| params.diagnostics.is_empty())
            .map(|params| params.uri)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(delivered, expected);
        assert!(
            lsp_sender
                .diagnostic_attempts
                .load(std::sync::atomic::Ordering::Acquire)
                > expected.len(),
            "one tombstone should have been retried after temporary saturation"
        );
    }

    #[test]
    fn readded_diagnostics_supersede_a_saturated_retirement_tombstone() {
        let ws = TempWorkspace::new("readd_supersedes_retired_clear");
        ws.file("proj/baml_src/main.baml", "function f() -> int { 1 }");
        let lsp_sender = Arc::new(HeldTombstoneSender::default());
        let session = multi_with_senders(
            lsp_sender.clone(),
            Arc::new(CapturingPlaygroundSender::default()),
        );
        session
            .session_config
            .set(SessionConfig {
                position_encoding: PositionEncoding::Utf16,
            })
            .unwrap();
        let workspace_root = ws.root_vfs_path();
        let project_root = ws.vfs_path("proj");
        let source_path = crate::fs::FsPath::from_vfs(&ws.vfs_path("proj/baml_src/main.baml"));
        let source_uri = lsp_types::Url::from_file_path(source_path.as_path()).unwrap();
        session.discover_workspace_projects(std::slice::from_ref(&workspace_root));
        let old = session
            .project_for_root_str(project_root.as_str())
            .expect("old project");
        seed_published_diagnostic(
            &session,
            &project_root,
            &old,
            source_path.clone(),
            source_uri.clone(),
        );

        std::fs::remove_dir_all(ws.root.join("proj")).unwrap();
        session.discover_workspace_projects(std::slice::from_ref(&workspace_root));
        assert_eq!(session.retired_diagnostics.lock().unwrap().pending.len(), 1);

        ws.file("proj/baml_src/main.baml", "function f() -> int {");
        session.discover_workspace_projects(std::slice::from_ref(&workspace_root));
        let replacement = session
            .project_for_root_str(project_root.as_str())
            .expect("replacement project");
        let replacement_key = crate::fs::FsPath::from_vfs(&project_root);
        let publication = crate::project::ProjectPublication::LspDiagnostics {
            path: source_path,
            present: true,
            params: lsp_types::PublishDiagnosticsParams::new(
                source_uri.clone(),
                vec![lsp_types::Diagnostic::new_simple(
                    lsp_types::Range::new(
                        lsp_types::Position::new(0, 0),
                        lsp_types::Position::new(0, 1),
                    ),
                    "replacement diagnostic".to_string(),
                )],
                Some(1),
            ),
        };
        let drain_guard = replacement.project.lock_publication_drainer().unwrap();
        assert!(session.enqueue_and_drain_diagnostics_locked(
            &replacement_key,
            &replacement,
            replacement.project.source_revision(),
            vec![publication],
        ));
        drop(drain_guard);
        assert!(
            session
                .retired_diagnostics
                .lock()
                .unwrap()
                .pending
                .is_empty()
        );

        lsp_sender
            .allow_tombstones
            .store(true, std::sync::atomic::Ordering::Release);
        if let Some(output) = session.lsp_output_context.clone() {
            session.flush_retired_diagnostics_for(output);
        }
        let diagnostics = lsp_sender
            .messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|message| {
                let lsp_server::Message::Notification(notification) = message else {
                    return None;
                };
                (notification.method == "textDocument/publishDiagnostics")
                    .then(|| {
                        serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(
                            notification.params.clone(),
                        )
                        .ok()
                    })
                    .flatten()
            })
            .filter(|params| params.uri == source_uri)
            .collect::<Vec<_>>();
        assert!(
            diagnostics
                .iter()
                .any(|params| !params.diagnostics.is_empty())
        );
        assert!(
            diagnostics
                .iter()
                .all(|params| !params.diagnostics.is_empty()),
            "a delayed old-incarnation clear must never follow replacement diagnostics"
        );
    }

    #[test]
    fn diagnostic_retry_resumes_after_successful_uri_prefix() {
        let ws = TempWorkspace::new("diagnostic_retry_prefix_checkpoint");
        ws.file("proj/baml_src/a.baml", "function a() -> int { 1 }");
        ws.file("proj/baml_src/b.baml", "function b() -> int { 2 }");
        ws.file("proj/baml_src/c.baml", "function c() -> int { 3 }");
        let lsp_sender = Arc::new(TimeGatedDiagnosticSender::default());
        let session = multi_with_senders(
            lsp_sender.clone(),
            Arc::new(CapturingPlaygroundSender::default()),
        );
        session
            .session_config
            .set(SessionConfig {
                position_encoding: PositionEncoding::Utf16,
            })
            .unwrap();
        let project_root = ws.vfs_path("proj");
        let project = session.get_or_create_project(project_root.clone()).unwrap();
        let sources = session.load_project_sources(&project_root).unwrap();
        let revision = project.project.apply_all_sources(&sources);
        let project_key = crate::fs::FsPath::from_vfs(&project_root);
        let publications = ["a.baml", "b.baml", "c.baml"]
            .into_iter()
            .map(|filename| {
                let path =
                    crate::fs::FsPath::from_vfs(&ws.vfs_path(&format!("proj/baml_src/{filename}")));
                let uri = lsp_types::Url::from_file_path(path.as_path()).unwrap();
                crate::project::ProjectPublication::LspDiagnostics {
                    path,
                    present: true,
                    params: lsp_types::PublishDiagnosticsParams::new(uri, Vec::new(), None),
                }
            })
            .collect::<Vec<_>>();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let drain_guard = project.project.lock_publication_drainer().unwrap();
            let complete = session.enqueue_and_drain_diagnostics_locked(
                &project_key,
                &project,
                revision,
                publications.clone(),
            );
            drop(drain_guard);
            if complete {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "all URI publications should eventually make progress"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let delivered = lsp_sender
            .messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|message| {
                let lsp_server::Message::Notification(notification) = message else {
                    return None;
                };
                (notification.method == "textDocument/publishDiagnostics")
                    .then(|| {
                        serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(
                            notification.params.clone(),
                        )
                        .ok()
                    })
                    .flatten()
            })
            .map(|params| params.uri)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(delivered.len(), 3);
    }

    #[test]
    fn blocked_transport_serialization_does_not_hold_source_publication_barrier() {
        let ws = TempWorkspace::new("transport_outside_publication_barrier");
        ws.file("proj/baml_src/main.baml", "function f() -> int { 1 }");
        let lsp_sender = Arc::new(BlockingDiagnosticSender::default());
        let multi = multi_with_senders(
            lsp_sender.clone(),
            Arc::new(CapturingPlaygroundSender::default()),
        );
        let workspace_root = ws.root_vfs_path();
        let project_root = ws.vfs_path("proj");
        multi.discover_workspace_projects(std::slice::from_ref(&workspace_root));
        let project = multi
            .project_for_root_str(project_root.as_str())
            .expect("project should be discovered");
        let project_key = crate::fs::FsPath::from_vfs(&project_root);
        let source_path = crate::fs::FsPath::from_vfs(&ws.vfs_path("proj/baml_src/main.baml"));
        let uri = lsp_types::Url::from_file_path(source_path.as_path()).unwrap();
        let revision = project.project.source_revision();
        assert!(project.project.enqueue_publication_if_current(
            crate::project::BexProject::source_publication_identity(revision),
            crate::project::ProjectPublication::LspDiagnostics {
                path: source_path.clone(),
                present: true,
                params: lsp_types::PublishDiagnosticsParams::new(uri, Vec::new(), Some(1)),
            },
        ));

        let mutating_project = project.clone();
        let drain =
            std::thread::spawn(move || multi.drain_project_publications(&project_key, &project));
        lsp_sender.wait_until_entered();

        let (mutated_tx, mutated_rx) = std::sync::mpsc::channel();
        let mutation = std::thread::spawn(move || {
            let mut sources = std::collections::HashMap::new();
            sources.insert(source_path, "function f() -> int { 2 }".to_string());
            mutating_project.project.apply_all_sources(&sources);
            mutated_tx.send(()).unwrap();
        });
        mutated_rx
            .recv_timeout(std::time::Duration::from_millis(500))
            .expect("source mutation must not wait for downstream serialization/send");

        lsp_sender.release();
        mutation.join().unwrap();
        drain.join().unwrap();
    }

    #[test]
    fn playground_sessions_coexist_and_closed_session_work_is_dropped() {
        let ws = TempWorkspace::new("concurrent_playground_sessions");
        ws.file("proj/baml_src/main.baml", "function f() -> int { 1 }");
        let sender = Arc::new(CapturingPlaygroundSender::default());
        let multi = test_multi_project(sender.clone());
        let workspace_root = ws.root_vfs_path();
        let project_root = ws.vfs_path("proj");
        multi.discover_workspace_projects(std::slice::from_ref(&workspace_root));
        let project = multi
            .project_for_root_str(project_root.as_str())
            .expect("project should be discovered");
        let project_key = crate::fs::FsPath::from_vfs(&project_root);
        let first_epoch =
            <BexMulitProject as crate::bex_lsp::BexLsp>::begin_playground_session(&multi);
        let second_epoch =
            <BexMulitProject as crate::bex_lsp::BexLsp>::begin_playground_session(&multi);
        assert!(multi.playground_session_is_current_internal(first_epoch));
        assert!(multi.playground_session_is_current_internal(second_epoch));
        let identity = crate::project::BexProject::source_publication_identity(
            project.project.source_revision(),
        );
        assert!(multi.enqueue_project_playground_publication(
            &project_key,
            &project,
            identity,
            first_epoch,
            crate::bex_lsp::PlaygroundNotification::CursorContext {
                context: serde_json::json!({ "firstSession": true }),
            },
        ));
        let _ = multi.drain_project_publications(&project_key, &project);
        assert!(
            sender
                .notifications
                .lock()
                .unwrap()
                .iter()
                .any(|notification| { notification["context"]["firstSession"] == true })
        );

        <BexMulitProject as crate::bex_lsp::BexLsp>::end_playground_session(&multi, first_epoch);
        assert!(!multi.playground_session_is_current_internal(first_epoch));
        assert!(multi.playground_session_is_current_internal(second_epoch));

        assert!(!multi.enqueue_project_playground_publication(
            &project_key,
            &project,
            identity,
            first_epoch,
            crate::bex_lsp::PlaygroundNotification::CursorContext {
                context: serde_json::json!({ "lateFirstSession": true }),
            },
        ));
        assert!(multi.enqueue_project_playground_publication(
            &project_key,
            &project,
            identity,
            second_epoch,
            crate::bex_lsp::PlaygroundNotification::CursorContext {
                context: serde_json::json!({ "secondSession": true }),
            },
        ));
        let _ = multi.drain_project_publications(&project_key, &project);
        let notifications = sender.notifications.lock().unwrap();
        assert!(
            notifications
                .iter()
                .all(|notification| { notification["context"]["lateFirstSession"] != true })
        );
        assert!(
            notifications
                .iter()
                .any(|notification| { notification["context"]["secondSession"] == true })
        );
        drop(notifications);

        let mut bound_second = multi.clone();
        bound_second.playground_session_context = Some(second_epoch);
        <BexMulitProject as crate::bex_lsp::BexLsp>::end_playground_session(&multi, second_epoch);
        assert!(
            !bound_second.playground_session_is_current_internal(second_epoch),
            "an empty active set must not resurrect the last disconnected bound session"
        );
        assert!(!bound_second.enqueue_project_playground_publication(
            &project_key,
            &project,
            identity,
            second_epoch,
            crate::bex_lsp::PlaygroundNotification::CursorContext {
                context: serde_json::json!({ "lateLastSession": true }),
            },
        ));
    }

    #[test]
    fn stale_incarnation_release_cannot_decrement_readded_project_demand() {
        let ws = TempWorkspace::new("stale_release_incarnation");
        ws.file("proj/baml_src/main.baml", "function f() -> int { 1 }");
        let multi = test_multi_project(Arc::new(CapturingPlaygroundSender::default()));
        let workspace_root = ws.root_vfs_path();
        let project_root = ws.vfs_path("proj");
        multi.discover_workspace_projects(std::slice::from_ref(&workspace_root));
        let first = multi
            .project_for_root_str(project_root.as_str())
            .expect("first incarnation");

        std::fs::remove_dir_all(ws.root.join("proj")).unwrap();
        multi.discover_workspace_projects(std::slice::from_ref(&workspace_root));
        ws.file("proj/baml_src/main.baml", "function f() -> int { 2 }");
        multi.discover_workspace_projects(std::slice::from_ref(&workspace_root));
        let second = multi
            .project_for_root_str(project_root.as_str())
            .expect("second incarnation");
        second
            .runtime_demand
            .store(1, std::sync::atomic::Ordering::Release);

        <BexMulitProject as crate::bex_lsp::BexLsp>::release_project_runtime(
            &multi,
            project_root.as_str(),
            Some(first.incarnation),
        );

        assert_eq!(
            second
                .runtime_demand
                .load(std::sync::atomic::Ordering::Acquire),
            1
        );
    }

    #[test]
    fn active_run_completion_releases_exact_removed_project_incarnation() {
        let ws = TempWorkspace::new("active_run_removed_incarnation");
        ws.file("proj/baml_src/main.baml", "function f() -> int { 1 }");
        let multi = test_multi_project(Arc::new(CapturingPlaygroundSender::default()));
        let workspace_root = ws.root_vfs_path();
        let project_root = ws.vfs_path("proj");
        multi.discover_workspace_projects(std::slice::from_ref(&workspace_root));
        let first = multi
            .project_for_root_str(project_root.as_str())
            .expect("first incarnation");
        first
            .project
            .ensure_engine_current()
            .expect("first incarnation should build");
        let call_id = sys_types::CallId::next();
        first
            .project
            .prepare_and_register_run(call_id, None, None, false)
            .expect("run should register on first incarnation");
        first
            .runtime_demand
            .store(1, std::sync::atomic::Ordering::Release);
        multi
            .active_runs
            .lock()
            .unwrap()
            .insert(call_id, (project_root.as_str().to_string(), first.clone()));

        std::fs::remove_dir_all(ws.root.join("proj")).unwrap();
        multi.discover_workspace_projects(std::slice::from_ref(&workspace_root));
        ws.file("proj/baml_src/main.baml", "function f() -> int { 2 }");
        multi.discover_workspace_projects(std::slice::from_ref(&workspace_root));
        let second = multi
            .project_for_root_str(project_root.as_str())
            .expect("second incarnation");
        second
            .runtime_demand
            .store(2, std::sync::atomic::Ordering::Release);

        <BexMulitProject as crate::bex_lsp::BexLsp>::finish_project_run(
            &multi,
            project_root.as_str(),
            call_id,
        );

        assert_eq!(
            first
                .runtime_demand
                .load(std::sync::atomic::Ordering::Acquire),
            0
        );
        assert_eq!(
            second
                .runtime_demand
                .load(std::sync::atomic::Ordering::Acquire),
            2
        );
        assert!(first.project.active_run(call_id).is_none());
    }

    #[test]
    fn runtime_input_change_rebuilds_only_demanded_projects() {
        let ws = TempWorkspace::new("runtime_input_demand_gate");
        ws.file("warm/baml_src/main.baml", "function warm() -> int { 1 }");
        ws.file("cold/baml_src/main.baml", "function cold() -> int { 2 }");
        let multi = test_multi_project(Arc::new(CapturingPlaygroundSender::default()));
        let workspace_root = ws.root_vfs_path();
        let warm_root = ws.vfs_path("warm");
        let cold_root = ws.vfs_path("cold");
        multi.discover_workspace_projects(std::slice::from_ref(&workspace_root));

        let warm = multi
            .project_for_root_str(warm_root.as_str())
            .expect("warm project");
        let cold = multi
            .project_for_root_str(cold_root.as_str())
            .expect("cold project");
        let warm_generation = warm
            .project
            .ensure_engine_current()
            .expect("initial warm build")
            .generation;
        let cold_generation = cold
            .project
            .ensure_engine_current()
            .expect("initial cold build")
            .generation;
        warm.runtime_demand
            .store(1, std::sync::atomic::Ordering::Release);

        <BexMulitProject as crate::bex_lsp::BexLsp>::runtime_inputs_changed(&multi);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let status = warm.project.runtime_status();
            if status.phase == crate::project::RuntimeBuildPhase::Ready
                && status
                    .generation
                    .is_some_and(|generation| generation > warm_generation)
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "demanded project did not rebuild after runtime inputs changed: {status:?}"
            );
            std::thread::yield_now();
        }

        let cold_status = cold.project.runtime_status();
        assert_eq!(
            cold_status.phase,
            crate::project::RuntimeBuildPhase::IdleStale
        );
        assert_eq!(cold_status.generation, Some(cold_generation));
        assert!(cold_status.has_last_known_good);
        assert!(
            cold.project.get_bex().is_err(),
            "the undemanded project's previous engine must remain last-known-good, not current"
        );
    }
}
