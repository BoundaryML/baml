//! PR4 gates for the BEX profiling event stream (plan §5 PR4):
//!
//! - **G3 lossless**: a spawn-heavy, call-heavy program, run with the
//!   smallest legal segment size (forcing constant ring growth/recycling);
//!   the on-disk event balance must be exact — every `CallFunction` has
//!   exactly one `EndFunction`, every `StartThread` an `EndThread` — with
//!   exact per-function call counts.
//! - **Reconstruction smoke**: rebuild the per-thread call trees from the
//!   `.bamlprof` (v2 §7.2 shape) and assert nesting + spawn-edge sanity.
//! - Sys-op pairs (`PR4b`) and the unwind (`EndFunction{Error}`) path.
//!
//! This file is its own test binary: the profiling knobs are environment
//! variables latched once per process (`ProfConfig::global`), so they must
//! be set before anything builds an engine — `init_prof_env` runs first in
//! every test here, and no other test binary shares the process.
#![allow(unsafe_code)]

mod common;

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Once},
    time::Duration,
};

use bex_engine::{
    BexEngine, BexExternalValue, CancellationToken, EngineError, FunctionCallContextBuilder,
};
use bex_events::prof::{file::read_bamlprof, pb};
use common::compile_for_engine;
use pb::disk_event_v1::Event;
use sys_native::SysOpsExt;

/// Synthetic header rows (see `bex_engine`'s `SPAWN_CLOSURE_FQN` /
/// `UNKNOWN_FUNCTION_FQN`).
const SPAWN_CLOSURE_FQN: &str = "baml.<spawn-closure>";
const UNKNOWN_FUNCTION_FQN: &str = "baml.<unknown-function>";

fn prof_dir() -> PathBuf {
    // pid + startup nonce: pid reuse must not let a stale run's profiles
    // satisfy (or trip) this run's marker demux.
    static NONCE: std::sync::OnceLock<u128> = std::sync::OnceLock::new();
    let nonce = NONCE.get_or_init(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    });
    std::env::temp_dir().join(format!("bamlprof-gate-{}-{nonce}", std::process::id()))
}

/// Serializes the gate tests: they share one profile directory and one
/// global consumer, and reading a file another engine is actively
/// heartbeat-appending to could observe a partially flushed tail.
async fn test_lock() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

fn init_prof_env() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir = prof_dir();
        // SAFETY: runs before any engine exists in this binary; the config
        // is latched immediately below, so nothing re-reads the environment
        // concurrently afterwards.
        unsafe {
            std::env::set_var("BAML_PROFILE", "1");
            std::env::set_var("BAML_PROFILE_DIR", &dir);
            // The smallest legal segment: forces constant growth + recycling
            // under real producer load (G3's requirement).
            std::env::set_var("BAML_RING_SEG_BYTES", "65536");
        }
        let cfg = bex_events::prof::ProfConfig::global();
        assert!(cfg.enabled, "profiling must be on for the gate tests");
        assert_eq!(cfg.profile_dir, dir);
    });
}

async fn run_main(source: &str) -> Result<BexExternalValue, EngineError> {
    let program = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            program,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("engine construction"),
    );
    engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
}

/// Flushes the consumer and loads the (unique) profile whose function table
/// contains `marker_fqn` — each test uses distinct function names, and each
/// engine writes its own file, so the marker demuxes this binary's shared
/// profile directory.
fn load_profile(marker_fqn: &str) -> (pb::EventFileHeaderV1, Vec<Event>) {
    assert!(
        bex_events::prof::flush_and_join(Duration::from_secs(60)),
        "consumer never acked the flush"
    );
    let mut found = None;
    for entry in std::fs::read_dir(prof_dir()).expect("profile dir exists") {
        let path = entry.unwrap().path();
        // The tolerant reader hands back the whole-message prefix even when
        // a live heartbeat append tears the tail; our events were synced
        // whole before the flush ack. A file whose HEADER is unreadable is
        // mid-creation and cannot be ours — skip it.
        let Ok(contents) = read_bamlprof(&path) else {
            continue;
        };
        let (header, events) = (contents.header, contents.events);
        let has_marker = header
            .function_table
            .as_ref()
            .is_some_and(|t| t.functions.iter().any(|f| f.fqn == marker_fqn));
        if has_marker {
            assert!(
                found.is_none(),
                "two profiles claim marker {marker_fqn} — engine demux broken"
            );
            let events: Vec<Event> = events
                .into_iter()
                .filter_map(|e| e.event)
                .filter(|e| !matches!(e, Event::Heartbeat(_)))
                .collect();
            found = Some((header, events));
        }
    }
    found.unwrap_or_else(|| panic!("no profile contains {marker_fqn}"))
}

