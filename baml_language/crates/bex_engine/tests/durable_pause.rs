//! Durable functions proof of concept: pause a run into a snapshot and
//! continue it in a fresh engine.
//!
//! Every test runs a function in one engine, pauses it with
//! `BexEngine::durable_snapshot`, builds a second engine from the same source
//! (the stand-in for a new process), resumes there with
//! `BexEngine::durable_resume`, and compares the result with an uninterrupted
//! run.

#![cfg(not(target_arch = "wasm32"))]

mod common;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use bex_engine::{
    BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder,
    durable::{
        DurableHost, DurableThreadId, ParkedKind, PauseProgress, RemoteCallRequest,
        RemoteResultWait, RestoredInfo, SnapshotFailure, SnapshotMode, SnapshotReport,
        SnapshotRequest, YieldPosition, YieldReason,
    },
};
use common::compile_for_engine;
use futures::future::BoxFuture;
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
class Plan {
  city string
  ideas string[]
  weather string
}

function remote_weather(city: string) -> string {
  "local weather in " + city
}

function spin(n: int) -> int {
  let items: int[] = [];
  let i = 0;
  while (i < n) {
    items.push(i * 2);
    i += 1;
  }
  let sum = 0;
  for (let x in items) {
    sum += x;
  }
  sum
}

function napper(city: string) -> Plan {
  let ideas: string[] = [];
  let day = 1;
  while (day < 4) {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(150n));
    ideas.push("day " + day.to_string() + " in " + city);
    day += 1;
  }
  Plan { city: city, ideas: ideas, weather: "unknown" }
}

function caller(city: string) -> Plan {
  let ideas: string[] = ["walk in " + city];
  let weather = remote_weather(city);
  Plan { city: city, ideas: ideas, weather: weather }
}

function catching_caller(city: string) -> string {
  {
    remote_weather(city)
  } catch (e) {
    baml.errors.Io => "caught for " + city
  }
}

function parallel(city: string) -> string {
  let pending = spawn { remote_weather(city) };
  let day = 1;
  while (day < 4) {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(100n));
    day += 1;
  }
  let weather = await pending;
  weather + " after " + day.to_string()
}

function bump(x: int) -> int {
  x + 1
}

function spin_calls(n: int) -> int {
  let total = 0;
  let i = 0;
  while (i < n) {
    let a = bump(i);
    let b = bump(a) + bump(total);
    let label = "i=" + i.to_string() + " a=" + a.to_string();
    total = bump(b) + label.length() - label.length();
    total = bump(total) - b;
    i = bump(i);
  }
  total
}

function kick(city: string) -> int {
  let forgotten = spawn {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(400n));
    1
  };
  0
}

function forgetful(city: string) -> string {
  kick(city);
  let day = 1;
  while (day < 10) {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(100n));
    day += 1;
  }
  city + " after " + day.to_string()
}

function detached(city: string) -> int {
  let forgotten = spawn {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(600n));
    1
  };
  0
}

function racer(city: string) -> string {
  let nothing: baml.future.Future<int, never>[] = [];
  let never_settles = baml.future.race(nothing);
  let day = 1;
  while (day < 8) {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(100n));
    day += 1;
  }
  city + " after " + day.to_string()
}
"#;

const SPIN_N: i64 = 400_000;

fn engine() -> (Arc<BexEngine>, [u8; 32]) {
    let program = compile_for_engine(SOURCE);
    let hash = bex_snapshot::program_hash(&program).expect("program hashes");
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );
    (engine, hash)
}

fn ctx() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}

fn string(value: &str) -> BexExternalValue {
    BexExternalValue::String(value.into())
}

/// A host whose remote results are delivered by the test.
#[derive(Default)]
struct TestHost {
    started: Mutex<Vec<(DurableThreadId, Option<DurableThreadId>)>>,
    yields: Mutex<Vec<(YieldReason, Option<String>)>>,
    announced: Mutex<Vec<String>>,
    results: Mutex<HashMap<String, Result<BexExternalValue, String>>>,
    changed: tokio::sync::Notify,
}

impl TestHost {
    fn deliver(&self, call_id: &str, result: Result<BexExternalValue, String>) {
        self.results
            .lock()
            .unwrap()
            .insert(call_id.to_string(), result);
        self.changed.notify_waiters();
    }

