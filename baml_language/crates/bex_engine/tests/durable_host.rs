//! Durable functions proof of concept: `DurableHost` hooks and the
//! `VmExecState::RemoteCall` intercept.
//!
//! The intercept is off unless a host is installed. With a host, a call to a
//! user function whose name starts with `remote_` is handed to the host, while
//! the root function of a call (even a `remote_` one) executes locally.

mod common;

use std::sync::{Arc, Mutex};

use bex_engine::{
    BexEngine, BexExternalValue, CancellationToken, FunctionCallContextBuilder,
    durable::{
        DurableHost, DurableThreadId, RemoteCallRequest, RemoteCancel, RemoteResultWait,
        YieldPosition, YieldReason,
    },
};
use common::compile_for_engine;
use futures::future::BoxFuture;
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
class Report {
  city string
  degrees int
}

function remote_report(city: string, degrees: int) -> Report {
  Report { city: city + " (local)", degrees: degrees }
}

function caller(city: string) -> string {
  let report = remote_report(city, 20);
  report.city + ": " + report.degrees.to_string()
}

function spawning_caller(city: string) -> string {
  let pending = spawn { remote_report(city, 5) };
  let report = await pending;
  report.city
}

function catching_caller(city: string) -> string {
  {
    remote_report(city, 1).city
  } catch (e) {
    baml.errors.Io => "caught"
  }
}

function sleeper() -> int {
  baml.sys.sleep(baml.time.Duration.from_milliseconds(10n));
  7
}

function loop_caller(city: string) -> string {
  let seen = "";
  let day = 0;
  while (day < 2) {
    seen = seen + remote_report(city, day).city;
    day += 1;
  }
  seen
}

function helper(city: string) -> string {
  remote_report(city, 3).city
}

function nested_caller(city: string) -> string {
  helper(city)
}

function spawn_caller(city: string) -> string {
  let handle = spawn {
    let inner = remote_report(city, 4);
    inner
  };
  (await handle).city
}

function gc_caller(city: string) -> string {
  let notes: string[] = [];
  notes.push(city + " before");
  let first = remote_report(notes[0], 1);
  notes.push(first.city);
  let second = remote_report(notes[1], 2);
  notes.push(second.city);
  notes[0] + " | " + notes[1] + " | " + notes[2] + " | " + first.degrees.to_string()
}
"#;

fn engine() -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            compile_for_engine(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .unwrap(),
    )
}

fn ctx() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}

fn string(value: &str) -> BexExternalValue {
    BexExternalValue::String(value.into())
}

#[derive(Clone, Copy)]
enum Reply {
    /// Answer with a `Report` built from the request's arguments.
    Echo,
    Error,
    Never,
}

#[derive(Debug, PartialEq, Eq)]
enum Note {
    Started(DurableThreadId, Option<DurableThreadId>),
    Ended(DurableThreadId),
    Yielded(DurableThreadId, YieldReason, Option<String>, Option<usize>),
    Remote(DurableThreadId, String, Vec<String>),
}

struct RecordingHost {
    reply: Reply,
    notes: Mutex<Vec<Note>>,
    /// Announced calls by call id, for `remote_result`.
    calls: Mutex<std::collections::HashMap<String, RemoteCallRequest>>,
    /// The call site of every announced call, in announcement order.
    sites: Mutex<Vec<Option<YieldPosition>>>,
    /// The `spawn` site of every started thread, in order.
    spawn_sites: Mutex<Vec<(DurableThreadId, Option<YieldPosition>)>>,
    /// What `remote_cancelled` reported, in order.
    cancels: Mutex<Vec<RemoteCancel>>,
}

impl RecordingHost {
    fn new(reply: Reply) -> Arc<Self> {
        Arc::new(Self {
            reply,
            notes: Mutex::new(Vec::new()),
            calls: Mutex::new(std::collections::HashMap::new()),
            sites: Mutex::new(Vec::new()),
            spawn_sites: Mutex::new(Vec::new()),
            cancels: Mutex::new(Vec::new()),
        })
    }

    fn notes(&self) -> Vec<Note> {
        std::mem::take(&mut self.notes.lock().unwrap())
    }

    /// The call site of the `n`-th announced call.
    fn site(&self, n: usize) -> YieldPosition {
        self.sites.lock().unwrap()[n]
            .clone()
            .unwrap_or_else(|| panic!("call {n} reported no site"))
    }

    /// The `spawn` site reported for the one thread that has a parent.
    fn only_spawn_site(&self) -> Option<YieldPosition> {
        let sites = self.spawn_sites.lock().unwrap();
        let spawned: Vec<_> = sites.iter().skip(1).collect();
        assert_eq!(spawned.len(), 1, "expected one spawned thread: {sites:?}");
        spawned[0].1.clone()
    }
}