/// Asserts the G3 balance invariants and returns per-fqn `CallFunction`
/// counts plus the set of thread ids.
fn assert_balance(
    header: &pb::EventFileHeaderV1,
    events: &[Event],
) -> (HashMap<String, u64>, HashSet<u64>) {
    let fqn_by_id: HashMap<u32, &str> = header
        .function_table
        .as_ref()
        .map(|t| {
            t.functions
                .iter()
                .map(|f| (f.function_id, f.fqn.as_str()))
                .collect()
        })
        .unwrap_or_default();

    // File order interleaves rings (a call pair can span two rings when the
    // task migrates OS threads, and the entry call of a spawned thread is
    // emitted on the spawner's ring) — so balance is checked as set
    // equality, not as a streaming stack.
    let mut calls: HashSet<(u64, u64)> = HashSet::new();
    let mut ends: HashSet<(u64, u64)> = HashSet::new();
    let mut started_threads: HashSet<u64> = HashSet::new();
    let mut ended_threads: HashSet<u64> = HashSet::new();
    let mut counts: HashMap<String, u64> = HashMap::new();

    for event in events {
        match event {
            Event::CallFunction(cf) => {
                assert!(
                    calls.insert((cf.thread_id, cf.call_id)),
                    "duplicate CallFunction ({}, {})",
                    cf.thread_id,
                    cf.call_id
                );
                let fqn = fqn_by_id
                    .get(&cf.function_id)
                    .copied()
                    .unwrap_or("<unassigned>");
                *counts.entry(fqn.to_string()).or_default() += 1;
            }
            Event::EndFunction(ef) => {
                assert!(
                    ends.insert((ef.thread_id, ef.call_id)),
                    "duplicate EndFunction ({}, {})",
                    ef.thread_id,
                    ef.call_id
                );
            }
            Event::StartThread(st) => {
                assert!(
                    started_threads.insert(st.thread_id),
                    "duplicate StartThread {}",
                    st.thread_id
                );
            }
            Event::EndThread(et) => {
                assert!(
                    ended_threads.insert(et.thread_id),
                    "duplicate EndThread {}",
                    et.thread_id
                );
            }
            Event::SetFunctionId(_) | Event::Heartbeat(_) => {}
        }
    }
    assert_eq!(
        calls, ends,
        "every CallFunction must have exactly one EndFunction"
    );
    assert_eq!(
        started_threads, ended_threads,
        "every StartThread must have exactly one EndThread"
    );
    (counts, started_threads)
}

/// G3: spawn-heavy + call-heavy, exact counts, forced growth/recycling.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn g3_lossless_spawn_and_call_heavy() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function g3_leaf(n: int) -> int { n }
        function g3_mid(n: int) -> int { g3_leaf(n) + g3_leaf(n + 1) }
        function g3_work(n: int) -> int {
            let s = 0;
            for (let i = 0; i < 300; i += 1) { s += g3_mid(i); };
            s
        }
        function main() -> int {
            let f0 = spawn { g3_work(0) };
            let f1 = spawn { g3_work(1) };
            let f2 = spawn { g3_work(2) };
            let f3 = spawn { g3_work(3) };
            let f4 = spawn { g3_work(4) };
            let f5 = spawn { g3_work(5) };
            let f6 = spawn { g3_work(6) };
            let f7 = spawn { g3_work(7) };
            let local = g3_work(8);
            (await f0) + (await f1) + (await f2) + (await f3)
                + (await f4) + (await f5) + (await f6) + (await f7) + local
        }
    "#;
    run_main(source).await.expect("g3 program runs");

    let (header, events) = load_profile("user.g3_work");
    let (counts, threads) = assert_balance(&header, &events);

    // 1 root + 8 spawned children.
    assert_eq!(threads.len(), 9, "expected 9 logical threads: {threads:?}");
    // Exact call counts: 9 work invocations × (1 work + 300 mid + 600 leaf).
    assert_eq!(counts.get("user.g3_work"), Some(&9));
    assert_eq!(counts.get("user.g3_mid"), Some(&2700));
    assert_eq!(counts.get("user.g3_leaf"), Some(&5400));
    assert_eq!(counts.get("user.main"), Some(&1));
}

