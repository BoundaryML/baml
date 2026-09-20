//! Harness of the multi-thread durable tests (`durable_threads.rs`).
//!
//! A [`Site`] plays the site server: it outlives the engines of one run,
//! executes `remote_` calls on a separate engine, and keeps their results
//! while the run has no process. A [`Host`] is the `DurableHost` of one
//! segment. It can request a pause at an exact yield: the `yielded` callback
//! of yield number `k` wakes the test's coordinator and holds the reporting
//! thread until the engine's pause gate is open, so the thread parks at the
//! very next opportunity.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use bex_engine::{
    BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder,
    durable::{
        DurableHost, DurableThreadId, PauseProgress, RecordedResult, RemoteCallRequest,
        RemoteCancel, RemoteResultWait, RestoredInfo, ResumeOptions, SnapshotFailure, SnapshotMode,
        SnapshotReport, SnapshotRequest, YieldPosition, YieldReason,
    },
};
use bex_vm_types::Program;
use futures::future::BoxFuture;
use sys_native::SysOpsExt;

use crate::common::compile_for_engine;

pub(crate) struct Compiled {
    pub(crate) program: Program,
    pub(crate) hash: [u8; 32],
}

/// Compile `source` once per test binary; every engine is built from a clone.
pub(crate) fn compiled(cell: &'static OnceLock<Compiled>, source: &str) -> &'static Compiled {
    cell.get_or_init(|| {
        let program = compile_for_engine(source);
        let hash = bex_snapshot::program_hash(&program).expect("program hashes");
        Compiled { program, hash }
    })
}

pub(crate) fn new_engine(compiled: &Compiled) -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            compiled.program.clone(),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("the engine builds"),
    )
}

pub(crate) fn ctx() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}

pub(crate) fn string(value: &str) -> BexExternalValue {
    BexExternalValue::String(value.into())
}

/// How the site answers remote calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Remote {
    /// Run the callee on the site's own engine and store the result.
    Execute,
    /// The test delivers results with [`Site::deliver`].
    Manual,
}

/// A result with the time it arrived, in Unix milliseconds.
type StoredResult = (u64, Result<BexExternalValue, String>);

/// State that outlives the processes of one run.
pub(crate) struct Site {
    compiled: &'static Compiled,
    remote: Remote,
    /// Delay before a dispatched call starts, the stand-in for a slow
    /// controller between the sites.
    pub(crate) dispatch_delay: Mutex<Duration>,
    /// How long the host takes to announce a call (`DurableHost::remote_call`
    /// resolves after this time).
    pub(crate) announce_delay: Mutex<Duration>,
    remote_engine: OnceLock<Arc<BexEngine>>,
    pub(crate) call_counter: AtomicU64,
    /// `(call id, function, thread)` of every announced call, in order.
    pub(crate) announced: Mutex<Vec<(String, String, DurableThreadId)>>,
    /// `(thread path, call index, function)` of every announced call.
    pub(crate) announced_paths: Mutex<Vec<(String, u64, String)>>,
    /// The first string argument of every announced call, by call id and in
    /// the order of the announcements (parallel to `announced_paths`).
    pub(crate) announced_vendors: Mutex<std::collections::BTreeMap<String, String>>,
    pub(crate) announced_vendors_in_order: Mutex<Vec<String>>,
    /// Call ids the run abandoned, with the cancelled thread.
    pub(crate) cancelled: Mutex<Vec<(String, DurableThreadId)>>,
    /// Everything the engine reported about each abandoned call, in order.
    pub(crate) cancel_reports: Mutex<Vec<RemoteCancel>>,
    /// The call site of every announced call, by call id.
    pub(crate) announced_sites: Mutex<HashMap<String, Option<YieldPosition>>>,
    /// Call ids whose result a thread took, in the order the engine reported
    /// the deliveries.
    pub(crate) received: Mutex<Vec<String>>,
    /// Results with the time they arrived (Unix milliseconds).
    results: Mutex<HashMap<String, StoredResult>>,
    changed: tokio::sync::Notify,
}

