//! The protocol layer driven end to end against `GlobalState`: a real
//! thread pool, a recording client sender per session, and an in-memory
//! `ProjectFs` that counts every read and every discovery walk.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, atomic::AtomicUsize},
    time::{Duration, Instant},
};

use baml_lsp::{
    ClientSender, GlobalState, LspError, OwnerEvent, SessionKey,
    discovery::{DiscoveredRoot, ProjectFs, workspace_root_spec},
    executor::{Executor, Executors, Inline, Job, ThreadPool},
    snapshot::TaskFailure,
    state::DIAGNOSTICS_DEBOUNCE,
};
use lsp_types::{PublishDiagnosticsParams, Url};
use serde_json::{Value, json};

// ── Fakes ────────────────────────────────────────────────────────────────

#[derive(Default)]
struct RecordingSender {
    sent: Mutex<Vec<(String, Value)>>,
}

impl ClientSender for RecordingSender {
    fn send_notification(&self, method: &str, params: Value) -> Result<(), LspError> {
        self.sent.lock().unwrap().push((method.to_owned(), params));
        Ok(())
    }
}

impl RecordingSender {
    fn publications(&self) -> Vec<PublishDiagnosticsParams> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .filter(|(method, _)| method == "textDocument/publishDiagnostics")
            .map(|(_, params)| serde_json::from_value(params.clone()).unwrap())
            .collect()
    }

    fn publications_for(&self, uri: &Url) -> Vec<PublishDiagnosticsParams> {
        self.publications()
            .into_iter()
            .filter(|p| &p.uri == uri)
            .collect()
    }

    fn methods(&self) -> Vec<String> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .map(|(m, _)| m.clone())
            .collect()
    }
}

/// An in-memory filesystem: `projects` are project-root directories; every
/// `.baml` file under a project root belongs to it.
#[derive(Default)]
struct MemFs {
    files: Mutex<HashMap<PathBuf, String>>,
    projects: Mutex<Vec<PathBuf>>,
    reads: Mutex<Vec<PathBuf>>,
    walks: AtomicUsize,
}

impl MemFs {
    fn write(&self, path: impl Into<PathBuf>, text: &str) {
        self.files
            .lock()
            .unwrap()
            .insert(path.into(), text.to_owned());
    }

    fn remove(&self, path: &Path) {
        self.files.lock().unwrap().remove(path);
    }

    fn add_project(&self, root: impl Into<PathBuf>) {
        self.projects.lock().unwrap().push(root.into());
    }

    fn reads(&self) -> Vec<PathBuf> {
        self.reads.lock().unwrap().clone()
    }

    fn reset_counters(&self) {
        self.reads.lock().unwrap().clear();
        self.walks.store(0, std::sync::atomic::Ordering::SeqCst);
    }

    fn walks(&self) -> usize {
        self.walks.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl ProjectFs for MemFs {
    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        self.reads.lock().unwrap().push(path.to_path_buf());
        self.files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"))
    }

    fn discover_roots(&self, folder: &Path) -> Vec<DiscoveredRoot> {
        self.walks.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let files = self.files.lock().unwrap();
        self.projects
            .lock()
            .unwrap()
            .iter()
            .filter(|root| folder.starts_with(root) || root.starts_with(folder))
            .map(|root| {
                let mut owned: Vec<PathBuf> = files
                    .keys()
                    .filter(|path| path.starts_with(root))
                    .filter(|path| path.extension().is_some_and(|ext| ext == "baml"))
                    .cloned()
                    .collect();
                owned.sort();
                DiscoveredRoot {
                    spec: workspace_root_spec(root.clone()),
                    files: owned,
                }
            })
            .collect()
    }
}

/// An executor that parks jobs until the test runs them, so a test can
/// interleave owner-side work (a `$/cancelRequest`, an injected event)
/// between a job's spawn and its execution without racing a real thread.
#[derive(Default)]
struct ManualExecutor {
    jobs: Mutex<Vec<Job>>,
}

impl ManualExecutor {
    fn run_all(&self) {
        loop {
            let job = self.jobs.lock().unwrap().pop();
            match job {
                Some(job) => job(),
                None => return,
            }
        }
    }

    fn parked(&self) -> usize {
        self.jobs.lock().unwrap().len()
    }
}

impl Executor for ManualExecutor {
    fn spawn_job(&self, job: Job) {
        self.jobs.lock().unwrap().push(job);
    }
}

// ── Harness ──────────────────────────────────────────────────────────────

struct Harness {
    state: GlobalState,
    fs: Arc<MemFs>,
    senders: HashMap<SessionKey, Arc<RecordingSender>>,
    _temp: tempfile::TempDir,
    /// A canonical, real directory (so URI canonicalization is exercised).
    ws: PathBuf,
}

impl Harness {
    fn new() -> Self {
        Self::with_stdlib_dir(None)
    }

    fn with_stdlib_dir(stdlib_dir: Option<PathBuf>) -> Self {
        Self::with_executors(Executors::single(Arc::new(ThreadPool::new(2))), stdlib_dir)
    }

    fn with_executors(executors: Executors, stdlib_dir: Option<PathBuf>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let ws = temp.path().canonicalize().unwrap();
        let fs = Arc::new(MemFs::default());
        let state =
            GlobalState::with_fs(executors, stdlib_dir, Arc::clone(&fs) as Arc<dyn ProjectFs>);
        Self {
            state,
            fs,
            senders: HashMap::new(),
            _temp: temp,
            ws,
        }
    }

    fn uri(&self, rel: &str) -> Url {
        Url::from_file_path(self.ws.join(rel)).unwrap()
    }

    fn sender(&self, session: SessionKey) -> &RecordingSender {
        &self.senders[&session]
    }

    /// Open a session and run `initialize` (+ `initialized`) with the
    /// harness workspace as the one folder.
    fn init_session(&mut self, session: SessionKey, encodings: &[lsp_types::PositionEncodingKind]) {
        let ws = self.ws.clone();
        self.init_session_with_folders(session, encodings, std::slice::from_ref(&ws));
    }

    fn init_session_with_folders(
        &mut self,
        session: SessionKey,
        encodings: &[lsp_types::PositionEncodingKind],
        folders: &[PathBuf],
    ) {
        let sender = Arc::new(RecordingSender::default());
        self.senders.insert(session, Arc::clone(&sender));
        self.state.open_session(session, sender);
        let params = json!({
            "processId": null,
            "capabilities": { "general": { "positionEncodings": encodings } },
            "workspaceFolders": folders.iter().map(|f| json!({
                "uri": Url::from_file_path(f).unwrap(),
                "name": "ws",
            })).collect::<Vec<_>>(),
        });
        let result = self.request(session, "initialize", params).unwrap();
        assert_eq!(result["serverInfo"]["name"], "baml-lsp");
        self.notify(session, "initialized", json!({})).unwrap();
    }

    /// Dispatch a request and drain events until its response arrives.
    fn request(
        &mut self,
        session: SessionKey,
        method: &str,
        params: Value,
    ) -> Result<Value, LspError> {
        let (tx, rx) = std::sync::mpsc::channel();
        let req =
            lsp_server::Request::new(lsp_server::RequestId::from(1), method.to_owned(), params);
        self.state.dispatch_request(
            session,
            req,
            Box::new(move |result| tx.send(result).unwrap()),
        );
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Ok(result) = rx.try_recv() {
                return result;
            }
            assert!(Instant::now() < deadline, "no response to {method}");
            if let Ok(event) = self.state.events().recv_timeout(Duration::from_millis(20)) {
                self.state.handle_event(event);
            }
        }
    }