/// Reconstruction smoke (v2 §7.2): per-thread stack discipline from the
/// on-disk events, plus spawn-edge validity.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reconstruction_smoke() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function rc_leaf(n: int) -> int { n * 2 }
        function rc_mid(n: int) -> int { rc_leaf(n) + 1 }
        function main() -> int {
            let f = spawn { rc_mid(10) };
            let a = rc_mid(1);
            (await f) + a
        }
    "#;
    run_main(source).await.expect("rc program runs");

    let (header, events) = load_profile("user.rc_mid");
    assert_balance(&header, &events);

    // Group events per thread, sorted by timestamp (events for one logical
    // thread can arrive via several rings when the task migrates OS
    // threads; the clock orders them).
    let mut per_thread: HashMap<u64, Vec<&Event>> = HashMap::new();
    for event in &events {
        let tid = match event {
            Event::CallFunction(cf) => cf.thread_id,
            Event::EndFunction(ef) => ef.thread_id,
            Event::StartThread(st) => st.thread_id,
            Event::EndThread(et) => et.thread_id,
            _ => continue,
        };
        per_thread.entry(tid).or_default().push(event);
    }
    let ts_of = |e: &Event| match e {
        Event::CallFunction(cf) => cf.timestamp_ns,
        Event::EndFunction(ef) => ef.timestamp_ns,
        Event::StartThread(st) => st.timestamp_ns,
        Event::EndThread(et) => et.timestamp_ns,
        _ => 0,
    };

    let mut calls_per_thread: HashMap<u64, HashSet<u64>> = HashMap::new();
    for (tid, mut thread_events) in per_thread.clone() {
        thread_events.sort_by_key(|e| ts_of(e));
        // Stack discipline: a call's parent must be the innermost open call.
        let mut stack: Vec<u64> = Vec::new();
        for event in thread_events {
            match event {
                Event::CallFunction(cf) => {
                    let expected_parent = stack.last().copied();
                    assert_eq!(
                        cf.parent_call_id, expected_parent,
                        "thread {tid}: call {} has parent {:?}, expected {:?}",
                        cf.call_id, cf.parent_call_id, expected_parent
                    );
                    stack.push(cf.call_id);
                    calls_per_thread.entry(tid).or_default().insert(cf.call_id);
                }
                Event::EndFunction(ef) => {
                    assert_eq!(
                        stack.pop(),
                        Some(ef.call_id),
                        "thread {tid}: EndFunction {} out of nesting order",
                        ef.call_id
                    );
                }
                _ => {}
            }
        }
        assert!(stack.is_empty(), "thread {tid}: unclosed calls {stack:?}");
    }

    // Spawn edges: every non-root StartThread points at a real parent
    // thread and (when present) a real call in that parent.
    for event in &events {
        if let Event::StartThread(st) = event
            && let Some(parent_tid) = st.parent_thread_id
        {
            assert!(
                per_thread.contains_key(&parent_tid),
                "StartThread {} has unknown parent thread {parent_tid}",
                st.thread_id
            );
            if let Some(parent_call) = st.parent_call_id {
                assert!(
                    calls_per_thread
                        .get(&parent_tid)
                        .is_some_and(|calls| calls.contains(&parent_call)),
                    "StartThread {}: parent call {parent_call} not found in thread {parent_tid}",
                    st.thread_id
                );
            }
        }
    }
}

/// `PR4b`: sys-op calls (here `baml.sys.sleep`, an async op that releases the
/// heap permit and may resume on another OS thread) appear as balanced
/// pairs with a sysop-kind function id.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sysop_pair_emitted() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function sy_wait() -> int {
            baml.sys.sleep(2);
            7
        }
        function main() -> int { sy_wait() }
    "#;
    run_main(source).await.expect("sysop program runs");

    let (header, events) = load_profile("user.sy_wait");
    assert_balance(&header, &events);

    let sleep_ids: HashSet<u32> = header
        .function_table
        .as_ref()
        .map(|t| {
            t.functions
                .iter()
                .filter(|f| f.kind == "sysop" && f.fqn.contains("sleep"))
                .map(|f| f.function_id)
                .collect()
        })
        .unwrap_or_default();
    assert!(!sleep_ids.is_empty(), "sleep sysop missing from the table");
    let sleep_calls = events
        .iter()
        .filter(|e| matches!(e, Event::CallFunction(cf) if sleep_ids.contains(&cf.function_id)))
        .count();
    assert_eq!(sleep_calls, 1, "expected exactly one sleep sysop call");
}