/// The 1-based line of the only line of [`SOURCE`] that contains `needle`.
fn line_of(needle: &str) -> usize {
    let mut found = None;
    for (index, line) in SOURCE.lines().enumerate() {
        if line.contains(needle) {
            assert!(found.is_none(), "`{needle}` is on more than one line");
            found = Some(index + 1);
        }
    }
    found.unwrap_or_else(|| panic!("`{needle}` is not in the source"))
}

impl DurableHost for RecordingHost {
    fn remote_call(
        &self,
        request: RemoteCallRequest,
    ) -> BoxFuture<'static, Result<String, String>> {
        self.notes.lock().unwrap().push(Note::Remote(
            request.thread,
            request.function.clone(),
            request.args.iter().map(|arg| arg.name.clone()).collect(),
        ));
        self.sites.lock().unwrap().push(request.site.clone());
        let mut calls = self.calls.lock().unwrap();
        let call_id = format!("c{}", calls.len() + 1);
        calls.insert(call_id.clone(), request);
        Box::pin(async move { Ok(call_id) })
    }

    fn remote_result(
        &self,
        wait: RemoteResultWait,
    ) -> BoxFuture<'static, Result<BexExternalValue, String>> {
        let request = self
            .calls
            .lock()
            .unwrap()
            .get(&wait.call_id)
            .cloned()
            .expect("remote_result for a call that was announced");
        let reply = self.reply;
        Box::pin(async move {
            // Give up the executor once so the caller really parks.
            tokio::task::yield_now().await;
            match reply {
                Reply::Echo => {
                    let mut fields = indexmap::IndexMap::new();
                    for arg in request.args {
                        let value = match (arg.name.as_str(), arg.value) {
                            ("city", BexExternalValue::String(city)) => {
                                BexExternalValue::String(format!("{city} (remote)").into())
                            }
                            (_, other) => other,
                        };
                        fields.insert(arg.name, value);
                    }
                    Ok(BexExternalValue::Instance {
                        class_name: "user.Report".to_string(),
                        type_args: Vec::new(),
                        fields,
                    })
                }
                Reply::Error => Err("cloud is down".to_string()),
                Reply::Never => std::future::pending().await,
            }
        })
    }

    fn thread_started(
        &self,
        thread: DurableThreadId,
        parent: Option<DurableThreadId>,
        site: Option<&YieldPosition>,
    ) {
        self.notes
            .lock()
            .unwrap()
            .push(Note::Started(thread, parent));
        self.spawn_sites
            .lock()
            .unwrap()
            .push((thread, site.cloned()));
    }

    fn remote_cancelled(&self, cancel: RemoteCancel) {
        self.cancels.lock().unwrap().push(cancel);
    }

    fn thread_ended(&self, thread: DurableThreadId) {
        self.notes.lock().unwrap().push(Note::Ended(thread));
    }

    fn yielded(
        &self,
        thread: DurableThreadId,
        reason: YieldReason,
        op: Option<&str>,
        position: Option<&YieldPosition>,
    ) {
        self.notes.lock().unwrap().push(Note::Yielded(
            thread,
            reason,
            op.map(str::to_string),
            position.map(|p| p.line),
        ));
    }
}

#[tokio::test]
async fn remote_functions_run_locally_without_a_host() {
    let engine = engine();
    let result = engine
        .call_function("caller", vec![string("Lisbon")], ctx(), true)
        .await
        .unwrap();
    assert_eq!(result, string("Lisbon (local): 20"));
}

#[tokio::test]
async fn host_services_a_direct_remote_call() {
    let engine = engine();
    let host = RecordingHost::new(Reply::Echo);
    engine.set_durable_host(Some(host.clone()));
    let result = engine
        .call_function("caller", vec![string("Lisbon")], ctx(), true)
        .await
        .unwrap();
    assert_eq!(result, string("Lisbon (remote): 20"));

    let notes = host.notes();
    let Note::Started(root, None) = notes[0] else {
        panic!("expected the root thread to start first: {notes:?}");
    };
    assert_eq!(
        notes,
        vec![
            Note::Started(root, None),
            // `let report = remote_report(city, 20);` is line 12 of SOURCE.
            Note::Yielded(
                root,
                YieldReason::RemoteCall,
                Some("user.remote_report".to_string()),
                Some(12)
            ),
            Note::Remote(
                root,
                "user.remote_report".to_string(),
                vec!["city".to_string(), "degrees".to_string()]
            ),
            Note::Ended(root),
        ]
    );

    // Clearing the host restores local execution.
    engine.set_durable_host(None);
    let result = engine
        .call_function("caller", vec![string("Lisbon")], ctx(), true)
        .await
        .unwrap();
    assert_eq!(result, string("Lisbon (local): 20"));
    assert!(host.notes().is_empty());
}

