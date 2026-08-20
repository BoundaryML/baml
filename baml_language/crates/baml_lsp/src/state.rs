//! The owner-thread state.
//!
//! One [`GlobalState`] per server process, touched by exactly one thread (the
//! owner). It holds the unique [`ProjectDatabase`] handle privately with two
//! accessors — [`GlobalState::apply`] for mutations and
//! [`GlobalState::snapshot`] for reads — so no tracked query ever runs on the
//! owner's handle and every read runs on a pool [`Snapshot`]. Everything
//! else here (sessions, open documents, per-root diagnostics fences, tail
//! deadlines) is plain owner-only data: no locks, nothing to poison.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicUsize},
    time::{Duration, Instant},
};

use baml_base::{SourceRoot, SourceRootKind};
use baml_db::{ProjectDatabase, SourceRootSpec};
use lsp_types::Url;

use crate::{
    diagnostics,
    discovery::{LoadedRoot, NoFs, ProjectFs},
    error::LspError,
    executor::{Executor, Executors, ReadOutcome},
    mutation::SourceMutation,
    paths,
    position_codec::PositionEncoding,
    roots::{RootEntry, RootsView},
    snapshot::{RequestCx, Snapshot},
};

/// Monotonic source revision: bumped once per applied mutation batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SourceRevision(pub u64);

impl std::fmt::Display for SourceRevision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "r{}", self.0)
    }
}

/// A client connection (stdio, a browser socket, ...). Minted by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionKey(pub u64);

/// How long after the last edit before diagnostics are recomputed.
pub const DIAGNOSTICS_DEBOUNCE: Duration = Duration::from_millis(150);

/// An editor buffer whose text is authoritative over disk.
///
/// The owner keeps the buffer text itself (not just a pointer into the
/// database): a root that is removed and re-added underneath an open
/// document — a provisional single-file root merging into its discovered
/// project — parks the database's copy, and the overlay must survive that
/// to be re-applied. Cheap to share: snapshots hold the map by `Arc`.
#[derive(Debug, Clone)]
pub struct OpenDocument {
    /// The URI as the client spelled it (what publications must echo).
    pub uri: Url,
    /// `None` for non-editor writers (playground edits).
    pub version: Option<i32>,
    pub session: SessionKey,
    /// The buffer's current text.
    pub text: Arc<str>,
}

/// Canonical database path → open document.
pub type OpenDocuments = HashMap<PathBuf, OpenDocument>;