/// Engine teardown: dropping a `BexEngine` must close its `.bamlprof`
/// (stopping its heartbeats and freeing the fd) while later engines keep
/// working. Catches the engine-churn leak class (LSP-shaped hosts).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn engine_teardown_closes_profile() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function td_work(n: int) -> int { n + 1 }
        function main() -> int { td_work(41) }
    "#;
    run_main(source).await.expect("td program runs");
    // run_main dropped its Arc<BexEngine> on return -> EngineClosed was sent
    // before this flush on the same channel (FIFO), so the ack implies the
    // close (sync + writer removed) already happened.
    let (_, events) = load_profile("user.td_work");
    assert!(!events.is_empty());

    let path = {
        let mut found = None;
        for entry in std::fs::read_dir(prof_dir()).unwrap() {
            let path = entry.unwrap().path();
            let Ok(contents) = read_bamlprof(&path) else {
                continue;
            };
            let has_marker = contents
                .header
                .function_table
                .as_ref()
                .is_some_and(|t| t.functions.iter().any(|f| f.fqn == "user.td_work"));
            if has_marker {
                found = Some(path);
            }
        }
        found.expect("td profile exists")
    };
    let size_before = std::fs::metadata(&path).unwrap().len();
    // Two heartbeat intervals: a still-open writer would have grown.
    std::thread::sleep(Duration::from_millis(2_500));
    assert!(
        bex_events::prof::flush_and_join(Duration::from_secs(60)),
        "post-close flush must still ack"
    );
    let size_after = std::fs::metadata(&path).unwrap().len();
    assert_eq!(
        size_before, size_after,
        "closed engine's profile kept growing (heartbeats after close)"
    );

    // Later engines are unaffected.
    let source2 = r#"
        function td_after(n: int) -> int { n }
        function main() -> int { td_after(1) }
    "#;
    run_main(source2).await.expect("post-teardown engine runs");
    let (_, events2) = load_profile("user.td_after");
    assert!(!events2.is_empty());
}

/// `$id` overrides (M1): `baml.id.set()` must land a `SetFunctionId` record
/// in the stream, keyed by the same (thread, call) ids as the call's
/// `CallFunction` — one id universe across `$id` and the artifact.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn set_function_id_recorded() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function sid_work() -> string {
            let next = baml.id.new();
            baml.id.set(next);
            $id
        }
        function main() -> string { sid_work() }
    "#;
    let value = run_main(source).await.expect("sid program runs");
    let bex_engine::BexExternalValue::String(returned_id) = value else {
        panic!("expected the overridden $id string");
    };

    let (header, events) = load_profile("user.sid_work");
    assert_balance(&header, &events);

    let set_events: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::SetFunctionId(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(set_events.len(), 1, "exactly one SetFunctionId expected");
    let set = set_events[0];
    assert_eq!(set.id.len(), 16);

    // The override belongs to an open call on the same thread.
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::CallFunction(cf)
                if cf.thread_id == set.thread_id && cf.call_id == set.call_id
        )),
        "SetFunctionId must reference a recorded call"
    );

    // And the returned $id is that override (the encoded uuid matches).
    let decoded =
        bex_events::ids::RuntimeId::decode(returned_id.as_str()).expect("returned $id decodes");
    match decoded {
        bex_events::ids::RuntimeId::OverrideUuid(uuid) => {
            assert_eq!(uuid.as_slice(), set.id.as_slice());
        }
        other @ bex_events::ids::RuntimeId::DefaultCall(_) => {
            panic!("expected an override id, got {other:?}")
        }
    }
}

/// The unwind path: a thrown error must close every unwound frame with
/// `EndFunction{Error}` and the thread with `EndThread{Errored}`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unwind_emits_error_ends() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function uw_inner(n: int) -> int throws string { throw "boom" }
        function uw_mid(n: int) -> int throws string { uw_inner(n) }
        function main() -> int throws string { uw_mid(0) }
    "#;
    let result = run_main(source).await;
    assert!(result.is_err(), "program must surface the throw");

    let (header, events) = load_profile("user.uw_inner");
    let (counts, _) = assert_balance(&header, &events);
    assert_eq!(counts.get("user.uw_inner"), Some(&1));

    let fqn_by_id: HashMap<u32, &str> = header
        .function_table
        .as_ref()
        .map(|t| {
            t.functions
                .iter()
                .map(|f| (f.function_id, f.fqn.as_str()))
                .collect()
        })
        .unwrap_or_default();
    // Map call ids -> fqn from the CallFunctions, then check the unwound
    // frames ended with Error.
    let mut fqn_by_call: HashMap<(u64, u64), &str> = HashMap::new();
    for event in &events {
        if let Event::CallFunction(cf) = event {
            fqn_by_call.insert(
                (cf.thread_id, cf.call_id),
                fqn_by_id.get(&cf.function_id).copied().unwrap_or(""),
            );
        }
    }
    let mut errored_fqns: HashSet<&str> = HashSet::new();
    let mut thread_errored = false;
    for event in &events {
        match event {
            Event::EndFunction(ef) if ef.status == pb::FunctionEndStatus::Error as i32 => {
                errored_fqns.insert(fqn_by_call[&(ef.thread_id, ef.call_id)]);
            }
            Event::EndThread(et) if et.status == pb::ThreadEndStatus::Errored as i32 => {
                thread_errored = true;
            }
            _ => {}
        }
    }
    for fqn in ["user.uw_inner", "user.uw_mid", "user.main"] {
        assert!(
            errored_fqns.contains(fqn),
            "{fqn} should have ended with Error (got {errored_fqns:?})"
        );
    }
    assert!(thread_errored, "the root thread should end Errored");
}