    async fn wait_until(&self, condition: impl Fn(&Self) -> bool) {
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let changed = self.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if condition(self) {
                    return;
                }
                tokio::select! {
                    () = &mut changed => {}
                    () = tokio::time::sleep(Duration::from_millis(5)) => {}
                }
            }
        })
        .await
        .expect("the run reaches the expected state");
    }

    fn saw_yield(&self, reason: YieldReason, op: Option<&str>) -> bool {
        self.yields
            .lock()
            .unwrap()
            .iter()
            .any(|(r, o)| *r == reason && o.as_deref() == op)
    }
}

/// The engine-facing side of a [`TestHost`].
struct HostRef(Arc<TestHost>);

fn new_host() -> (Arc<TestHost>, Arc<dyn DurableHost>) {
    let host = Arc::new(TestHost::default());
    (Arc::clone(&host), Arc::new(HostRef(host)))
}

impl DurableHost for HostRef {
    fn remote_call(
        &self,
        request: RemoteCallRequest,
    ) -> BoxFuture<'static, Result<String, String>> {
        let mut announced = self.0.announced.lock().unwrap();
        let call_id = format!("r-test-c{}", announced.len() + 1);
        announced.push(call_id.clone());
        drop(announced);
        assert_eq!(request.function, "user.remote_weather");
        self.0.changed.notify_waiters();
        Box::pin(async move { Ok(call_id) })
    }

    fn remote_result(
        &self,
        wait: RemoteResultWait,
    ) -> BoxFuture<'static, Result<BexExternalValue, String>> {
        let host = Arc::clone(&self.0);
        Box::pin(async move {
            loop {
                let changed = host.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if let Some(result) = host.results.lock().unwrap().remove(&wait.call_id) {
                    return result;
                }
                changed.await;
            }
        })
    }

    fn thread_started(&self, thread: DurableThreadId, parent: Option<DurableThreadId>) {
        self.0.started.lock().unwrap().push((thread, parent));
        self.0.changed.notify_waiters();
    }

    fn thread_ended(&self, _thread: DurableThreadId) {}

    fn yielded(
        &self,
        _thread: DurableThreadId,
        reason: YieldReason,
        op: Option<&str>,
        _position: Option<&YieldPosition>,
    ) {
        self.0
            .yields
            .lock()
            .unwrap()
            .push((reason, op.map(str::to_string)));
        self.0.changed.notify_waiters();
    }
}

#[derive(Debug)]
struct Snapshot {
    bytes: Vec<u8>,
    state: serde_json::Value,
}

fn request(hash: [u8; 32], mode: SnapshotMode) -> SnapshotRequest {
    SnapshotRequest {
        run_id: "r-test".to_string(),
        segment: 1,
        seq: 1,
        program_hash: hash,
        embed_program: None,
        compress: true,
        mode,
    }
}

async fn snapshot(
    engine: &Arc<BexEngine>,
    hash: [u8; 32],
    mode: SnapshotMode,
    progress: &Mutex<Vec<PauseProgress>>,
) -> Result<SnapshotReport<Snapshot>, SnapshotFailure> {
    engine
        .durable_snapshot(
            request(hash, mode),
            || b"run state".to_vec(),
            |event| progress.lock().unwrap().push(event),
            |parts| {
                Ok(Snapshot {
                    bytes: parts.bytes.to_vec(),
                    state: parts.state_dump.clone(),
                })
            },
        )
        .await
}

type RunHandle = tokio::task::JoinHandle<Result<BexExternalValue, EngineError>>;

fn start(
    engine: &Arc<BexEngine>,
    function: &'static str,
    args: Vec<BexExternalValue>,
) -> RunHandle {
    let engine = Arc::clone(engine);
    tokio::spawn(async move { engine.call_function(function, args, ctx(), true).await })
}

async fn plain(function: &'static str, args: Vec<BexExternalValue>) -> BexExternalValue {
    let (engine, _) = engine();
    engine
        .call_function(function, args, ctx(), true)
        .await
        .unwrap()
}

