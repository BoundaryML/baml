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
    executor::ThreadPool,
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
        let temp = tempfile::tempdir().unwrap();
        let ws = temp.path().canonicalize().unwrap();
        let fs = Arc::new(MemFs::default());
        let state = GlobalState::with_fs(
            Box::new(ThreadPool::new(2)),
            stdlib_dir,
            Arc::clone(&fs) as Arc<dyn ProjectFs>,
        );
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

    let error = h.request(s, "textDocument/hover", json!({})).unwrap_err();
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

    // Incremental sync is rejected.
    let uri = h.uri("main.baml");
    h.open(s, &uri, 1, "class A { x int }\n");
    let error = h
        .notify(
            s,
            "textDocument/didChange",
            json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "text": "x" }] }),
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