/// Joins each `EndFunction` back to its call's fqn (via the header's
/// function table): fqn -> the raw statuses of its ended calls, in file
/// order. A call whose function id is missing from the table lands under
/// `"<unassigned>"`.
fn end_statuses_by_fqn(
    header: &pb::EventFileHeaderV1,
    events: &[Event],
) -> HashMap<String, Vec<i32>> {
    let fqn_by_id: HashMap<u32, &str> = header
        .function_table
        .as_ref()
        .map(|t| {
            t.functions
                .iter()
                .map(|f| (f.function_id, f.fqn.as_str()))
                .collect()
        })
        .unwrap_or_default();
    let mut fqn_by_call: HashMap<(u64, u64), &str> = HashMap::new();
    for event in events {
        if let Event::CallFunction(cf) = event {
            fqn_by_call.insert(
                (cf.thread_id, cf.call_id),
                fqn_by_id
                    .get(&cf.function_id)
                    .copied()
                    .unwrap_or("<unassigned>"),
            );
        }
    }
    let mut statuses: HashMap<String, Vec<i32>> = HashMap::new();
    for event in events {
        if let Event::EndFunction(ef) = event {
            let fqn = fqn_by_call
                .get(&(ef.thread_id, ef.call_id))
                .copied()
                .unwrap_or("<orphan>");
            statuses.entry(fqn.to_string()).or_default().push(ef.status);
        }
    }
    statuses
}

/// Cancellation-tolerant variant of [`assert_balance`]: duplicates are
/// still illegal, every `EndFunction` must match a recorded `CallFunction`,
/// and every started thread must still end — but calls that were OPEN when
/// their thread was cancelled may lack an `EndFunction` (see the
/// `KNOWN GAP` notes at the call sites). Returns the fqns of the unended
/// calls, sorted.
fn assert_balance_allowing_unended(
    header: &pb::EventFileHeaderV1,
    events: &[Event],
) -> Vec<String> {
    let fqn_by_id: HashMap<u32, &str> = header
        .function_table
        .as_ref()
        .map(|t| {
            t.functions
                .iter()
                .map(|f| (f.function_id, f.fqn.as_str()))
                .collect()
        })
        .unwrap_or_default();

    let mut calls: HashMap<(u64, u64), &str> = HashMap::new();
    let mut ends: HashSet<(u64, u64)> = HashSet::new();
    let mut started_threads: HashSet<u64> = HashSet::new();
    let mut ended_threads: HashSet<u64> = HashSet::new();
    for event in events {
        match event {
            Event::CallFunction(cf) => {
                let fqn = fqn_by_id
                    .get(&cf.function_id)
                    .copied()
                    .unwrap_or("<unassigned>");
                assert!(
                    calls.insert((cf.thread_id, cf.call_id), fqn).is_none(),
                    "duplicate CallFunction ({}, {})",
                    cf.thread_id,
                    cf.call_id
                );
            }
            Event::EndFunction(ef) => {
                assert!(
                    ends.insert((ef.thread_id, ef.call_id)),
                    "duplicate EndFunction ({}, {})",
                    ef.thread_id,
                    ef.call_id
                );
            }
            Event::StartThread(st) => {
                assert!(
                    started_threads.insert(st.thread_id),
                    "duplicate StartThread {}",
                    st.thread_id
                );
            }
            Event::EndThread(et) => {
                assert!(
                    ended_threads.insert(et.thread_id),
                    "duplicate EndThread {}",
                    et.thread_id
                );
            }
            Event::SetFunctionId(_) | Event::Heartbeat(_) => {}
        }
    }
    for key in &ends {
        assert!(
            calls.contains_key(key),
            "orphan EndFunction {key:?} without a CallFunction"
        );
    }
    assert_eq!(
        started_threads, ended_threads,
        "every StartThread must have exactly one EndThread"
    );
    let mut unended: Vec<String> = calls
        .iter()
        .filter(|(key, _)| !ends.contains(key))
        .map(|(_, fqn)| (*fqn).to_string())
        .collect();
    unended.sort();
    unended
}