    fn notify(&mut self, session: SessionKey, method: &str, params: Value) -> Result<(), LspError> {
        let notif = lsp_server::Notification::new(method.to_owned(), params);
        self.state.dispatch_notification(session, notif)
    }

    fn open(&mut self, session: SessionKey, uri: &Url, version: i32, text: &str) {
        self.notify(
            session,
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "baml", "version": version, "text": text } }),
        )
        .unwrap();
    }

    fn change(&mut self, session: SessionKey, uri: &Url, version: i32, text: &str) {
        self.notify(
            session,
            "textDocument/didChange",
            json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": [{ "text": text }] }),
        )
        .unwrap();
    }

    fn close(&mut self, session: SessionKey, uri: &Url) {
        self.notify(
            session,
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        )
        .unwrap();
    }

    fn any_in_flight(&self) -> bool {
        self.state.roots().workspace_roots().any(|entry| {
            self.state
                .root_state(entry.root)
                .is_some_and(|s| s.diagnostics_in_flight)
        })
    }

    /// Fire every tail whose debounce would have elapsed, then handle events
    /// until nothing is pending: no event for a quiet period, no armed
    /// deadline, no diagnostics job in flight.
    fn settle(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            assert!(Instant::now() < deadline, "state did not settle");
            self.state
                .on_tick(Instant::now() + DIAGNOSTICS_DEBOUNCE * 2);
            match self.state.events().recv_timeout(Duration::from_millis(150)) {
                Ok(event) => self.state.handle_event(event),
                Err(_) => {
                    if self.state.next_deadline().is_none() && !self.any_in_flight() {
                        return;
                    }
                }
            }
        }
    }
}

const BAD_SOURCE: &str = "class A {\n  x Undefined\n}\n";

fn has_error(publication: &PublishDiagnosticsParams) -> bool {
    publication
        .diagnostics
        .iter()
        .any(|d| d.severity == Some(lsp_types::DiagnosticSeverity::ERROR))
}

// ── Tests ────────────────────────────────────────────────────────────────

/// initialize → didOpen → debounce → publishDiagnostics with the client's
/// URI, the document version, and a real error for bad source.
#[test]
fn open_bad_document_publishes_versioned_diagnostics() {
    let mut h = Harness::new();
    h.fs.add_project(&h.ws);
    h.fs.write(h.ws.join("main.baml"), "class A { x int }\n");
    let s = SessionKey(1);
    h.init_session(s, &[]);
    let uri = h.uri("main.baml");
    h.open(s, &uri, 7, BAD_SOURCE);
    h.settle();

    let publications = h.sender(s).publications_for(&uri);
    let last = publications
        .last()
        .expect("a publication for the opened document");
    assert_eq!(last.version, Some(7));
    assert!(has_error(last), "{last:?}");
    let diagnostic = last
        .diagnostics
        .iter()
        .find(|d| d.severity == Some(lsp_types::DiagnosticSeverity::ERROR))
        .unwrap();
    assert_eq!(diagnostic.range.start.line, 1, "{diagnostic:?}");
    assert_eq!(diagnostic.source.as_deref(), Some("baml"));
    // Only initialized sessions receive publications; nothing else was sent.
    assert!(
        h.sender(s)
            .methods()
            .iter()
            .all(|m| m == "textDocument/publishDiagnostics"),
        "{:?}",
        h.sender(s).methods()
    );
}

/// The opened buffer is authoritative: didOpen never reads it from disk,
/// neither inline nor through the discovery it triggers.
#[test]
fn did_open_never_reads_the_opened_file() {
    let mut h = Harness::new();
    h.fs.add_project(&h.ws);
    h.fs.write(h.ws.join("main.baml"), "// stale disk copy\n");
    h.fs.write(h.ws.join("other.baml"), "class Other { y int }\n");
    let s = SessionKey(1);
    // No workspace folders: the project is discovered from the open.
    h.init_session_with_folders(s, &[], &[]);
    h.fs.reset_counters();

    let uri = h.uri("main.baml");
    h.open(s, &uri, 1, "class A { x Other }\n");
    assert!(
        h.fs.reads().is_empty(),
        "didOpen is inline and read nothing"
    );
    h.settle();

    assert_eq!(
        h.fs.walks(),
        1,
        "one discovery walk for the enclosing project"
    );
    assert_eq!(h.fs.reads(), vec![h.ws.join("other.baml")]);
    // The buffer (which references Other from disk) is what got checked.
    let last = h.sender(s).publications_for(&uri).last().cloned().unwrap();
    assert!(last.diagnostics.is_empty(), "{last:?}");
    assert!(
        h.state
            .roots()
            .workspace_roots()
            .all(|e| !h.state.is_provisional_root(&e.path)),
        "the provisional root merged into the discovered project"
    );
}

/// A file opened in a *subdirectory* before discovery lands mints a
/// provisional root at its directory; discovery must then merge it into the
/// enclosing project in ONE mutation batch — the removal of the provisional
/// root and the upsert of the discovered root land together, and the
/// single-workspace guard must not see the ghost of the provisional root.
#[test]
fn subdirectory_open_merges_into_the_discovered_project() {
    let mut h = Harness::new();
    h.fs.add_project(&h.ws);
    h.fs.write(h.ws.join("baml_src/other.baml"), "class Other { y int }\n");
    let s = SessionKey(1);
    // No workspace folders: the project is discovered from the open.
    h.init_session_with_folders(s, &[], &[]);

    let uri = h.uri("baml_src/nested/a.baml");
    h.open(s, &uri, 1, "class A { x Other }\n");
    h.settle();

    let workspace_roots: Vec<_> = h.state.roots().workspace_roots().collect();
    assert_eq!(
        workspace_roots.len(),
        1,
        "exactly the discovered project root: {workspace_roots:?}"
    );
    assert_eq!(workspace_roots[0].path, h.ws);
    assert!(
        h.state
            .roots()
            .workspace_roots()
            .all(|e| !h.state.is_provisional_root(&e.path)),
        "no provisional root survives the merge"
    );
    // The document stayed served across the merge: its buffer resolves
    // `Other` from the project and checks clean.
    let last = h.sender(s).publications_for(&uri).last().cloned().unwrap();
    assert!(last.diagnostics.is_empty(), "{last:?}");
    // And subsequent edits are routed: no NoRootForPath rejection.
    h.change(s, &uri, 2, "class A { x Other y int }\n");
    h.settle();
    let last = h.sender(s).publications_for(&uri).last().cloned().unwrap();
    assert_eq!(last.version, Some(2));
}

/// A watched-files event touches exactly its URIs: three reads, no walk.
#[test]
fn watched_files_reload_exactly_the_named_paths() {
    let mut h = Harness::new();
    h.fs.add_project(&h.ws);
    for name in ["a.baml", "b.baml", "c.baml", "d.baml"] {
        h.fs.write(h.ws.join(name), "class A { x int }\n");
    }
    let s = SessionKey(1);
    h.init_session(s, &[]);
    h.settle();
    h.fs.reset_counters();

    h.fs.write(h.ws.join("a.baml"), "class A { x Undefined }\n");
    let changes: Vec<Value> = ["a.baml", "b.baml", "c.baml"]
        .iter()
        .map(|name| json!({ "uri": h.uri(name), "type": 2 }))
        .collect();
    h.notify(
        s,
        "workspace/didChangeWatchedFiles",
        json!({ "changes": changes }),
    )
    .unwrap();
    h.settle();

    let mut reads = h.fs.reads();
    reads.sort();
    assert_eq!(
        reads,
        vec![
            h.ws.join("a.baml"),
            h.ws.join("b.baml"),
            h.ws.join("c.baml")
        ]
    );
    assert_eq!(h.fs.walks(), 0);
    let a = h.sender(s).publications_for(&h.uri("a.baml"));
    assert!(
        has_error(a.last().unwrap()),
        "the changed disk text was checked"
    );
    assert_eq!(
        a.last().unwrap().version,
        None,
        "closed files publish unversioned"
    );
}

