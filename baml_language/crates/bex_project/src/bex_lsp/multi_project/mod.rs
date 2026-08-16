//! Multi-project LSP server core.
//!
//! Implements the refresh → diagnostics → rebuild pipeline:
//!
//! - **Refresh:** every editor/watcher/playground event becomes one
//!   [`crate::project::SourceBatch`] applied atomically with document
//!   versions; the source revision advances with every applied batch.
//! - **Diagnostics:** each project owns a latest-revision dirty
//!   fence. Publication converts an owned revision-tagged candidate; `Busy`
//!   retains the last publication and schedules a trailing retry; stale
//!   candidates are discarded; poison surfaces as an internal failure and
//!   stops retrying.
//! - **Engine rebuild:** the debounce epoch is a pre-work ticket
//!   only. The rebuild itself is single-flight per project and installs
//!   through [`crate::project::BexProject::commit_engine_if_current`] — a
//!   superseded candidate changes nothing.
//! - **Test collection/expansion:** collection runs under an
//!   atomically captured ticket and installs through an ABA fence keyed by
//!   engine generation + collection epoch; expansions serialize on the
//!   installed registry's mutation gate. Stale results emit nothing.
//! - **Typed errors:** `send_response` is the one place request errors
//!   become wire codes; `-32001` is never emitted.

mod commands;
mod diagnostics;
mod notification;
mod request;
mod wasm_helpers;

use std::{
    collections::{HashMap, HashSet},
    io::Read,
    sync::{Arc, Mutex, OnceLock},
};

#[cfg(not(target_arch = "wasm32"))]
use baml_path::{NativePathBuf, VfsPathBuf};
use baml_workspace::{BAML_SRC_DIR, BAML_TOML, find_baml_project_root_from_ancestors};
pub use wasm_helpers::BackgroundSpawner;

/// Factory that creates [`sys_ops::SysOps`] for a given project root.
type SysOpFactory =
    std::sync::Arc<dyn Fn(&vfs::VfsPath) -> std::sync::Arc<sys_ops::SysOps> + Send + Sync>;

use crate::{
    RuntimeError,
    bex_lsp::{
        LspError,
        multi_project::diagnostics::{PublishableDocument, candidate_to_publishable},
        position_codec::PositionEncoding,
    },
    project::{
        BexProject, DbReadError, DiagnosticCandidate, EngineBuildOutcome, PrepareRunError,
        RegistryLeaseError, SourceBatch, SourceGuard, SourceRevision,
    },
};

/// Debounce window for the diagnostics/project-update tail.
#[cfg(not(target_arch = "wasm32"))]
const DIAGNOSTICS_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(150);

/// Debounce window for the heavy engine tail (bytecode + `$init`).
#[cfg(not(target_arch = "wasm32"))]
const ENGINE_REBUILD_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(300);

/// Backoff before retrying a `Busy` diagnostics read.
#[cfg(not(target_arch = "wasm32"))]
const DIAGNOSTICS_BUSY_RETRY: std::time::Duration = std::time::Duration::from_millis(40);

/// Text + version of one open editor document. Version and text travel
/// together so publications carry the exact checked document version.
struct OverlayDocument {
    text: String,
    /// `Some` for editor-owned documents; `None` for playground edits.
    version: Option<i32>,
}

/// Latest-revision diagnostics publication fence.
///
/// ```text
/// source mutation           → mark latest revision dirty; schedule attempt
/// Ready(current revision)   → conditionally publish; compare-and-clear
/// Ready(stale revision)     → discard; retain latest dirty
/// Busy                      → publish nothing; trailing retry with backoff
/// Poisoned                  → publish nothing; surface internal failure
/// ```
///
/// Staleness is decided against the authoritative source revision, not
/// only the dirty mark: an unsolicited candidate (e.g. a superseded
/// rebuild's) and a candidate racing `mark_dirty` are both discarded once a
/// newer revision exists, so they can never regress newer markers.
#[derive(Default)]
struct DiagnosticsFence {
    /// Newest revision requiring publication. Compare-and-clear only.
    dirty: Option<SourceRevision>,
    /// Files covered by the last successful publication, so deleted files
    /// get one empty publish. Not updated by busy/stale attempts.
    last_published: HashSet<std::path::PathBuf>,
    /// A trailing busy-retry is already scheduled. Only the native scheduler
    /// can observe `Busy`; the WASM tail is synchronous and single-threaded.
    #[cfg(not(target_arch = "wasm32"))]
    retry_scheduled: bool,
}

impl DiagnosticsFence {
    /// A mutation happened: the newest revision needs publication.
    fn mark_dirty(&mut self, revision: SourceRevision) {
        self.dirty = Some(self.dirty.map_or(revision, |d| d.max(revision)));
    }

    /// Decide a computed candidate's fate. `true` compare-and-clears the
    /// dirty revision and admits publication; `false` discards a stale
    /// candidate — a newer mutation owns the next publication, whether or
    /// not its dirty mark has landed yet (`refresh_project` always
    /// schedules a tail for it).
    fn admit(
        &mut self,
        candidate_revision: SourceRevision,
        current_revision: SourceRevision,
    ) -> bool {
        if candidate_revision < current_revision {
            return false;
        }
        if let Some(dirty) = self.dirty {
            if candidate_revision < dirty {
                return false;
            }
            self.dirty = None;
        }
        true
    }

    /// Record a successful publication's file coverage. Returns the files
    /// covered by the previous publication but absent now — each needs one
    /// empty publish so stale editor markers clear.
    fn record_publication(
        &mut self,
        current: HashSet<std::path::PathBuf>,
    ) -> Vec<std::path::PathBuf> {
        let deleted = self.last_published.difference(&current).cloned().collect();
        self.last_published = current;
        deleted
    }
}

struct LiveProject {
    project: BexProject,
    in_memory_changes: Mutex<HashMap<crate::fs::FsPath, OverlayDocument>>,
    diagnostics_fence: Mutex<DiagnosticsFence>,
    /// The latest build failure that was not represented by source
    /// diagnostics. Keep it revision-scoped so a later edit immediately stops
    /// surfacing a stale failure, while `requestState` can still replay the
    /// failure that made the current build unavailable.
    build_failure: Mutex<BuildFailureState>,
    /// Debounce tickets (pre-work suppression only — never authorization;
    /// installation is guarded by the revision-conditional commit).
    #[cfg(not(target_arch = "wasm32"))]
    diagnostics_epoch: std::sync::atomic::AtomicU64,
    #[cfg(not(target_arch = "wasm32"))]
    rebuild_epoch: std::sync::atomic::AtomicU64,
    /// Single-flight gate for engine rebuilds: concurrent debounced
    /// tails queue here instead of racing two `spawn_blocking` builds.
    #[cfg(not(target_arch = "wasm32"))]
    rebuild_gate: tokio::sync::Mutex<()>,
}

impl LiveProject {
    fn new(project: BexProject) -> Self {
        Self {
            project,
            in_memory_changes: Mutex::new(HashMap::new()),
            diagnostics_fence: Mutex::new(DiagnosticsFence::default()),
            build_failure: Mutex::new(BuildFailureState::default()),
            #[cfg(not(target_arch = "wasm32"))]
            diagnostics_epoch: std::sync::atomic::AtomicU64::new(0),
            #[cfg(not(target_arch = "wasm32"))]
            rebuild_epoch: std::sync::atomic::AtomicU64::new(0),
            #[cfg(not(target_arch = "wasm32"))]
            rebuild_gate: tokio::sync::Mutex::new(()),
        }
    }
}

#[derive(Default)]
struct BuildFailureState {
    latest: Option<(SourceRevision, String)>,
}

impl BuildFailureState {
    fn record(&mut self, revision: SourceRevision, message: String) {
        if self
            .latest
            .as_ref()
            .is_some_and(|(latest_revision, _)| *latest_revision > revision)
        {
            return;
        }
        self.latest = Some((revision, message));
    }

    fn clear_through(&mut self, revision: SourceRevision) {
        if self
            .latest
            .as_ref()
            .is_some_and(|(latest_revision, _)| *latest_revision <= revision)
        {
            self.latest = None;
        }
    }

    fn message_for(&self, revision: SourceRevision) -> Option<&str> {
        self.latest
            .as_ref()
            .filter(|(failed_revision, _)| *failed_revision == revision)
            .map(|(_, message)| message.as_str())
    }

    fn project_diagnostic_for(
        &self,
        revision: SourceRevision,
    ) -> Option<crate::bex_lsp::ProjectDiagnostic> {
        self.message_for(revision)
            .map(|message| crate::bex_lsp::ProjectDiagnostic {
                severity: "error",
                message: format!("Current build failed: {message}"),
            })
    }
}