/// A throw caught two frames above the thrower (port of the JSONL-era
/// `bex_disk_events_balance_across_catch_two_frames_up`): the multi-frame
/// unwind must keep the ring balanced — the unwound frames end `Error`, the
/// catching frame and `main` end `Ok`, and the thread completes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn caught_exception_keeps_ring_balance() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function ce_boom() -> int throws string { throw "deep" }
        function ce_mid() -> int throws string { ce_boom() }
        function ce_safe() -> int {
            ce_mid() catch (e) {
                _ => 42
            }
        }
        function main() -> int { ce_safe() }
    "#;
    let value = run_main(source)
        .await
        .expect("the catch swallows the throw");
    assert_eq!(value, BexExternalValue::Int(42));

    let (header, events) = load_profile("user.ce_safe");
    let (_, threads) = assert_balance(&header, &events);
    assert_eq!(threads.len(), 1, "single-threaded program");

    let statuses = end_statuses_by_fqn(&header, &events);
    for fqn in ["user.ce_boom", "user.ce_mid"] {
        assert_eq!(
            statuses.get(fqn),
            Some(&vec![pb::FunctionEndStatus::Error as i32]),
            "{fqn} was unwound and must end Error: {statuses:?}"
        );
    }
    for fqn in ["user.ce_safe", "user.main"] {
        assert_eq!(
            statuses.get(fqn),
            Some(&vec![pb::FunctionEndStatus::Ok as i32]),
            "{fqn} caught (or sat above the catch) and must end Ok: {statuses:?}"
        );
    }
    assert!(
        events.iter().any(|e| matches!(e, Event::EndThread(et)
            if et.status == pb::ThreadEndStatus::Completed as i32)),
        "the root thread must end Completed"
    );
}

/// `call_callable` (the HTTP-handler path; port of the JSONL-era
/// `call_callable_emits_balanced_disk_lifecycle`): the callable invocation
/// must be balanced and its root `CallFunction` must carry the *real*
/// callee's function id — not the unknown-function sentinel and not an
/// unassigned id (the regression left an orphan `EndFunction`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn call_callable_has_real_identity_and_balance() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function cc_callee(x: int) -> int { x + 1 }
        function cc_get() -> (int) -> int { cc_callee }
    "#;
    let program = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            program,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("engine construction"),
    );
    let handle = match engine
        .call_function(
            "cc_get",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            false,
        )
        .await
        .expect("cc_get runs")
    {
        BexExternalValue::Handle(handle) => handle,
        other => panic!("expected a callable handle, got {other:?}"),
    };
    let value = engine
        .call_callable(
            handle,
            vec![BexExternalValue::Int(41)],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("call_callable runs");
    assert_eq!(value, BexExternalValue::Int(42));
    drop(engine);

    let (header, events) = load_profile("user.cc_callee");
    let (counts, threads) = assert_balance(&header, &events);
    // One root thread per entry call: cc_get's and call_callable's.
    assert_eq!(threads.len(), 2, "two entry calls -> two root threads");
    assert_eq!(
        counts.get("user.cc_callee"),
        Some(&1),
        "the callable's CallFunction must resolve to the real callee: {counts:?}"
    );
    assert!(
        !counts.contains_key("<unassigned>"),
        "no CallFunction may carry an unassigned function id: {counts:?}"
    );
    assert!(
        !counts.contains_key("baml.<unknown-function>"),
        "the callee must not fall back to the unknown sentinel: {counts:?}"
    );
    // The callee runs as its own thread's root call.
    let callee_ids: HashSet<u32> = header
        .function_table
        .as_ref()
        .map(|t| {
            t.functions
                .iter()
                .filter(|f| f.fqn == "user.cc_callee")
                .map(|f| f.function_id)
                .collect()
        })
        .unwrap_or_default();
    assert!(
        events.iter().any(|e| matches!(e, Event::CallFunction(cf)
            if callee_ids.contains(&cf.function_id) && cf.parent_call_id.is_none())),
        "cc_callee must be a thread-root call"
    );
}

/// Cancelling the root call mid-`sleep` (port of the JSONL-era
/// `root_cancellation_emits_cancelled_statuses`): the call errors out and
/// the root thread's `EndThread` lands with `Cancelled`.
///
/// KNOWN GAP: unlike the old JSONL stream (which drained every open span
/// with a `Cancelled` `EndFunction`), engine-side cancellation does not
/// unwind the VM — the root call's `CallFunction` gets NO `EndFunction` at
/// all (the in-flight sleep sysop pair does close, with `Error`). So full
/// `assert_balance` cannot hold here; this pins what does: no duplicates,
/// no orphan ends, the thread closes `Cancelled`, and the only unended
/// call is the open root frame.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn root_cancellation_ends_thread_cancelled() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function rcx_pin() -> int { 1 }
        function main() -> int {
            rcx_pin();
            baml.sys.sleep(5000);
            2
        }
    "#;
    let program = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            program,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("engine construction"),
    );
    let cancel = CancellationToken::new();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_cancel_token(cancel.clone())
        .build();
    let engine_clone = Arc::clone(&engine);
    let task = tokio::spawn(async move {
        engine_clone
            .call_function("main", vec![], call_ctx, true)
            .await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    cancel.cancel();
    let result = task.await.expect("call task joins");
    assert!(result.is_err(), "a cancelled call must not return Ok");
    drop(engine);

    let (header, events) = load_profile("user.rcx_pin");
    let unended = assert_balance_allowing_unended(&header, &events);
    assert_eq!(
        unended,
        vec!["user.main".to_string()],
        "only the frame open at cancellation (the root call) may lack an EndFunction"
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::EndThread(et)
            if et.status == pb::ThreadEndStatus::Cancelled as i32)),
        "the root thread must end Cancelled"
    );
}