/// A burst of edits: the final publication is for the final version and the
/// owner queue is empty afterwards.
#[test]
fn rapid_edit_burst_ends_on_the_last_version() {
    let mut h = Harness::new();
    h.fs.add_project(&h.ws);
    let s = SessionKey(1);
    h.init_session(s, &[]);
    let uri = h.uri("main.baml");
    h.open(s, &uri, 1, "class A { x int }\n");
    for version in 2..=201 {
        h.change(
            s,
            &uri,
            version,
            &format!("class A {{ x int }}\n// {version}\n"),
        );
        if version % 50 == 0 {
            // Interleave ticks so passes start (and get cancelled) mid-burst.
            h.state.on_tick(Instant::now() + DIAGNOSTICS_DEBOUNCE * 2);
            while let Ok(event) = h.state.events().try_recv() {
                h.state.handle_event(event);
            }
        }
    }
    h.settle();

    let publications = h.sender(s).publications_for(&uri);
    assert_eq!(publications.last().unwrap().version, Some(201));
    assert!(h.state.events().is_empty());
    assert!(h.state.next_deadline().is_none());
}

/// Two sessions with different encodings each get their own conversion of
/// the same candidate; the ranges differ after a multibyte character.
#[test]
fn each_session_is_published_in_its_own_encoding() {
    let mut h = Harness::new();
    h.fs.add_project(&h.ws);
    let utf16 = SessionKey(16);
    let utf8 = SessionKey(8);
    h.init_session(utf16, &[lsp_types::PositionEncodingKind::UTF16]);
    h.init_session(utf8, &[lsp_types::PositionEncodingKind::UTF8]);
    let uri = h.uri("main.baml");
    // "😀" is 4 UTF-8 bytes / 2 UTF-16 units before the error on its line.
    h.open(utf16, &uri, 3, "class A {\n  x \"😀\" | Undefined\n}\n");
    h.settle();

    let for_utf16 = h
        .sender(utf16)
        .publications_for(&uri)
        .last()
        .cloned()
        .unwrap();
    let for_utf8 = h
        .sender(utf8)
        .publications_for(&uri)
        .last()
        .cloned()
        .unwrap();
    let start = |p: &PublishDiagnosticsParams| {
        p.diagnostics
            .iter()
            .find(|d| d.severity == Some(lsp_types::DiagnosticSeverity::ERROR))
            .map(|d| d.range.start)
            .unwrap_or_else(|| panic!("no error in {p:?}"))
    };
    let (s16, s8) = (start(&for_utf16), start(&for_utf8));
    assert_eq!(s16.line, 1);
    assert_eq!(s8.line, 1);
    assert_eq!(s8.character, s16.character + 2, "utf16={s16:?} utf8={s8:?}");
    // The version belongs to the session that has the document open.
    assert_eq!(for_utf16.version, Some(3));
    assert_eq!(for_utf8.version, None);
}

/// Formatting round-trips through the snapshot pool.
#[test]
fn formatting_request_runs_on_the_pool() {
    let mut h = Harness::new();
    h.fs.add_project(&h.ws);
    let s = SessionKey(1);
    h.init_session(s, &[]);
    let uri = h.uri("main.baml");
    let text = "class   A {\n        x   int\n}\n";
    h.open(s, &uri, 1, text);
    // Let discovery land first: a mutation mid-request is `ContentModified`
    // (the client's cue to retry), not what this test is about.
    h.settle();
    let result = h
        .request(
            s,
            "textDocument/formatting",
            json!({ "textDocument": { "uri": uri }, "options": { "tabSize": 4, "insertSpaces": true } }),
        )
        .unwrap();
    let edits: Vec<lsp_types::TextEdit> = serde_json::from_value(result).unwrap();
    assert_eq!(edits.len(), 1, "{edits:?}");
    assert_eq!(edits[0].range.start, lsp_types::Position::new(0, 0));
    assert_eq!(edits[0].range.end, lsp_types::Position::new(3, 0));
    assert_ne!(edits[0].new_text, text);
    assert!(
        edits[0].new_text.contains("class A {"),
        "{:?}",
        edits[0].new_text
    );

    // A file the database does not have is FileNotFound.
    let missing = h.uri("missing.baml");
    let error = h
        .request(s, "textDocument/formatting", json!({ "textDocument": { "uri": missing }, "options": { "tabSize": 4, "insertSpaces": true } }))
        .unwrap_err();
    assert!(matches!(error, LspError::FileNotFound(_)), "{error:?}");
}

/// didClose on a file that vanished from disk removes it and clears its
/// markers.
#[test]
fn closing_a_vanished_file_removes_it_and_clears_markers() {
    let mut h = Harness::new();
    h.fs.add_project(&h.ws);
    h.fs.write(h.ws.join("main.baml"), BAD_SOURCE);
    h.fs.write(h.ws.join("keep.baml"), "class Keep { k int }\n");
    let s = SessionKey(1);
    h.init_session(s, &[]);
    let uri = h.uri("main.baml");
    h.open(s, &uri, 1, BAD_SOURCE);
    h.settle();
    assert!(has_error(
        h.sender(s).publications_for(&uri).last().unwrap()
    ));

    h.fs.remove(&h.ws.join("main.baml"));
    h.close(s, &uri);
    h.settle();

    let last = h.sender(s).publications_for(&uri).last().cloned().unwrap();
    assert!(last.diagnostics.is_empty(), "{last:?}");
    assert_eq!(last.version, None);
    assert!(h.state.file_text(&h.ws.join("main.baml")).is_none());
    assert!(h.state.file_text(&h.ws.join("keep.baml")).is_some());
}

/// A stdlib file presents under the materialized directory and maps back to
/// its virtual database path; opening it tracks nothing.
#[test]
fn stdlib_paths_round_trip_through_the_materialized_directory() {
    let stdlib_temp = tempfile::tempdir().unwrap();
    // Deliberately the non-canonical spelling: the owner canonicalizes it.
    let mut h = Harness::with_stdlib_dir(Some(stdlib_temp.path().to_path_buf()));
    let canonical_dir = stdlib_temp.path().canonicalize().unwrap();
    let s = SessionKey(1);
    h.init_session(s, &[]);

    let db_path = h
        .state
        .roots()
        .iter_kind(baml_base::SourceRootKind::Stdlib)
        .flat_map(|entry| {
            h.state
                .roots()
                .entries()
                .iter()
                .filter(move |e| e.root == entry.root)
        })
        .next()
        .map(|entry| entry.path.join("prelude.baml"))
        .expect("a stdlib root");
    let uri = baml_lsp::paths::uri_for_db_path(h.state.roots(), &db_path).unwrap();
    let presented = uri.to_file_path().unwrap();
    assert!(presented.starts_with(&canonical_dir), "{presented:?}");
    assert_eq!(
        baml_lsp::paths::canonical_document_path(h.state.roots(), &uri).unwrap(),
        db_path
    );

    // Opening a stdlib document is accepted, tracks no overlay, mints no root.
    let roots_before = h.state.roots().entries().len();
    h.open(s, &uri, 1, "// user edit of a read-only file\n");
    assert!(h.state.open_document(&db_path).is_none());
    assert_eq!(h.state.roots().entries().len(), roots_before);
    h.settle();
    assert!(
        h.sender(s).publications().is_empty(),
        "stdlib is never published"
    );
}