/// Per-file `result_id` + last-sent encoded token array, keyed by file path.
type SemanticTokensCache = std::sync::Arc<
    std::sync::Mutex<
        std::collections::HashMap<crate::fs::FsPath, (String, Vec<lsp_types::SemanticToken>)>,
    >,
>;

#[derive(Clone)]
struct BexMultiProject {
    projects:
        std::sync::Arc<std::sync::Mutex<HashMap<crate::fs::FsPath, std::sync::Arc<LiveProject>>>>,
    sys_op_factory: SysOpFactory,
    sender: std::sync::Arc<dyn LspClientSenderTrait + Send + Sync>,
    playground_sender: std::sync::Arc<dyn crate::bex_lsp::PlaygroundSender>,

    /// The position encoding negotiated during `initialize`: UTF-8 when
    /// the client offers it, otherwise UTF-16. Set exactly once; reading
    /// before negotiation never freezes a default.
    negotiated_encoding: std::sync::Arc<OnceLock<PositionEncoding>>,

    /// Whether the client advertised completion-item snippet support during
    /// `initialize`. Connection-scoped because capabilities belong to one
    /// client session.
    snippet_support: std::sync::Arc<OnceLock<bool>>,

    /// Workspace root directories provided by the LSP client during
    /// `initialize`. Used by `on_notification_initialized` to scope
    /// project discovery instead of walking the entire filesystem.
    workspace_roots: std::sync::Arc<std::sync::Mutex<Vec<vfs::VfsPath>>>,

    /// The VFS path to the project root.
    fs: crate::fs::BamlVFS,

    spawner: BackgroundSpawner,

    /// Per-file cache of the last semantic tokens returned (its `result_id` and
    /// the encoded token array), so `semanticTokens/full/delta` can reply with
    /// only the changed edits instead of the whole array.
    ///
    /// Connection-scoped, like `negotiated_encoding`: the cached arrays are
    /// encoded in this connection's negotiated encoding, and each client owns
    /// its delta baseline — a shared cache would serve tokens encoded for the
    /// wrong client and let concurrent sessions trample each other's
    /// `previous_result_id`.
    semantic_tokens_cache: SemanticTokensCache,
    /// Monotonic source of semantic-token `result_id`s. Process-global on
    /// purpose: ids only ever need to be unique, never dense, so one sequence
    /// shared across connections keeps them unambiguous in logs.
    semantic_tokens_seq: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

pub trait LspClientSenderTrait {
    fn send_notification(&self, msg: lsp_server::Notification) -> Result<(), LspError>;
    fn send_response_impl(&self, msg: lsp_server::Response) -> Result<(), LspError>;
    /// The one request error-code mapping boundary. Every error is
    /// serialized through [`LspError::to_response_error`]; `-32001` is dead.
    fn send_response(
        &self,
        id: lsp_server::RequestId,
        msg: Result<serde_json::Value, LspError>,
    ) -> Result<(), LspError> {
        let (result, error) = match msg {
            Err(error) => (None, Some(error.to_response_error())),
            Ok(result) => (Some(result), None),
        };
        let response = lsp_server::Response { id, result, error };
        self.send_response_impl(response)
    }
    fn make_request(&self, msg: lsp_server::Request) -> Result<(), LspError>;
}

/// Map a bounded database read failure to its typed LSP error:
/// timeout with a revision change is `ContentModified`, a same-revision
/// timeout is `RequestFailed`, poison is `InternalError`.
pub(super) fn db_read_error_to_lsp(e: DbReadError) -> LspError {
    match e {
        DbReadError::Busy {
            revision_changed: true,
        } => LspError::ContentModified(
            "sources changed while the request waited for the project database".to_string(),
        ),
        DbReadError::Busy {
            revision_changed: false,
        } => LspError::RequestFailed(
            "project database is busy; the request timed out waiting".to_string(),
        ),
        DbReadError::Broken => {
            LspError::Internal("project is in a broken state (poisoned lock)".to_string())
        }
    }
}

/// Bounded request-lane read of a project's source gate.
///
/// Cancellation safe point: right after the gate is acquired — the request
/// may have waited up to the bounded deadline — the ambient dispatch
/// cancellation is checked, so a request whose cancellation already owns the
/// response stops here instead of paying for the database read. The token is
/// observed, not unwound (abort-profile invariant).
pub(super) fn read_for_request(project: &BexProject) -> Result<SourceGuard<'_>, LspError> {
    let guard = project
        .read_source_for_request()
        .map_err(db_read_error_to_lsp)?;
    if crate::bex_lsp::request_cancellation::current_request_is_cancelled() {
        return Err(LspError::RequestCanceled(
            "request canceled after acquiring the source gate".to_string(),
        ));
    }
    Ok(guard)
}

enum ProjectRefreshMode {
    Full,
    /// Re-apply open-editor buffers. When `changed` is set, only that file is
    /// re-applied (a didChange touches exactly one document); `None` re-applies
    /// every open buffer.
    InMemoryChangesOnly {
        changed: Option<vfs::VfsPath>,
    },
    /// Documents were closed: re-apply their on-disk content and drop their
    /// open-document versions in the same batch.
    ClosedDocuments(Vec<vfs::VfsPath>),
}