/// Resume `snapshot` in a fresh engine with a fresh host.
async fn resume(
    function: &'static str,
    snapshot: &Snapshot,
    prepare: impl FnOnce(&Arc<BexEngine>, &Arc<TestHost>),
) -> (
    Result<BexExternalValue, EngineError>,
    RestoredInfo,
    Arc<TestHost>,
) {
    let (engine, hash) = engine();
    let (host, shared) = new_host();
    engine.set_durable_host(Some(shared));
    prepare(&engine, &host);
    let info = Mutex::new(None);
    let result = tokio::time::timeout(
        Duration::from_secs(60),
        engine.durable_resume(function, &snapshot.bytes, hash, ctx(), true, |restored| {
            *info.lock().unwrap() = Some(restored.clone());
        }),
    )
    .await
    .expect("the resumed run finishes");
    let info = info.into_inner().unwrap().expect("the run was restored");
    (result, info, host)
}

fn local<'a>(state: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    state["threads"][0]["frames"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|frame| frame["locals"].as_array().unwrap())
        .find(|local| local["name"] == name)
        .unwrap_or_else(|| panic!("no local `{name}` in {state:#}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_compute_loop_pauses_at_an_early_yield_and_resumes_elsewhere() {
    let expected = BexExternalValue::Int((0..SPIN_N).map(|i| i * 2).sum());
    assert_eq!(
        plain("spin", vec![BexExternalValue::Int(SPIN_N)]).await,
        expected
    );

    let (engine, hash) = engine();
    let (host, shared) = new_host();
    engine.set_durable_host(Some(shared));
    let run = start(&engine, "spin", vec![BexExternalValue::Int(SPIN_N)]);
    host.wait_until(|host| !host.started.lock().unwrap().is_empty())
        .await;

    let progress = Mutex::new(Vec::new());
    let report = snapshot(&engine, hash, SnapshotMode::Suspend, &progress)
        .await
        .expect("a compute loop has clean points");
    assert_eq!(report.threads, 1);
    assert_eq!(report.blocked_attempts, 0);
    assert!(report.stats.objects > 0);
    assert_eq!(*progress.lock().unwrap(), Vec::new());
    assert_eq!(
        run.await.unwrap().unwrap_err(),
        EngineError::DurableSuspended,
        "the paused thread's call ends without a result"
    );
    assert!(!engine.durable_run_is_live());

    let state = &report.committed.state;
    assert_eq!(state["threads"][0]["parked"]["kind"], "runnable");
    assert_eq!(state["threads"][0]["frames"][0]["function"], "user.spin");
    // The pause landed in the middle of the computation, not at its end.
    let i: i64 = local(state, "i")["value"]["preview"]
        .as_str()
        .and_then(|preview| preview.parse().ok())
        .expect("`i` is an int local");
    assert!(i > 0 && i < SPIN_N, "paused at i = {i}");
    assert!(report.pause_latency_ms < 2_000.0, "{report:?}");

    let (result, info, _) = resume("spin", &report.committed, |_, _| {}).await;
    assert_eq!(info.root_parked, ParkedKind::Runnable);
    assert_eq!(info.run_state, b"run state");
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_sleep_is_interrupted_and_finished_after_the_resume() {
    let expected = plain("napper", vec![string("Lisbon")]).await;

    let (engine, hash) = engine();
    let (host, shared) = new_host();
    engine.set_durable_host(Some(shared));
    let run = start(&engine, "napper", vec![string("Lisbon")]);
    host.wait_until(|host| host.saw_yield(YieldReason::SysOp, Some("baml.sys.sleep")))
        .await;

    let progress = Mutex::new(Vec::new());
    let requested = std::time::Instant::now();
    let report = snapshot(&engine, hash, SnapshotMode::Suspend, &progress)
        .await
        .expect("a sleep is a clean point");
    assert!(
        requested.elapsed() < Duration::from_millis(140),
        "the pause must not wait for the sleep to end ({:?})",
        requested.elapsed()
    );
    assert_eq!(
        *progress.lock().unwrap(),
        Vec::new(),
        "a sleep is not reported as in flight"
    );
    assert_eq!(
        run.await.unwrap().unwrap_err(),
        EngineError::DurableSuspended
    );

    let state = &report.committed.state;
    assert_eq!(state["threads"][0]["parked"]["kind"], "sleep");
    assert!(
        state["threads"][0]["parked"]["detail"]
            .as_str()
            .unwrap()
            .contains("deadline_unix_ms")
    );

    let (result, info, _) = resume("napper", &report.committed, |_, _| {}).await;
    assert!(matches!(info.root_parked, ParkedKind::Sleep { .. }));
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_paused_in_a_remote_wait_takes_the_result_after_the_resume() {
    let (engine, hash) = engine();
    let (host, shared) = new_host();
    engine.set_durable_host(Some(shared));
    let run = start(&engine, "caller", vec![string("Porto")]);
    host.wait_until(|host| !host.announced.lock().unwrap().is_empty())
        .await;

    let progress = Mutex::new(Vec::new());
    let report = snapshot(&engine, hash, SnapshotMode::Suspend, &progress)
        .await
        .expect("the wait for a remote result is a clean point");
    assert_eq!(
        run.await.unwrap().unwrap_err(),
        EngineError::DurableSuspended
    );
    let state = &report.committed.state;
    assert_eq!(state["threads"][0]["parked"]["kind"], "remote_call");

    // Result known at resume time.
    let (result, info, resumed_host) = resume("caller", &report.committed, |_, host| {
        host.deliver("r-test-c1", Ok(string("sunny in Porto")));
    })
    .await;
    let ParkedKind::RemoteCall {
        call_id, function, ..
    } = &info.root_parked
    else {
        panic!("expected a remote_call park, got {:?}", info.root_parked);
    };
    assert_eq!(call_id, "r-test-c1");
    assert_eq!(function, "user.remote_weather");
    let BexExternalValue::Instance { fields, .. } = result.unwrap() else {
        panic!("expected a Plan");
    };
    assert_eq!(fields["weather"], string("sunny in Porto"));
    assert_eq!(fields["city"], string("Porto"));
    assert_eq!(
        fields["ideas"],
        BexExternalValue::Array {
            element_type: bex_engine::RuntimeTy::String,
            items: vec![string("walk in Porto")],
        }
    );
    assert!(
        resumed_host.announced.lock().unwrap().is_empty(),
        "the call is not announced a second time"
    );
    assert_eq!(resumed_host.started.lock().unwrap().len(), 1);
    assert_eq!(resumed_host.started.lock().unwrap()[0].1, None);

    // Result delivered some time after the resume.
    let (result, _, _) = resume("caller", &report.committed, |_, host| {
        let host = Arc::clone(host);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            host.deliver("r-test-c1", Ok(string("rain in Porto")));
        });
    })
    .await;
    let BexExternalValue::Instance { fields, .. } = result.unwrap() else {
        panic!("expected a Plan");
    };
    assert_eq!(fields["weather"], string("rain in Porto"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_remote_error_after_the_resume_is_still_catchable() {
    let (engine, hash) = engine();
    let (host, shared) = new_host();
    engine.set_durable_host(Some(shared));
    let run = start(&engine, "catching_caller", vec![string("Faro")]);
    host.wait_until(|host| !host.announced.lock().unwrap().is_empty())
        .await;
    let progress = Mutex::new(Vec::new());
    let report = snapshot(&engine, hash, SnapshotMode::Suspend, &progress)
        .await
        .unwrap();
    assert_eq!(
        run.await.unwrap().unwrap_err(),
        EngineError::DurableSuspended
    );

    let (result, _, _) = resume("catching_caller", &report.committed, |_, host| {
        host.deliver("r-test-c1", Err("cloud is down".to_string()));
    })
    .await;
    assert_eq!(result.unwrap(), string("caught for Faro"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_second_pause_during_the_resumed_remote_wait_works() {
    let (engine, hash) = engine();
    let (host, shared) = new_host();
    engine.set_durable_host(Some(shared));
    let run = start(&engine, "caller", vec![string("Porto")]);
    host.wait_until(|host| !host.announced.lock().unwrap().is_empty())
        .await;
    let progress = Mutex::new(Vec::new());
    let first = snapshot(&engine, hash, SnapshotMode::Suspend, &progress)
        .await
        .unwrap();
    assert_eq!(
        run.await.unwrap().unwrap_err(),
        EngineError::DurableSuspended
    );

    // Segment 2: resume without a result and pause again.
    let (engine2, hash2) = self::engine();
    let (host2, shared2) = new_host();
    engine2.set_durable_host(Some(shared2));
    let run2 = tokio::spawn({
        let engine2 = Arc::clone(&engine2);
        let bytes = first.committed.bytes.clone();
        async move {
            engine2
                .durable_resume("caller", &bytes, hash2, ctx(), true, |_| {})
                .await
        }
    });
    host2
        .wait_until(|host| !host.started.lock().unwrap().is_empty())
        .await;
    let second = snapshot(&engine2, hash2, SnapshotMode::Suspend, &progress)
        .await
        .unwrap();
    assert_eq!(
        run2.await.unwrap().unwrap_err(),
        EngineError::DurableSuspended
    );
    assert_eq!(
        second.committed.state["threads"][0]["parked"]["kind"],
        "remote_call"
    );

    // Segment 3 gets the result.
    let (result, _, _) = resume("caller", &second.committed, |_, host| {
        host.deliver("r-test-c1", Ok(string("fog in Porto")));
    })
    .await;
    let BexExternalValue::Instance { fields, .. } = result.unwrap() else {
        panic!("expected a Plan");
    };
    assert_eq!(fields["weather"], string("fog in Porto"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_collection_between_restore_and_completion_keeps_the_run_intact() {
    let expected = plain("napper", vec![string("Lisbon")]).await;

    let (engine, hash) = engine();
    let (host, shared) = new_host();
    engine.set_durable_host(Some(shared));
    let run = start(&engine, "napper", vec![string("Lisbon")]);
    // Pause in the second sleep, so that `ideas` already holds a string.
    host.wait_until(|host| {
        host.yields
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, op)| op.as_deref() == Some("baml.sys.sleep"))
            .count()
            >= 2
    })
    .await;
    let progress = Mutex::new(Vec::new());
    let report = snapshot(&engine, hash, SnapshotMode::Suspend, &progress)
        .await
        .unwrap();
    assert_eq!(
        run.await.unwrap().unwrap_err(),
        EngineError::DurableSuspended
    );
    assert_eq!(
        local(&report.committed.state, "day")["value"]["preview"],
        "2"
    );

    // The restored thread sleeps first; collect (twice, so that restored
    // objects move and move again) while it does and again later.
    let (result, _, _) = resume("napper", &report.committed, |engine, _| {
        let engine = Arc::clone(engine);
        tokio::spawn(async move {
            for _ in 0..4 {
                engine
                    .collect_garbage(bex_heap::CollectionLevel::Major)
                    .await;
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
        });
    })
    .await;
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pending_future_blocks_the_snapshot_and_the_run_still_completes() {
    let (engine, hash) = engine();
    let (host, shared) = new_host();
    engine.set_durable_host(Some(shared));
    let run = start(&engine, "parallel", vec![string("Faro")]);
    host.wait_until(|host| !host.announced.lock().unwrap().is_empty())
        .await;

    let progress = Arc::new(Mutex::new(Vec::new()));
    let pause = tokio::spawn({
        let engine = Arc::clone(&engine);
        let progress = Arc::clone(&progress);
        async move { snapshot(&engine, hash, SnapshotMode::Suspend, &progress).await }
    });
    // Wait for at least two attempts, then let the run finish.
    tokio::time::sleep(Duration::from_millis(350)).await;
    host.deliver("r-test-c1", Ok(string("windy in Faro")));

    let result = tokio::time::timeout(Duration::from_secs(30), run)
        .await
        .expect("a blocked pause must not stall the run")
        .unwrap()
        .unwrap();
    assert_eq!(result, string("windy in Faro after 4"));
    let failure = tokio::time::timeout(Duration::from_secs(30), pause)
        .await
        .unwrap()
        .unwrap()
        .expect_err("the future stays on the stack until the function returns");
    assert!(matches!(failure, SnapshotFailure::RunEnded), "{failure:?}");

    let progress = progress.lock().unwrap();
    let blocked: Vec<_> = progress
        .iter()
        .filter_map(|event| match event {
            PauseProgress::Blocked { reason, path } => Some((reason, path)),
            PauseProgress::Pausing { .. } => None,
        })
        .collect();
    assert!(!blocked.is_empty(), "{progress:?}");
    assert!(
        blocked[0].0.to_lowercase().contains("future"),
        "{blocked:?}"
    );
    assert!(
        !blocked[0].1.is_empty(),
        "the path names the value: {blocked:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_automatic_snapshot_leaves_the_run_running_and_can_be_resumed() {
    let expected = plain("napper", vec![string("Lisbon")]).await;

    let (engine, hash) = engine();
    let (host, shared) = new_host();
    engine.set_durable_host(Some(shared));
    let run = start(&engine, "napper", vec![string("Lisbon")]);
    host.wait_until(|host| host.saw_yield(YieldReason::SysOp, Some("baml.sys.sleep")))
        .await;

    let progress = Mutex::new(Vec::new());
    let report = snapshot(
        &engine,
        hash,
        SnapshotMode::KeepRunning {
            park_timeout: Duration::from_millis(500),
        },
        &progress,
    )
    .await
    .expect("an automatic snapshot at a sleep");
    // The original run is unaffected.
    assert_eq!(run.await.unwrap().unwrap(), expected);
    // And the snapshot is a valid starting point (recovery after a crash).
    let (result, _, _) = resume("napper", &report.committed, |_, _| {}).await;
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_snapshot_of_another_program_or_build_is_refused() {
    let (engine, hash) = engine();
    let (host, shared) = new_host();
    engine.set_durable_host(Some(shared));
    let run = start(&engine, "napper", vec![string("Lisbon")]);
    host.wait_until(|host| host.saw_yield(YieldReason::SysOp, Some("baml.sys.sleep")))
        .await;
    let progress = Mutex::new(Vec::new());
    let report = snapshot(&engine, hash, SnapshotMode::Suspend, &progress)
        .await
        .unwrap();
    assert_eq!(
        run.await.unwrap().unwrap_err(),
        EngineError::DurableSuspended
    );

    let (other, _) = self::engine();
    let mut wrong = hash;
    wrong[0] ^= 1;
    let error = other
        .durable_resume(
            "napper",
            &report.committed.bytes,
            wrong,
            ctx(),
            true,
            |_| {},
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("different program"), "{error}");
}

/// The VM checks for an early yield right after a call has pushed the callee's
/// frame and right after a return has popped it. A pause that lands there used
/// to write a snapshot whose live program counter belonged to the other
/// function, and the import refused it. The run is moved several times, so
/// that some pauses land on such a boundary.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_compute_loop_that_makes_calls_survives_a_chain_of_pauses() {
    const N: i64 = 60_000;
    let expected = plain("spin_calls", vec![BexExternalValue::Int(N)]).await;

    let (engine, hash) = engine();
    let (host, shared) = new_host();
    engine.set_durable_host(Some(shared));
    let run = start(&engine, "spin_calls", vec![BexExternalValue::Int(N)]);
    host.wait_until(|host| !host.started.lock().unwrap().is_empty())
        .await;
    let progress = Mutex::new(Vec::new());
    let mut latest = snapshot(&engine, hash, SnapshotMode::Suspend, &progress)
        .await
        .expect("a compute loop has clean points")
        .committed;
    assert_eq!(
        run.await.unwrap().unwrap_err(),
        EngineError::DurableSuspended
    );

    let mut top_functions = std::collections::BTreeSet::new();
    for hop in 0..4u64 {
        let top = latest.state["threads"][0]["frames"][0]["function"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        top_functions.insert(top);
        let (engine, hash) = self::engine();
        let (host, shared) = new_host();
        engine.set_durable_host(Some(shared));
        let run = tokio::spawn({
            let engine = Arc::clone(&engine);
            let bytes = latest.bytes.clone();
            async move {
                engine
                    .durable_resume("spin_calls", &bytes, hash, ctx(), true, |_| {})
                    .await
            }
        });
        host.wait_until(|host| !host.started.lock().unwrap().is_empty())
            .await;
        // Vary the time the segment runs, so that the pauses do not all land
        // on the same instruction.
        tokio::time::sleep(Duration::from_millis(3 + hop * 2)).await;
        match snapshot(&engine, hash, SnapshotMode::Suspend, &progress).await {
            Ok(report) => {
                latest = report.committed;
                match run.await.unwrap() {
                    Err(EngineError::DurableSuspended) => {}
                    other => panic!("hop {hop}: the paused segment ended with {other:?}"),
                }
            }
            // The run finished before the pause was served.
            Err(SnapshotFailure::RunEnded) => {
                assert_eq!(run.await.unwrap().unwrap(), expected, "hop {hop}");
                return;
            }
            Err(other) => panic!("hop {hop}: {other:?}"),
        }
    }
    let (result, _, _) = resume("spin_calls", &latest, |_, _| {}).await;
    assert_eq!(result.unwrap(), expected, "paused in {top_functions:?}");
}

/// A spawned thread whose future was dropped is not visible to the snapshot
/// writer through any value. While it is alive the run has two threads, which
/// `durable_resume` cannot restore, so the pause must not suspend into such a
/// snapshot. It reports `Blocked` and succeeds once the thread has ended.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_forgotten_spawned_thread_blocks_the_pause_until_it_ends() {
    let expected = plain("forgetful", vec![string("Braga")]).await;

    let (engine, hash) = engine();
    let (host, shared) = new_host();
    engine.set_durable_host(Some(shared));
    let run = start(&engine, "forgetful", vec![string("Braga")]);
    host.wait_until(|host| host.started.lock().unwrap().len() >= 2)
        .await;

    let progress = Mutex::new(Vec::new());
    let report = snapshot(&engine, hash, SnapshotMode::Suspend, &progress)
        .await
        .expect("the pause succeeds once the forgotten thread has ended");
    assert_eq!(report.threads, 1);
    assert!(report.blocked_attempts >= 1, "{report:?}");
    let progress = progress.into_inner().unwrap();
    assert!(
        progress.iter().any(|event| matches!(
            event,
            PauseProgress::Blocked { reason, path }
                if reason.contains("live threads") && path.len() == 2
        )),
        "{progress:?}"
    );
    assert_eq!(
        run.await.unwrap().unwrap_err(),
        EngineError::DurableSuspended
    );
    assert_eq!(
        report.committed.state["threads"].as_array().unwrap().len(),
        1
    );

    let (result, _, _) = resume("forgetful", &report.committed, |_, _| {}).await;
    assert_eq!(result.unwrap(), expected);
}

/// An automatic snapshot of a run with a forgotten thread is refused too: it
/// could not be used to recover the run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_automatic_snapshot_of_two_threads_is_blocked() {
    let (engine, hash) = engine();
    let (host, shared) = new_host();
    engine.set_durable_host(Some(shared));
    let run = start(&engine, "forgetful", vec![string("Braga")]);
    host.wait_until(|host| host.started.lock().unwrap().len() >= 2)
        .await;
    let progress = Mutex::new(Vec::new());
    let mode = SnapshotMode::KeepRunning {
        park_timeout: Duration::from_secs(5),
    };
    let failure = snapshot(&engine, hash, mode, &progress)
        .await
        .expect_err("two live threads");
    assert!(
        matches!(&failure, SnapshotFailure::Blocked { reason, .. } if reason.contains("live threads")),
        "{failure:?}"
    );
    assert_eq!(run.await.unwrap().unwrap(), string("Braga after 10"));
}

/// When the root function has returned, the run is over, even while a thread
/// it spawned and never awaited is still running. A pause then reports that
/// the run ended and writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pause_after_the_root_returned_reports_that_the_run_ended() {
    let (engine, hash) = engine();
    let (host, shared) = new_host();
    engine.set_durable_host(Some(shared));
    let run = start(&engine, "detached", vec![string("Evora")]);
    host.wait_until(|host| host.started.lock().unwrap().len() >= 2)
        .await;
    assert_eq!(run.await.unwrap().unwrap(), BexExternalValue::Int(0));
    assert!(
        engine.durable_run_is_live(),
        "the detached thread is still asleep"
    );

    let progress = Mutex::new(Vec::new());
    let failure = tokio::time::timeout(
        Duration::from_millis(400),
        snapshot(&engine, hash, SnapshotMode::Suspend, &progress),
    )
    .await
    .expect("the answer does not wait for the detached thread")
    .expect_err("no snapshot of a run without its root");
    assert!(matches!(failure, SnapshotFailure::RunEnded), "{failure:?}");
}

/// A cancelled run in a compute loop has no sys-op or await at which the
/// engine would notice the cancellation. `durable_request_yield` makes the VM
/// yield, and the engine loop ends a cancelled durable thread there.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cancelled_compute_loop_ends_at_the_next_early_yield() {
    let (engine, _) = engine();
    let (host, shared) = new_host();
    engine.set_durable_host(Some(shared));
    let cancel = bex_engine::CancellationToken::new();
    let run = tokio::spawn({
        let engine = Arc::clone(&engine);
        let context = FunctionCallContextBuilder::new(sys_types::CallId::next())
            .with_cancel_token(cancel.clone())
            .build();
        // Long enough to run for many seconds in a debug build.
        async move {
            engine
                .call_function(
                    "spin_calls",
                    vec![BexExternalValue::Int(50_000_000)],
                    context,
                    true,
                )
                .await
        }
    });
    host.wait_until(|host| !host.started.lock().unwrap().is_empty())
        .await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let cancelled_at = std::time::Instant::now();
    cancel.cancel();
    let result = loop {
        engine.durable_request_yield();
        if run.is_finished() {
            break run.await.unwrap();
        }
        assert!(
            cancelled_at.elapsed() < Duration::from_secs(10),
            "the cancelled loop is still running"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert!(
        matches!(result, Err(EngineError::UnhandledThrow { .. })),
        "{result:?}"
    );
    assert!(!engine.durable_run_is_live());
}

/// A helper call made between `set_durable_host` and the run (the embedder's
/// argument decoding, a finalizer the engine runs for itself) uses a
/// suppressed profile. It must not become the root of the durable run, or the
/// real run could never be paused.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_helper_call_before_the_run_does_not_become_the_run() {
    let expected = plain("napper", vec![string("Lisbon")]).await;

    let (engine, hash) = engine();
    let (host, shared) = new_host();
    engine.set_durable_host(Some(shared));
    let helper = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .suppress_internal_profile()
        .build();
    engine
        .call_function("spin", vec![BexExternalValue::Int(10)], helper, true)
        .await
        .unwrap();

    let run = start(&engine, "napper", vec![string("Lisbon")]);
    host.wait_until(|host| host.saw_yield(YieldReason::SysOp, Some("baml.sys.sleep")))
        .await;
    let progress = Mutex::new(Vec::new());
    let report = snapshot(&engine, hash, SnapshotMode::Suspend, &progress)
        .await
        .expect("the run, not the helper call, is the durable run");
    assert_eq!(
        run.await.unwrap().unwrap_err(),
        EngineError::DurableSuspended
    );
    let (result, _, _) = resume("napper", &report.committed, |_, _| {}).await;
    assert_eq!(result.unwrap(), expected);
}

/// `race` over an empty array never settles. The thread inside it waits in an
/// `AwaitAny` without waiters, which the pause gate has to wake like any other
/// wait: the pause then gets an answer (here `Blocked`, because of the second
/// thread) and does not hang without a report.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_thread_in_an_empty_race_does_not_hang_the_pause() {
    let (engine, hash) = engine();
    let (host, shared) = new_host();
    engine.set_durable_host(Some(shared));
    let run = start(&engine, "racer", vec![string("Tomar")]);
    host.wait_until(|host| host.saw_yield(YieldReason::SysOp, Some("baml.sys.sleep")))
        .await;
    let progress = Mutex::new(Vec::new());
    let mode = SnapshotMode::KeepRunning {
        park_timeout: Duration::from_secs(5),
    };
    let failure = snapshot(&engine, hash, mode, &progress)
        .await
        .expect_err("two threads");
    assert!(
        matches!(failure, SnapshotFailure::Blocked { .. }),
        "every thread parked, so the attempt got as far as the writer: {failure:?}"
    );
    assert_eq!(run.await.unwrap().unwrap(), string("Tomar after 8"));
}