/// A document under no project gets a provisional root that lives exactly
/// as long as the document is open.
#[test]
fn detached_document_gets_a_provisional_root_until_closed() {
    let mut h = Harness::new();
    let s = SessionKey(1);
    h.init_session_with_folders(s, &[], &[]);
    let uri = h.uri("scratch/notes.baml");
    h.open(s, &uri, 1, BAD_SOURCE);
    let root = h.ws.join("scratch");
    assert!(h.state.is_provisional_root(&root));
    h.settle();
    assert!(has_error(
        h.sender(s).publications_for(&uri).last().unwrap()
    ));
    assert!(
        h.state.is_provisional_root(&root),
        "nothing to discover; stays provisional"
    );

    h.close(s, &uri);
    assert!(!h.state.is_provisional_root(&root));
    assert!(h.state.roots().workspace_roots().next().is_none());
    // The removed root's markers were cleared immediately.
    let last = h.sender(s).publications_for(&uri).last().cloned().unwrap();
    assert!(last.diagnostics.is_empty(), "{last:?}");
    h.settle();
    assert!(
        h.fs.reads().is_empty(),
        "a removed provisional root is not reloaded"
    );
}

/// Lifecycle gating and unsupported methods.
#[test]
fn request_gating_and_unsupported_methods() {
    let mut h = Harness::new();
    let s = SessionKey(1);
    let sender = Arc::new(RecordingSender::default());
    h.senders.insert(s, Arc::clone(&sender));
    h.state.open_session(s, sender);

    let error = h
        .request(s, "textDocument/formatting", json!({ "textDocument": { "uri": h.uri("x.baml") }, "options": { "tabSize": 4, "insertSpaces": true } }))
        .unwrap_err();
    assert!(
        matches!(error, LspError::ServerNotInitialized(_)),
        "{error:?}"
    );

    h.request(
        s,
        "initialize",
        json!({ "processId": null, "capabilities": {} }),
    )
    .unwrap();
    let error = h
        .request(
            s,
            "initialize",
            json!({ "processId": null, "capabilities": {} }),
        )
        .unwrap_err();
    assert!(matches!(error, LspError::RequestFailed(_)), "{error:?}");

    let error = h.request(s, "textDocument/rename", json!({})).unwrap_err();
    assert!(
        matches!(error, LspError::RequestNotSupported(_)),
        "{error:?}"
    );
    let error = h
        .request(s, "textDocument/formatting", json!({ "nonsense": true }))
        .unwrap_err();
    assert_eq!(
        error.to_response_error().code,
        lsp_server::ErrorCode::InvalidParams as i32
    );

    let error = h
        .notify(s, "window/workDoneProgress/cancel", json!({ "token": 1 }))
        .unwrap_err();
    assert!(
        matches!(error, LspError::NotificationNotSupported(_)),
        "{error:?}"
    );
    h.notify(
        s,
        "workspace/didChangeConfiguration",
        json!({ "settings": {} }),
    )
    .unwrap();
    h.notify(s, "$/setTrace", json!({ "value": "off" }))
        .unwrap();

    // A malformed incremental range (a nonexistent line) is InvalidParams.
    let uri = h.uri("main.baml");
    h.open(s, &uri, 1, "class A { x int }\n");
    let error = h
        .notify(
            s,
            "textDocument/didChange",
            json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "range": { "start": { "line": 99, "character": 0 }, "end": { "line": 99, "character": 0 } }, "text": "x" }] }),
        )
        .unwrap_err();
    assert!(matches!(error, LspError::InvalidParams(_)), "{error:?}");

    let result = h.request(s, "shutdown", Value::Null).unwrap();
    assert_eq!(result, Value::Null);
    let error = h.request(s, "shutdown", Value::Null).unwrap_err();
    assert!(matches!(error, LspError::RequestFailed(_)), "{error:?}");
    h.notify(s, "exit", Value::Null).unwrap();
    // The database survives shutdown.
    assert!(h.state.file_text(&h.ws.join("main.baml")).is_some());
    h.settle();
}

/// The event enum is handled exhaustively; a `Call` runs on the owner.
#[test]
fn call_events_run_on_the_owner() {
    let mut h = Harness::new();
    let ran = Arc::new(AtomicUsize::new(0));
    let ran2 = Arc::clone(&ran);
    h.state.handle().post(OwnerEvent::Call(Box::new(
        move |state: &mut GlobalState| {
            assert!(state.roots().workspace_roots().next().is_none());
            ran2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        },
    )));
    h.settle();
    assert_eq!(ran.load(std::sync::atomic::Ordering::SeqCst), 1);
}

// ── Position-based features (2.3b) ──────────────────────────────────────────

/// The LSP position of `needle`'s first byte in ASCII-only `text`.
fn pos_of(text: &str, needle: &str) -> lsp_types::Position {
    let offset = text
        .find(needle)
        .unwrap_or_else(|| panic!("fixture contains {needle:?}"));
    let before = &text[..offset];
    let line = u32::try_from(before.matches('\n').count()).unwrap();
    let character = u32::try_from(offset - before.rfind('\n').map_or(0, |p| p + 1)).unwrap();
    lsp_types::Position { line, character }
}

const TYPES_FIXTURE: &str = "class Person {\n    name string\n}\n";
const FUNCS_FIXTURE: &str = "/// Greets someone.\nfunction greet(p: Person) -> string {\n    \
p.name\n}\n\nfunction tally(xs: int[]) -> int? {\n    xs.at(0)\n}\n";

/// A settled single-session workspace with the two feature fixtures.
fn feature_harness() -> Harness {
    let mut harness = Harness::new();
    harness.fs.add_project(&harness.ws);
    harness
        .fs
        .write(harness.ws.join("types.baml"), TYPES_FIXTURE);
    harness
        .fs
        .write(harness.ws.join("funcs.baml"), FUNCS_FIXTURE);
    harness.init_session(SessionKey(1), &[lsp_types::PositionEncodingKind::UTF16]);
    harness.settle();
    harness
}

/// A lens-enabled workspace: one file with a runnable function and a test,
/// plus a recording open-panel handler installed *before* `initialize` (the
/// capability is read there).
const LENS_FIXTURE: &str = "function greet(name: string) -> string {\n    name\n}\n\n\
                            test \"greets\" {\n    assert.is_true(greet(\"a\") == \"a\")\n}\n";

type PanelLog = Arc<std::sync::Mutex<Vec<baml_lsp::OpenPanelRequest>>>;

fn lens_harness() -> (Harness, PanelLog) {
    let mut harness = Harness::new();
    harness.fs.add_project(&harness.ws);
    harness.fs.write(harness.ws.join("lens.baml"), LENS_FIXTURE);
    let log: PanelLog = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorder = Arc::clone(&log);
    harness
        .state
        .set_open_panel_handler(Arc::new(move |request: &baml_lsp::OpenPanelRequest| {
            recorder.lock().unwrap().push(request.clone());
        }));
    harness.init_session(SessionKey(1), &[lsp_types::PositionEncodingKind::UTF16]);
    harness.settle();
    (harness, log)
}