/// Cancelling only a spawned child (port of the JSONL-era
/// `spawned_child_cancellation_emits_cancelled`): the child's `EndThread`
/// is `Cancelled` while the root (which catches at the `await`) completes.
///
/// KNOWN GAP: as with root cancellation, the child's open root frame (the
/// spawn-closure lambda) gets NO `EndFunction` — cancellation does not
/// unwind the VM — so full `assert_balance` cannot hold; the old JSONL
/// contract's `EndFunction{Cancelled}` has no ring equivalent (the proto's
/// `FunctionEndStatus` is Ok | Error only).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn spawned_child_cancellation_ends_child_cancelled() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function scc_pin() -> int { 0 }
        function main() -> int {
            scc_pin();
            let tok = baml.spawn.CancelToken.new();
            let f = spawn with baml.spawn.options(cancel = tok) {
                baml.sys.sleep(10000);
                42
            };
            let _ = tok.cancel();
            (await f) catch (e) {
                baml.panics.Cancelled => 7
            }
        }
    "#;
    let value = run_main(source).await.expect("scc program runs");
    assert_eq!(value, BexExternalValue::Int(7));

    let (header, events) = load_profile("user.scc_pin");
    let unended = assert_balance_allowing_unended(&header, &events);
    assert_eq!(
        unended.len(),
        1,
        "only the child's open root frame may lack an EndFunction"
    );
    assert!(
        unended[0].contains("<lambda"),
        "the unended call is the spawn-closure lambda, got {unended:?}"
    );
    let child_threads: HashSet<u64> = events
        .iter()
        .filter_map(|e| match e {
            Event::StartThread(st) if st.parent_thread_id.is_some() => Some(st.thread_id),
            _ => None,
        })
        .collect();
    assert_eq!(child_threads.len(), 1, "exactly one spawned child");
    assert!(
        events.iter().any(|e| matches!(e, Event::EndThread(et)
            if child_threads.contains(&et.thread_id)
                && et.status == pb::ThreadEndStatus::Cancelled as i32)),
        "the child thread must end Cancelled"
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::EndThread(et)
            if !child_threads.contains(&et.thread_id)
                && et.status == pb::ThreadEndStatus::Completed as i32)),
        "the root thread must end Completed"
    );
}

/// An unhandled throw in a spawned child (port of the JSONL-era
/// `spawned_child_error_emits_error_statuses`): the child's `EndThread` is
/// `Errored` while the root (which catches at the `await`) completes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn spawned_child_error_ends_child_errored() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function sce_boom() -> int throws string { throw "child boom" }
        function main() -> int {
            let f = spawn { sce_boom() };
            (await f) catch (e) {
                _ => 9
            }
        }
    "#;
    let value = run_main(source).await.expect("sce program runs");
    assert_eq!(value, BexExternalValue::Int(9));

    let (header, events) = load_profile("user.sce_boom");
    // The error path DOES unwind the VM, so (unlike cancellation) the
    // child's stream is fully balanced.
    assert_balance(&header, &events);
    let child_threads: HashSet<u64> = events
        .iter()
        .filter_map(|e| match e {
            Event::StartThread(st) if st.parent_thread_id.is_some() => Some(st.thread_id),
            _ => None,
        })
        .collect();
    assert_eq!(child_threads.len(), 1, "exactly one spawned child");
    assert!(
        events.iter().any(|e| matches!(e, Event::EndThread(et)
            if child_threads.contains(&et.thread_id)
                && et.status == pb::ThreadEndStatus::Errored as i32)),
        "the child thread must end Errored"
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::EndThread(et)
            if !child_threads.contains(&et.thread_id)
                && et.status == pb::ThreadEndStatus::Completed as i32)),
        "the root thread must end Completed"
    );
}

