//! End-to-end tests of the interim JSONL trace artifact: run real programs
//! through a real `NativeEventSink` and then act as the consumer — parse
//! every line, resolve ids against the header, and verify the structural
//! invariants reconstruction relies on. This is the closest thing to running
//! the downstream consumer inside this repo's CI.

#![cfg(not(target_arch = "wasm32"))]

mod common;

use std::{collections::HashMap, sync::Arc};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

fn read_jsonl(path: &std::path::Path) -> Vec<serde_json::Value> {
    let content = std::fs::read_to_string(path).expect("trace file readable");
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("line is not valid JSON ({e}): {line}"))
        })
        .collect()
}

/// T31: the full artifact is consumer-parseable — header first, every
/// `function_id` resolves in the header's function table to the expected
/// callee, per-thread Call/End balance and ordering hold, and the spawn edge
/// fields are present.
#[tokio::test]
async fn end_to_end_jsonl_file_is_consumer_parseable() {
    let source = r#"
        function inner() -> int {
            1
        }

        function main() -> int {
            let f = spawn { inner() };
            let x = inner();
            (await f) + x
        }
    "#;

    let dir = tempfile::tempdir().unwrap();
    let trace_path = dir.path().join("trace.jsonl");
    let sink = bex_events_native::start(trace_path.clone());

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            Some(sink.clone()),
            Vec::new(),
        )
        .unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();
    assert_eq!(value, BexExternalValue::Int(2));
    sink.flush();

    let lines = read_jsonl(&trace_path);
    assert!(!lines.is_empty());

    // (a) The first line is the header.
    assert_eq!(lines[0]["type"], "bex_header_v1");
    let function_table: HashMap<u64, String> = lines[0]["function_table"]
        .as_array()
        .expect("header carries the function table")
        .iter()
        .map(|f| {
            (
                f["function_id"].as_u64().unwrap(),
                f["fqn"].as_str().unwrap().to_string(),
            )
        })
        .collect();

    // (b) Every bex_call_function's function_id resolves in the table.
    let mut call_fqns = Vec::new();
    for line in &lines[1..] {
        if line["type"] == "bex_call_function" {
            let id = line["function_id"].as_u64().unwrap();
            let fqn = function_table
                .get(&id)
                .unwrap_or_else(|| panic!("function_id {id} not in header table"));
            call_fqns.push(fqn.clone());
        }
    }
    assert!(
        call_fqns.contains(&"user.main".to_string()),
        "{call_fqns:?}"
    );
    assert!(
        call_fqns.contains(&"user.inner".to_string()),
        "{call_fqns:?}"
    );
    assert!(
        call_fqns.contains(&"baml.<spawn-closure>".to_string()),
        "{call_fqns:?}"
    );

    // (c) Per-thread Call/End balance and ordering.
    let mut open: HashMap<(u64, u64), usize> = HashMap::new();
    let mut closed: HashMap<(u64, u64), usize> = HashMap::new();
    for line in &lines[1..] {
        match line["type"].as_str().unwrap_or_default() {
            "bex_call_function" => {
                let key = (
                    line["thread_id"].as_u64().unwrap(),
                    line["call_id"].as_u64().unwrap(),
                );
                *open.entry(key).or_default() += 1;
            }
            "bex_end_function" => {
                let key = (
                    line["thread_id"].as_u64().unwrap(),
                    line["call_id"].as_u64().unwrap(),
                );
                assert!(
                    open.get(&key).copied().unwrap_or(0) > closed.get(&key).copied().unwrap_or(0),
                    "EndFunction before CallFunction for {key:?}"
                );
                *closed.entry(key).or_default() += 1;
            }
            _ => {}
        }
    }
    assert_eq!(open, closed, "unbalanced Call/End in the artifact");
    for count in open.values() {
        assert_eq!(*count, 1);
    }

    // (d) The spawn edge: the child's bex_start_thread carries the parent
    // thread and parent call.
    let spawn_edge = lines[1..]
        .iter()
        .find(|l| l["type"] == "bex_start_thread" && !l["parent_thread_id"].is_null())
        .expect("child StartThread present");
    assert_eq!(spawn_edge["parent_call_id"], 1);
}