/// A lens is a fully-resolved button for `baml.openBamlPanel`, positioned on
/// the item's name, carrying the workspace root it belongs to.
#[test]
fn code_lenses_run_functions_and_tests_through_the_panel_command() {
    let (mut harness, _log) = lens_harness();
    let uri = harness.uri("lens.baml");
    let response = harness
        .request(
            SessionKey(1),
            "textDocument/codeLens",
            serde_json::json!({ "textDocument": { "uri": uri } }),
        )
        .expect("code lenses succeed");
    let lenses = response.as_array().expect("lens array");
    assert_eq!(
        lenses.len(),
        2,
        "one per function and test, got: {lenses:?}"
    );

    let project = harness.ws.to_string_lossy().into_owned();
    let function_lens = &lenses[0];
    assert_eq!(function_lens["command"]["command"], "baml.openBamlPanel");
    assert_eq!(
        function_lens["command"]["arguments"][0],
        serde_json::json!({ "projectPath": project, "functionName": "greet" })
    );
    // Positioned on the name token, not the whole declaration.
    assert_eq!(function_lens["range"]["start"]["line"], 0);
    assert_eq!(
        function_lens["range"]["start"]["character"],
        serde_json::json!(9)
    );

    let test_lens = &lenses[1];
    assert_eq!(
        test_lens["command"]["arguments"][0],
        serde_json::json!({ "projectPath": project, "testName": "greets" })
    );
    assert_eq!(test_lens["command"]["title"], "▶ Run test");

    // Lenses ship resolved: `codeLens/resolve` is the identity.
    let resolved = harness
        .request(SessionKey(1), "codeLens/resolve", function_lens.clone())
        .expect("resolve succeeds");
    assert_eq!(&resolved, function_lens);
}

/// Clicking a lens round-trips: the argument the server produced is the
/// argument it accepts, and it reaches the host as a resolved request.
#[test]
fn executing_a_lens_command_reaches_the_host() {
    let (mut harness, log) = lens_harness();
    let uri = harness.uri("lens.baml");
    let lenses = harness
        .request(
            SessionKey(1),
            "textDocument/codeLens",
            serde_json::json!({ "textDocument": { "uri": uri } }),
        )
        .expect("code lenses succeed");
    let arguments = lenses[1]["command"]["arguments"].clone();

    harness
        .request(
            SessionKey(1),
            "workspace/executeCommand",
            serde_json::json!({ "command": "baml.openBamlPanel", "arguments": arguments }),
        )
        .expect("the command runs");

    let recorded = log.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].project, harness.ws);
    assert_eq!(recorded[0].test_name.as_deref(), Some("greets"));
    assert!(recorded[0].function_name.is_none());
}

/// A bare open (no argument) resolves to the workspace root; an argument
/// naming something that is not a root is rejected rather than guessed at.
#[test]
fn panel_command_resolves_the_project_or_rejects_it() {
    let (mut harness, log) = lens_harness();
    harness
        .request(
            SessionKey(1),
            "workspace/executeCommand",
            serde_json::json!({ "command": "baml.openBamlPanel", "arguments": [] }),
        )
        .expect("a bare open uses the workspace root");
    assert_eq!(log.lock().unwrap()[0].project, harness.ws);

    let error = harness
        .request(
            SessionKey(1),
            "workspace/executeCommand",
            serde_json::json!({
                "command": "baml.openBamlPanel",
                "arguments": [{ "projectPath": "/not/a/root" }],
            }),
        )
        .expect_err("a non-root project path is rejected");
    assert!(matches!(error, LspError::InvalidParams(_)), "got {error:?}");
    assert_eq!(
        log.lock().unwrap().len(),
        1,
        "nothing further reached the host"
    );

    let error = harness
        .request(
            SessionKey(1),
            "workspace/executeCommand",
            serde_json::json!({ "command": "baml.unknownCommand", "arguments": [] }),
        )
        .expect_err("unknown commands are not silently accepted");
    assert!(
        matches!(error, LspError::RequestNotSupported(_)),
        "got {error:?}"
    );
}

/// Without a playground host there are no lenses to click, and the command
/// is not advertised — so `initialize` says so and the request is refused.
#[test]
fn a_host_without_a_playground_advertises_no_lenses() {
    let mut harness = Harness::new();
    harness.fs.add_project(&harness.ws);
    harness.fs.write(harness.ws.join("lens.baml"), LENS_FIXTURE);
    let sender = Arc::new(RecordingSender::default());
    harness.senders.insert(SessionKey(1), Arc::clone(&sender));
    harness.state.open_session(SessionKey(1), sender);
    let result = harness
        .request(
            SessionKey(1),
            "initialize",
            serde_json::json!({
                "processId": null,
                "capabilities": {},
                "workspaceFolders": [{
                    "uri": Url::from_file_path(&harness.ws).unwrap(),
                    "name": "ws",
                }],
            }),
        )
        .unwrap();
    assert!(result["capabilities"]["codeLensProvider"].is_null());
    assert!(result["capabilities"]["executeCommandProvider"].is_null());
    harness
        .notify(SessionKey(1), "initialized", json!({}))
        .unwrap();
    harness.settle();

    let error = harness
        .request(
            SessionKey(1),
            "workspace/executeCommand",
            serde_json::json!({ "command": "baml.openBamlPanel", "arguments": [] }),
        )
        .expect_err("no host, no command");
    assert!(
        matches!(error, LspError::RequestNotSupported(_)),
        "got {error:?}"
    );
}

fn position_params(uri: &Url, position: lsp_types::Position) -> Value {
    serde_json::json!({
        "textDocument": { "uri": uri },
        "position": position,
    })
}