impl Site {
    pub(crate) fn new(compiled: &'static Compiled, remote: Remote) -> Arc<Self> {
        Arc::new(Self {
            compiled,
            remote,
            dispatch_delay: Mutex::new(Duration::ZERO),
            announce_delay: Mutex::new(Duration::ZERO),
            remote_engine: OnceLock::new(),
            call_counter: AtomicU64::new(0),
            announced: Mutex::new(Vec::new()),
            announced_paths: Mutex::new(Vec::new()),
            announced_vendors: Mutex::new(std::collections::BTreeMap::new()),
            announced_vendors_in_order: Mutex::new(Vec::new()),
            cancelled: Mutex::new(Vec::new()),
            cancel_reports: Mutex::new(Vec::new()),
            announced_sites: Mutex::new(HashMap::new()),
            received: Mutex::new(Vec::new()),
            results: Mutex::new(HashMap::new()),
            changed: tokio::sync::Notify::new(),
        })
    }

    pub(crate) fn deliver(&self, call_id: &str, result: Result<BexExternalValue, String>) {
        self.deliver_at(call_id, result, unix_ms_now());
    }

    /// Store a result as if it had arrived at `at_unix_ms`.
    #[allow(dead_code)]
    pub(crate) fn deliver_at(
        &self,
        call_id: &str,
        result: Result<BexExternalValue, String>,
        at_unix_ms: u64,
    ) {
        self.results
            .lock()
            .unwrap()
            .entry(call_id.to_string())
            .or_insert((at_unix_ms, result));
        self.changed.notify_waiters();
    }

    /// What a site server passes to a resume: every stored result with the
    /// time it arrived.
    pub(crate) fn recorded_results(&self) -> Vec<RecordedResult> {
        self.results
            .lock()
            .unwrap()
            .iter()
            .map(|(call_id, (at_unix_ms, _))| RecordedResult {
                call_id: call_id.clone(),
                at_unix_ms: *at_unix_ms,
            })
            .collect()
    }

    pub(crate) async fn wait_until(&self, what: &str, condition: impl Fn(&Self) -> bool) {
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
        .unwrap_or_else(|_| panic!("timed out waiting until {what}"));
    }

    fn execute(self: &Arc<Self>, call_id: String, request: RemoteCallRequest) {
        let site = Arc::clone(self);
        let engine = Arc::clone(self.remote_engine.get_or_init(|| new_engine(self.compiled)));
        let delay = *self.dispatch_delay.lock().unwrap();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let args = request.args.into_iter().map(|arg| arg.value).collect();
            let result = engine
                .call_function(&request.function, args, ctx(), true)
                .await
                .map_err(|error| error.to_string());
            site.deliver(&call_id, result);
        });
    }
}

pub(crate) fn unix_ms_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// The `DurableHost` of one segment.
pub(crate) struct Host {
    pub(crate) site: Arc<Site>,
    /// Calls whose result a thread of this segment has taken.
    taken: Mutex<HashSet<String>>,
    engine: OnceLock<Weak<BexEngine>>,
    pub(crate) yields: AtomicUsize,
    /// The yield number at which the host requests a pause.
    pause_at: AtomicUsize,
    pause_wanted: tokio::sync::Notify,
    pub(crate) started: Mutex<Vec<(DurableThreadId, Option<DurableThreadId>)>>,
    /// The `spawn` site reported with every `thread_started`, in order.
    pub(crate) spawn_sites: Mutex<Vec<(DurableThreadId, Option<YieldPosition>)>>,
    pub(crate) ended: Mutex<Vec<DurableThreadId>>,
    pub(crate) yield_log: Mutex<Vec<(DurableThreadId, YieldReason, Option<String>)>>,
}

impl Host {
    pub(crate) fn install(
        site: &Arc<Site>,
        engine: &Arc<BexEngine>,
        pause_at: Option<usize>,
    ) -> Arc<Self> {
        let host = Arc::new(Self {
            site: Arc::clone(site),
            taken: Mutex::new(HashSet::new()),
            engine: OnceLock::new(),
            yields: AtomicUsize::new(0),
            pause_at: AtomicUsize::new(pause_at.unwrap_or(usize::MAX)),
            pause_wanted: tokio::sync::Notify::new(),
            started: Mutex::new(Vec::new()),
            spawn_sites: Mutex::new(Vec::new()),
            ended: Mutex::new(Vec::new()),
            yield_log: Mutex::new(Vec::new()),
        });
        let _ = host.engine.set(Arc::downgrade(engine));
        engine.set_durable_host(Some(Arc::new(HostRef(Arc::clone(&host)))));
        host
    }

    /// Resolves when the run reaches the yield the host was told to pause at.
    pub(crate) async fn pause_wanted(&self) {
        self.pause_wanted.notified().await;
    }