/// `baml.sys.exit` status mapping (port of the JSONL-era
/// `sys_exit_status_mapping`): exit(0) is a clean termination — the root
/// thread ends `Completed`; a non-zero code ends it `Errored`. Two separate
/// programs/profiles, one per code.
///
/// Observed (not pinned): exit is implemented as a synthetic VM throw, so
/// `main`'s `EndFunction` is `Error` for BOTH exit(0) and exit(3) — a delta
/// from the old JSONL contract, where exit(0) ended the root frame `Ok`.
/// The thread-level mapping above is the deliberate contract; the
/// frame-level statuses are left unasserted.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sys_exit_status_mapping() {
    let _guard = test_lock().await;
    init_prof_env();

    let source_zero = r#"
        function sxz_pin() -> int { 0 }
        function main() -> int {
            sxz_pin();
            baml.sys.exit(0);
            1
        }
    "#;
    let result = run_main(source_zero).await;
    assert!(
        matches!(result, Err(EngineError::Exit { code: 0 })),
        "exit(0) must surface as EngineError::Exit: {result:?}"
    );
    let (header, events) = load_profile("user.sxz_pin");
    assert_balance(&header, &events);
    assert!(
        events.iter().any(|e| matches!(e, Event::EndThread(et)
            if et.status == pb::ThreadEndStatus::Completed as i32)),
        "exit(0): the root thread must end Completed"
    );

    let source_three = r#"
        function sxn_pin() -> int { 0 }
        function main() -> int {
            sxn_pin();
            baml.sys.exit(3);
            1
        }
    "#;
    let result = run_main(source_three).await;
    assert!(
        matches!(result, Err(EngineError::Exit { code: 3 })),
        "exit(3) must surface as EngineError::Exit: {result:?}"
    );
    let (header, events) = load_profile("user.sxn_pin");
    assert_balance(&header, &events);
    assert!(
        events.iter().any(|e| matches!(e, Event::EndThread(et)
            if et.status == pb::ThreadEndStatus::Errored as i32)),
        "exit(3): the root thread must end Errored"
    );
}

/// Every header's function table carries the two reserved sentinel rows —
/// the spawn-closure row and the unknown-function row — sitting one and two
/// past the highest real function id (so they can never collide), with all
/// ids in the table unique.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sentinel_rows_present_in_header() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function sn_pin() -> int { 5 }
        function main() -> int { sn_pin() }
    "#;
    run_main(source).await.expect("sn program runs");

    let (header, _events) = load_profile("user.sn_pin");
    let table = header.function_table.as_ref().expect("header has a table");

    let spawn_row = table
        .functions
        .iter()
        .find(|f| f.fqn == SPAWN_CLOSURE_FQN)
        .expect("spawn-closure sentinel row missing");
    let unknown_row = table
        .functions
        .iter()
        .find(|f| f.fqn == UNKNOWN_FUNCTION_FQN)
        .expect("unknown-function sentinel row missing");
    let max_real_id = table
        .functions
        .iter()
        .filter(|f| f.fqn != SPAWN_CLOSURE_FQN && f.fqn != UNKNOWN_FUNCTION_FQN)
        .map(|f| f.function_id)
        .max()
        .expect("table has real functions");
    assert_eq!(
        spawn_row.function_id,
        max_real_id + 1,
        "spawn-closure row must sit one past the real ids"
    );
    assert_eq!(
        unknown_row.function_id,
        max_real_id + 2,
        "unknown-function row must sit two past the real ids"
    );

    let mut seen = HashSet::new();
    for f in &table.functions {
        assert!(
            seen.insert(f.function_id),
            "duplicate function id {} in the header table",
            f.function_id
        );
    }
}

/// Two methods with the same display name (`run` on two classes; port of
/// the JSONL-era test of the same name) must each be attributed to their
/// own fqn — resolution is by identity, never a display-name scan that
/// takes the first match.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn same_display_name_functions_are_not_misattributed() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        class DnClsA {
            x: int
            function run(self) -> int {
                1
            }
        }

        class DnClsB {
            x: int
            function run(self) -> int {
                2
            }
        }

        function main() -> int {
            let a = DnClsA { x: 0 };
            let b = DnClsB { x: 0 };
            a.run() + b.run()
        }
    "#;
    let value = run_main(source).await.expect("dn program runs");
    assert_eq!(value, BexExternalValue::Int(3));

    let (header, events) = load_profile("user.DnClsA.run");
    let (counts, _) = assert_balance(&header, &events);
    assert_eq!(
        counts.get("user.DnClsA.run"),
        Some(&1),
        "DnClsA.run must get exactly its own call: {counts:?}"
    );
    assert_eq!(
        counts.get("user.DnClsB.run"),
        Some(&1),
        "DnClsB.run must get exactly its own call: {counts:?}"
    );
}