/// T32: two engines sharing one sink (the LSP/bridge topology) write an
/// attributable artifact — every disk-event line carries its `engine_id`, so
/// the two `{thread 1, call 1}` streams are separable.
#[tokio::test]
async fn two_engines_one_file_is_attributable() {
    let source = r#"
        function main() -> int {
            1
        }
    "#;

    let dir = tempfile::tempdir().unwrap();
    let trace_path = dir.path().join("trace.jsonl");
    let sink = bex_events_native::start(trace_path.clone());

    let mut engine_ids = Vec::new();
    for _ in 0..2 {
        let snapshot = compile_for_engine(source);
        let engine = Arc::new(
            BexEngine::new(
                snapshot,
                Arc::new(sys_native::SysOps::native()),
                Some(sink.clone()),
                Vec::new(),
            )
            .unwrap(),
        );
        engine_ids.push(engine.engine_id().0);
        let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
        engine
            .call_function("main", vec![], call_ctx, true)
            .await
            .unwrap();
    }
    sink.flush();
    assert_ne!(engine_ids[0], engine_ids[1], "engine ids must be distinct");

    let lines = read_jsonl(&trace_path);
    let headers: Vec<u64> = lines
        .iter()
        .filter(|l| l["type"] == "bex_header_v1")
        .map(|l| l["engine_id"].as_u64().unwrap())
        .collect();
    assert_eq!(headers, engine_ids, "one header per engine, in order");

    // Each engine's stream is complete and attributable by per-line engine_id.
    for engine_id in &engine_ids {
        let events: Vec<&serde_json::Value> = lines
            .iter()
            .filter(|l| l["type"] != "bex_header_v1" && l["engine_id"] == *engine_id)
            .collect();
        let types: Vec<&str> = events.iter().filter_map(|l| l["type"].as_str()).collect();
        assert_eq!(
            types,
            vec![
                "bex_start_thread",
                "bex_call_function",
                "bex_end_function",
                "bex_end_thread"
            ],
            "engine {engine_id}: unexpected stream"
        );
    }
}

/// T34: the header is never dropped, even when the sink's channel is held
/// saturated across the entire engine construction — header sends retry for
/// room instead of silently discarding the one line every other line depends
/// on. (Hammer threads keep refilling the channel until `BexEngine::new`
/// returns, so the header send genuinely contends; an earlier version of
/// this test stopped hammering before construction and passed even with the
/// header-drop regression reintroduced.)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn header_is_never_dropped_under_channel_pressure() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let source = r#"
        function main() -> int {
            1
        }
    "#;
    // Compile BEFORE the hammers start: compilation takes seconds, and the
    // pressure must exist at header-send time (inside BexEngine::new), not
    // merely sometime earlier.
    let snapshot = compile_for_engine(source);

    let dir = tempfile::tempdir().unwrap();
    let trace_path = dir.path().join("trace.jsonl");
    let sink = bex_events_native::start(trace_path.clone());

    let stop = Arc::new(AtomicBool::new(false));
    let mut hammers = Vec::new();
    for _ in 0..3 {
        let sink = sink.clone();
        let stop = Arc::clone(&stop);
        hammers.push(std::thread::spawn(move || {
            let mut i = 0u64;
            while !stop.load(Ordering::Relaxed) {
                sink.send_disk_event(
                    bex_events::ids::EngineId(999),
                    bex_events::DiskEventV1::Heartbeat { timestamp_ns: i },
                );
                i = i.wrapping_add(1);
            }
        }));
    }

    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            Some(sink.clone()),
            Vec::new(),
        )
        .unwrap(),
    );
    stop.store(true, Ordering::Relaxed);
    for hammer in hammers {
        hammer.join().unwrap();
    }
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();
    sink.flush();

    let lines = read_jsonl(&trace_path);
    let header_count = lines
        .iter()
        .filter(|l| l["type"] == "bex_header_v1" && l["engine_id"] == engine.engine_id().0)
        .count();
    assert_eq!(
        header_count, 1,
        "the engine's header must survive channel pressure"
    );
}