#[test]
fn completion_after_a_dot_offers_the_receivers_members() {
    let mut harness = feature_harness();
    let uri = harness.uri("funcs.baml");
    let response = harness
        .request(
            SessionKey(1),
            "textDocument/completion",
            position_params(&uri, pos_of(FUNCS_FIXTURE, "at(0")),
        )
        .expect("completion succeeds");
    let items = response.as_array().expect("an item array");
    let at = items
        .iter()
        .find(|item| item["label"] == "at")
        .unwrap_or_else(|| panic!("`at` completes on an int[] receiver, got {items:#?}"));

    assert_eq!(at["kind"], 2, "METHOD");
    assert!(
        at["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("at(index: int)")),
        "detail is the signature minus the receiver already written \
         (`xs.at(…)`, not the UFCS `T[].at(xs, …)`), got {:?}",
        at["detail"]
    );
    // The edit is explicit: the server decides what an item replaces, never
    // the editor's idea of a word boundary.
    assert!(at["textEdit"]["range"].is_object(), "{at:#?}");
    // Ranked by the ide layer; the wire preserves that order.
    let ranks: Vec<&str> = items
        .iter()
        .filter_map(|item| item["sortText"].as_str())
        .collect();
    let mut sorted = ranks.clone();
    sorted.sort_unstable();
    assert_eq!(ranks, sorted, "items arrive best-first");
}

#[test]
fn completion_is_advertised_with_the_dot_trigger() {
    let capabilities = baml_lsp::dispatch::server_capabilities(
        baml_lsp::position_codec::PositionEncoding::UTF16,
        false,
    );
    let completion = capabilities
        .completion_provider
        .expect("completion is always available; it needs no host");
    assert_eq!(completion.trigger_characters, Some(vec![".".to_owned()]));
    assert_eq!(completion.resolve_provider, Some(false));
}

#[test]
fn hover_renders_the_resolved_signature() {
    let mut harness = feature_harness();
    let uri = harness.uri("funcs.baml");
    let response = harness
        .request(
            SessionKey(1),
            "textDocument/hover",
            position_params(&uri, pos_of(FUNCS_FIXTURE, "greet")),
        )
        .expect("hover succeeds");
    let markdown = response["contents"]["value"].as_str().expect("markdown");
    assert!(
        markdown.contains("function greet(p: Person) -> string throws never"),
        "resolved signature with explicit throws, got:\n{markdown}"
    );
    // rust-analyzer's shape: the owning path fenced above the declaration,
    // docs below a separator (not `///` inside the fence).
    assert!(
        markdown.starts_with("```baml\nuser\n```\n\n---\n\n"),
        "the compiler's package name leads, separated from the item, got:\n{markdown}"
    );
    assert!(
        markdown.contains("---\n\nGreets someone."),
        "docs render below the separator, got:\n{markdown}"
    );
}

#[test]
fn method_hover_owner_is_the_receiver_subject_type() {
    let mut harness = feature_harness();
    let uri = harness.uri("funcs.baml");
    let response = harness
        .request(
            SessionKey(1),
            "textDocument/hover",
            position_params(&uri, pos_of(FUNCS_FIXTURE, "at(0")),
        )
        .expect("hover succeeds");
    let markdown = response["contents"]["value"].as_str().expect("markdown");
    // The owner fence spells the receiver subject the way the reader writes
    // it — the impl/companion generics (`T[]`), never a companion class path.
    assert!(
        markdown.starts_with("```baml\nT[]\n```\n\n---\n\n"),
        "the companion subject type leads, got:\n{markdown}"
    );
    assert!(
        markdown.contains("function at(self, index: int) -> T? throws never"),
        "resolved method signature, got:\n{markdown}"
    );
}

#[test]
fn goto_definition_crosses_files() {
    let mut harness = feature_harness();
    let funcs = harness.uri("funcs.baml");
    let types = harness.uri("types.baml");
    let response = harness
        .request(
            SessionKey(1),
            "textDocument/definition",
            position_params(&funcs, pos_of(FUNCS_FIXTURE, "Person")),
        )
        .expect("definition succeeds");
    assert_eq!(response["uri"], serde_json::json!(types));
    assert_eq!(
        response["range"]["start"],
        serde_json::json!(pos_of(TYPES_FIXTURE, "Person"))
    );
}

#[test]
fn references_include_the_declaration_on_request() {
    let mut harness = feature_harness();
    let types = harness.uri("types.baml");
    let mut params = position_params(&types, pos_of(TYPES_FIXTURE, "Person"));
    params["context"] = serde_json::json!({ "includeDeclaration": true });
    let response = harness
        .request(SessionKey(1), "textDocument/references", params)
        .expect("references succeed");
    let locations = response.as_array().expect("location array");
    // The declaration plus the parameter-type use in funcs.baml.
    assert_eq!(locations.len(), 2, "got: {locations:?}");
    assert!(
        locations
            .iter()
            .any(|loc| loc["uri"] == serde_json::json!(harness.uri("funcs.baml"))),
        "cross-file use found, got: {locations:?}"
    );
}

#[test]
fn document_symbols_nest_members_with_distinct_ranges() {
    let mut harness = feature_harness();
    let types = harness.uri("types.baml");
    let response = harness
        .request(
            SessionKey(1),
            "textDocument/documentSymbol",
            serde_json::json!({ "textDocument": { "uri": types } }),
        )
        .expect("symbols succeed");
    let symbols = response.as_array().expect("symbol array");
    assert_eq!(symbols[0]["name"], "Person");
    let children = symbols[0]["children"].as_array().expect("members nest");
    assert_eq!(children[0]["name"], "name");
    // The class's full range extends past its selection range.
    assert_ne!(
        symbols[0]["range"]["end"],
        symbols[0]["selectionRange"]["end"]
    );
}

#[test]
fn workspace_symbols_cover_user_and_materialized_stdlib() {
    let temp = tempfile::tempdir().unwrap();
    let stdlib_dir = temp.path().canonicalize().unwrap();
    let mut harness = Harness::with_stdlib_dir(Some(stdlib_dir.clone()));
    harness.fs.add_project(&harness.ws);
    harness
        .fs
        .write(harness.ws.join("types.baml"), TYPES_FIXTURE);
    harness.init_session(SessionKey(1), &[lsp_types::PositionEncodingKind::UTF16]);
    harness.settle();

    let response = harness
        .request(
            SessionKey(1),
            "workspace/symbol",
            serde_json::json!({ "query": "Person" }),
        )
        .expect("workspace symbols succeed");
    let symbols = response.as_array().expect("symbol array");
    assert!(
        symbols.iter().any(|s| s["name"] == "Person"),
        "user symbol found, got: {symbols:?}"
    );

    // A stdlib symbol resolves to a URI under the materialized directory.
    let response = harness
        .request(
            SessionKey(1),
            "workspace/symbol",
            serde_json::json!({ "query": "deep_copy" }),
        )
        .expect("workspace symbols succeed");
    let symbols = response.as_array().expect("symbol array");
    let stdlib_hit = symbols
        .iter()
        .find(|s| s["name"] == "deep_copy")
        .unwrap_or_else(|| panic!("stdlib symbol found, got: {symbols:?}"));
    let uri = stdlib_hit["location"]["uri"].as_str().unwrap();
    assert!(
        uri.starts_with(Url::from_file_path(&stdlib_dir).unwrap().as_str()),
        "stdlib URI maps under the materialized dir, got: {uri}"
    );
}

#[test]
fn semantic_tokens_delta_edits_only_the_changed_region() {
    let mut harness = feature_harness();
    let funcs = harness.uri("funcs.baml");

    let full = harness
        .request(
            SessionKey(1),
            "textDocument/semanticTokens/full",
            serde_json::json!({ "textDocument": { "uri": funcs } }),
        )
        .expect("full tokens succeed");
    let result_id = full["resultId"].as_str().expect("result id").to_owned();
    let data = full["data"].as_array().expect("token data");
    assert!(!data.is_empty(), "fixture produces tokens");

    // Unchanged document → an empty edit list against the same baseline.
    let delta = harness
        .request(
            SessionKey(1),
            "textDocument/semanticTokens/full/delta",
            serde_json::json!({
                "textDocument": { "uri": funcs },
                "previousResultId": result_id,
            }),
        )
        .expect("delta succeeds");
    assert_eq!(
        delta["edits"].as_array().map(Vec::len),
        Some(0),
        "got: {delta}"
    );

    // Append a declaration; the delta must be an edit, not a full resend.
    let changed =
        format!("{FUNCS_FIXTURE}\nfunction shout(p: Person) -> string {{\n    p.name\n}}\n");
    harness.open(SessionKey(1), &funcs, 1, FUNCS_FIXTURE);
    harness.settle();
    harness.change(SessionKey(1), &funcs, 2, &changed);
    harness.settle();
    let delta = harness
        .request(
            SessionKey(1),
            "textDocument/semanticTokens/full/delta",
            serde_json::json!({
                "textDocument": { "uri": funcs },
                "previousResultId": delta["resultId"].as_str().unwrap_or(&result_id),
            }),
        )
        .expect("delta succeeds");
    let edits = delta["edits"].as_array().expect("edits, not a full resend");
    assert_eq!(edits.len(), 1, "one contiguous edit, got: {delta}");

    // A stale/unknown baseline falls back to full tokens.
    let fallback = harness
        .request(
            SessionKey(1),
            "textDocument/semanticTokens/full/delta",
            serde_json::json!({
                "textDocument": { "uri": funcs },
                "previousResultId": "not-a-result-id",
            }),
        )
        .expect("delta succeeds");
    assert!(
        fallback.get("data").is_some(),
        "full fallback, got: {fallback}"
    );
}

#[test]
fn semantic_tokens_range_covers_a_subset() {
    let mut harness = feature_harness();
    let funcs = harness.uri("funcs.baml");
    let full = harness
        .request(
            SessionKey(1),
            "textDocument/semanticTokens/full",
            serde_json::json!({ "textDocument": { "uri": funcs } }),
        )
        .expect("full succeeds");
    let full_len = full["data"].as_array().map(Vec::len).unwrap_or(0);

    let ranged = harness
        .request(
            SessionKey(1),
            "textDocument/semanticTokens/range",
            serde_json::json!({
                "textDocument": { "uri": funcs },
                "range": {
                    "start": pos_of(FUNCS_FIXTURE, "function"),
                    "end": pos_of(FUNCS_FIXTURE, "{"),
                },
            }),
        )
        .expect("range succeeds");
    let ranged_len = ranged["data"].as_array().map(Vec::len).unwrap_or(0);
    assert!(ranged_len > 0, "signature line yields tokens");
    assert!(
        ranged_len < full_len,
        "range is a strict subset ({ranged_len} vs {full_len})"
    );
}

#[test]
fn inlay_hints_appear_for_inferred_let_types() {
    let mut harness = Harness::new();
    harness.fs.add_project(&harness.ws);
    let fixture = "function main() -> string {\n    let greeting = \"hi\"\n    greeting\n}\n";
    harness.fs.write(harness.ws.join("main.baml"), fixture);
    harness.init_session(SessionKey(1), &[lsp_types::PositionEncodingKind::UTF16]);
    harness.settle();

    let uri = harness.uri("main.baml");
    let response = harness
        .request(
            SessionKey(1),
            "textDocument/inlayHint",
            serde_json::json!({
                "textDocument": { "uri": uri },
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 4, "character": 0 },
                },
            }),
        )
        .expect("inlay hints succeed");
    let hints = response.as_array().expect("hint array");
    assert!(
        hints.iter().any(|hint| hint["label"] == ": string"),
        "let-binding type hint present, got: {hints:?}"
    );
}