    pub(crate) fn saw(&self, reason: YieldReason, op: Option<&str>) -> bool {
        self.yield_log
            .lock()
            .unwrap()
            .iter()
            .any(|(_, r, o)| *r == reason && o.as_deref() == op)
    }

    pub(crate) async fn wait_until(&self, what: &str, condition: impl Fn(&Self) -> bool) {
        tokio::time::timeout(Duration::from_secs(30), async {
            while !condition(self) {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting until {what}"));
    }
}

struct HostRef(Arc<Host>);

impl DurableHost for HostRef {
    fn remote_call(
        &self,
        request: RemoteCallRequest,
    ) -> BoxFuture<'static, Result<String, String>> {
        let site = Arc::clone(&self.0.site);
        let counter = site.call_counter.fetch_add(1, Ordering::Relaxed) + 1;
        let call_id = format!("r-test-c{counter}");
        site.announced.lock().unwrap().push((
            call_id.clone(),
            request.function.clone(),
            request.thread,
        ));
        {
            // One lock order for the parallel vectors.
            let mut paths = site.announced_paths.lock().unwrap();
            let vendor = request
                .args
                .iter()
                .find_map(|arg| match &arg.value {
                    BexExternalValue::String(text) => Some(text.to_string()),
                    _ => None,
                })
                .unwrap_or_default();
            paths.push((
                request.thread_path.clone(),
                request.call_index,
                request.function.clone(),
            ));
            site.announced_vendors
                .lock()
                .unwrap()
                .insert(call_id.clone(), vendor.clone());
            site.announced_vendors_in_order.lock().unwrap().push(vendor);
            site.announced_sites
                .lock()
                .unwrap()
                .insert(call_id.clone(), request.site.clone());
        }
        if site.remote == Remote::Execute {
            site.execute(call_id.clone(), request);
        }
        site.changed.notify_waiters();
        let delay = *site.announce_delay.lock().unwrap();
        Box::pin(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            Ok(call_id)
        })
    }

    fn remote_result(
        &self,
        wait: RemoteResultWait,
    ) -> BoxFuture<'static, Result<BexExternalValue, String>> {
        let site = Arc::clone(&self.0.site);
        Box::pin(async move {
            loop {
                let changed = site.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                // A result stays available: a duplicate delivery, a fork, or
                // a recovery from an older snapshot may ask again.
                let ready = site.results.lock().unwrap().get(&wait.call_id).cloned();
                if let Some((_, result)) = ready {
                    return result;
                }
                changed.await;
            }
        })
    }

    fn remote_result_available(&self, call_id: &str) -> bool {
        self.0.site.results.lock().unwrap().contains_key(call_id)
            && !self.0.taken.lock().unwrap().contains(call_id)
    }

    fn remote_result_delivered(&self, call_id: &str, _thread: DurableThreadId) {
        self.0.taken.lock().unwrap().insert(call_id.to_string());
        self.0
            .site
            .received
            .lock()
            .unwrap()
            .push(call_id.to_string());
        self.0.site.changed.notify_waiters();
    }

    fn thread_started(
        &self,
        thread: DurableThreadId,
        parent: Option<DurableThreadId>,
        site: Option<&YieldPosition>,
    ) {
        self.0.started.lock().unwrap().push((thread, parent));
        self.0
            .spawn_sites
            .lock()
            .unwrap()
            .push((thread, site.cloned()));
    }

    fn thread_ended(&self, thread: DurableThreadId) {
        self.0.ended.lock().unwrap().push(thread);
    }

    fn remote_cancelled(&self, cancel: RemoteCancel) {
        self.0
            .site
            .cancelled
            .lock()
            .unwrap()
            .push((cancel.call_id.clone(), cancel.thread));
        self.0.site.cancel_reports.lock().unwrap().push(cancel);
        self.0.site.changed.notify_waiters();
    }

    fn yielded(
        &self,
        thread: DurableThreadId,
        reason: YieldReason,
        op: Option<&str>,
        _position: Option<&YieldPosition>,
    ) {
        self.0
            .yield_log
            .lock()
            .unwrap()
            .push((thread, reason, op.map(str::to_string)));
        let number = self.0.yields.fetch_add(1, Ordering::SeqCst);
        if number != self.0.pause_at.load(Ordering::SeqCst) {
            return;
        }
        self.0.pause_wanted.notify_one();
        // Hold this thread until the coordinator has opened the gate, so that
        // the pause lands at this yield and not some yields later. The
        // coordinator needs no permit to open the gate, so this cannot
        // deadlock; the deadline only guards against a test bug.
        let Some(engine) = self.0.engine.get().and_then(Weak::upgrade) else {
            return;
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !engine.durable_pause_requested() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_micros(200));
        }
    }
}