/// The host's channel to one client.
pub trait ClientSender: Send + Sync {
    fn send_notification(&self, method: &str, params: serde_json::Value) -> Result<(), LspError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLifecycle {
    Uninitialized,
    Initialized,
    ShuttingDown,
}

pub struct SessionState {
    pub sender: Arc<dyn ClientSender>,
    /// `None` until `initialize` negotiates it.
    pub encoding: Option<PositionEncoding>,
    pub snippet_support: bool,
    pub workspace_folders: Vec<PathBuf>,
    pub lifecycle: SessionLifecycle,
    /// Semantic-token delta baselines, keyed by database path.
    ///
    /// `Arc`-shared so a snapshot carries an immutable view of what this
    /// session was last served (the owner writes through `Arc::make_mut`);
    /// there is no lock, and the owner records a new baseline *before* the
    /// response carrying its result id leaves.
    pub semantic_tokens: Arc<HashMap<PathBuf, TokenBaseline>>,
}

/// What a session was last served for one document: the result id the
/// client holds, and the encoded tokens behind it.
///
/// The result id is the [`SourceRevision`] the tokens were computed at —
/// tokens cannot differ within a revision, so two requests at the same
/// revision legitimately share an id.
/// A baseline the owner must record for a session before responding.
pub struct BaselineCommit {
    pub path: PathBuf,
    pub baseline: TokenBaseline,
}

#[derive(Debug, Clone)]
pub struct TokenBaseline {
    pub result_id: SourceRevision,
    pub tokens: Arc<Vec<lsp_types::SemanticToken>>,
}

/// What the last admitted publication said for one file, for change
/// detection: a hash of the text the spans were converted against, and the
/// diagnostics themselves. Positions are text-dependent, so a text change
/// alone forces a resend even when the diagnostic set is identical.
#[derive(Debug)]
pub struct PublishedFile {
    pub text_hash: u64,
    pub diagnostics: Vec<baml_compiler_diagnostics::Diagnostic>,
}

/// The change-detection hash for [`PublishedFile::text_hash`].
pub fn published_text_hash(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Per-root publication fence.
///
/// A computed candidate carries the revision it saw; the owner admits it
/// only if no newer mutation touched the root since. Because every mutation
/// re-arms the debounce timer, a discarded candidate is always followed by
/// a fresh one, so nothing is lost.
#[derive(Debug, Default)]
pub struct DiagnosticsFence {
    /// Newest revision requiring publication. Compare-and-clear only.
    dirty: Option<SourceRevision>,
    /// What the last admitted publication covered, per file: the coverage
    /// clears deleted files with one empty publish, the content lets an
    /// unchanged file skip republication entirely.
    last_published: HashMap<PathBuf, PublishedFile>,
}

impl DiagnosticsFence {
    pub fn mark_dirty(&mut self, revision: SourceRevision) {
        self.dirty = Some(self.dirty.map_or(revision, |d| d.max(revision)));
    }

    /// `true` admits the candidate and clears the dirty mark; `false`
    /// discards a stale one — a newer mutation owns the next publication.
    pub fn admit(&mut self, candidate: SourceRevision, current: SourceRevision) -> bool {
        if candidate < current {
            return false;
        }
        if let Some(dirty) = self.dirty {
            if candidate < dirty {
                return false;
            }
            self.dirty = None;
        }
        true
    }

    /// A file whose text and diagnostics both match the last admitted
    /// publication needs no resend to a session that received it.
    pub fn is_unchanged(
        &self,
        path: &Path,
        text_hash: u64,
        diagnostics: &[baml_compiler_diagnostics::Diagnostic],
    ) -> bool {
        self.last_published.get(path).is_some_and(|previous| {
            previous.text_hash == text_hash && previous.diagnostics == diagnostics
        })
    }

    /// Record a publication's coverage; returns files covered before but not
    /// now — each needs one empty publish so stale markers clear.
    pub fn record_publication(&mut self, current: HashMap<PathBuf, PublishedFile>) -> Vec<PathBuf> {
        let vanished = self
            .last_published
            .keys()
            .filter(|path| !current.contains_key(*path))
            .cloned()
            .collect();
        self.last_published = current;
        vanished
    }

    pub fn last_published(&self) -> &HashMap<PathBuf, PublishedFile> {
        &self.last_published
    }

    /// A mutation since the last admitted publication is still awaiting one.
    pub fn is_dirty(&self) -> bool {
        self.dirty.is_some()
    }
}

/// Owner-only bookkeeping for one `Workspace` root.
#[derive(Debug, Default)]
pub struct RootState {
    pub fence: DiagnosticsFence,
    /// Revision of the last mutation that touched this root.
    pub last_mutated: SourceRevision,
    /// A debounced diagnostics pass is due at this instant.
    pub diagnostics_due: Option<Instant>,
    /// A diagnostics job is currently running on the pool.
    pub diagnostics_in_flight: bool,
    /// Sessions that have received the fence's `last_published` state. A
    /// session outside this set gets a full publication (every file), one
    /// inside it only the files that changed.
    pub published_sessions: HashSet<SessionKey>,
}

/// One computed pass over a workspace root, produced on a snapshot.
///
/// Fully owned: publication converts it with each session's position codec
/// and never touches the database, so it carries the text of every file a
/// span can point into.
#[derive(Debug)]
pub struct DiagnosticCandidate {
    pub root: SourceRoot,
    /// The revision the snapshot was minted at.
    pub revision: SourceRevision,
    /// Every file of the root, each with the diagnostics published under it.
    pub files: Vec<CandidateFile>,
    /// Files outside the root that a diagnostic's span refers to (related
    /// information into a dependency or the stdlib).
    pub referenced: Vec<ReferencedFile>,
}

impl DiagnosticCandidate {
    /// Whether any file of the root carries an error. Emit is only attempted
    /// for a clean candidate, so the host's build pipeline reads this instead
    /// of re-deriving the check through `get_bytecode`'s own error gate.
    pub fn has_errors(&self) -> bool {
        self.files
            .iter()
            .flat_map(|file| &file.diagnostics)
            .any(|diagnostic| {
                matches!(
                    diagnostic.severity,
                    baml_compiler_diagnostics::Severity::Error
                )
            })
    }
}

#[derive(Debug)]
pub struct CandidateFile {
    pub file_id: baml_base::FileId,
    pub path: PathBuf,
    pub text: Arc<str>,
    pub diagnostics: Vec<baml_compiler_diagnostics::Diagnostic>,
}

#[derive(Debug)]
pub struct ReferencedFile {
    pub file_id: baml_base::FileId,
    pub path: PathBuf,
    pub text: Arc<str>,
}

/// A read job's answer to a request, ready to be sent.
pub type Responder = Box<dyn FnOnce(Result<serde_json::Value, LspError>) + Send + 'static>;

/// Everything the owner reacts to. Posted by hosts, timers, and pool jobs.
pub enum OwnerEvent {
    RequestDone {
        /// Which in-flight read finished, so its cancellation token is
        /// unregistered before the response leaves.
        session: SessionKey,
        request_id: lsp_server::RequestId,
        respond: Responder,
        outcome: ReadOutcome<serde_json::Value>,
    },
    DiagnosticsDue {
        root: SourceRoot,
    },
    DiagnosticsResult {
        root: SourceRoot,
        outcome: ReadOutcome<DiagnosticCandidate>,
    },
    /// Discovery finished: the roots found for `folder`, with their on-disk
    /// contents. `folder` is `Some` for a discovery the owner asked for
    /// (roots previously found under it that vanished are reconciled) and
    /// `None` for a host-supplied root set (upsert only).
    RootsLoaded {
        folder: Option<PathBuf>,
        roots: Vec<LoadedRoot>,
    },
    /// Disk reads finished for individual files; `None` = the file is gone.
    FilesReloaded {
        files: Vec<(PathBuf, Option<String>)>,
    },
    /// An arbitrary owner-thread continuation.
    Call(Box<dyn FnOnce(&mut GlobalState) + Send + 'static>),
}

/// A `Send + Sync` handle for posting [`OwnerEvent`]s to the owner thread.
///
/// The only thing a pool job or a host may hold that reaches the owner: it
/// can post, never wait, so no job can deadlock the owner's `set_*`.
#[derive(Clone)]
pub struct OwnerHandle {
    tx: crossbeam_channel::Sender<OwnerEvent>,
}

impl OwnerHandle {
    pub fn post(&self, event: OwnerEvent) {
        if self.tx.send(event).is_err() {
            tracing::debug!("owner loop has stopped; dropping event");
        }
    }
}

/// The result of applying a mutation batch.
#[derive(Debug, Default)]
pub struct Applied {
    pub revision: SourceRevision,
    /// Roots whose file set or text changed.
    pub touched: Vec<SourceRoot>,
    /// Files whose diagnostics must be cleared (their root was removed).
    pub cleared: Vec<PathBuf>,
    /// Mutations the database refused, with why.
    pub rejected: Vec<(SourceMutation, LspError)>,
    /// The root set changed (added/removed roots).
    pub roots_changed: bool,
    /// A `Workspace` root was removed by this batch.
    pub workspace_root_removed: bool,
}

/// A host's callback for applied source mutations (see
/// [`GlobalState::set_source_observer`]).
pub type SourceObserver = Arc<dyn Fn(&Applied) + Send + Sync>;

/// A host's implementation of the open-panel command (see
/// [`GlobalState::set_open_panel_handler`]).
pub type OpenPanelHandler = Arc<dyn Fn(&crate::dispatch::OpenPanelRequest) + Send + Sync>;

pub struct GlobalState {
    db: ProjectDatabase,
    revision: SourceRevision,
    roots: Arc<RootsView>,
    /// Canonical, so a URI under it maps back onto `<builtin>/…` exactly.
    stdlib_dir: Option<PathBuf>,
    open_documents: Arc<OpenDocuments>,
    root_state: HashMap<SourceRoot, RootState>,
    /// Paths of `Workspace` roots minted by `didOpen` for a document under
    /// no known root. They exist only to serve open documents: superseded
    /// when discovery finds the enclosing project, removed when their last
    /// document closes.
    provisional_roots: HashSet<PathBuf>,
    sessions: HashMap<SessionKey, SessionState>,
    live_snapshots: Arc<AtomicUsize>,
    executors: Executors,
    /// Cancellation tokens of the snapshot reads currently on the pool, so
    /// `$/cancelRequest` (and session teardown) can cut the running query
    /// short instead of letting it burn to completion.
    in_flight_reads: HashMap<(SessionKey, lsp_server::RequestId), salsa::CancellationToken>,
    fs: Arc<dyn ProjectFs>,
    /// Notified after every batch that advanced the revision, so a host can
    /// drive work that is not the protocol layer's business (the playground's
    /// engine rebuilds, its project-list pushes). The observer is called on
    /// the owner thread with the owner's own `&Applied`: it must only hand the
    /// facts off (a channel send), never block or re-enter the state.
    source_observer: Option<SourceObserver>,
    /// How this host opens the playground. `None` means it has none, which
    /// is why the capability (and therefore code lenses) is not advertised.
    open_panel_handler: Option<OpenPanelHandler>,
    handle: OwnerHandle,
    events: crossbeam_channel::Receiver<OwnerEvent>,
}

impl GlobalState {
    /// A fresh state with the embedded stdlib loaded, no workspace, and no
    /// filesystem ([`NoFs`]: discovery finds nothing, reloads fail). Hosts
    /// with a filesystem use [`GlobalState::with_fs`].
    pub fn new(executors: Executors, stdlib_dir: Option<PathBuf>) -> Self {
        Self::with_fs(executors, stdlib_dir, Arc::new(NoFs))
    }

    /// A fresh state whose discovery and reload jobs read through `fs`.
    pub fn with_fs(
        executors: Executors,
        stdlib_dir: Option<PathBuf>,
        fs: Arc<dyn ProjectFs>,
    ) -> Self {
        let (tx, events) = crossbeam_channel::unbounded();
        let mut db = ProjectDatabase::new();
        db.ensure_stdlib_sources();
        let mut state = Self {
            db,
            revision: SourceRevision(0),
            roots: Arc::default(),
            stdlib_dir: stdlib_dir.map(|dir| paths::canonical_physical_path(&dir)),
            open_documents: Arc::default(),
            root_state: HashMap::new(),
            provisional_roots: HashSet::new(),
            sessions: HashMap::new(),
            live_snapshots: Arc::new(AtomicUsize::new(0)),
            executors,
            in_flight_reads: HashMap::new(),
            fs,
            source_observer: None,
            open_panel_handler: None,
            handle: OwnerHandle { tx },
            events,
        };
        state.rebuild_roots_view();
        state
    }

    /// Point stdlib presentation at a materialized copy of the stubs (or
    /// none). Rebuilds the roots view; open snapshots keep the old view.
    pub fn set_stdlib_dir(&mut self, stdlib_dir: Option<PathBuf>) {
        self.stdlib_dir = stdlib_dir.map(|dir| paths::canonical_physical_path(&dir));
        self.rebuild_roots_view();
    }

    pub fn fs(&self) -> &Arc<dyn ProjectFs> {
        &self.fs
    }

    /// Install the host's source-change observer (see the field docs). One
    /// observer per process; installing a second replaces the first.
    pub fn set_source_observer(&mut self, observer: SourceObserver) {
        self.source_observer = Some(observer);
    }

    /// Install the host's playground opener. Advertised capabilities are read
    /// at `initialize`, so this must be installed before any client connects.
    pub fn set_open_panel_handler(&mut self, handler: OpenPanelHandler) {
        self.open_panel_handler = Some(handler);
    }

    pub fn open_panel_handler(&self) -> Option<&OpenPanelHandler> {
        self.open_panel_handler.as_ref()
    }

    // ── Read side ─────────────────────────────────────────────────────────

    /// Mint a snapshot for a session's request. Hand it straight to
    /// [`crate::executor::spawn_read`]; never store it.
    pub fn snapshot(&self, cx: RequestCx) -> Snapshot {
        Snapshot::mint(
            &self.db,
            self.revision,
            Arc::clone(&self.roots),
            Arc::clone(&self.open_documents),
            cx,
            Arc::clone(&self.live_snapshots),
        )
    }

    /// The request context for `session` (its negotiated encoding etc.).
    pub fn request_cx(&self, session: SessionKey) -> Result<RequestCx, LspError> {
        let state = self.session(session)?;
        let encoding = state
            .encoding
            .ok_or_else(|| LspError::ServerNotInitialized("initialize has not run".into()))?;
        Ok(RequestCx {
            encoding,
            token_baselines: Arc::clone(&state.semantic_tokens),
            snippet_support: state.snippet_support,
        })
    }

    /// The lane for snapshot reads answering client requests.
    pub fn request_executor(&self) -> &dyn Executor {
        self.executors.requests.as_ref()
    }

    /// The lane for diagnostics sweeps.
    pub fn diagnostics_executor(&self) -> &dyn Executor {
        self.executors.diagnostics.as_ref()
    }

    /// The lane for filesystem jobs (discovery, reloads). Jobs here never
    /// hold a snapshot — see [`Executors`].
    pub fn io_executor(&self) -> &dyn Executor {
        self.executors.io.as_ref()
    }

    pub fn handle(&self) -> OwnerHandle {
        self.handle.clone()
    }

    // ── In-flight snapshot reads ──────────────────────────────────────────

    /// Record a spawned read's cancellation token under its request id.
    /// Owner-thread only, so registration happens strictly before any
    /// forwarded `$/cancelRequest` for the same id is dispatched.
    pub(crate) fn register_read(
        &mut self,
        session: SessionKey,
        request_id: lsp_server::RequestId,
        token: salsa::CancellationToken,
    ) {
        self.in_flight_reads.insert((session, request_id), token);
    }

    /// Forget a finished read. Called from the completion event before the
    /// response leaves; a cancelled read still completes (with
    /// `RequestCanceled`) and unregisters here.
    pub(crate) fn finish_read(&mut self, session: SessionKey, request_id: &lsp_server::RequestId) {
        self.in_flight_reads.remove(&(session, request_id.clone()));
    }

    /// Cancel the running read for `request_id`, if one is on the pool: its
    /// next Salsa query entry unwinds with `Cancelled::Local`. Returns
    /// whether a read was found (a request still queued in the transport, or
    /// already finished, has nothing to cancel).
    pub fn cancel_read(&self, session: SessionKey, request_id: &lsp_server::RequestId) -> bool {
        match self.in_flight_reads.get(&(session, request_id.clone())) {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }

    /// The owner's event queue (drained by the host's loop).
    pub fn events(&self) -> &crossbeam_channel::Receiver<OwnerEvent> {
        &self.events
    }

    pub fn revision(&self) -> SourceRevision {
        self.revision
    }

    pub fn roots(&self) -> &Arc<RootsView> {
        &self.roots
    }

    pub fn open_documents(&self) -> &OpenDocuments {
        &self.open_documents
    }

    pub fn open_document(&self, path: &Path) -> Option<&OpenDocument> {
        self.open_documents.get(path)
    }

    /// Input reads on the owner are fine (no tracked query runs).
    pub fn file_text(&self, path: &Path) -> Option<String> {
        self.db
            .get_file(path)
            .map(|file| file.text(&self.db).clone())
    }

    // ── Sessions ──────────────────────────────────────────────────────────

    pub fn open_session(&mut self, key: SessionKey, sender: Arc<dyn ClientSender>) {
        self.sessions.insert(
            key,
            SessionState {
                sender,
                encoding: None,
                snippet_support: false,
                workspace_folders: Vec::new(),
                lifecycle: SessionLifecycle::Uninitialized,
                semantic_tokens: Arc::new(HashMap::new()),
            },
        );
    }

    /// Drop a session and close every document it had open (as
    /// [`GlobalState::close_document`] does for `didClose`). Returns the
    /// closed documents' database paths; closing them again is a no-op.
    pub fn close_session(&mut self, key: SessionKey) -> Vec<PathBuf> {
        self.sessions.remove(&key);
        // Nothing can deliver the answers, so stop the reads still running
        // for the session and drop their tokens.
        self.in_flight_reads.retain(|(session, _), token| {
            if *session == key {
                token.cancel();
                false
            } else {
                true
            }
        });
        for root_state in self.root_state.values_mut() {
            root_state.published_sessions.remove(&key);
        }
        let paths: Vec<PathBuf> = self
            .open_documents
            .iter()
            .filter(|(_, doc)| doc.session == key)
            .map(|(path, _)| path.clone())
            .collect();
        for path in &paths {
            self.close_document(path);
        }
        paths
    }

    /// Sessions that completed `initialize` (the only ones that receive
    /// server-initiated notifications).
    pub fn initialized_sessions(&self) -> impl Iterator<Item = (SessionKey, &SessionState)> + '_ {
        self.sessions()
            .filter(|(_, state)| state.lifecycle == SessionLifecycle::Initialized)
    }

    /// Send one notification to every initialized session. Delivery
    /// failures are logged, not returned: a broadcast has no single caller
    /// to fail.
    pub fn notify_all(&self, method: &str, params: &serde_json::Value) {
        for (key, session) in self.initialized_sessions() {
            if let Err(error) = session.sender.send_notification(method, params.clone()) {
                tracing::warn!(?key, method, %error, "notification not delivered");
            }
        }
    }

    pub fn session(&self, key: SessionKey) -> Result<&SessionState, LspError> {
        self.sessions
            .get(&key)
            .ok_or_else(|| LspError::Internal(format!("unknown session {key:?}")))
    }

    /// Record the semantic-token baseline `session` was just served for
    /// `path`. Called on the owner thread before the response is sent, so a
    /// result id can never reach a client the owner cannot diff against.
    pub fn store_token_baseline(
        &mut self,
        session: SessionKey,
        path: PathBuf,
        baseline: TokenBaseline,
    ) {
        if let Ok(state) = self.session_mut(session) {
            Arc::make_mut(&mut state.semantic_tokens).insert(path, baseline);
        }
    }

    /// Drop every session's token baseline for `path` (the document closed,
    /// so the client will re-request from scratch if it reopens).
    pub fn evict_token_baselines(&mut self, path: &Path) {
        for state in self.sessions.values_mut() {
            if state.semantic_tokens.contains_key(path) {
                Arc::make_mut(&mut state.semantic_tokens).remove(path);
            }
        }
    }

    pub fn session_mut(&mut self, key: SessionKey) -> Result<&mut SessionState, LspError> {
        self.sessions
            .get_mut(&key)
            .ok_or_else(|| LspError::Internal(format!("unknown session {key:?}")))
    }

    pub fn sessions(&self) -> impl Iterator<Item = (SessionKey, &SessionState)> + '_ {
        self.sessions.iter().map(|(key, state)| (*key, state))
    }

    // ── Open documents ────────────────────────────────────────────────────

    pub fn track_open_document(&mut self, path: PathBuf, doc: OpenDocument) {
        Arc::make_mut(&mut self.open_documents).insert(path, doc);
    }

    pub fn set_document_version(&mut self, path: &Path, version: Option<i32>) {
        if let Some(doc) = Arc::make_mut(&mut self.open_documents).get_mut(path) {
            doc.version = version;
        }
    }

    /// Open documents whose path lies under `root_path`.
    pub fn open_documents_under<'a>(
        &'a self,
        root_path: &'a Path,
    ) -> impl Iterator<Item = (&'a Path, &'a OpenDocument)> + 'a {
        self.open_documents
            .iter()
            .filter(move |(path, _)| path.starts_with(root_path))
            .map(|(path, doc)| (path.as_path(), doc))
    }