// ── 2.3b-3 substrate: cancellation, incremental sync, diffing, rescue ────

/// `$/cancelRequest` cancels the running read's Salsa token: the parked job
/// unwinds with `Cancelled::Local` at its first query entry and reports
/// `RequestCanceled`; an uncancelled request on the same lane completes.
#[test]
fn cancel_request_unwinds_the_running_read() {
    let manual = Arc::new(ManualExecutor::default());
    let executors = Executors {
        requests: Arc::clone(&manual) as Arc<dyn Executor>,
        diagnostics: Arc::new(Inline),
        io: Arc::new(Inline),
    };
    let mut h = Harness::with_executors(executors, None);
    h.fs.add_project(&h.ws);
    h.fs.write(h.ws.join("main.baml"), "class A { x int }\n");
    let s = SessionKey(1);
    h.init_session(s, &[]);
    h.settle();
    let uri = h.uri("main.baml");

    let dispatch_hover = |h: &mut Harness, id: i32| {
        let (tx, rx) = std::sync::mpsc::channel();
        let req = lsp_server::Request::new(
            lsp_server::RequestId::from(id),
            "textDocument/hover".to_owned(),
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": 0, "character": 7 },
            }),
        );
        h.state
            .dispatch_request(s, req, Box::new(move |result| tx.send(result).unwrap()));
        rx
    };

    let rx = dispatch_hover(&mut h, 9);
    assert_eq!(manual.parked(), 1, "the read is parked on the request lane");
    h.notify(s, "$/cancelRequest", json!({ "id": 9 })).unwrap();
    manual.run_all();
    let deadline = Instant::now() + Duration::from_secs(10);
    let result = loop {
        if let Ok(result) = rx.try_recv() {
            break result;
        }
        assert!(Instant::now() < deadline, "no completion for the read");
        if let Ok(event) = h.state.events().recv_timeout(Duration::from_millis(20)) {
            h.state.handle_event(event);
        }
    };
    match result {
        Err(LspError::RequestCanceled(_)) => {}
        other => panic!("expected RequestCanceled, got {other:?}"),
    }

    // Control: the same request without a cancel completes normally.
    let rx = dispatch_hover(&mut h, 10);
    manual.run_all();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(result) = rx.try_recv() {
            result.expect("uncancelled hover succeeds");
            break;
        }
        assert!(Instant::now() < deadline, "no completion for the read");
        if let Ok(event) = h.state.events().recv_timeout(Duration::from_millis(20)) {
            h.state.handle_event(event);
        }
    }
}

/// Incremental change events apply in order, each against the text produced
/// by the ones before it, with ranges measured in the session's negotiated
/// encoding (UTF-16 here: 🐑 is two units). A rangeless event still replaces
/// the whole document, so FULL-sync clients keep working.
#[test]
fn incremental_changes_apply_in_order_with_utf16_positions() {
    let mut h = Harness::new();
    h.fs.add_project(&h.ws);
    let s = SessionKey(1);
    h.init_session(s, &[]);
    let uri = h.uri("main.baml");
    let path = h.ws.join("main.baml");
    h.open(s, &uri, 1, "/// doc\nclass A {\n  s string\n}\n");

    // Two deltas in one batch: the second is positioned in the coordinates
    // produced by the first (`🐑🐑` occupies UTF-16 units 4..8).
    h.notify(
        s,
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [
                {
                    "range": { "start": { "line": 0, "character": 4 }, "end": { "line": 0, "character": 7 } },
                    "text": "🐑🐑",
                },
                {
                    "range": { "start": { "line": 0, "character": 6 }, "end": { "line": 0, "character": 8 } },
                    "text": " sheep",
                },
            ],
        }),
    )
    .unwrap();
    assert_eq!(
        h.state.file_text(&path).as_deref(),
        Some("/// 🐑 sheep\nclass A {\n  s string\n}\n")
    );
    assert_eq!(
        h.state.open_document(&path).unwrap().version,
        Some(2),
        "the overlay tracks the change's version"
    );

    // A rangeless event replaces the document; a delta after it applies to
    // the replacement.
    h.notify(
        s,
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": uri, "version": 3 },
            "contentChanges": [
                { "text": "class B {}\n" },
                {
                    "range": { "start": { "line": 0, "character": 6 }, "end": { "line": 0, "character": 7 } },
                    "text": "C",
                },
            ],
        }),
    )
    .unwrap();
    assert_eq!(h.state.file_text(&path).as_deref(), Some("class C {}\n"));
}