#[tokio::test]
async fn a_remote_function_started_as_the_root_runs_locally() {
    let engine = engine();
    let host = RecordingHost::new(Reply::Never);
    engine.set_durable_host(Some(host.clone()));
    let result = engine
        .call_function(
            "remote_report",
            vec![string("Porto"), BexExternalValue::Int(3)],
            ctx(),
            true,
        )
        .await
        .unwrap();
    let BexExternalValue::Instance { fields, .. } = result else {
        panic!("expected an instance, got {result:?}");
    };
    assert_eq!(fields["city"], string("Porto (local)"));
    assert!(
        !host
            .notes()
            .iter()
            .any(|note| matches!(note, Note::Remote(..))),
        "the root call must not be intercepted"
    );
}

#[tokio::test]
async fn spawned_threads_inherit_the_host() {
    let engine = engine();
    let host = RecordingHost::new(Reply::Echo);
    engine.set_durable_host(Some(host.clone()));
    let result = engine
        .call_function("spawning_caller", vec![string("Faro")], ctx(), true)
        .await
        .unwrap();
    assert_eq!(result, string("Faro (remote)"));

    let notes = host.notes();
    let Note::Started(root, None) = notes[0] else {
        panic!("expected the root thread to start first: {notes:?}");
    };
    let child = notes
        .iter()
        .find_map(|note| match note {
            Note::Started(child, Some(parent)) if *parent == root => Some(*child),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no spawned thread reported: {notes:?}"));
    assert!(notes.contains(&Note::Remote(
        child,
        "user.remote_report".to_string(),
        vec!["city".to_string(), "degrees".to_string()]
    )));
    assert!(notes.contains(&Note::Ended(child)));
    assert!(notes.contains(&Note::Ended(root)));
}

#[tokio::test]
async fn a_remote_error_is_a_catchable_throw() {
    let engine = engine();
    let host = RecordingHost::new(Reply::Error);
    engine.set_durable_host(Some(host.clone()));

    let caught = engine
        .call_function("catching_caller", vec![string("Faro")], ctx(), true)
        .await
        .unwrap();
    assert_eq!(caught, string("caught"));

    let uncaught = engine
        .call_function("caller", vec![string("Faro")], ctx(), true)
        .await
        .unwrap_err();
    assert!(uncaught.to_string().contains("cloud is down"), "{uncaught}");
}

#[tokio::test]
async fn cancelling_a_pending_remote_call_ends_the_run() {
    let engine = engine();
    let host = RecordingHost::new(Reply::Never);
    engine.set_durable_host(Some(host.clone()));
    let cancel = CancellationToken::new();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_cancel_token(cancel.clone())
        .build();
    let run = tokio::spawn({
        let engine = Arc::clone(&engine);
        async move {
            engine
                .call_function("caller", vec![string("Faro")], call_ctx, true)
                .await
        }
    });
    // Wait until the call is pending, then cancel.
    while !host
        .notes
        .lock()
        .unwrap()
        .iter()
        .any(|note| matches!(note, Note::Remote(..)))
    {
        tokio::task::yield_now().await;
    }
    cancel.cancel();
    let error = run.await.unwrap().unwrap_err();
    assert!(error.to_string().contains("Cancelled"), "{error}");
    assert!(matches!(host.notes().last(), Some(Note::Ended(_))));
}

#[tokio::test]
async fn sys_op_yields_report_the_operation_and_line() {
    let engine = engine();
    let host = RecordingHost::new(Reply::Never);
    engine.set_durable_host(Some(host.clone()));
    let result = engine
        .call_function("sleeper", vec![], ctx(), true)
        .await
        .unwrap();
    assert_eq!(result, BexExternalValue::Int(7));
    let notes = host.notes();
    assert!(
        notes.iter().any(|note| matches!(
            note,
            // `baml.sys.sleep(...)` is line 31 of SOURCE.
            Note::Yielded(_, YieldReason::SysOp, Some(op), Some(31)) if op == "baml.sys.sleep"
        )),
        "{notes:?}"
    );
}

/// A host that runs a major collection while the calling thread waits. The
/// collection needs every heap permit parked, so this also checks that the
/// caller released its permit; the caller's heap locals and the converted
/// result must survive the move.
struct CollectingHost {
    engine: std::sync::OnceLock<std::sync::Weak<BexEngine>>,
    inner: Arc<RecordingHost>,
}

impl DurableHost for CollectingHost {
    fn remote_call(
        &self,
        request: RemoteCallRequest,
    ) -> BoxFuture<'static, Result<String, String>> {
        self.inner.remote_call(request)
    }

    fn remote_result(
        &self,
        wait: RemoteResultWait,
    ) -> BoxFuture<'static, Result<BexExternalValue, String>> {
        let engine = self
            .engine
            .get()
            .and_then(std::sync::Weak::upgrade)
            .unwrap();
        let reply = self.inner.remote_result(wait);
        Box::pin(async move {
            engine
                .collect_garbage(bex_heap::CollectionLevel::Major)
                .await;
            let result = reply.await;
            engine
                .collect_garbage(bex_heap::CollectionLevel::Major)
                .await;
            result
        })
    }

    fn thread_started(
        &self,
        thread: DurableThreadId,
        parent: Option<DurableThreadId>,
        site: Option<&YieldPosition>,
    ) {
        self.inner.thread_started(thread, parent, site);
    }

    fn thread_ended(&self, thread: DurableThreadId) {
        self.inner.thread_ended(thread);
    }

    fn yielded(
        &self,
        thread: DurableThreadId,
        reason: YieldReason,
        op: Option<&str>,
        position: Option<&YieldPosition>,
    ) {
        self.inner.yielded(thread, reason, op, position);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_collection_while_a_remote_call_is_pending_keeps_the_caller_intact() {
    let engine = engine();
    let host = Arc::new(CollectingHost {
        engine: std::sync::OnceLock::new(),
        inner: RecordingHost::new(Reply::Echo),
    });
    host.engine.set(Arc::downgrade(&engine)).unwrap();
    engine.set_durable_host(Some(host.clone()));
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        engine.call_function("gc_caller", vec![string("Lisbon")], ctx(), true),
    )
    .await
    .expect("the caller must release its heap permit while the remote call is pending")
    .unwrap();
    assert_eq!(
        result,
        string("Lisbon before | Lisbon before (remote) | Lisbon before (remote) (remote) | 1")
    );
}

// ============================================================================
// Contract 10.1: `remote_call` and `thread_started` carry the source location
// ============================================================================

#[tokio::test]
async fn a_remote_call_in_a_loop_body_reports_the_call_site() {
    let engine = engine();
    let host = RecordingHost::new(Reply::Echo);
    engine.set_durable_host(Some(host.clone()));
    engine
        .call_function("loop_caller", vec![string("Lisbon")], ctx(), true)
        .await
        .unwrap();

    let expected = line_of("seen = seen + remote_report(city, day).city;");
    let sites = host.sites.lock().unwrap().clone();
    assert_eq!(sites.len(), 2, "the loop body runs twice: {sites:?}");
    for (index, site) in sites.iter().enumerate() {
        let site = site
            .as_ref()
            .unwrap_or_else(|| panic!("call {index} reported no site"));
        assert_eq!(site.line, expected, "call {index}: {site:?}");
        assert_eq!(site.function, "user.loop_caller");
        assert!(!site.file.is_empty());
    }
}

#[tokio::test]
async fn a_remote_call_inside_a_called_function_reports_that_function() {
    let engine = engine();
    let host = RecordingHost::new(Reply::Echo);
    engine.set_durable_host(Some(host.clone()));
    engine
        .call_function("nested_caller", vec![string("Porto")], ctx(), true)
        .await
        .unwrap();

    // The innermost user frame is `helper`, not the `helper(city)` line of
    // `nested_caller`.
    let site = host.site(0);
    assert_eq!(site.line, line_of("remote_report(city, 3).city"));
    assert_eq!(site.function, "user.helper");
}

#[tokio::test]
async fn a_remote_call_inside_a_spawn_reports_the_spawned_body() {
    let engine = engine();
    let host = RecordingHost::new(Reply::Echo);
    engine.set_durable_host(Some(host.clone()));
    engine
        .call_function("spawn_caller", vec![string("Faro")], ctx(), true)
        .await
        .unwrap();

    let site = host.site(0);
    assert_eq!(site.line, line_of("let inner = remote_report(city, 4);"));
    // The site of the call, not the site of the `spawn` that made the thread.
    let spawn_site = host
        .only_spawn_site()
        .expect("the spawned thread reports its spawn site");
    assert_eq!(spawn_site.line, line_of("let handle = spawn {"));
    assert_eq!(spawn_site.function, "user.spawn_caller");
    assert_eq!(spawn_site.file, site.file);
}

#[tokio::test]
async fn the_root_thread_of_a_segment_has_no_spawn_site() {
    let engine = engine();
    let host = RecordingHost::new(Reply::Echo);
    engine.set_durable_host(Some(host.clone()));
    engine
        .call_function("caller", vec![string("Lisbon")], ctx(), true)
        .await
        .unwrap();

    let sites = host.spawn_sites.lock().unwrap().clone();
    assert_eq!(sites.len(), 1, "only the root thread runs: {sites:?}");
    assert_eq!(sites[0].1, None);
}