    // ── Provisional roots ─────────────────────────────────────────────────

    /// Record that the `Workspace` root at `path` was minted for an open
    /// document rather than discovered.
    pub fn mark_provisional_root(&mut self, path: PathBuf) {
        self.provisional_roots.insert(path);
    }

    /// Forget the provisional status of `path` (it was discovered for real
    /// or removed).
    pub fn unmark_provisional_root(&mut self, path: &Path) -> bool {
        self.provisional_roots.remove(path)
    }

    pub fn is_provisional_root(&self, path: &Path) -> bool {
        self.provisional_roots.contains(path)
    }

    pub fn provisional_roots(&self) -> impl Iterator<Item = &Path> + '_ {
        self.provisional_roots.iter().map(PathBuf::as_path)
    }

    // ── Roots ─────────────────────────────────────────────────────────────

    pub fn root_state(&self, root: SourceRoot) -> Option<&RootState> {
        self.root_state.get(&root)
    }

    pub fn root_state_mut(&mut self, root: SourceRoot) -> Option<&mut RootState> {
        self.root_state.get_mut(&root)
    }

    /// The single-workspace stopgap.
    ///
    /// The compiler is single-world until the world-viewpoint unit lands
    /// (every workspace package is named `user`), so a server hosts one
    /// `Workspace` root. Nothing else in this crate depends on that: roots,
    /// fences, discovery and routing are all built for N. Removing this
    /// function is the multi-package switch.
    pub fn single_workspace_guard(
        &self,
        canonical_path: &Path,
        kind: SourceRootKind,
    ) -> Result<(), LspError> {
        if kind != SourceRootKind::Workspace {
            return Ok(());
        }
        // Consult the database, not the roots view: mid-batch (a provisional
        // root removed and its discovered project added in one `apply`) the
        // view is stale until the post-batch rebuild, and a guard reading it
        // would reject the replacement root.
        match self.db.workspace_root() {
            Some(existing) if existing.path(&self.db) != canonical_path => {
                Err(LspError::RequestFailed(format!(
                    "this server already hosts the workspace at {}; one workspace per server until multi-package support lands (ignoring {})",
                    existing.path(&self.db).display(),
                    canonical_path.display()
                )))
            }
            Some(_) | None => Ok(()),
        }
    }

    // ── Mutations ─────────────────────────────────────────────────────────

    /// Apply a batch on the owner thread. Bumps the revision once, marks
    /// touched workspace roots dirty, re-arms their diagnostics debounce,
    /// and clears the publications of removed roots. An empty batch is a
    /// no-op (no revision bump).
    ///
    /// Salsa's `set_*` blocks until every live [`Snapshot`] unwinds; on
    /// wasm (single-threaded) a live snapshot here can only be a bug, so it
    /// is asserted rather than waited on.
    pub fn apply(&mut self, batch: Vec<SourceMutation>) -> Applied {
        if batch.is_empty() {
            return Applied {
                revision: self.revision,
                ..Applied::default()
            };
        }
        #[cfg(target_arch = "wasm32")]
        assert_eq!(
            self.live_snapshots
                .load(std::sync::atomic::Ordering::SeqCst),
            0,
            "a snapshot outlived its task across a mutation (would hang the single-threaded owner)"
        );
        let started = Instant::now();
        let mut applied = Applied::default();
        let mut touched: HashSet<SourceRoot> = HashSet::new();

        for mutation in batch {
            match self.apply_one(&mutation, &mut applied) {
                Ok(roots) => touched.extend(roots),
                Err(error) => applied.rejected.push((mutation, error)),
            }
        }

        if applied.roots_changed {
            self.rebuild_roots_view();
        }
        self.revision = SourceRevision(self.revision.0 + 1);
        applied.revision = self.revision;
        applied.touched = touched.into_iter().collect();

        for &root in &applied.touched {
            if let Some(state) = self.root_state.get_mut(&root) {
                state.last_mutated = self.revision;
                state.fence.mark_dirty(self.revision);
                state.diagnostics_due = Some(Instant::now() + DIAGNOSTICS_DEBOUNCE);
            }
        }
        // A change to a shared root (stdlib/dependency/dynamic) can change
        // every workspace root's diagnostics.
        let shared_touched = applied
            .touched
            .iter()
            .any(|root| root.kind(&self.db) != SourceRootKind::Workspace);
        if shared_touched {
            let revision = self.revision;
            for state in self.root_state.values_mut() {
                state.last_mutated = revision;
                state.fence.mark_dirty(revision);
                state.diagnostics_due = Some(Instant::now() + DIAGNOSTICS_DEBOUNCE);
            }
        }

        let waited = started.elapsed();
        if waited > Duration::from_millis(50) {
            tracing::info!(
                "apply blocked {}ms waiting for in-flight snapshots",
                waited.as_millis()
            );
        }
        // A removed root's markers must not outlive it in any editor.
        diagnostics::publish_cleared(self, &applied.cleared);
        // The single-workspace stopgap can have refused a real project while
        // a scratch document's provisional root held the slot (see
        // `single_workspace_guard`). When the batch frees the slot, re-run
        // discovery as a posted continuation so whatever was refused gets
        // its chance; this hook is deleted together with the guard.
        if applied.workspace_root_removed && self.db.workspace_root().is_none() {
            self.handle.post(OwnerEvent::Call(Box::new(|state| {
                state.rediscover_freed_workspace_slot();
            })));
        }
        if let Some(observer) = &self.source_observer {
            observer(&applied);
        }
        applied
    }

    /// The workspace slot just became free: re-run discovery for every
    /// initialized session's folders, and re-serve open documents that are
    /// under no root (their provisional mint lost the slot race earlier).
    fn rediscover_freed_workspace_slot(&mut self) {
        let folders: BTreeSet<PathBuf> = self
            .initialized_sessions()
            .flat_map(|(_, session)| session.workspace_folders.iter().cloned())
            .collect();
        for folder in folders {
            self.spawn_discovery(folder);
        }

        let unserved: Vec<(PathBuf, OpenDocument)> = self
            .open_documents
            .iter()
            .filter(|(path, _)| self.roots.root_for_path(path).is_none())
            .map(|(path, doc)| (path.clone(), doc.clone()))
            .collect();
        for (path, doc) in unserved {
            let Some(parent) = path.parent().map(Path::to_path_buf) else {
                continue;
            };
            let applied = self.apply(vec![
                SourceMutation::UpsertRoot {
                    spec: crate::discovery::workspace_root_spec(parent.clone()),
                    files: Vec::new(),
                },
                SourceMutation::SetOverlay {
                    path,
                    text: doc.text.to_string(),
                    version: doc.version,
                },
            ]);
            if applied.rejected.is_empty() {
                self.mark_provisional_root(parent.clone());
                self.spawn_discovery(parent);
            } else {
                // Another unserved document already claimed the slot; this
                // one waits for the next free.
                for (mutation, error) in &applied.rejected {
                    tracing::debug!(?mutation, %error, "document still unserved after slot rescue");
                }
            }
        }
    }

    fn apply_one(
        &mut self,
        mutation: &SourceMutation,
        applied: &mut Applied,
    ) -> Result<Vec<SourceRoot>, LspError> {
        match mutation {
            SourceMutation::UpsertRoot { spec, files } => {
                // The database keys roots by canonical path; compare and
                // insert in that form so a re-upsert under a different
                // spelling (`/tmp` vs `/private/tmp`) matches its root.
                let root_path = baml_db::canonicalize_lossy(&spec.path);
                self.single_workspace_guard(&root_path, spec.kind)?;
                let root = match self.db.source_root_for_path(&root_path) {
                    Some(existing) if existing.path(&self.db) == &root_path => existing,
                    _ => {
                        let root = self
                            .db
                            .add_source_root(SourceRootSpec {
                                path: root_path.clone(),
                                package: spec.package.clone(),
                                kind: spec.kind,
                            })
                            .map_err(|e| LspError::RequestFailed(e.to_string()))?;
                        applied.roots_changed = true;
                        if spec.kind == SourceRootKind::Workspace {
                            self.root_state.entry(root).or_default();
                        }
                        root
                    }
                };
                // Open overlays are authoritative over the disk text. A
                // BTreeMap keeps the root's file order deterministic (path
                // order — the same order CLI discovery loads), so the emitted
                // program layout cannot vary run to run.
                let open_documents = Arc::clone(&self.open_documents);
                let mut merged: BTreeMap<&Path, &str> = files
                    .iter()
                    .map(|(path, text)| (path.as_path(), text.as_str()))
                    .collect();
                for (path, doc) in open_documents
                    .iter()
                    .filter(|(path, _)| path.starts_with(&root_path))
                {
                    merged.insert(path.as_path(), &doc.text);
                }
                self.db.set_root_files(root, merged);
                Ok(vec![root])
            }
            SourceMutation::RemoveRoot { path } => {
                let Some(root) = self.db.source_root_for_path(path) else {
                    return Ok(Vec::new());
                };
                if root.path(&self.db) != path {
                    return Ok(Vec::new());
                }
                if let Some(state) = self.root_state.remove(&root) {
                    applied
                        .cleared
                        .extend(state.fence.last_published().keys().cloned());
                }
                if root.kind(&self.db) == SourceRootKind::Workspace {
                    applied.workspace_root_removed = true;
                }
                self.provisional_roots.remove(path);
                self.db.remove_source_root(root);
                applied.roots_changed = true;
                Ok(Vec::new())
            }
            SourceMutation::SetOverlay {
                path,
                text,
                version,
            } => {
                let root = self
                    .db
                    .source_root_for_path(path)
                    .ok_or_else(|| LspError::NoRootForPath(path.clone()))?;
                self.db.add_or_update_file_in(root, path, text);
                if let Some(doc) = Arc::make_mut(&mut self.open_documents).get_mut(path) {
                    doc.version = *version;
                    doc.text = Arc::from(text.as_str());
                }
                Ok(vec![root])
            }
            SourceMutation::SetDisk { path, text } => {
                if self.open_documents.contains_key(path) {
                    return Ok(Vec::new());
                }
                let root = self
                    .db
                    .source_root_for_path(path)
                    .ok_or_else(|| LspError::NoRootForPath(path.clone()))?;
                self.db.add_or_update_file_in(root, path, text);
                Ok(vec![root])
            }
            SourceMutation::RemoveFile { path } => {
                if self.open_documents.contains_key(path) {
                    return Ok(Vec::new());
                }
                let root = self.db.source_root_for_path(path);
                self.db.remove_file(path);
                Ok(root.into_iter().collect())
            }
            SourceMutation::CloseDocument { path } => {
                Arc::make_mut(&mut self.open_documents).remove(path);
                Ok(Vec::new())
            }
        }
    }

    fn rebuild_roots_view(&mut self) {
        let entries: Vec<RootEntry> = self
            .db
            .source_roots()
            .into_iter()
            .map(|root| RootEntry {
                root,
                path: root.path(&self.db).clone(),
                package: root.package(&self.db),
                kind: root.kind(&self.db),
            })
            .collect();
        self.roots = RootsView::new(entries, self.stdlib_dir.clone());
    }

    /// Schedule a diagnostics pass for every root whose current published
    /// state `session` has not received (a session that initialized after
    /// the state settled would otherwise never see standing diagnostics —
    /// publication is push-on-change and per-file diffing skips unchanged
    /// files for sessions that already have them).
    pub fn schedule_publications_for(&mut self, session: SessionKey) {
        let revision = self.revision;
        for root_state in self.root_state.values_mut() {
            if !root_state.published_sessions.contains(&session) {
                root_state.fence.mark_dirty(revision);
                root_state.diagnostics_due = Some(Instant::now());
            }
        }
    }

    // ── Tails ─────────────────────────────────────────────────────────────

    /// The earliest pending tail deadline, for the host's timer.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.root_state
            .values()
            .filter_map(|state| state.diagnostics_due)
            .min()
    }

    /// Fire every tail whose deadline has passed by posting its event.
    pub fn on_tick(&mut self, now: Instant) {
        let due: Vec<SourceRoot> = self
            .root_state
            .iter()
            .filter(|(_, state)| state.diagnostics_due.is_some_and(|at| at <= now))
            .map(|(root, _)| *root)
            .collect();
        for root in due {
            if let Some(state) = self.root_state.get_mut(&root) {
                state.diagnostics_due = None;
            }
            self.handle.post(OwnerEvent::DiagnosticsDue { root });
        }
    }
}