#[derive(Debug)]
pub(crate) struct Snapshot {
    pub(crate) bytes: Vec<u8>,
    pub(crate) state: serde_json::Value,
}

pub(crate) async fn snapshot(
    engine: &Arc<BexEngine>,
    hash: [u8; 32],
    mode: SnapshotMode,
    site: &Site,
) -> Result<SnapshotReport<Snapshot>, SnapshotFailure> {
    let progress = Mutex::new(Vec::new());
    let report = engine
        .durable_snapshot(
            SnapshotRequest {
                run_id: "r-test".to_string(),
                segment: 1,
                seq: 1,
                program_hash: hash,
                embed_program: None,
                compress: true,
                mode,
            },
            || {
                site.call_counter
                    .load(Ordering::Relaxed)
                    .to_le_bytes()
                    .to_vec()
            },
            |event| progress.lock().unwrap().push(event),
            |parts| {
                Ok(Snapshot {
                    bytes: parts.bytes.to_vec(),
                    state: parts.state_dump.clone(),
                })
            },
        )
        .await;
    let progress = progress.into_inner().unwrap();
    assert!(
        !progress
            .iter()
            .any(|event| matches!(event, PauseProgress::Blocked { .. })),
        "the snapshot was blocked: {progress:?}"
    );
    report
}

pub(crate) type RunResult = Result<BexExternalValue, EngineError>;

/// One process in the life of a run.
pub(crate) struct Segment {
    pub(crate) engine: Arc<BexEngine>,
    pub(crate) host: Arc<Host>,
    run: tokio::task::JoinHandle<RunResult>,
    /// Set when `pause_at_requested_yield` saw the run end.
    result: Option<RunResult>,
    pub(crate) restored: Arc<Mutex<Option<RestoredInfo>>>,
}

