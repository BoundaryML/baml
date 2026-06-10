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

use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use bex_events::prof::{file::read_bamlprof, pb};
use common::compile_for_engine;
use pb::disk_event_v1::Event;
use sys_native::SysOpsExt;

fn prof_dir() -> PathBuf {
    std::env::temp_dir().join(format!("bamlprof-gate-{}", std::process::id()))
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
        // The consumer keeps every engine's file open and heartbeats it ~1/s;
        // a read can race a partially flushed trailing message. Whole-message
        // states recur on the consumer's idle flush, so retry briefly.
        let mut parsed = None;
        for _ in 0..25 {
            match read_bamlprof(&path) {
                Ok(ok) => {
                    parsed = Some(ok);
                    break;
                }
                Err(_) => std::thread::sleep(Duration::from_millis(200)),
            }
        }
        let (header, events) = parsed.unwrap_or_else(|| panic!("unparseable profile {path:?}"));
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