/// Per-file diffing: a pass after an edit republishes only the files whose
/// text or diagnostics changed; the untouched file is not resent.
#[test]
fn unchanged_files_are_not_republished() {
    let mut h = Harness::new();
    h.fs.add_project(&h.ws);
    h.fs.write(h.ws.join("clean.baml"), "class Clean { x int }\n");
    let s = SessionKey(1);
    h.init_session(s, &[]);
    let bad_uri = h.uri("bad.baml");
    h.open(s, &bad_uri, 1, BAD_SOURCE);
    h.settle();

    let clean_uri = h.uri("clean.baml");
    let clean_before = h.sender(s).publications_for(&clean_uri).len();
    let bad_before = h.sender(s).publications_for(&bad_uri).len();
    assert!(clean_before > 0, "the first publication covers every file");
    assert!(has_error(
        h.sender(s).publications_for(&bad_uri).last().unwrap()
    ));

    h.change(s, &bad_uri, 2, "class A {\n  x AlsoUndefined\n}\n");
    h.settle();

    assert_eq!(
        h.sender(s).publications_for(&clean_uri).len(),
        clean_before,
        "the untouched file is not republished"
    );
    let bad_after = h.sender(s).publications_for(&bad_uri);
    assert!(bad_after.len() > bad_before, "the edited file republishes");
    assert!(has_error(bad_after.last().unwrap()));
}

/// A session that initializes after the workspace has settled receives the
/// standing diagnostics in full (per-file diffing must not starve it), and
/// the recompute does not respam the session that already has them.
#[test]
fn a_late_session_receives_standing_diagnostics_in_full() {
    let mut h = Harness::new();
    h.fs.add_project(&h.ws);
    h.fs.write(h.ws.join("bad.baml"), BAD_SOURCE);
    let s1 = SessionKey(1);
    h.init_session(s1, &[]);
    h.settle();

    let bad_uri = h.uri("bad.baml");
    let s1_before = h.sender(s1).publications_for(&bad_uri).len();
    assert!(s1_before > 0);

    // No workspace folders: nothing but the new-session schedule triggers
    // the publication pass.
    let s2 = SessionKey(2);
    h.init_session_with_folders(s2, &[], &[]);
    h.settle();

    let s2_publications = h.sender(s2).publications_for(&bad_uri);
    assert!(
        s2_publications.last().is_some_and(has_error),
        "the late session sees the standing error: {s2_publications:?}"
    );
    assert_eq!(
        h.sender(s1).publications_for(&bad_uri).len(),
        s1_before,
        "the already-current session is not respammed"
    );
}

/// A scratch document outside every project mints a provisional root that
/// holds the single-workspace slot; the real project's discovery is refused
/// while it lives. Closing the scratch document frees the slot, and the
/// owner re-runs discovery so the real project loads without further client
/// action.
#[test]
fn closing_a_detached_document_frees_the_workspace_slot() {
    let mut h = Harness::new();
    h.fs.add_project(&h.ws);
    h.fs.write(h.ws.join("main.baml"), "class A { x int }\n");
    let s = SessionKey(1);
    h.init_session_with_folders(s, &[], &[]);

    let scratch_dir = tempfile::tempdir().unwrap();
    let scratch = scratch_dir.path().canonicalize().unwrap();
    let scratch_uri = Url::from_file_path(scratch.join("scratch.baml")).unwrap();
    h.open(s, &scratch_uri, 1, "class S { y int }\n");
    h.settle();
    let roots: Vec<PathBuf> = h
        .state
        .roots()
        .workspace_roots()
        .map(|entry| entry.path.clone())
        .collect();
    assert_eq!(
        roots,
        vec![scratch.clone()],
        "the scratch dir holds the slot"
    );

    // The client announces the real workspace folder; the guard refuses it.
    h.notify(
        s,
        "workspace/didChangeWorkspaceFolders",
        json!({
            "event": {
                "added": [{ "uri": Url::from_file_path(&h.ws).unwrap(), "name": "ws" }],
                "removed": [],
            }
        }),
    )
    .unwrap();
    h.settle();
    let roots: Vec<PathBuf> = h
        .state
        .roots()
        .workspace_roots()
        .map(|entry| entry.path.clone())
        .collect();
    assert_eq!(roots, vec![scratch], "the slot is still held");

    // Closing the scratch document removes its provisional root; the freed
    // slot triggers rediscovery of the announced folder.
    h.close(s, &scratch_uri);
    h.settle();
    let roots: Vec<PathBuf> = h
        .state
        .roots()
        .workspace_roots()
        .map(|entry| entry.path.clone())
        .collect();
    assert_eq!(roots, vec![h.ws.clone()], "the real project loaded");
}

/// A diagnostics pass unwound by `PropagatedPanic` retries; one unwound by
/// a REAL panic does not.
///
/// Salsa raises `PropagatedPanic` whenever the thread computing a query this
/// pass blocked on released its claim by unwinding for anything other than
/// that thread's own `$/cancelRequest` — a mutation cancelling the producer
/// included. So it usually means the same thing `PendingWrite` means, and
/// refusing to retry stalled diagnostics until the next edit every time a
/// request and the pass raced one keystroke. A genuine panic still stops the
/// retry, on the `Panicked` arm, where the panicking thread has already
/// logged the message and backtrace.
#[test]
fn a_propagated_panic_retries_diagnostics_but_a_real_panic_does_not() {
    // `true` = a real panic, `false` = salsa's `PropagatedPanic`. Passed as
    // a flag so the outcome is built inside, where its types are in scope.
    let park_a_pass = |real_panic: bool| {
        let outcome = if real_panic {
            Err(TaskFailure::Panicked("boom".to_owned()))
        } else {
            Err(TaskFailure::Cancelled(salsa::Cancelled::PropagatedPanic))
        };
        let manual = Arc::new(ManualExecutor::default());
        let executors = Executors {
            requests: Arc::new(Inline),
            diagnostics: Arc::clone(&manual) as Arc<dyn Executor>,
            io: Arc::new(Inline),
        };
        let mut h = Harness::with_executors(executors, None);
        h.fs.add_project(&h.ws);
        h.fs.write(h.ws.join("main.baml"), "class A { x int }\n");
        let s = SessionKey(1);
        h.init_session(s, &[]);
        // Apply the queued discovery mutations before any snapshot can park
        // on the manual lane (a parked snapshot would block `apply`).
        while let Ok(event) = h.state.events().try_recv() {
            h.state.handle_event(event);
        }
        let root = h
            .state
            .roots()
            .workspace_roots()
            .next()
            .expect("the discovered project root")
            .root;

        // Fire the tail: the pass parks on the manual lane, in flight.
        h.state.on_tick(Instant::now() + DIAGNOSTICS_DEBOUNCE * 2);
        while let Ok(event) = h.state.events().try_recv() {
            h.state.handle_event(event);
        }
        assert_eq!(manual.parked(), 1, "the diagnostics pass is parked");
        assert!(h.state.root_state(root).unwrap().diagnostics_in_flight);

        h.state
            .handle_event(OwnerEvent::DiagnosticsResult { root, outcome });

        let root_state = h.state.root_state(root).unwrap();
        assert!(!root_state.diagnostics_in_flight);
        assert!(
            root_state.fence.is_dirty(),
            "the root still owes a publication"
        );
        let reposted = matches!(
            h.state.events().try_recv(),
            Ok(OwnerEvent::DiagnosticsDue { .. })
        );
        // Cleanup: run the parked pass so its snapshot drops before the state.
        manual.run_all();
        reposted
    };

    assert!(
        park_a_pass(false),
        "a producer that unwound (all but certainly a mutation) is retried"
    );
    assert!(
        !park_a_pass(true),
        "a real panic waits for the next edit rather than re-running the \
         query that panicked"
    );
}