impl Segment {
    pub(crate) fn start(
        site: &Arc<Site>,
        function: &'static str,
        args: Vec<BexExternalValue>,
        pause_at: Option<usize>,
    ) -> Self {
        let engine = new_engine(site.compiled);
        let host = Host::install(site, &engine, pause_at);
        let run = tokio::spawn({
            let engine = Arc::clone(&engine);
            async move { engine.call_function(function, args, ctx(), true).await }
        });
        Self {
            engine,
            host,
            run,
            result: None,
            restored: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn resume(
        site: &Arc<Site>,
        function: &'static str,
        snapshot: &Snapshot,
        pause_at: Option<usize>,
        options: ResumeOptions,
    ) -> Self {
        let engine = new_engine(site.compiled);
        let host = Host::install(site, &engine, pause_at);
        let restored = Arc::new(Mutex::new(None));
        // Like a site server, the harness tells the resumed run which results
        // arrived while it had no process, and when.
        let mut options = options;
        if options.recorded_results.is_empty() {
            options.recorded_results = site.recorded_results();
        }
        let run = tokio::spawn({
            let engine = Arc::clone(&engine);
            let bytes = snapshot.bytes.clone();
            let hash = site.compiled.hash;
            let restored = Arc::clone(&restored);
            async move {
                engine
                    .durable_resume_with(function, &bytes, hash, ctx(), true, options, |info| {
                        *restored.lock().unwrap() = Some(info.clone());
                    })
                    .await
            }
        });
        Self {
            engine,
            host,
            run,
            result: None,
            restored,
        }
    }

    /// Wait for the run to end, with a deadline.
    pub(crate) async fn finish(self) -> RunResult {
        if let Some(result) = self.result {
            return result;
        }
        tokio::time::timeout(Duration::from_secs(60), self.run)
            .await
            .expect("the segment ends")
            .expect("the run task does not panic")
    }

    /// Suspend the run into a snapshot. `None` when the run ended first; its
    /// result is then in `self.run`.
    pub(crate) async fn pause(&self, site: &Site) -> Option<SnapshotReport<Snapshot>> {
        match snapshot(
            &self.engine,
            site.compiled.hash,
            SnapshotMode::Suspend,
            site,
        )
        .await
        {
            Ok(report) => Some(report),
            Err(SnapshotFailure::RunEnded) => None,
            Err(other) => panic!("the pause failed: {other:?}"),
        }
    }

    /// Play the worker's self-suspend task once: wait until the self-suspend
    /// rule holds with the threshold `min_remaining_ms`, then try to suspend
    /// the run for that sleep. The first value is what the rule reported.
    // Only `durable_sleep.rs` uses it; the harness is compiled per test file.
    #[allow(dead_code)]
    pub(crate) async fn self_suspend(
        &self,
        site: &Site,
        min_remaining_ms: u64,
    ) -> (
        bex_engine::durable::IdleSleep,
        Result<SnapshotReport<Snapshot>, SnapshotFailure>,
    ) {
        let idle = tokio::time::timeout(
            Duration::from_secs(30),
            self.engine.durable_idle_sleep(min_remaining_ms),
        )
        .await
        .expect("the run becomes idle in a long sleep");
        let report = snapshot(
            &self.engine,
            site.compiled.hash,
            SnapshotMode::SuspendIfIdle { min_remaining_ms },
            site,
        )
        .await;
        (idle, report)
    }

    /// Wait for the yield the host was told to pause at, then pause. `None`
    /// when the run ended before that yield or before the pause was served.
    pub(crate) async fn pause_at_requested_yield(
        &mut self,
        site: &Site,
    ) -> Option<SnapshotReport<Snapshot>> {
        tokio::select! {
            () = self.host.pause_wanted() => {}
            result = &mut self.run => {
                self.result = Some(result.expect("the run task does not panic"));
                return None;
            }
        }
        self.pause(site).await
    }
}

/// How one run went, for comparison with an uninterrupted run.
#[derive(Debug)]
pub(crate) struct Outcome {
    pub(crate) result: RunResult,
    /// Functions of the remote calls the run announced, sorted: a resume must
    /// not announce a call twice.
    pub(crate) remote_calls: Vec<String>,
    pub(crate) segments: usize,
}

fn remote_calls(site: &Site) -> Vec<String> {
    let mut calls: Vec<String> = site
        .announced
        .lock()
        .unwrap()
        .iter()
        .map(|(_, function, _)| function.clone())
        .collect();
    calls.sort();
    calls
}

/// Run `function` without any pause. Returns the outcome and the number of
/// yields the run reported.
pub(crate) async fn run_plain(
    compiled: &'static Compiled,
    function: &'static str,
    args: Vec<BexExternalValue>,
) -> (Outcome, usize) {
    let site = Site::new(compiled, Remote::Execute);
    let segment = Segment::start(&site, function, args, None);
    let host = Arc::clone(&segment.host);
    let result = segment.finish().await;
    (
        Outcome {
            result,
            remote_calls: remote_calls(&site),
            segments: 1,
        },
        host.yields.load(Ordering::SeqCst),
    )
}

/// Run `function` and move it to a fresh engine at each yield number of
/// `pause_at` (counted per segment). With `collect`, the resumed engine runs
/// major collections while the restored threads continue.
pub(crate) async fn run_with_pauses(
    compiled: &'static Compiled,
    function: &'static str,
    args: Vec<BexExternalValue>,
    pause_at: &[usize],
    collect: bool,
) -> Outcome {
    let site = Site::new(compiled, Remote::Execute);
    let mut pauses = pause_at.iter().copied();
    let mut segment = Segment::start(&site, function, args, pauses.next());
    let mut segments = 1;
    loop {
        let Some(report) = segment.pause_at_requested_yield(&site).await else {
            let result = segment.finish().await;
            return Outcome {
                result,
                remote_calls: remote_calls(&site),
                segments,
            };
        };
        let suspended = segment.finish().await;
        assert_eq!(
            suspended.unwrap_err(),
            EngineError::DurableSuspended,
            "a paused segment ends without a result"
        );
        segments += 1;
        segment = Segment::resume(
            &site,
            function,
            &report.committed,
            pauses.next(),
            ResumeOptions::default(),
        );
        if collect {
            let engine = Arc::clone(&segment.engine);
            tokio::spawn(async move {
                for _ in 0..6 {
                    engine
                        .collect_garbage(bex_heap::CollectionLevel::Major)
                        .await;
                    tokio::time::sleep(Duration::from_millis(7)).await;
                }
            });
        }
    }
}