impl BexMultiProject {
    fn new(
        sys_op_factory: SysOpFactory,
        sender: std::sync::Arc<dyn LspClientSenderTrait + Send + Sync>,
        playground_sender: std::sync::Arc<dyn crate::bex_lsp::PlaygroundSender>,
        fs: crate::fs::BamlVFS,
        spawner: BackgroundSpawner,
    ) -> Self {
        Self {
            projects: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            sys_op_factory,
            sender,
            playground_sender,
            negotiated_encoding: std::sync::Arc::new(OnceLock::new()),
            snippet_support: std::sync::Arc::new(OnceLock::new()),
            workspace_roots: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            fs,
            spawner,
            semantic_tokens_cache: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            semantic_tokens_seq: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Connection-scoped dispatcher: shares the process-owned project
    /// registry (and everything hanging off it) but owns fresh capability
    /// negotiation, fresh initialize workspace roots, and a fresh
    /// semantic-token delta cache (encoding-dependent), and writes only
    /// through the connection's revocable `sender`.
    /// After browser takeover revokes that sender, a retained clone of
    /// this session fails `send_*` with `ClientClosed` instead of leaking
    /// into the replacement session.
    fn connection_scoped_lsp_session(
        &self,
        sender: std::sync::Arc<dyn LspClientSenderTrait + Send + Sync>,
    ) -> Self {
        let mut session = self.clone();
        session.sender = sender;
        session.negotiated_encoding = std::sync::Arc::new(OnceLock::new());
        session.snippet_support = std::sync::Arc::new(OnceLock::new());
        session.workspace_roots = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        session.semantic_tokens_cache =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        session
    }

    /// Encoding for request handlers: requests before `initialize` completes
    /// are a protocol error and must not freeze a default.
    fn encoding_for_request(&self) -> Result<PositionEncoding, LspError> {
        self.negotiated_encoding.get().copied().ok_or_else(|| {
            LspError::ServerNotInitialized(
                "position encoding has not been negotiated yet".to_string(),
            )
        })
    }

    /// Encoding for server-initiated publications. Pre-negotiation
    /// publications (CLI-seeded workspaces) fall back to UTF-16 *without*
    /// initializing the cell, so a later `initialize` still selects freely.
    fn encoding_for_publication(&self) -> PositionEncoding {
        match self.negotiated_encoding.get() {
            Some(e) => *e,
            None => {
                log::debug!("publishing before encoding negotiation; using UTF-16 for this batch");
                PositionEncoding::UTF16
            }
        }
    }

    fn negotiate_encoding(
        &self,
        client_capabilities: &lsp_types::ClientCapabilities,
    ) -> PositionEncoding {
        let offered = client_capabilities
            .general
            .as_ref()
            .and_then(|g| g.position_encodings.as_deref());
        let selected = PositionEncoding::negotiate(offered);
        // First negotiation wins; a duplicate initialize cannot flip it.
        let _ = self.negotiated_encoding.set(selected);
        *self
            .negotiated_encoding
            .get()
            .expect("negotiated encoding was just set")
    }

    fn negotiate_snippet_support(
        &self,
        client_capabilities: &lsp_types::ClientCapabilities,
    ) -> bool {
        let supported = client_capabilities
            .text_document
            .as_ref()
            .and_then(|text_document| text_document.completion.as_ref())
            .and_then(|completion| completion.completion_item.as_ref())
            .and_then(|completion_item| completion_item.snippet_support)
            .unwrap_or(false);
        // First negotiation wins; a duplicate initialize cannot flip it.
        let _ = self.snippet_support.set(supported);
        *self
            .snippet_support
            .get()
            .expect("snippet support was just set")
    }

    fn snippet_support_for_request(&self) -> Result<bool, LspError> {
        self.snippet_support.get().copied().ok_or_else(|| {
            LspError::ServerNotInitialized(
                "completion capabilities have not been negotiated yet".to_string(),
            )
        })
    }

    fn get_path_from_uri(&self, uri: &lsp_types::Url) -> Result<vfs::VfsPath, LspError> {
        let path = wasm_helpers::to_file_path(uri)
            .map_err(|()| LspError::InvalidParams(format!("URI is not a file path: {uri}")))?;
        self.fs
            .get_path_from_platform_path(&path, "get_path_from_uri")
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
        let project = crate::project::BexProject::new(&root_path, sys_ops);
        let project = std::sync::Arc::new(LiveProject::new(project));
        projects.insert(crate::fs::FsPath::from_vfs(&root_path), project.clone());
        Ok(project)
    }

    fn find_project(&self, project_root_str: &str) -> Option<std::sync::Arc<LiveProject>> {
        let projects = self.projects.lock().unwrap();
        projects
            .iter()
            .find(|(k, _)| k.as_path().to_string_lossy() == project_root_str)
            .map(|(_, v)| v.clone())
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

        for project_root in &project_roots {
            let Ok(_) = self.get_or_create_project(project_root.clone()) else {
                continue;
            };
            self.refresh_project(project_root, ProjectRefreshMode::Full);
        }

        project_roots
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
        let os_root = VfsPathBuf::new(root.as_str().to_string())
            .and_then(|root| NativePathBuf::try_from(&root));
        if let Ok(os_root) = os_root
            && os_root.as_path().is_dir()
        {
            return self.collect_marked_project_roots_native(os_root.as_path());
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
                .get_path_from_platform_path(&dir, "discover_workspace_projects")
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

    // ── Refresh pipeline ─────────────────────────────────────────────────

    /// Apply one editor/watcher/playground event as a source batch, then
    /// schedule the diagnostics and engine tails. `didChange` cost is the
    /// batch apply plus timer resets; all computation is debounced.
    fn refresh_project(&self, project_root: &vfs::VfsPath, refresh_mode: ProjectRefreshMode) {
        use crate::bex_lsp::notification::BexLspNotification;
        let mode_label = match &refresh_mode {
            ProjectRefreshMode::Full => "Full",
            ProjectRefreshMode::InMemoryChangesOnly { .. } => "InMemoryChangesOnly",
            ProjectRefreshMode::ClosedDocuments(_) => "ClosedDocuments",
        };
        tracing::debug!(
            "refresh_project({}, mode={})",
            project_root.as_str(),
            mode_label
        );

        let Ok(project) = self.get_or_create_project(project_root.to_owned()) else {
            return;
        };

        let batch = match self.build_source_batch(project_root, &project, refresh_mode) {
            Ok(batch) => batch,
            Err(e) => {
                let _ = self.send_notification_show_message(lsp_types::ShowMessageParams {
                    typ: lsp_types::MessageType::ERROR,
                    message: format!("Failed to read project files for {project_root:?}: {e}"),
                });
                return;
            }
        };

        let Ok(revision) = project.project.mutate_sources(batch) else {
            log::error!("refresh_project: project is broken; dropping refresh");
            return;
        };

        // Catalog is never gated (B4): announce on every refresh.
        self.send_list_projects();

        project
            .diagnostics_fence
            .lock()
            .unwrap()
            .mark_dirty(revision);

        #[cfg(not(target_arch = "wasm32"))]
        {
            self.schedule_diagnostics_tail(project_root, &project);
            self.schedule_engine_rebuild(project_root, &project);
        }

        // WASM has no timers or blocking facilities wired here: run the
        // whole tail synchronously (single-threaded, so never busy).
        #[cfg(target_arch = "wasm32")]
        self.run_project_tail_blocking(project_root, &project);

        tracing::debug!("refresh_project done");
    }

    /// Build the atomic source batch for a refresh mode: texts plus the
    /// open-document version ops that must commit with them.
    fn build_source_batch(
        &self,
        project_root: &vfs::VfsPath,
        project: &LiveProject,
        refresh_mode: ProjectRefreshMode,
    ) -> Result<SourceBatch, LspError> {
        match refresh_mode {
            ProjectRefreshMode::Full => {
                let mut sources = self.load_project_sources(project_root)?;
                let mut versions = Vec::new();
                {
                    let overlay = project.in_memory_changes.lock().unwrap();
                    for (path, doc) in overlay.iter() {
                        sources.insert(path.clone(), doc.text.clone());
                        versions.push((path.clone(), doc.version));
                    }
                }
                Ok(SourceBatch {
                    replace_all: true,
                    sources,
                    versions,
                })
            }
            ProjectRefreshMode::InMemoryChangesOnly { changed } => {
                let overlay = project.in_memory_changes.lock().unwrap();
                // A didChange names the one document that changed; re-applying
                // every open buffer would dirty their whole query chains.
                let selected: Vec<(crate::fs::FsPath, &OverlayDocument)> = match &changed {
                    Some(path) => {
                        let key = crate::fs::FsPath::from_vfs(path);
                        overlay
                            .get(&key)
                            .map(|doc| (key, doc))
                            .into_iter()
                            .collect()
                    }
                    None => overlay
                        .iter()
                        .map(|(path, doc)| (path.clone(), doc))
                        .collect(),
                };
                let mut sources = HashMap::new();
                let mut versions = Vec::new();
                for (path, doc) in selected {
                    sources.insert(path.clone(), doc.text.clone());
                    versions.push((path, doc.version));
                }
                Ok(SourceBatch {
                    replace_all: false,
                    sources,
                    versions,
                })
            }
            ProjectRefreshMode::ClosedDocuments(paths) => {
                let disk_sources = self.load_project_sources(project_root)?;
                let mut sources = HashMap::new();
                let mut versions = Vec::new();
                for path in paths {
                    let key = crate::fs::FsPath::from_vfs(&path);
                    if let Some(text) = disk_sources.get(&key) {
                        sources.insert(key.clone(), text.clone());
                    }
                    versions.push((key, None));
                }
                Ok(SourceBatch {
                    replace_all: false,
                    sources,
                    versions,
                })
            }
        }
    }

    // ── Diagnostics tail ─────────────────────────────────────────────────

    /// Debounced diagnostics/project-update tail. The epoch only suppresses
    /// superseded timers; staleness of results is decided by the fence.
    #[cfg(not(target_arch = "wasm32"))]
    fn schedule_diagnostics_tail(
        &self,
        project_root: &vfs::VfsPath,
        project: &std::sync::Arc<LiveProject>,
    ) {
        use std::sync::atomic::Ordering;
        let epoch = project.diagnostics_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        let this = self.clone();
        let project = project.clone();
        let project_root = project_root.clone();
        self.spawner.spawn(async move {
            tokio::time::sleep(DIAGNOSTICS_DEBOUNCE).await;
            if project.diagnostics_epoch.load(Ordering::SeqCst) != epoch {
                return;
            }
            let compute_project = project.clone();
            let outcome = tokio::task::spawn_blocking(move || {
                compute_project.project.diagnostic_candidate_nowait()
            })
            .await
            .unwrap_or(Ok(None));
            this.handle_diagnostics_outcome(&project_root, &project, outcome);

            // The playground project snapshot rides the same debounced tail:
            // emit one owned payload per quiet period, not one per
            // keystroke.
            this.send_update_project(project_root.as_str(), &project);
        });
    }

    /// Apply one diagnostics computation outcome to the fence.
    #[cfg(not(target_arch = "wasm32"))]
    fn handle_diagnostics_outcome(
        &self,
        project_root: &vfs::VfsPath,
        project: &std::sync::Arc<LiveProject>,
        outcome: Result<Option<DiagnosticCandidate>, crate::project::ProjectBroken>,
    ) {
        match outcome {
            Err(_) => {
                // Poison is an internal failure, never an empty publication
                // The project is terminally broken; do not retry.
                log::error!(
                    "diagnostics: project {} is broken; keeping last publication",
                    project_root.as_str()
                );
            }
            Ok(None) => {
                self.schedule_diagnostics_busy_retry(project_root, project);
            }
            Ok(Some(candidate)) => {
                self.publish_candidate(&candidate, project);
            }
        }
    }

    /// Busy: keep the last publication and re-arm one trailing retry.
    /// The retry is fence-tagged, so it publishes only if still relevant.
    #[cfg(not(target_arch = "wasm32"))]
    fn schedule_diagnostics_busy_retry(
        &self,
        project_root: &vfs::VfsPath,
        project: &std::sync::Arc<LiveProject>,
    ) {
        {
            let mut fence = project.diagnostics_fence.lock().unwrap();
            if fence.dirty.is_none() || fence.retry_scheduled {
                return;
            }
            fence.retry_scheduled = true;
        }
        let this = self.clone();
        let project = project.clone();
        let project_root = project_root.clone();
        self.spawner.spawn(async move {
            tokio::time::sleep(DIAGNOSTICS_BUSY_RETRY).await;
            {
                let mut fence = project.diagnostics_fence.lock().unwrap();
                fence.retry_scheduled = false;
                if fence.dirty.is_none() {
                    return;
                }
            }
            let compute_project = project.clone();
            let outcome = tokio::task::spawn_blocking(move || {
                compute_project.project.diagnostic_candidate_nowait()
            })
            .await
            .unwrap_or(Ok(None));
            this.handle_diagnostics_outcome(&project_root, &project, outcome);
        });
    }

    /// Conditionally publish an owned candidate through the fence:
    /// stale candidates are discarded; publication compare-and-clears the
    /// dirty revision, so a newer mutation that raced in stays scheduled.
    fn publish_candidate(
        &self,
        candidate: &DiagnosticCandidate,
        project: &std::sync::Arc<LiveProject>,
    ) {
        use crate::bex_lsp::notification::BexLspNotification;

        let encoding = self.encoding_for_publication();
        let documents = candidate_to_publishable(candidate, encoding);

        // The current revision is read under the fence lock: publications
        // serialize on it, so a candidate admitted here can never regress
        // one already admitted for a newer revision.
        let mut fence = project.diagnostics_fence.lock().unwrap();
        if !fence.admit(
            candidate.source_revision,
            project.project.current_revision(),
        ) {
            // A newer mutation owns the next publication; its tail is
            // already scheduled. Publishing this would regress markers.
            return;
        }

        let current_paths: HashSet<std::path::PathBuf> =
            documents.iter().map(|d| d.path.clone()).collect();

        for doc in &documents {
            let Ok(uri) = wasm_helpers::from_file_path(&doc.path) else {
                continue;
            };
            let _ = self.send_notification_publish_diagnostics(
                lsp_types::PublishDiagnosticsParams::new(uri, doc.diagnostics.clone(), doc.version),
            );
        }

        // Clear markers for files that disappeared since the last successful
        // publication (busy/stale attempts never update this set).
        for deleted in fence.record_publication(current_paths) {
            let Ok(uri) = wasm_helpers::from_file_path(&deleted) else {
                continue;
            };
            let _ = self.send_notification_publish_diagnostics(
                lsp_types::PublishDiagnosticsParams::new(uri, vec![], None),
            );
        }
    }

    // ── Engine tail ──────────────────────────────────────────────────────

    /// Debounced engine rebuild: bytecode generation, `BexEngine::new` (which
    /// executes `$init`), and test collection are the heavy tail of a refresh.
    /// The debounce epoch is a pre-work ticket; the rebuild gate makes builds
    /// single-flight; installation is authorized only by the
    /// revision-conditional commit inside `rebuild_once`.
    #[cfg(not(target_arch = "wasm32"))]
    fn schedule_engine_rebuild(
        &self,
        project_root: &vfs::VfsPath,
        project: &std::sync::Arc<LiveProject>,
    ) {
        use std::sync::atomic::Ordering;

        let epoch = project.rebuild_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        let this = self.clone();
        let project = project.clone();
        let project_root = project_root.clone();
        self.spawner.spawn(async move {
            tokio::time::sleep(ENGINE_REBUILD_DEBOUNCE).await;
            if project.rebuild_epoch.load(Ordering::SeqCst) != epoch {
                // A newer refresh superseded this one; its own rebuild is scheduled.
                return;
            }
            let _flight = project.rebuild_gate.lock().await;
            if project.rebuild_epoch.load(Ordering::SeqCst) != epoch {
                return;
            }
            let rebuild_project = project.clone();
            let Ok(report) =
                tokio::task::spawn_blocking(move || rebuild_project.project.rebuild_once()).await
            else {
                log::error!("engine rebuild task panicked");
                return;
            };
            this.apply_rebuild_report(&project_root, &project, report);
        });
    }

    /// Publish a rebuild's diagnostics through the fence and, on a winning
    /// commit, announce the ready runtime state and collect tests. Superseded
    /// candidates change nothing.
    fn apply_rebuild_report(
        &self,
        project_root: &vfs::VfsPath,
        project: &std::sync::Arc<LiveProject>,
        report: crate::project::RebuildReport,
    ) {
        if let Some(candidate) = &report.diagnostics {
            self.publish_candidate(candidate, project);
        }

        match report.engine {
            EngineBuildOutcome::Committed(receipt) => {
                project
                    .build_failure
                    .lock()
                    .unwrap()
                    .clear_through(receipt.source_revision);
                log::info!(
                    "engine rebuild: generation {} committed at {}",
                    receipt.generation,
                    receipt.source_revision
                );
                self.send_update_project(project_root.as_str(), project);
                self.collect_tests_for_project(project_root.as_str(), project);
            }
            EngineBuildOutcome::BlockedByDiagnostics { source_revision } => {
                project
                    .build_failure
                    .lock()
                    .unwrap()
                    .clear_through(source_revision);
                log::info!("engine rebuild: blocked by diagnostics at {source_revision}");
                self.send_update_project(project_root.as_str(), project);
            }
            EngineBuildOutcome::Failed {
                source_revision,
                message,
            } => {
                project
                    .build_failure
                    .lock()
                    .unwrap()
                    .record(source_revision, message.clone());
                log::warn!("engine rebuild failed at {source_revision}: {message}");
                self.send_update_project(project_root.as_str(), project);
            }
            EngineBuildOutcome::Superseded { current_revision } => {
                log::info!(
                    "engine rebuild superseded by {current_revision}; candidate dropped quietly"
                );
            }
            EngineBuildOutcome::Broken => {
                log::error!("engine rebuild: project is broken");
            }
        }
    }

    /// Synchronous project tail for WASM (no timers/blocking facilities):
    /// rebuild, publish diagnostics, announce, and collect.
    #[cfg(target_arch = "wasm32")]
    fn run_project_tail_blocking(
        &self,
        project_root: &vfs::VfsPath,
        project: &std::sync::Arc<LiveProject>,
    ) {
        let report = project.project.rebuild_once();
        self.apply_rebuild_report(project_root, project, report);
    }

    // ── Playground state ─────────────────────────────────────────────────

    fn flatten_diagnostics(
        documents: &[PublishableDocument],
    ) -> Vec<crate::bex_lsp::ProjectDiagnostic> {
        let mut out = Vec::new();
        for doc in documents {
            let filename = doc
                .path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            for d in &doc.diagnostics {
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

    /// Build one owned, complete `ProjectUpdate` payload from the caller's
    /// source lease. The caller keeps that lease through notification
    /// publication so an edit cannot make this snapshot obsolete mid-send.
    fn build_project_update(
        &self,
        guard: &crate::project::SourceGuard<'_>,
        is_bex_current: bool,
        generation: u64,
    ) -> crate::bex_lsp::ProjectUpdate {
        let candidate = crate::project::collect_diagnostic_candidate(guard);
        let listing = baml_project::list_functions_with_metadata(guard.db());
        let tests_listing = baml_project::list_tests_with_metadata(guard.db());

        let documents = candidate_to_publishable(&candidate, self.encoding_for_publication());
        let diagnostics = Self::flatten_diagnostics(&documents);

        let functions = listing
            .functions
            .into_iter()
            .map(|f| crate::bex_lsp::FunctionInfo {
                name: f.name,
                signature: f.signature,
                source_position: crate::bex_lsp::FunctionSourcePosition {
                    file: f.source_position.file,
                    line: f.source_position.line,
                    column: f.source_position.column,
                },
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
        let tests = tests_listing
            .into_iter()
            .map(|test| crate::bex_lsp::TestInfo {
                name: test.name,
                function_name: test.function_name,
                args_json: test.args_json,
            })
            .collect();

        crate::bex_lsp::ProjectUpdate {
            is_bex_current,
            generation,
            functions,
            tests,
            types: Some(listing.types),
            diagnostics,
        }
    }

    fn send_list_projects(&self) {
        let projects = self.projects.lock().unwrap();
        let roots: Vec<String> = projects
            .keys()
            .map(|p| p.as_path().to_string_lossy().into_owned())
            .collect();
        drop(projects);
        self.playground_sender.send_playground_notification(
            crate::bex_lsp::PlaygroundNotification::ListProjects { projects: roots },
        );
    }

    fn send_update_project(&self, project_root: &str, project: &LiveProject) {
        let Ok(guard) = project.project.read_source_for_request() else {
            log::debug!("skipping UpdateProject for busy project {project_root}");
            return;
        };
        let source_revision = guard.revision();
        let (is_bex_current, generation) =
            project.project.runtime_status_for_source(source_revision);
        let mut update = self.build_project_update(&guard, is_bex_current, generation);

        // Keep the failure read and notification publication in one critical
        // section. Otherwise a diagnostics-tail snapshot that read "no
        // failure" could publish after the engine tail recorded and sent a
        // failure for the same revision, regressing the UI to its transient
        // preparing state.
        let build_failure = project.build_failure.lock().unwrap();
        if let Some(diagnostic) = build_failure.project_diagnostic_for(source_revision) {
            update.diagnostics.push(diagnostic);
        }
        self.playground_sender.send_playground_notification(
            crate::bex_lsp::PlaygroundNotification::UpdateProject {
                project: project_root.to_string(),
                update,
            },
        );
        drop(build_failure);
        drop(guard);
    }

    // ── Test collection / runs ───────────────────────────────────────────

    fn request_collect_tests_impl(&self, project_root_str: &str) {
        let Some(project) = self.find_project(project_root_str) else {
            return;
        };
        self.collect_tests_for_project(project_root_str, &project);
    }

    /// Start one test-collection attempt against the installed engine. The
    /// ticket captures engine, generation, cancel token, and collection
    /// epoch atomically; installation and every emission are fenced by that
    /// identity, so stale collections emit nothing.
    fn collect_tests_for_project(
        &self,
        project_root_str: &str,
        project: &std::sync::Arc<LiveProject>,
    ) {
        log::info!("[collect_tests] project={project_root_str}");
        let ticket = match project.project.begin_test_collection() {
            Ok(Some(ticket)) => ticket,
            Ok(None) => {
                log::info!("[collect_tests] no current engine; skipping");
                return;
            }
            Err(_) => return,
        };

        let sender = self.playground_sender.clone();
        let live = project.clone();
        let project = project_root_str.to_string();
        let package = "user".to_string();
        let call_id = sys_types::CallId::next();
        let generation = ticket.generation;
        let cancel = ticket.cancel.clone();
        let engine = ticket.engine.clone();

        self.spawner.spawn(async move {
            match engine
                .collect_tests(&package, call_id, cancel.clone())
                .await
            {
                Ok(registry) => {
                    // Extract Handle from BexExternalValue::Handle.
                    // Null means the project has no tests ($init_test absent).
                    let handle = match &registry {
                        bex_engine::BexExternalValue::Handle(h) => Some(h.clone()),
                        bex_engine::BexExternalValue::Null => None,
                        _ => {
                            log::error!("[collect_tests] unexpected result type");
                            return;
                        }
                    };

                    // ABA fence: install only if the engine generation and
                    // collection epoch still match; stale results emit nothing.
                    match live.project.install_collected_registry(&ticket, handle) {
                        Ok(true) => {}
                        Ok(false) => {
                            log::info!(
                                "[collect_tests] discarding stale result (gen {generation})"
                            );
                            return;
                        }
                        Err(_) => return,
                    }

                    // If the project has no tests, send an empty test tree.
                    if matches!(registry, bex_engine::BexExternalValue::Null) {
                        sender.send_playground_notification(
                            crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                                project,
                                generation,
                                call_id: call_id.0,
                                data: serde_json::to_vec(&serde_json::json!([]))
                                    .unwrap_or_default(),
                                expand_error: None,
                            },
                        );
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
                            // Emission is fenced too: if a newer engine or
                            // collection superseded us during serialize, stay
                            // silent (the newer attempt owns the tree).
                            if !live.project.collection_ticket_is_current(&ticket) {
                                return;
                            }
                            let data = serde_json::to_vec(&bex_value_to_json(&serialized))
                                .unwrap_or_default();
                            sender.send_playground_notification(
                                crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                                    project,
                                    generation,
                                    call_id: call_id.0,
                                    data,
                                    expand_error: None,
                                },
                            );
                        }
                        Err(e) => {
                            log::error!("[collect_tests] serialize failed: {e}");
                            if !live.project.collection_ticket_is_current(&ticket) {
                                return;
                            }
                            sender.send_playground_notification(
                                crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                                    project: project.clone(),
                                    generation,
                                    call_id: call_id.0,
                                    data: serde_json::to_vec(&serde_json::json!([]))
                                        .unwrap_or_default(),
                                    expand_error: None,
                                },
                            );
                        }
                    }
                }
                Err(e) => {
                    // Stale/canceled collection emits nothing; a failure for
                    // the still-current build unblocks the frontend with an
                    // empty result.
                    log::error!("[collect_tests] collect_tests failed: {e}");
                    if !live.project.collection_ticket_is_current(&ticket) {
                        return;
                    }
                    sender.send_playground_notification(
                        crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                            project,
                            generation,
                            call_id: call_id.0,
                            data: serde_json::to_vec(&serde_json::json!([])).unwrap_or_default(),
                            expand_error: None,
                        },
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
        ctx: bex_engine::FunctionCallContext,
    ) -> Result<bex_engine::BexCallResult, bex_engine::EngineError> {
        let project = self.find_project(project_root_str).ok_or_else(|| {
            bex_engine::EngineError::FunctionNotFound {
                name: format!("project not found: {project_root_str}"),
            }
        })?;

        // One coherent lease validates the generation against the installed
        // engine, requires current source, and captures the engine + registry
        // handle atomically.
        let lease = project.project.lease_registry(generation).map_err(|e| {
            bex_engine::EngineError::FunctionNotFound {
                name: registry_lease_error_message(e).to_string(),
            }
        })?;

        log::info!("[call_test_function] test_name={test_name} generation={generation}");

        let result = lease
            .engine
            .call_function_with_trace(
                "testing.TestRegistry.run_test",
                vec![
                    bex_engine::BexExternalValue::Handle(lease.handle.clone()),
                    bex_engine::BexExternalValue::String(test_name.into()),
                ],
                ctx,
                true, // deep copy TestReport for wire
            )
            .await;

        match &result {
            Ok(_) => log::info!("[call_test_function] test_name={test_name} succeeded"),
            Err(e) => log::error!("[call_test_function] test_name={test_name} failed: {e}"),
        }

        result
    }

    fn expand_test_set_impl(&self, project_root_str: &str, generation: u64, testset_name: &str) {
        let Some(project) = self.find_project(project_root_str) else {
            return;
        };
        // Stale expansion requests emit nothing: the newer collection owns
        // the tree. Source changes cancel stale tree maintenance.
        let lease = match project.project.lease_registry(generation) {
            Ok(lease) => lease,
            Err(e) => {
                log::info!(
                    "[expand_test_set] not expanding '{testset_name}': {}",
                    registry_lease_error_message(e)
                );
                return;
            }
        };

        let call_id = sys_types::CallId::next();
        let sender = self.playground_sender.clone();
        let live = project;
        let project = project_root_str.to_string();
        let name = testset_name.to_string();

        self.spawner.spawn(async move {
            // One mutation owner per installed registry: expansions
            // mutate the registry heap object in place, so they serialize.
            #[cfg(not(target_arch = "wasm32"))]
            let _mutation_owner = lease.expansion_gate.lock().await;

            let engine = lease.engine.clone();
            let registry_value = bex_engine::BexExternalValue::Handle(lease.handle.clone());
            let cancel = lease.cancel.clone();

            let ctx = bex_engine::FunctionCallContextBuilder::new(call_id)
                .with_cancel_token(cancel.clone())
                .with_profile_enabled(false)
                .build();

            // Expand — mutates registry.expansions in-place on the heap
            log::info!("[expand_test_set] expanding testset: {name}");
            if let Err(e) = engine
                .call_function(
                    "testing.TestRegistry.expand_set",
                    vec![
                        registry_value.clone(),
                        bex_engine::BexExternalValue::String(name.as_str().into()),
                    ],
                    ctx,
                    true,
                )
                .await
            {
                log::error!("[expand_test_set] expand failed for testset '{name}': {e}");
                if cancel.is_cancelled() {
                    // Superseded mid-expansion: emit nothing.
                    return;
                }
                // Re-serialize and send the current (pre-expansion) state so the
                // UI unblocks from the loading spinner instead of spinning forever.
                let ctx_resend =
                    bex_engine::FunctionCallContextBuilder::new(sys_types::CallId::next())
                        .with_cancel_token(cancel)
                        .with_profile_enabled(false)
                        .build();
                let data = match engine
                    .call_function(
                        "testing.TestRegistry.serialize",
                        vec![registry_value],
                        ctx_resend,
                        true,
                    )
                    .await
                {
                    Ok(serialized) => {
                        serde_json::to_vec(&bex_value_to_json(&serialized)).unwrap_or_default()
                    }
                    Err(serialize_err) => {
                        log::error!(
                            "[expand_test_set] serialize after failed expand for '{name}' also failed: {serialize_err}"
                        );
                        serde_json::to_vec(&serde_json::json!([])).unwrap_or_default()
                    }
                };
                if !live.project.registry_lease_is_current(&lease) {
                    return;
                }
                sender.send_playground_notification(
                    crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                        project,
                        generation,
                        call_id: call_id.0,
                        data,
                        expand_error: Some(crate::bex_lsp::TestExpandError {
                            testset_name: name.clone(),
                            message: format!("{e}"),
                        }),
                    },
                );
                return;
            }
            log::info!("[expand_test_set] expanded testset '{name}' successfully");

            // Re-serialize full state
            let ctx2 = bex_engine::FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_cancel_token(cancel)
                .with_profile_enabled(false)
                .build();
            match engine
                .call_function(
                    "testing.TestRegistry.serialize",
                    vec![registry_value],
                    ctx2,
                    true,
                )
                .await
            {
                Ok(serialized) => {
                    // Stale expansion success emits nothing.
                    if !live.project.registry_lease_is_current(&lease) {
                        return;
                    }
                    let data =
                        serde_json::to_vec(&bex_value_to_json(&serialized)).unwrap_or_default();
                    sender.send_playground_notification(
                        crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                            project,
                            generation,
                            call_id: call_id.0,
                            data,
                            expand_error: None,
                        },
                    );
                }
                Err(e) => {
                    log::error!("[expand_test_set] serialize after expanding '{name}' failed: {e}");
                    if !live.project.registry_lease_is_current(&lease) {
                        return;
                    }
                    // Send empty result so the UI unblocks
                    sender.send_playground_notification(
                        crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                            project,
                            generation,
                            call_id: call_id.0,
                            data: serde_json::to_vec(&serde_json::json!([])).unwrap_or_default(),
                            expand_error: None,
                        },
                    );
                }
            }
        });
    }
}

fn registry_lease_error_message(e: RegistryLeaseError) -> &'static str {
    match e {
        RegistryLeaseError::NeedsCurrentBuild => {
            "engine is not current with the latest sources; wait for the rebuild"
        }
        RegistryLeaseError::NoRegistry => "no test registry collected yet",
        RegistryLeaseError::NoTests => "project has no tests",
        RegistryLeaseError::Broken => "project is in a broken state",
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
        let source_root = BexMultiProject::project_source_root(project_root)?;
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

#[async_trait::async_trait]
impl super::BexLsp for BexMultiProject {
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

    fn prepare_function_run(
        &self,
        project_root: &str,
        overlay_function: Option<&str>,
    ) -> Result<super::PreparedRun, LspError> {
        let project = self
            .find_project(project_root)
            .ok_or_else(|| LspError::RequestFailed(format!("project not found: {project_root}")))?;
        let snapshot = project
            .project
            .prepare_function_run(overlay_function)
            .map_err(|e| match e {
                PrepareRunError::NeedsCurrentBuild => LspError::ContentModified(
                    "engine is not current with the latest sources; wait for the rebuild"
                        .to_string(),
                ),
                PrepareRunError::Busy => {
                    LspError::RequestFailed("project is busy; retry shortly".to_string())
                }
                PrepareRunError::Broken => {
                    LspError::Internal("project is in a broken state".to_string())
                }
            })?;
        Ok(super::PreparedRun {
            generation: snapshot.generation,
            engine: snapshot.engine,
        })
    }

    fn engine_for_generation(
        &self,
        project_root: &str,
        generation: u64,
    ) -> Option<Arc<dyn crate::Bex>> {
        let project = self.find_project(project_root)?;
        project
            .project
            .engine_for_generation(generation)
            .map(|engine| engine as Arc<dyn crate::Bex>)
    }

    fn all_env_var_names(&self) -> Vec<String> {
        let projects: Vec<_> = {
            let projects = self.projects.lock().unwrap();
            projects.values().cloned().collect()
        };
        let mut names = std::collections::BTreeSet::new();
        for project in projects {
            // Loop lane: skip busy projects instead of blocking dispatch.
            let Ok(Some(guard)) = project.project.read_source_nowait() else {
                continue;
            };
            for name in baml_lsp2_actions::all_env_var_names(guard.db()) {
                names.insert(name);
            }
        }
        names.into_iter().collect()
    }

    fn playground_source_files(
        &self,
        project: &str,
    ) -> Result<Vec<crate::bex_lsp::PlaygroundSourceFile>, LspError> {
        let project_root = self.fs.get_path_from_vfs_path(
            &crate::fs::FsPath::from_str(project.to_string()),
            "playground source files",
        )?;
        let project_handle = self.get_or_create_project(project_root.clone())?;
        let mut sources = self.load_project_sources(&project_root)?;
        {
            let in_memory_changes = project_handle.in_memory_changes.lock().unwrap();
            for (path, doc) in in_memory_changes.iter() {
                sources.insert(path.clone(), doc.text.clone());
            }
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
        let project_root = self.fs.get_path_from_vfs_path(
            &crate::fs::FsPath::from_str(project.to_string()),
            "playground update source file",
        )?;
        let source_path = if path.starts_with('/') {
            self.fs.get_path_from_vfs_path(
                &crate::fs::FsPath::from_str(path.to_string()),
                "playground update source file path",
            )?
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
        let mut in_memory_changes = project_handle.in_memory_changes.lock().unwrap();
        // Playground edits are unversioned; a previously known editor version
        // no longer describes this text.
        in_memory_changes.insert(
            crate::fs::FsPath::from_vfs(&source_path),
            OverlayDocument {
                text: content,
                version: None,
            },
        );
        drop(in_memory_changes);

        self.refresh_project(
            &project_root,
            ProjectRefreshMode::InMemoryChangesOnly {
                changed: Some(source_path),
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
            .map(|root| {
                self.fs
                    .get_path_from_platform_path(&root, "lsp --workspace")
            })
            .collect::<Result<Vec<_>, _>>()?;
        let projects = self.discover_workspace_projects(&roots);
        Ok(projects
            .into_iter()
            .map(|project| project.as_str().to_string())
            .collect())
    }

    fn request_playground_state(&self) {
        self.send_list_projects();
        let projects: Vec<(crate::fs::FsPath, std::sync::Arc<LiveProject>)> = {
            let projects = self.projects.lock().unwrap();
            projects
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        for (fs_path, project) in projects {
            let root = fs_path.as_path().to_string_lossy();
            self.send_update_project(&root, &project);
        }
    }

    fn ast_control_flow_graph(
        &self,
        function_name: &str,
    ) -> Option<baml_compiler2_visualization::control_flow::ControlFlowGraph> {
        let projects: Vec<_> = {
            let projects = self.projects.lock().ok()?;
            projects.values().cloned().collect()
        };
        for project in projects {
            let Ok(guard) = read_for_request(&project.project) else {
                continue;
            };
            if let Some(graph) = guard.db().ast_control_flow_graph(function_name) {
                return Some(graph);
            }
        }
        None
    }

    fn project_generation(&self, project_root: &str) -> Option<u64> {
        let project = self.find_project(project_root)?;
        Some(project.project.current_generation())
    }

    fn control_flow_graph_for_generation(
        &self,
        project_root: &str,
        generation: u64,
        function_name: &str,
    ) -> Option<std::sync::Arc<baml_compiler2_visualization::control_flow::ControlFlowGraph>> {
        // Clone the project handle out of the registry lock: building a
        // missing graph takes the project's source gate, which must not be
        // held while the registry lock is.
        let project = self.find_project(project_root)?;
        project
            .project
            .control_flow_graph_for_generation(generation, function_name)
    }

    fn request_control_flow_graph(&self, function_name: &str, request_id: Option<u32>) {
        let graph = self.ast_control_flow_graph(function_name);
        let graph = graph.map(|g| {
            baml_compiler2_visualization::control_flow::prepare_control_flow_graph_for_visualization(&g)
        });
        let graph_json = graph.as_ref().and_then(|g| serde_json::to_value(g).ok());
        self.playground_sender.send_playground_notification(
            crate::bex_lsp::PlaygroundNotification::ControlFlowGraphResult {
                function_name: function_name.to_string(),
                graph: graph_json,
                request_id,
            },
        );
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

        let projects: Vec<_> = {
            let Ok(projects) = self.projects.lock() else {
                return empty;
            };
            projects.values().cloned().collect()
        };

        for project in projects {
            let Ok(guard) = read_for_request(&project.project) else {
                continue;
            };
            let db = guard.db();

            // The file_path from Monaco may be relative — find matching file.
            let Some(source_file) = db.find_source_file(file_path) else {
                continue;
            };

            // Playground wire coordinates are fixed zero-based UTF-16 (C2),
            // independent of the negotiated LSP encoding.
            let text: &str = source_file.text(db);
            let codec =
                crate::bex_lsp::position_codec::PositionCodec::new(text, PositionEncoding::UTF16);
            let byte_offset = match codec.position_to_offset(lsp_types::Position {
                line,
                character: column,
            }) {
                Ok(offset) => u32::from(offset),
                Err(_) => 0,
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
        self.request_collect_tests_impl(project);
    }

    async fn call_test_function(
        &self,
        project: &str,
        generation: u64,
        test_name: &str,
        ctx: bex_engine::FunctionCallContext,
    ) -> Result<bex_engine::BexExternalValue, bex_engine::EngineError> {
        self.call_test_function_impl(project, generation, test_name, ctx)
            .await
            .and_then(|result| result.value)
    }

    async fn call_test_function_with_trace(
        &self,
        project: &str,
        generation: u64,
        test_name: &str,
        ctx: bex_engine::FunctionCallContext,
    ) -> Result<bex_engine::BexCallResult, bex_engine::EngineError> {
        self.call_test_function_impl(project, generation, test_name, ctx)
            .await
    }

    fn expand_test_set(&self, project: &str, generation: u64, testset_name: &str) {
        self.expand_test_set_impl(project, generation, testset_name);
    }

    fn resolve_file_id(&self, file_id: u32) -> Option<String> {
        let projects: Vec<_> = {
            let projects = self.projects.lock().unwrap();
            projects.values().cloned().collect()
        };
        for project in projects {
            let Ok(Some(guard)) = project.project.read_source_nowait() else {
                continue;
            };
            if let Some(path) = guard.db().file_id_to_path(baml_base::FileId::new(file_id)) {
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
    BexMultiProject::new(sys_op_factory, sender, playground_sender, fs, spawner)
}

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;

    impl crate::fs::BulkReadFileSystem for vfs::PhysicalFS {
        fn read_many(&self, _glob: &str) -> vfs::VfsResult<Vec<(String, Vec<u8>)>> {
            Ok(Vec::new())
        }
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
                .get_path_from_platform_path(&abs, "test workspace path")
                .unwrap()
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn standalone_baml_file_is_not_promoted_by_strict_resolver() {
        let ws = TempWorkspace::new("standalone_baml_language");
        // Path contains a `baml_language` segment, triggering the lenient
        // internal-dev fallback.
        ws.file("baml_language/case.baml", "// standalone");
        let file = ws.vfs_path("baml_language/case.baml");

        let lenient = BexMultiProject::get_baml_project_root(&file).unwrap();
        assert_eq!(lenient.as_str(), file.as_str());

        let strict = BexMultiProject::get_marked_baml_project_root(&file);
        assert!(matches!(strict, Err(LspError::ProjectRootNotFound(..))));
    }

    #[test]
    fn strict_resolver_finds_marked_project_root() {
        let ws = TempWorkspace::new("marked_root");
        ws.file("proj/baml_src/main.baml", "// main");
        let file = ws.vfs_path("proj/baml_src/main.baml");

        let root = BexMultiProject::get_marked_baml_project_root(&file).unwrap();
        assert_eq!(root.as_str(), ws.vfs_path("proj").as_str());
    }

    #[test]
    fn native_scan_skips_generated_and_hidden_dirs() {
        let ws = TempWorkspace::new("scan_skips");
        ws.file("proj/baml_src/main.baml", "// main");
        ws.dir("target/junk/baml_src");
        ws.dir("node_modules/pkg/baml_src");
        ws.dir(".hidden/baml_src");

        let found = BexMultiProject::scan_marked_project_roots_native(&ws.root);
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

        let found = BexMultiProject::scan_marked_project_roots_native(&ws.root);
        assert_eq!(
            found,
            vec![ws.root.join("app")],
            "gitignored directories must not be discovered"
        );
    }

    #[test]
    fn diagnostics_fence_publishes_current_and_discards_stale() {
        let mut fence = DiagnosticsFence::default();

        // r1 dirty, candidate at r1: publish and clear.
        fence.mark_dirty(SourceRevision(1));
        assert!(fence.admit(SourceRevision(1), SourceRevision(1)));
        assert_eq!(fence.dirty, None);

        // r2 and r3 dirty (newest wins), candidate from r2 arrives after r3
        // was marked: discard, r3 stays dirty for its own attempt.
        fence.mark_dirty(SourceRevision(2));
        fence.mark_dirty(SourceRevision(3));
        assert!(!fence.admit(SourceRevision(2), SourceRevision(3)));
        assert_eq!(fence.dirty, Some(SourceRevision(3)));

        // The r3 candidate publishes and clears.
        assert!(fence.admit(SourceRevision(3), SourceRevision(3)));
        assert_eq!(fence.dirty, None);

        // A candidate *newer* than the dirty mark also publishes (the fence
        // only rejects candidates older than the newest known mutation).
        fence.mark_dirty(SourceRevision(4));
        assert!(fence.admit(SourceRevision(5), SourceRevision(5)));
        assert_eq!(fence.dirty, None);

        // With nothing dirty, an unsolicited current candidate (e.g. a
        // winning rebuild's) still publishes.
        assert!(fence.admit(SourceRevision(5), SourceRevision(5)));
    }

    /// Candidates older than the authoritative revision are discarded even
    /// when nothing is dirty: a superseded rebuild's diagnostics (computed
    /// before a newer edit) and a candidate racing `mark_dirty` must never
    /// regress markers a newer revision already owns. In particular, a
    /// revision-7 candidate that completes after revision 8 arrives must not
    /// publish revision-7 diagnostics.
    #[test]
    fn diagnostics_fence_discards_stale_unsolicited_candidates() {
        let mut fence = DiagnosticsFence::default();

        // r5 published normally; dirty is clear.
        fence.mark_dirty(SourceRevision(5));
        assert!(fence.admit(SourceRevision(5), SourceRevision(5)));

        // A superseded rebuild finishes late with r4 diagnostics: discarded.
        assert!(!fence.admit(SourceRevision(4), SourceRevision(5)));

        // An edit advanced the revision to r6 but its dirty mark has not
        // landed yet (`mark_dirty` runs after `mutate_sources` returns): an
        // in-flight r5 candidate is already stale.
        assert!(!fence.admit(SourceRevision(5), SourceRevision(6)));

        // The r6 tail publishes normally.
        fence.mark_dirty(SourceRevision(6));
        assert!(fence.admit(SourceRevision(6), SourceRevision(6)));
        assert_eq!(fence.dirty, None);
    }

    #[test]
    fn diagnostics_fence_out_of_order_marks_keep_newest() {
        let mut fence = DiagnosticsFence::default();
        fence.mark_dirty(SourceRevision(7));
        fence.mark_dirty(SourceRevision(5));
        assert_eq!(fence.dirty, Some(SourceRevision(7)));
    }

    #[test]
    fn diagnostics_fence_publication_coverage_clears_deleted_files() {
        let mut fence = DiagnosticsFence::default();
        let a = std::path::PathBuf::from("/p/a.baml");
        let b = std::path::PathBuf::from("/p/b.baml");

        let deleted = fence.record_publication([a.clone(), b.clone()].into_iter().collect());
        assert!(deleted.is_empty());

        // b disappeared: exactly one empty publish for it.
        let deleted = fence.record_publication([a.clone()].into_iter().collect());
        assert_eq!(deleted, vec![b]);

        // Steady state: no repeat clears.
        let deleted = fence.record_publication([a].into_iter().collect());
        assert!(deleted.is_empty());
    }

    #[test]
    fn build_failures_are_revision_fenced_for_project_update_replay() {
        let mut failures = BuildFailureState::default();

        failures.record(SourceRevision(4), "failed to emit bytecode".to_string());
        assert_eq!(
            failures.message_for(SourceRevision(4)),
            Some("failed to emit bytecode")
        );
        let rendered = failures
            .project_diagnostic_for(SourceRevision(4))
            .expect("the current revision's build failure must reach ProjectUpdate");
        assert_eq!(rendered.severity, "error");
        assert_eq!(
            rendered.message,
            "Current build failed: failed to emit bytecode"
        );
        assert_eq!(
            failures.message_for(SourceRevision(5)),
            None,
            "an edit must not replay the previous revision's failure"
        );

        failures.record(SourceRevision(3), "older failure".to_string());
        assert_eq!(
            failures.message_for(SourceRevision(4)),
            Some("failed to emit bytecode"),
            "a late stale rebuild must not replace a newer failure"
        );

        failures.clear_through(SourceRevision(3));
        assert_eq!(
            failures.message_for(SourceRevision(4)),
            Some("failed to emit bytecode")
        );
        failures.clear_through(SourceRevision(4));
        assert_eq!(failures.message_for(SourceRevision(4)), None);
    }

    struct GatedPlaygroundSender {
        publication: std::sync::mpsc::SyncSender<Vec<String>>,
        release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl crate::bex_lsp::PlaygroundSender for GatedPlaygroundSender {
        fn send_playground_notification(
            &self,
            notification: crate::bex_lsp::PlaygroundNotification,
        ) {
            let crate::bex_lsp::PlaygroundNotification::UpdateProject { update, .. } = notification
            else {
                return;
            };
            self.publication
                .send(
                    update
                        .diagnostics
                        .into_iter()
                        .map(|diagnostic| diagnostic.message)
                        .collect(),
                )
                .unwrap();
            self.release.lock().unwrap().recv().unwrap();
        }
    }

    #[test]
    fn source_lease_fences_failure_publication_against_a_racing_edit() {
        let root = vfs::VfsPath::new(vfs::MemoryFS::new());
        let bex_project = crate::project::BexProject::new(
            &root,
            std::sync::Arc::new(sys_ops::SysOpsBuilder::new().build()),
        );
        let source_path = crate::fs::FsPath::from_str("/p/main.baml".to_string());
        let revision = bex_project
            .mutate_sources(crate::project::SourceBatch {
                replace_all: false,
                sources: [(
                    source_path.clone(),
                    "function main() -> int {\n    1\n}\n".to_string(),
                )]
                .into_iter()
                .collect(),
                versions: Vec::new(),
            })
            .unwrap();
        let project = std::sync::Arc::new(LiveProject::new(bex_project));
        project
            .build_failure
            .lock()
            .unwrap()
            .record(revision, "failed to emit bytecode".to_string());

        let (publication_tx, publication_rx) = std::sync::mpsc::sync_channel(1);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
        let lsp = std::sync::Arc::new(BexMultiProject::new(
            std::sync::Arc::new(|_: &vfs::VfsPath| {
                std::sync::Arc::new(sys_ops::SysOpsBuilder::new().build())
            }),
            std::sync::Arc::new(RecordingSender {
                notifications: std::sync::Mutex::new(Vec::new()),
            }),
            std::sync::Arc::new(GatedPlaygroundSender {
                publication: publication_tx,
                release: std::sync::Mutex::new(release_rx),
            }),
            crate::fs::BamlVFS::new(std::sync::Arc::new(Box::new(vfs::PhysicalFS::new("/")))),
            BackgroundSpawner::new(),
        ));

        let publishing_lsp = lsp;
        let publishing_project = project.clone();
        let publisher = std::thread::spawn(move || {
            publishing_lsp.send_update_project("/p", &publishing_project);
        });
        // Building the playground update walks the full builtin package listing.
        // The native provider stdlib makes that legitimately exceed 10 seconds
        // on slower targets (notably the musl CI runner), before the gated
        // sender is reached.
        let diagnostics = publication_rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("the failure-bearing update must reach the publication boundary");
        assert!(
            diagnostics
                .iter()
                .any(|message| message == "Current build failed: failed to emit bytecode")
        );

        let (edit_started_tx, edit_started_rx) = std::sync::mpsc::sync_channel(1);
        let (edit_done_tx, edit_done_rx) = std::sync::mpsc::sync_channel(1);
        let editing_project = project.clone();
        let editor = std::thread::spawn(move || {
            edit_started_tx.send(()).unwrap();
            let next_revision = editing_project
                .project
                .mutate_sources(crate::project::SourceBatch {
                    replace_all: false,
                    sources: [(
                        source_path,
                        "function main() -> int {\n    2\n}\n".to_string(),
                    )]
                    .into_iter()
                    .collect(),
                    versions: Vec::new(),
                })
                .unwrap();
            edit_done_tx.send(next_revision).unwrap();
        });
        edit_started_rx.recv().unwrap();
        assert!(
            edit_done_rx
                .recv_timeout(std::time::Duration::from_millis(100))
                .is_err(),
            "the edit must remain blocked while the old revision is publishing"
        );

        release_tx.send(()).unwrap();
        publisher.join().unwrap();
        assert_eq!(
            edit_done_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap(),
            SourceRevision(revision.0 + 1)
        );
        editor.join().unwrap();

        let current_revision = project.project.current_revision();
        assert_eq!(current_revision, SourceRevision(revision.0 + 1));
        assert!(
            project
                .build_failure
                .lock()
                .unwrap()
                .project_diagnostic_for(current_revision)
                .is_none(),
            "the previous revision's failure must not appear after the edit"
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
        BexMultiProject::collect_marked_project_roots_vfs(&root, &mut found);
        let mut names: Vec<_> = found.iter().map(vfs::VfsPath::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, vec!["/manifest_proj", "/proj"]);
    }

    /// Cancellation safe point: a request whose cancellation already claimed
    /// the response stops right after acquiring the source gate with a typed
    /// `RequestCanceled` instead of paying for the database read.
    #[test]
    fn read_for_request_cancels_at_the_source_gate_safe_point() {
        let ws = TempWorkspace::new("cancel_safe_point");
        ws.file("proj/baml_src/main.baml", "// main");
        let root = ws.vfs_path("proj");
        let project = crate::project::BexProject::new(
            &root,
            std::sync::Arc::new(sys_ops::SysOpsBuilder::new().build()),
        );

        let token = sys_types::CancellationToken::new();
        let _scope = crate::bex_lsp::request_cancellation::RequestCancellationScope::enter(Some(
            token.clone(),
        ));
        assert!(read_for_request(&project).is_ok());

        token.cancel();
        assert!(matches!(
            read_for_request(&project),
            Err(LspError::RequestCanceled(_))
        ));
    }

    struct RecordingSender {
        notifications: std::sync::Mutex<Vec<lsp_server::Notification>>,
    }

    impl LspClientSenderTrait for RecordingSender {
        fn send_notification(&self, msg: lsp_server::Notification) -> Result<(), LspError> {
            self.notifications.lock().unwrap().push(msg);
            Ok(())
        }

        fn send_response_impl(&self, _msg: lsp_server::Response) -> Result<(), LspError> {
            Ok(())
        }

        fn make_request(&self, _msg: lsp_server::Request) -> Result<(), LspError> {
            Ok(())
        }
    }

    /// A connection-scoped session shares the project registry but owns its
    /// own capability negotiation and workspace roots and routes output
    /// through its own sender.
    #[test]
    fn connection_scoped_session_is_fresh_but_shares_projects() {
        use crate::bex_lsp::notification::BexLspNotification as _;

        let sender = Arc::new(RecordingSender {
            notifications: std::sync::Mutex::new(Vec::new()),
        });
        let root = BexMultiProject::new(
            Arc::new(|_: &vfs::VfsPath| Arc::new(sys_ops::SysOpsBuilder::new().build())),
            sender.clone(),
            Arc::new(NoopPlaygroundSender),
            crate::fs::BamlVFS::new(Arc::new(Box::new(vfs::PhysicalFS::new("/")))),
            BackgroundSpawner::new(),
        );
        let _ = root
            .negotiated_encoding
            .set(crate::bex_lsp::position_codec::PositionEncoding::UTF8);
        let snippet_capabilities = serde_json::from_value(serde_json::json!({
            "textDocument": {
                "completion": {
                    "completionItem": {
                        "snippetSupport": true
                    }
                }
            }
        }))
        .expect("valid client capabilities");
        assert!(root.negotiate_snippet_support(&snippet_capabilities));
        assert!(root.snippet_support_for_request().unwrap());
        root.workspace_roots
            .lock()
            .unwrap()
            .push(vfs::VfsPath::new(vfs::MemoryFS::new()));

        let session_sender = Arc::new(RecordingSender {
            notifications: std::sync::Mutex::new(Vec::new()),
        });
        root.semantic_tokens_cache.lock().unwrap().insert(
            crate::fs::FsPath::from_str("/main.baml".to_string()),
            ("0".to_string(), Vec::new()),
        );

        let session = root.connection_scoped_lsp_session(session_sender.clone());

        // Fresh negotiation, roots, and semantic-token delta cache (its
        // entries are encoded per-connection); shared project registry and
        // result-id sequence.
        assert!(session.negotiated_encoding.get().is_none());
        assert!(session.snippet_support.get().is_none());
        assert!(matches!(
            session.snippet_support_for_request(),
            Err(LspError::ServerNotInitialized(_))
        ));
        assert!(!session.negotiate_snippet_support(&lsp_types::ClientCapabilities::default()));
        assert!(!session.snippet_support_for_request().unwrap());
        assert!(session.workspace_roots.lock().unwrap().is_empty());
        assert!(session.semantic_tokens_cache.lock().unwrap().is_empty());
        assert!(Arc::ptr_eq(&session.projects, &root.projects));
        assert!(Arc::ptr_eq(
            &session.semantic_tokens_seq,
            &root.semantic_tokens_seq
        ));

        // Output routes only through the session's own sender.
        session.handle_notification(lsp_server::Notification::new(
            "test/unsupported".to_string(),
            serde_json::Value::Null,
        ));
        assert_eq!(session_sender.notifications.lock().unwrap().len(), 1);
        assert!(sender.notifications.lock().unwrap().is_empty());
    }

    struct NoopPlaygroundSender;

    impl crate::bex_lsp::PlaygroundSender for NoopPlaygroundSender {
        fn send_playground_notification(
            &self,
            _notification: crate::bex_lsp::PlaygroundNotification,
        ) {
        }
    }
}
