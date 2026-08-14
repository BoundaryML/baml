//! PR4 gates for the BEX profiling event stream (plan §5 PR4):
//!
//! - **G3 lossless**: a spawn-heavy, call-heavy program, run with the
//!   smallest legal segment size (forcing constant ring growth/recycling);
//!   the on-disk event balance must be exact — every `CallFunction` has
//!   exactly one `EndFunction`, every `StartThread` an `EndThread` — with
//!   exact per-function call counts.
//! - **Reconstruction smoke**: rebuild the per-thread call trees from the
//!   `.bamlprof` (v2 §7.2 shape) and assert nesting + spawn-edge sanity.
//! - Sys-op pairs (`PR4b`) and the unwind (`EndFunction{Errored}`) path,
//!   plus the §7 status taxonomy: `Cancelled` (cancel drain + in-flight
//!   sysop) and `Exited` (`baml.sys.exit` unwinds).
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
    value_capture::{CaptureKind, EncodedTraceValue, TraceCaptureConfig, TraceCaptureProducer},
};
use bex_events::{
    ids::BoundaryId,
    prof::{file::read_bamlprof, pb},
    run::{
        self, CallNodeId, CallStatus, ReconstructedProfile, TraceCallKey, TraceThreadKey, bamlprof,
    },
    value::{
        ByteValueArtifactSink, ValueCaptureKind, ValueFileRecord, ValueWriter,
        read_bamlvalue_from_bytes,
    },
};
use bex_vm_types::{CaptureCategory, CaptureOption, FunctionCaptureProps, Object};
use bridge_ctypes::baml_bridge::cffi::{
    BamlOutboundValue, baml_outbound_value::Value as OutboundValue,
};
use common::compile_for_engine;
use pb::disk_event_v1::Event;
use prost::Message;
use sys_native::SysOpsExt;

/// Synthetic header rows (see `bex_engine`'s `SPAWN_CLOSURE_FQN` /
/// `UNKNOWN_FUNCTION_FQN`).
const SPAWN_CLOSURE_FQN: &str = "baml.<spawn-closure>";
const UNKNOWN_FUNCTION_FQN: &str = "baml.<unknown-function>";
const PROFILE_PARITY_SOURCE: &str = r#"
    function parity_leaf(n: int) -> int { n + 1 }
    function parity_mid(n: int) -> int {
        parity_leaf(n) + parity_leaf(n + 10)
    }
    function main() -> int {
        parity_mid(1) + parity_leaf(5)
    }
"#;

fn phase5_call_output_error_capture() -> FunctionCaptureProps {
    FunctionCaptureProps::disabled()
        .with_auto(CaptureCategory::Output)
        .with_auto(CaptureCategory::Error)
}

fn decode_outbound_value(body: &[u8]) -> BamlOutboundValue {
    BamlOutboundValue::decode(body).expect("captured body decodes as BamlOutboundValue")
}

fn outbound_map_field<'a>(value: &'a BamlOutboundValue, key: &str) -> &'a BamlOutboundValue {
    let Some(OutboundValue::MapValue(map)) = value.value.as_ref() else {
        panic!("expected captured value map, got {value:?}");
    };
    map.entries
        .iter()
        .find(|entry| entry.key == key)
        .and_then(|entry| entry.value.as_ref())
        .unwrap_or_else(|| panic!("captured input map omitted `{key}`: {map:?}"))
}

fn outbound_string_list(value: &BamlOutboundValue) -> Vec<String> {
    let Some(OutboundValue::ListValue(list)) = value.value.as_ref() else {
        panic!("expected captured value list, got {value:?}");
    };
    list.items
        .iter()
        .map(|item| {
            let Some(OutboundValue::StringValue(value)) = item.value.as_ref() else {
                panic!("expected captured list item string, got {item:?}");
            };
            value.clone()
        })
        .collect()
}

#[test]
fn llm_function_capture_defaults_auto_inputs_outputs_errors() {
    let source = r##"
        client C = openai.OpenAiClient.new(model = "gpt-4o", api_key = "sk-test");

        function capture_phase6_llm(name: string) -> string {
            client: C
            prompt: `Hello, ${name}`
        }

        function capture_phase6_plain(name: string) -> string {
            name
        }
    "##;
    let program = compile_for_engine(source);
    let mut llm_capture = None;
    let mut plain_capture = None;
    for object in &program.objects {
        let Object::Function(function) = object else {
            continue;
        };
        match function.name.as_str() {
            "user.capture_phase6_llm" => llm_capture = Some(function.capture),
            "user.capture_phase6_plain" => plain_capture = Some(function.capture),
            _ => {}
        }
    }
    let llm_capture = llm_capture.expect("LLM function emitted");
    assert_eq!(
        llm_capture.option(CaptureCategory::Input),
        CaptureOption::Auto
    );
    assert_eq!(
        llm_capture.option(CaptureCategory::Output),
        CaptureOption::Auto
    );
    assert_eq!(
        llm_capture.option(CaptureCategory::Error),
        CaptureOption::Auto
    );
    assert_eq!(
        plain_capture.expect("plain function emitted"),
        FunctionCaptureProps::disabled()
    );
}

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
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
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
        bex_events::prof::flush_and_join(Duration::from_mins(1)),
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

/// Returns whether any readable profile file contains `marker_fqn` in its
/// function table. Unlike [`load_profile`], absence is a valid result: tests
/// use this to assert a suppressed entry call did not create a profile
/// component.
fn profile_contains_marker(marker_fqn: &str) -> bool {
    assert!(
        bex_events::prof::flush_and_join(Duration::from_mins(1)),
        "consumer never acked the flush"
    );
    let Ok(entries) = std::fs::read_dir(prof_dir()) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(contents) = read_bamlprof(&path) else {
            continue;
        };
        let has_marker = contents
            .header
            .function_table
            .as_ref()
            .is_some_and(|t| t.functions.iter().any(|f| f.fqn == marker_fqn));
        if has_marker {
            return true;
        }
    }
    false
}

/// [`load_profile`] for tests whose program SPAWNS children: a spawned
/// thread's `EndThread` is emitted by its own tokio task after the parent
/// observes the settle, so nothing orders it before this test's flush.
/// Re-flush (bounded) until every started thread has ended — under heavy
/// parallel test load the child task can otherwise lose the race with the
/// first flush and the balance asserts see a truncated stream.
fn load_profile_quiesced(marker_fqn: &str) -> (pb::EventFileHeaderV1, Vec<Event>) {
    for _ in 0..40 {
        let (header, events) = load_profile(marker_fqn);
        let started: HashSet<u64> = events
            .iter()
            .filter_map(|e| match e {
                Event::StartThread(st) => Some(st.thread_id),
                _ => None,
            })
            .collect();
        let ended: HashSet<u64> = events
            .iter()
            .filter_map(|e| match e {
                Event::EndThread(et) => Some(et.thread_id),
                _ => None,
            })
            .collect();
        if started == ended {
            return (header, events);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    // Quiescence never arrived: return the last read; the caller's balance
    // asserts will fail with the truncated stream visible.
    load_profile(marker_fqn)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn profile_suppressed_context_does_not_emit_records() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function psc_marker() -> int { 41 }
    "#;
    let program = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine construction"),
    );

    let value = engine
        .call_function(
            "psc_marker",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_profile_enabled(false)
                .build(),
            true,
        )
        .await
        .expect("profile-disabled call succeeds");
    assert_eq!(value, BexExternalValue::Int(41));
    assert!(
        !profile_contains_marker("user.psc_marker"),
        "profile-disabled call must not emit records or create an orphan profile component"
    );

    let value = engine
        .call_function(
            "psc_marker",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("profile-enabled call succeeds");
    assert_eq!(value, BexExternalValue::Int(41));
    drop(engine);

    let (header, events) = load_profile("user.psc_marker");
    let (counts, threads) = assert_balance(&header, &events);
    assert_eq!(threads.len(), 1, "enabled call creates one root thread");
    assert_eq!(
        counts.get("user.psc_marker"),
        Some(&1),
        "enabled control call proves profiling is still active"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_profile_reconstructs_canonical_parity_shape() {
    let _guard = test_lock().await;
    init_prof_env();
    run_main(PROFILE_PARITY_SOURCE)
        .await
        .expect("profile parity program runs");

    let (header, events) = load_profile("user.parity_mid");
    assert_balance(&header, &events);
    let contents = bex_events::prof::file::BamlprofContents {
        header,
        events: events
            .into_iter()
            .map(|event| pb::DiskEventV1 { event: Some(event) })
            .collect(),
        truncated: false,
    };
    let reconstructed =
        bamlprof::reconstruct_bamlprof(&contents).expect("native profile reconstructs");
    assert_canonical_profile_parity_shape(&reconstructed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn log_capture_attributes_repeated_nested_calls() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function log_phase4_leaf() -> int {
            log.info("leaf");
            1
        }

        function log_phase4_branch() -> int {
            log.warn("branch");
            log_phase4_leaf()
        }

        function main() -> int {
            log_phase4_branch() + log_phase4_branch()
        }
    "#;
    let program = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine construction"),
    );
    let boundary_id = BoundaryId::from_bytes([4; 16]);
    let value_capture = TraceCaptureProducer::new(TraceCaptureConfig::enabled(16));
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_boundary_id(boundary_id)
        .with_capture_defaults(bex_engine::CaptureDefaults {
            values_enabled: false,
            logs_enabled: true,
        })
        .with_value_capture(value_capture.clone())
        .build();
    let result = engine
        .call_function_with_trace("main", vec![], call_ctx, true)
        .await
        .expect("logged program runs");
    assert_eq!(result.value.unwrap(), BexExternalValue::Int(2));
    drop(engine);

    let mut writer =
        ValueWriter::new(ByteValueArtifactSink::new(), boundary_id).expect("value writer");
    let captured = value_capture
        .drain_to_value_writer(&mut writer)
        .expect("log drafts encode");
    assert_eq!(captured.len(), 4, "expected 4 captured logs: {captured:#?}");
    assert_eq!(value_capture.trace_heap().retained_snapshot_count(), 0);

    let parsed = read_bamlvalue_from_bytes(writer.sink().bytes()).expect("value bytes parse");
    assert_eq!(parsed.records.len(), 4);
    assert!(
        parsed
            .records
            .iter()
            .all(|record| matches!(record, ValueFileRecord::LogEvent(_))),
        "all captured records should be log events: {:#?}",
        parsed.records
    );

    let (header, events) = load_profile("user.log_phase4_leaf");
    assert_balance(&header, &events);
    let contents = bex_events::prof::file::BamlprofContents {
        header,
        events: events
            .into_iter()
            .map(|event| pb::DiskEventV1 { event: Some(event) })
            .collect(),
        truncated: false,
    };
    let reconstructed =
        bamlprof::reconstruct_bamlprof(&contents).expect("log profile reconstructs");
    assert!(
        reconstructed.diagnostics.is_empty(),
        "profile reconstruction diagnostics: {:#?}",
        reconstructed.diagnostics
    );
    let function_by_call = reconstructed
        .calls
        .iter()
        .filter_map(|call| {
            call.function_name
                .as_ref()
                .map(|name| (call.trace_key, name))
        })
        .collect::<HashMap<_, _>>();
    let mut logs_by_function = HashMap::<String, usize>::new();
    let mut previews_by_function = HashMap::<String, Vec<String>>::new();
    for encoded in captured {
        let log = encoded.log.expect("encoded log metadata");
        let function = function_by_call
            .get(&encoded.call)
            .unwrap_or_else(|| panic!("captured log call not in profile: {:?}", encoded.call));
        *logs_by_function.entry((*function).clone()).or_default() += 1;
        previews_by_function
            .entry((*function).clone())
            .or_default()
            .push(log.message_preview.unwrap_or_default());
    }

    assert_eq!(
        logs_by_function.get("user.log_phase4_branch"),
        Some(&2),
        "branch logs should attach to repeated branch calls"
    );
    assert_eq!(
        logs_by_function.get("user.log_phase4_leaf"),
        Some(&2),
        "leaf logs should attach to nested repeated leaf calls"
    );
    assert_eq!(
        previews_by_function.get("user.log_phase4_branch").cloned(),
        Some(vec!["branch".to_string(), "branch".to_string()])
    );
    assert_eq!(
        previews_by_function.get("user.log_phase4_leaf").cloned(),
        Some(vec!["leaf".to_string(), "leaf".to_string()])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn call_output_capture_attributes_repeated_enabled_calls() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function capture_phase5_leaf(n: int) -> int {
            n + 1
        }

        function capture_phase5_branch(n: int) -> int {
            capture_phase5_leaf(n) + 10
        }

        function capture_phase5_plain(n: int) -> int {
            n + 100
        }

        function main() -> int {
            capture_phase5_branch(1) + capture_phase5_branch(10) + capture_phase5_plain(0)
        }
    "#;
    let mut program = compile_for_engine(source);
    for object in &mut program.objects {
        let Object::Function(function) = object else {
            continue;
        };
        if matches!(
            function.name.as_str(),
            "user.capture_phase5_branch" | "user.capture_phase5_leaf"
        ) {
            function.capture = phase5_call_output_error_capture();
        }
    }

    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine construction"),
    );
    let boundary_id = BoundaryId::from_bytes([5; 16]);
    let value_capture = TraceCaptureProducer::new(TraceCaptureConfig::enabled(16));
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_boundary_id(boundary_id)
        .with_capture_defaults(bex_engine::CaptureDefaults {
            values_enabled: true,
            logs_enabled: false,
        })
        .with_value_capture(value_capture.clone())
        .build();
    let result = engine
        .call_function_with_trace("main", vec![], call_ctx, true)
        .await
        .expect("captured-output program runs");
    assert_eq!(result.value.unwrap(), BexExternalValue::Int(133));
    drop(engine);

    let mut writer =
        ValueWriter::new(ByteValueArtifactSink::new(), boundary_id).expect("value writer");
    let captured = value_capture
        .drain_to_value_writer(&mut writer)
        .expect("call output drafts encode");
    assert_eq!(value_capture.trace_heap().retained_snapshot_count(), 0);

    let call_outputs = captured
        .iter()
        .filter(|encoded| encoded.kind == bex_engine::value_capture::CaptureKind::CallOutput)
        .collect::<Vec<_>>();
    assert_eq!(
        call_outputs.len(),
        4,
        "expected two branch outputs and two leaf outputs: {captured:#?}"
    );

    let parsed = read_bamlvalue_from_bytes(writer.sink().bytes()).expect("value bytes parse");
    let mut call_output_records = 0;
    for record in parsed.records {
        if let ValueFileRecord::CapturedValue(value) = record
            && value.capture.as_ref().is_some_and(|capture| {
                capture.kind == bex_events::value::ValueCaptureKind::CallOutput
            })
        {
            call_output_records += 1;
        }
    }
    assert_eq!(call_output_records, 4);

    let (header, events) = load_profile("user.capture_phase5_leaf");
    assert_balance(&header, &events);
    let contents = bex_events::prof::file::BamlprofContents {
        header,
        events: events
            .into_iter()
            .map(|event| pb::DiskEventV1 { event: Some(event) })
            .collect(),
        truncated: false,
    };
    let reconstructed =
        bamlprof::reconstruct_bamlprof(&contents).expect("call output profile reconstructs");
    assert!(
        reconstructed.diagnostics.is_empty(),
        "profile reconstruction diagnostics: {:#?}",
        reconstructed.diagnostics
    );
    let function_by_call = reconstructed
        .calls
        .iter()
        .filter_map(|call| {
            call.function_name
                .as_ref()
                .map(|name| (call.trace_key, name))
        })
        .collect::<HashMap<_, _>>();
    let mut outputs_by_function = HashMap::<String, usize>::new();
    for encoded in call_outputs {
        let function = function_by_call
            .get(&encoded.call)
            .unwrap_or_else(|| panic!("captured output call not in profile: {:?}", encoded.call));
        *outputs_by_function.entry((*function).clone()).or_default() += 1;
    }

    assert_eq!(
        outputs_by_function.get("user.capture_phase5_branch"),
        Some(&2),
        "branch outputs should attach to repeated branch calls"
    );
    assert_eq!(
        outputs_by_function.get("user.capture_phase5_leaf"),
        Some(&2),
        "leaf outputs should attach to repeated nested leaf calls"
    );
    assert!(
        !outputs_by_function.contains_key("user.capture_phase5_plain"),
        "ordinary helper without capture props should not emit call output"
    );
    assert!(
        !outputs_by_function.contains_key("user.main"),
        "root outcome stays on RunResult and should not duplicate as a call payload"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_local_id_source_to_value_artifact_snapshots_before_mutation() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function capture_phase6_summarize_array(xs: string[]) -> int {
            xs.length()
        }

        function capture_phase6_plain(xs: string[]) -> int {
            xs.length()
        }

        function main() -> int {
            let id = boundary.id()
            id.capture(inputs = true, output = false)
            let xs = ["a", "b", "c"]
            let before = capture_phase6_summarize_array(xs, $id = id)
            xs.push("d")
            let after = capture_phase6_plain(xs)
            before + after
        }
    "#;
    let program = compile_for_engine(source);
    let main_idx = program.function_indices["user.main"];
    let Object::Function(main) = &(*program.objects)[main_idx] else {
        panic!("user.main must compile to a function")
    };
    assert!(
        main.bytecode
            .instructions
            .iter()
            .any(|instruction| matches!(
                instruction,
                bex_vm_types::Instruction::CallWithRuntimeId { .. }
            )),
        "the source-level side channel must survive through runtime-ID bytecode"
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine construction"),
    );
    let boundary_id = BoundaryId::from_bytes([8; 16]);
    let value_capture = TraceCaptureProducer::new(TraceCaptureConfig::enabled(16));
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_boundary_id(boundary_id)
        .with_capture_defaults(bex_engine::CaptureDefaults {
            values_enabled: true,
            logs_enabled: false,
        })
        .with_value_capture(value_capture.clone())
        .build();
    let result = engine
        .call_function_with_trace("main", vec![], call_ctx, true)
        .await
        .expect("captured-input program runs");
    assert_eq!(result.value.unwrap(), BexExternalValue::Int(7));
    drop(engine);

    let mut writer =
        ValueWriter::new(ByteValueArtifactSink::new(), boundary_id).expect("value writer");
    let captured = value_capture
        .drain_to_value_writer(&mut writer)
        .expect("call input drafts encode");
    assert_eq!(value_capture.trace_heap().retained_snapshot_count(), 0);

    let call_inputs = captured
        .iter()
        .filter(|encoded| encoded.kind == bex_engine::value_capture::CaptureKind::CallInput)
        .collect::<Vec<_>>();
    assert_eq!(
        call_inputs.len(),
        1,
        "only the explicit-id call should emit call input: {captured:#?}"
    );

    let input_body = decode_outbound_value(&call_inputs[0].body);
    let xs = outbound_map_field(&input_body, "xs");
    assert_eq!(
        outbound_string_list(xs),
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
        "call input must preserve the pre-mutation array snapshot"
    );

    let parsed = read_bamlvalue_from_bytes(writer.sink().bytes()).expect("value bytes parse");
    let call_input_records = parsed
        .records
        .iter()
        .filter(|record| {
            matches!(
                record,
                ValueFileRecord::CapturedValue(value)
                    if value.capture.as_ref().is_some_and(|capture| capture.kind == ValueCaptureKind::CallInput)
            )
        })
        .count();
    assert_eq!(call_input_records, 1);

    let (header, events) = load_profile("user.capture_phase6_summarize_array");
    assert_balance(&header, &events);
    let contents = bex_events::prof::file::BamlprofContents {
        header,
        events: events
            .into_iter()
            .map(|event| pb::DiskEventV1 { event: Some(event) })
            .collect(),
        truncated: false,
    };
    let reconstructed =
        bamlprof::reconstruct_bamlprof(&contents).expect("call input profile reconstructs");
    assert!(
        reconstructed.diagnostics.is_empty(),
        "profile reconstruction diagnostics: {:#?}",
        reconstructed.diagnostics
    );
    let function_by_call = reconstructed
        .calls
        .iter()
        .filter_map(|call| {
            call.function_name
                .as_ref()
                .map(|name| (call.trace_key, name))
        })
        .collect::<HashMap<_, _>>();
    let input_function = function_by_call
        .get(&call_inputs[0].call)
        .unwrap_or_else(|| {
            panic!(
                "captured input call not in profile: {:?}",
                call_inputs[0].call
            )
        });
    assert_eq!(*input_function, "user.capture_phase6_summarize_array");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_local_id_reuse_is_catchable_invalid_argument() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function capture_phase6_id_leaf(n: int) -> int {
            n
        }

        function main() -> string {
            let id = boundary.id()
            let alias = id
            let first = capture_phase6_id_leaf(1, $id = id)
            let reuse = baml.json.to_string(capture_phase6_id_leaf(2, $id = alias)) catch (e) {
                baml.errors.InvalidArgument => "reuse-caught"
            }
            let mutation = baml.json.to_string(id.capture(inputs = true)) catch (e) {
                baml.errors.InvalidArgument => "mutation-caught"
            }
            reuse + "+" + mutation
        }
    "#;
    let program = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine construction"),
    );
    let value = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("single-use LocalId program runs");
    assert_eq!(
        value,
        BexExternalValue::String("reuse-caught+mutation-caught".into())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_local_id_output_and_error_overrides_write_artifacts() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function capture_phase6_output_leaf(n: int) -> int {
            n + 1
        }

        function capture_phase6_error_leaf() -> int throws string {
            throw "boom"
        }

        function main() -> int {
            let output_id = boundary.id().capture(inputs = false, output = true, error = false)
            let value = capture_phase6_output_leaf(4, $id = output_id)
            let error_id = boundary.id().capture(inputs = false, output = false, error = true)
            let _ = capture_phase6_error_leaf($id = error_id) catch (e) { _ => 0 }
            value
        }
    "#;
    let program = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine construction"),
    );
    let boundary_id = BoundaryId::from_bytes([24; 16]);
    let value_capture = TraceCaptureProducer::new(TraceCaptureConfig::enabled(16));
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_boundary_id(boundary_id)
        .with_capture_defaults(bex_engine::CaptureDefaults {
            values_enabled: true,
            logs_enabled: false,
        })
        .with_value_capture(value_capture.clone())
        .build();
    let result = engine
        .call_function_with_trace("main", vec![], call_ctx, true)
        .await
        .expect("output/error override program runs");
    assert_eq!(result.value.unwrap(), BexExternalValue::Int(5));
    drop(engine);

    let mut writer =
        ValueWriter::new(ByteValueArtifactSink::new(), boundary_id).expect("value writer");
    let captured = value_capture
        .drain_to_value_writer(&mut writer)
        .expect("explicit output/error drafts encode");
    assert_eq!(
        captured
            .iter()
            .filter(|value| value.kind == CaptureKind::CallInput)
            .count(),
        0,
        "inputs=false must override the disabled ordinary-function base without capturing"
    );
    assert_eq!(
        captured
            .iter()
            .filter(|value| value.kind == CaptureKind::CallOutput)
            .count(),
        1,
        "output=true must capture exactly the selected call"
    );
    assert_eq!(
        captured
            .iter()
            .filter(|value| value.kind == CaptureKind::CallError)
            .count(),
        1,
        "error=true must capture exactly the throw origin"
    );

    let parsed = read_bamlvalue_from_bytes(writer.sink().bytes()).expect("value bytes parse");
    let call_kinds = parsed
        .records
        .iter()
        .filter_map(|record| match record {
            ValueFileRecord::CapturedValue(value) => value.capture.as_ref().map(|c| c.kind),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        call_kinds
            .iter()
            .filter(|kind| **kind == ValueCaptureKind::CallOutput)
            .count(),
        1
    );
    assert_eq!(
        call_kinds
            .iter()
            .filter(|kind| **kind == ValueCaptureKind::CallError)
            .count(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_local_id_rejects_native_builtin_calls() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function main() -> string {
            let id = boundary.id()
            baml.json.to_string(7, $id = id) catch (e) {
                baml.errors.InvalidArgument => "caught"
            }
        }
    "#;
    let program = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine construction"),
    );
    let value = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("native explicit-id rejection is catchable");
    assert_eq!(value, BexExternalValue::String("caught".into()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_local_id_is_evaluated_last_and_exactly_once() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function capture_phase6_record_arg(events: string[]) -> int {
            events.push("arg")
            1
        }

        function capture_phase6_record_id(events: string[]) -> boundary.LocalId {
            events.push("id")
            boundary.id()
        }

        function capture_phase6_eval_leaf(n: int) -> int { n }

        function main() -> int {
            let events: string[] = []
            let _ = capture_phase6_eval_leaf(
                capture_phase6_record_arg(events),
                $id = capture_phase6_record_id(events),
            )
            if (events.length() == 2 && events[0] == "arg" && events[1] == "id") {
                1
            } else {
                0
            }
        }
    "#;
    let value = run_main(source)
        .await
        .expect("evaluation-order program runs");
    assert_eq!(value, BexExternalValue::Int(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn call_input_capture_attributes_enabled_sysop_calls() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function capture_phase6_sysop_marker() -> int {
            let id = boundary.id().capture(inputs = true)
            baml.sys.sleep(baml.time.Duration.from_milliseconds(0n), $id = id)
            1
        }

        function main() -> int {
            capture_phase6_sysop_marker()
        }
    "#;
    let program = compile_for_engine(source);

    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine construction"),
    );
    let boundary_id = BoundaryId::from_bytes([9; 16]);
    let value_capture = TraceCaptureProducer::new(TraceCaptureConfig::enabled(16));
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_boundary_id(boundary_id)
        .with_capture_defaults(bex_engine::CaptureDefaults {
            values_enabled: true,
            logs_enabled: false,
        })
        .with_value_capture(value_capture.clone())
        .build();
    let result = engine
        .call_function_with_trace("main", vec![], call_ctx, true)
        .await
        .expect("captured sys-op program runs");
    assert_eq!(result.value.unwrap(), BexExternalValue::Int(1));
    drop(engine);

    let mut writer =
        ValueWriter::new(ByteValueArtifactSink::new(), boundary_id).expect("value writer");
    let captured = value_capture
        .drain_to_value_writer(&mut writer)
        .expect("sys-op call input drafts encode");
    assert_eq!(value_capture.trace_heap().retained_snapshot_count(), 0);

    let call_inputs = captured
        .iter()
        .filter(|encoded| encoded.kind == bex_engine::value_capture::CaptureKind::CallInput)
        .collect::<Vec<_>>();
    assert_eq!(
        call_inputs.len(),
        1,
        "expected only the capture-enabled sys-op input: {captured:#?}"
    );

    let parsed = read_bamlvalue_from_bytes(writer.sink().bytes()).expect("value bytes parse");
    let call_input_records = parsed
        .records
        .iter()
        .filter(|record| {
            matches!(
                record,
                ValueFileRecord::CapturedValue(value)
                    if value.capture.as_ref().is_some_and(|capture| capture.kind == ValueCaptureKind::CallInput)
            )
        })
        .count();
    assert_eq!(call_input_records, 1);

    let (header, events) = load_profile("user.capture_phase6_sysop_marker");
    assert_balance(&header, &events);
    let contents = bex_events::prof::file::BamlprofContents {
        header,
        events: events
            .into_iter()
            .map(|event| pb::DiskEventV1 { event: Some(event) })
            .collect(),
        truncated: false,
    };
    let reconstructed =
        bamlprof::reconstruct_bamlprof(&contents).expect("sys-op input profile reconstructs");
    assert!(
        reconstructed.diagnostics.is_empty(),
        "profile reconstruction diagnostics: {:#?}",
        reconstructed.diagnostics
    );
    let function_by_call = reconstructed
        .calls
        .iter()
        .filter_map(|call| {
            call.function_name
                .as_ref()
                .map(|name| (call.trace_key, name))
        })
        .collect::<HashMap<_, _>>();
    let input_function = function_by_call
        .get(&call_inputs[0].call)
        .unwrap_or_else(|| {
            panic!(
                "captured sys-op input call not in profile: {:?}",
                call_inputs[0].call
            )
        });
    assert_eq!(*input_function, "baml.sys.sleep");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn call_output_capture_attributes_enabled_native_calls() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function capture_phase5_native_wrapper() -> string {
            baml.json.to_string(7)
        }

        function main() -> string {
            capture_phase5_native_wrapper()
        }
    "#;
    let mut program = compile_for_engine(source);
    for object in &mut program.objects {
        let Object::Function(function) = object else {
            continue;
        };
        if function.name == "baml.json.to_string" {
            function.capture = phase5_call_output_error_capture();
        }
    }

    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine construction"),
    );
    let boundary_id = BoundaryId::from_bytes([7; 16]);
    let value_capture = TraceCaptureProducer::new(TraceCaptureConfig::enabled(16));
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_boundary_id(boundary_id)
        .with_capture_defaults(bex_engine::CaptureDefaults {
            values_enabled: true,
            logs_enabled: false,
        })
        .with_value_capture(value_capture.clone())
        .build();
    let result = engine
        .call_function_with_trace("main", vec![], call_ctx, true)
        .await
        .expect("captured-native-output program runs");
    assert_eq!(result.value.unwrap(), BexExternalValue::String("7".into()));
    drop(engine);

    let mut writer =
        ValueWriter::new(ByteValueArtifactSink::new(), boundary_id).expect("value writer");
    let captured = value_capture
        .drain_to_value_writer(&mut writer)
        .expect("native call output drafts encode");
    assert_eq!(value_capture.trace_heap().retained_snapshot_count(), 0);

    let call_outputs = captured
        .iter()
        .filter(|encoded| encoded.kind == bex_engine::value_capture::CaptureKind::CallOutput)
        .collect::<Vec<_>>();
    assert_eq!(
        call_outputs.len(),
        1,
        "expected only the capture-enabled native output: {captured:#?}"
    );

    let (header, events) = load_profile("user.capture_phase5_native_wrapper");
    assert_balance(&header, &events);
    let contents = bex_events::prof::file::BamlprofContents {
        header,
        events: events
            .into_iter()
            .map(|event| pb::DiskEventV1 { event: Some(event) })
            .collect(),
        truncated: false,
    };
    let reconstructed =
        bamlprof::reconstruct_bamlprof(&contents).expect("native call profile reconstructs");
    assert!(
        reconstructed.diagnostics.is_empty(),
        "profile reconstruction diagnostics: {:#?}",
        reconstructed.diagnostics
    );
    let function_by_call = reconstructed
        .calls
        .iter()
        .filter_map(|call| {
            call.function_name
                .as_ref()
                .map(|name| (call.trace_key, name))
        })
        .collect::<HashMap<_, _>>();
    let native_function = function_by_call
        .get(&call_outputs[0].call)
        .unwrap_or_else(|| {
            panic!(
                "captured native output call not in profile: {:?}",
                captured[0]
            )
        });
    assert_eq!(*native_function, "baml.json.to_string");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn call_error_capture_records_throw_origin_without_rethrow_duplicate() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function capture_phase5_boom() -> int throws string {
            throw "boom"
        }

        function capture_phase5_rethrow() -> int throws string {
            capture_phase5_boom() catch (e) {
                _ => throw e
            }
        }

        function main() -> int throws string {
            capture_phase5_rethrow()
        }
    "#;
    let mut program = compile_for_engine(source);
    for object in &mut program.objects {
        let Object::Function(function) = object else {
            continue;
        };
        if matches!(
            function.name.as_str(),
            "user.capture_phase5_boom" | "user.capture_phase5_rethrow"
        ) {
            function.capture = phase5_call_output_error_capture();
        }
    }

    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine construction"),
    );
    let boundary_id = BoundaryId::from_bytes([6; 16]);
    let value_capture = TraceCaptureProducer::new(TraceCaptureConfig::enabled(16));
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_boundary_id(boundary_id)
        .with_capture_defaults(bex_engine::CaptureDefaults {
            values_enabled: true,
            logs_enabled: false,
        })
        .with_value_capture(value_capture.clone())
        .build();
    let result = engine
        .call_function_with_trace("main", vec![], call_ctx, true)
        .await
        .expect("runtime throw should still return a traced call result");
    assert!(
        result.value.is_err(),
        "main should rethrow the string error"
    );
    drop(engine);

    let mut writer =
        ValueWriter::new(ByteValueArtifactSink::new(), boundary_id).expect("value writer");
    let captured = value_capture
        .drain_to_value_writer(&mut writer)
        .expect("call error drafts encode");
    assert_eq!(value_capture.trace_heap().retained_snapshot_count(), 0);

    let call_errors = captured
        .iter()
        .filter(|encoded| encoded.kind == bex_engine::value_capture::CaptureKind::CallError)
        .collect::<Vec<_>>();
    assert_eq!(
        call_errors.len(),
        1,
        "rethrowing the same caught value must not duplicate wrapper errors: {captured:#?}"
    );

    let parsed = read_bamlvalue_from_bytes(writer.sink().bytes()).expect("value bytes parse");
    let call_error_records = parsed
        .records
        .iter()
        .filter(|record| {
            matches!(
                record,
                ValueFileRecord::CapturedValue(value)
                    if value.capture.as_ref().is_some_and(|capture| capture.kind == ValueCaptureKind::CallError)
            )
        })
        .count();
    assert_eq!(call_error_records, 1);

    let (header, events) = load_profile("user.capture_phase5_boom");
    assert_balance(&header, &events);
    let contents = bex_events::prof::file::BamlprofContents {
        header,
        events: events
            .into_iter()
            .map(|event| pb::DiskEventV1 { event: Some(event) })
            .collect(),
        truncated: false,
    };
    let reconstructed =
        bamlprof::reconstruct_bamlprof(&contents).expect("call error profile reconstructs");
    assert!(
        reconstructed.diagnostics.is_empty(),
        "profile reconstruction diagnostics: {:#?}",
        reconstructed.diagnostics
    );
    let function_by_call = reconstructed
        .calls
        .iter()
        .filter_map(|call| {
            call.function_name
                .as_ref()
                .map(|name| (call.trace_key, name))
        })
        .collect::<HashMap<_, _>>();
    let origin_function = function_by_call
        .get(&call_errors[0].call)
        .unwrap_or_else(|| {
            panic!(
                "captured error call not in profile: {:?}",
                call_errors[0].call
            )
        });
    assert_eq!(*origin_function, "user.capture_phase5_boom");
}

async fn captured_call_errors_for_source(
    source: &str,
    capture_functions: &[&str],
    marker_fqn: &str,
    boundary_id: BoundaryId,
) -> (Vec<EncodedTraceValue>, HashMap<TraceCallKey, String>) {
    let mut program = compile_for_engine(source);
    for object in &mut program.objects {
        let Object::Function(function) = object else {
            continue;
        };
        if capture_functions.contains(&function.name.as_str()) {
            function.capture = phase5_call_output_error_capture();
        }
    }

    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine construction"),
    );
    let value_capture = TraceCaptureProducer::new(TraceCaptureConfig::enabled(16));
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_boundary_id(boundary_id)
        .with_capture_defaults(bex_engine::CaptureDefaults {
            values_enabled: true,
            logs_enabled: false,
        })
        .with_value_capture(value_capture.clone())
        .build();
    let result = engine
        .call_function_with_trace("main", vec![], call_ctx, true)
        .await
        .expect("program should produce a traced call result");
    assert_eq!(result.value.unwrap(), BexExternalValue::Int(0));
    drop(engine);

    let mut writer =
        ValueWriter::new(ByteValueArtifactSink::new(), boundary_id).expect("value writer");
    let captured = value_capture
        .drain_to_value_writer(&mut writer)
        .expect("call error drafts encode");
    assert_eq!(value_capture.trace_heap().retained_snapshot_count(), 0);

    let call_errors = captured
        .into_iter()
        .filter(|encoded| encoded.kind == CaptureKind::CallError)
        .collect::<Vec<_>>();
    let parsed = read_bamlvalue_from_bytes(writer.sink().bytes()).expect("value bytes parse");
    let call_error_records = parsed
        .records
        .iter()
        .filter(|record| {
            matches!(
                record,
                ValueFileRecord::CapturedValue(value)
                    if value.capture.as_ref().is_some_and(|capture| capture.kind == ValueCaptureKind::CallError)
            )
        })
        .count();
    assert_eq!(call_error_records, call_errors.len());

    let (header, events) = load_profile(marker_fqn);
    assert_balance(&header, &events);
    let contents = bex_events::prof::file::BamlprofContents {
        header,
        events: events
            .into_iter()
            .map(|event| pb::DiskEventV1 { event: Some(event) })
            .collect(),
        truncated: false,
    };
    let reconstructed =
        bamlprof::reconstruct_bamlprof(&contents).expect("call error profile reconstructs");
    assert!(
        reconstructed.diagnostics.is_empty(),
        "profile reconstruction diagnostics: {:#?}",
        reconstructed.diagnostics
    );
    let function_by_call = reconstructed
        .calls
        .iter()
        .filter_map(|call| {
            call.function_name
                .as_ref()
                .map(|name| (call.trace_key, name.clone()))
        })
        .collect::<HashMap<_, _>>();

    (call_errors, function_by_call)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn call_error_capture_keeps_independent_equal_primitive_origins() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function capture_phase5_equal_fail_a() -> int throws int {
            throw 5
        }

        function capture_phase5_equal_fail_b() -> int throws int {
            throw 5
        }

        function main() -> int {
            let _ = capture_phase5_equal_fail_a() catch (e) { let e => 0 };
            let _ = capture_phase5_equal_fail_b() catch (e) { let e => 0 };
            0
        }
    "#;
    let (call_errors, function_by_call) = captured_call_errors_for_source(
        source,
        &[
            "user.capture_phase5_equal_fail_a",
            "user.capture_phase5_equal_fail_b",
        ],
        "user.capture_phase5_equal_fail_a",
        BoundaryId::from_bytes([18; 16]),
    )
    .await;

    let mut origin_functions = call_errors
        .iter()
        .map(|encoded| {
            function_by_call
                .get(&encoded.call)
                .unwrap_or_else(|| panic!("captured error call not in profile: {encoded:?}"))
                .clone()
        })
        .collect::<Vec<_>>();
    origin_functions.sort();
    assert_eq!(
        origin_functions,
        vec![
            "user.capture_phase5_equal_fail_a".to_string(),
            "user.capture_phase5_equal_fail_b".to_string(),
        ],
        "independent equal primitive throws must keep distinct origins: {call_errors:#?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn call_error_capture_treats_equal_wrapper_throw_as_new_origin() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function capture_phase5_equal_leaf() -> int throws int {
            throw 5
        }

        function capture_phase5_equal_wrapper_new_throw() -> int throws int {
            capture_phase5_equal_leaf() catch (e) {
                let e => throw 5
            }
        }

        function main() -> int {
            capture_phase5_equal_wrapper_new_throw() catch (e) { let e => 0 }
        }
    "#;
    let (call_errors, function_by_call) = captured_call_errors_for_source(
        source,
        &[
            "user.capture_phase5_equal_leaf",
            "user.capture_phase5_equal_wrapper_new_throw",
        ],
        "user.capture_phase5_equal_leaf",
        BoundaryId::from_bytes([19; 16]),
    )
    .await;

    let mut origin_functions = call_errors
        .iter()
        .map(|encoded| {
            function_by_call
                .get(&encoded.call)
                .unwrap_or_else(|| panic!("captured error call not in profile: {encoded:?}"))
                .clone()
        })
        .collect::<Vec<_>>();
    origin_functions.sort();
    assert_eq!(
        origin_functions,
        vec![
            "user.capture_phase5_equal_leaf".to_string(),
            "user.capture_phase5_equal_wrapper_new_throw".to_string(),
        ],
        "wrapper throw of a new equal value must not be collapsed into the leaf origin: {call_errors:#?}"
    );
}

fn assert_canonical_profile_parity_shape(reconstructed: &ReconstructedProfile) {
    assert!(
        reconstructed.diagnostics.is_empty(),
        "profile parity reconstruction diagnostics: {:#?}",
        reconstructed.diagnostics
    );

    let calls_by_id: HashMap<_, _> = reconstructed
        .calls
        .iter()
        .map(|call| (call.id, call))
        .collect();
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut edges: HashMap<(String, String), usize> = HashMap::new();

    for call in &reconstructed.calls {
        assert_eq!(
            call.status,
            CallStatus::Ok,
            "parity fixture should have only successful calls"
        );
        let start = call.started_at_ns.expect("call has start timestamp");
        let end = call.ended_at_ns.expect("call has end timestamp");
        assert!(end >= start, "call timestamps are monotonic");

        let function_name = call
            .function_name
            .clone()
            .unwrap_or_else(|| "<unknown>".to_string());
        *counts.entry(function_name.clone()).or_default() += 1;
        let parent_name = call
            .parent_id
            .and_then(|parent_id| calls_by_id.get(&parent_id))
            .and_then(|parent| parent.function_name.clone())
            .unwrap_or_else(|| "<root>".to_string());
        *edges.entry((parent_name, function_name)).or_default() += 1;
    }

    assert_eq!(counts.get("user.main"), Some(&1));
    assert_eq!(counts.get("user.parity_mid"), Some(&1));
    assert_eq!(counts.get("user.parity_leaf"), Some(&3));
    assert_eq!(
        edges.get(&("<root>".to_string(), "user.main".to_string())),
        Some(&1)
    );
    assert_eq!(
        edges.get(&("user.main".to_string(), "user.parity_mid".to_string())),
        Some(&1)
    );
    assert_eq!(
        edges.get(&(
            "user.parity_mid".to_string(),
            "user.parity_leaf".to_string()
        )),
        Some(&2)
    );
    assert_eq!(
        edges.get(&("user.main".to_string(), "user.parity_leaf".to_string())),
        Some(&1)
    );
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

    let (header, events) = load_profile_quiesced("user.g3_work");
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

    let (header, events) = load_profile_quiesced("user.rc_mid");
    assert_balance(&header, &events);

    let contents = bex_events::prof::file::BamlprofContents {
        header,
        events: events
            .iter()
            .cloned()
            .map(|event| pb::DiskEventV1 { event: Some(event) })
            .collect(),
        truncated: false,
    };
    let reconstructed = bamlprof::reconstruct_bamlprof(&contents)
        .expect("profile reconstructs through production module");
    assert!(
        reconstructed.diagnostics.is_empty(),
        "production reconstruction diagnostics: {:#?}",
        reconstructed.diagnostics
    );
    assert_eq!(
        reconstructed.threads.len(),
        2,
        "root plus one spawned thread"
    );
    let calls_by_id: HashMap<CallNodeId, _> = reconstructed
        .calls
        .iter()
        .map(|call| (call.id, call))
        .collect();
    for call in &reconstructed.calls {
        if let Some(parent_id) = call.parent_id {
            let parent = calls_by_id
                .get(&parent_id)
                .expect("same-thread parent call exists");
            assert_eq!(
                parent.thread_id, call.thread_id,
                "CallNode.parent_id is same-thread only"
            );
        }
    }
    let child_threads: Vec<_> = reconstructed
        .threads
        .iter()
        .filter(|thread| thread.parent_thread_id.is_some())
        .collect();
    assert_eq!(child_threads.len(), 1, "exactly one spawned thread");
    assert!(
        child_threads[0].parent_call_node_id.is_some(),
        "spawn edge keeps parent call provenance"
    );
    let reconstructed_counts =
        reconstructed
            .calls
            .iter()
            .fold(HashMap::<&str, usize>::new(), |mut counts, call| {
                if let Some(name) = call.function_name.as_deref() {
                    *counts.entry(name).or_default() += 1;
                }
                counts
            });
    assert_eq!(reconstructed_counts.get("user.rc_mid"), Some(&2));
    assert_eq!(reconstructed_counts.get("user.rc_leaf"), Some(&2));

    let normalized_events =
        bamlprof::normalized_events(&contents).expect("profile events normalize from .bamlprof");
    let reversed = run::reconstruct_with_function_table(
        normalized_events.iter().cloned().rev(),
        reconstructed.function_table.clone(),
    );
    assert!(
        reversed.diagnostics.is_empty(),
        "reverse-order reconstruction diagnostics: {:#?}",
        reversed.diagnostics
    );
    let call_ids = reconstructed
        .calls
        .iter()
        .map(|call| (call.trace_key, call.id))
        .collect::<HashMap<TraceCallKey, CallNodeId>>();
    let reversed_call_ids = reversed
        .calls
        .iter()
        .map(|call| (call.trace_key, call.id))
        .collect::<HashMap<TraceCallKey, CallNodeId>>();
    assert_eq!(
        call_ids, reversed_call_ids,
        "CallNodeId is trace-derived, not arrival-order-derived"
    );
    let thread_ids = reconstructed
        .threads
        .iter()
        .map(|thread| (thread.trace_key, thread.id))
        .collect::<HashMap<TraceThreadKey, _>>();
    let reversed_thread_ids = reversed
        .threads
        .iter()
        .map(|thread| (thread.trace_key, thread.id))
        .collect::<HashMap<TraceThreadKey, _>>();
    assert_eq!(
        thread_ids, reversed_thread_ids,
        "ThreadNodeId is trace-derived, not arrival-order-derived"
    );

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
        // §7 decision 7: every thread's first record is its StartThread —
        // roots emit it just before the entry frame's CallFunction, children
        // at the Spawn arm before their entry push.
        assert!(
            matches!(thread_events.first(), Some(Event::StartThread(_))),
            "thread {tid}: first record must be StartThread, got {:?}",
            thread_events.first()
        );
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
            baml.sys.sleep(baml.time.Duration.from_milliseconds(2n));
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
    let (_, events) = load_profile_quiesced("user.td_work");
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
        bex_events::prof::flush_and_join(Duration::from_mins(1)),
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
    let (_, events2) = load_profile_quiesced("user.td_after");
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

    let sid_work_function_id = header
        .function_table
        .as_ref()
        .and_then(|table| {
            table
                .functions
                .iter()
                .find(|f| f.fqn == "user.sid_work")
                .map(|f| f.function_id)
        })
        .expect("sid_work must be present in the function table");
    let sid_work_calls: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::CallFunction(cf) if cf.function_id == sid_work_function_id => Some(cf),
            _ => None,
        })
        .collect();
    assert_eq!(
        sid_work_calls.len(),
        1,
        "expected exactly one sid_work call"
    );
    let sid_work_call = sid_work_calls[0];

    let set_events: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::SetFunctionId(s)
                if s.thread_id == sid_work_call.thread_id && s.call_id == sid_work_call.call_id =>
            {
                Some(s)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        set_events.len(),
        1,
        "exactly one SetFunctionId expected for sid_work"
    );
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
        bex_events::ids::RuntimeId::Boundary(boundary_id) => {
            assert_eq!(boundary_id.as_bytes().as_slice(), set.id.as_slice());
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
            Event::EndFunction(ef) if ef.status == pb::FunctionEndStatus::Errored as i32 => {
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
            Some(&vec![pb::FunctionEndStatus::Errored as i32]),
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
        function cc_get() -> (int) -> int throws never { cc_callee }
    "#;
    let program = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
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
/// `root_cancellation_emits_cancelled_statuses`): §7 decision 2 — the
/// engine drains the calls left open by the cancel with
/// `EndFunction{Cancelled}` (cancelled threads never strand open calls),
/// the in-flight sleep sysop pair closes `Cancelled` (§7 decision 1), the
/// thread ends `Cancelled`, and full balance holds.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn root_cancellation_ends_thread_cancelled() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function rcx_pin() -> int { 1 }
        function main() -> int {
            rcx_pin();
            baml.sys.sleep(baml.time.Duration.from_milliseconds(5000n));
            2
        }
    "#;
    let program = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
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
    assert_balance(&header, &events);
    let statuses = end_statuses_by_fqn(&header, &events);
    assert_eq!(
        statuses.get("user.main"),
        Some(&vec![pb::FunctionEndStatus::Cancelled as i32]),
        "the cancel drain closes the open root call Cancelled: {statuses:?}"
    );
    assert_eq!(
        statuses.get("baml.sys.sleep"),
        Some(&vec![pb::FunctionEndStatus::Cancelled as i32]),
        "the in-flight sysop pair closes Cancelled: {statuses:?}"
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
/// §7 decision 2: the engine's cancel drain closes the child's open
/// spawn-closure frame with `EndFunction{Cancelled}`, so full balance
/// holds.
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
                baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n));
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

    let (header, events) = load_profile_quiesced("user.scc_pin");
    assert_balance(&header, &events);
    let statuses = end_statuses_by_fqn(&header, &events);
    // Two lambdas exist here: the `baml.spawn.options` argument closure
    // (runs to completion, Ok) and the spawned body. Exactly the spawned
    // body is closed Cancelled by the drain.
    let cancelled_lambdas: Vec<&String> = statuses
        .iter()
        .filter(|(fqn, s)| {
            fqn.contains("<lambda") && **s == vec![pb::FunctionEndStatus::Cancelled as i32]
        })
        .map(|(fqn, _)| fqn)
        .collect();
    assert_eq!(
        cancelled_lambdas.len(),
        1,
        "the cancel drain closes exactly the spawned-body lambda Cancelled: {statuses:?}"
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

/// B-405: an unobserved child error does not alter the parent profile.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unobserved_child_error_keeps_parent_completed() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function uce_pin() -> int { 0 }
        function uce_bad() -> int throws string { throw "boom" }
        function uce_other() -> int { baml.sys.sleep(baml.time.Duration.from_milliseconds(50n)); 1 }
        function main() -> int {
            uce_pin();
            spawn { uce_bad() };
            let g = spawn { uce_other() };
            await g
        }
    "#;
    let value = run_main(source).await.expect("parent call succeeds");
    assert_eq!(value, BexExternalValue::Int(1));

    let (header, events) = load_profile_quiesced("user.uce_pin");
    assert_balance(&header, &events);
    let statuses = end_statuses_by_fqn(&header, &events);
    assert_eq!(
        statuses.get("user.main"),
        Some(&vec![pb::FunctionEndStatus::Ok as i32]),
        "the parent frame completes normally: {statuses:?}"
    );
    assert_eq!(
        statuses.get("user.uce_bad"),
        Some(&vec![pb::FunctionEndStatus::Errored as i32]),
        "the child's thrower was unwound VM-side and ends Errored: {statuses:?}"
    );
    let child_threads: HashSet<u64> = events
        .iter()
        .filter_map(|e| match e {
            Event::StartThread(st) if st.parent_thread_id.is_some() => Some(st.thread_id),
            _ => None,
        })
        .collect();
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

    let (header, events) = load_profile_quiesced("user.sce_boom");
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
/// §7 decision 3: exit is a recognized unwind class. Frames unwound by a
/// `baml.panics.Exit` close `Exited` (the frame's fate — true for any exit
/// code; the code itself is a thread/program-level fact carried by
/// `EndThread.status`), and the `baml.sys.exit` native's own pair closes
/// `Exited` too.
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
    let statuses = end_statuses_by_fqn(&header, &events);
    for fqn in ["user.main", "baml.sys.exit"] {
        assert_eq!(
            statuses.get(fqn),
            Some(&vec![pb::FunctionEndStatus::Exited as i32]),
            "exit(0): {fqn} ends Exited: {statuses:?}"
        );
    }
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
    let statuses = end_statuses_by_fqn(&header, &events);
    for fqn in ["user.main", "baml.sys.exit"] {
        assert_eq!(
            statuses.get(fqn),
            Some(&vec![pb::FunctionEndStatus::Exited as i32]),
            "exit(3): {fqn} ends Exited (frame fate; the code is thread-level): {statuses:?}"
        );
    }
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

/// Renders events as comparable strings: timestamps stripped, function ids
/// joined to fqns (ids are NOT stable across compiles — the fqn is the
/// cross-run key), the run's marker fqn unified to "<pin>", grouped per
/// thread and sorted by timestamp (file order interleaves rings, so only
/// the per-thread timestamp order is the stream's real shape).
fn normalized_per_thread_streams(
    header: &pb::EventFileHeaderV1,
    events: &[Event],
    marker_fqn: &str,
) -> Vec<Vec<String>> {
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
    let fqn = |id: u32| -> String {
        let name = fqn_by_id.get(&id).copied().unwrap_or("<unassigned>");
        if name == marker_fqn {
            "<pin>".to_string()
        } else {
            name.to_string()
        }
    };
    let ts_of = |e: &Event| match e {
        Event::CallFunction(cf) => cf.timestamp_ns,
        Event::EndFunction(ef) => ef.timestamp_ns,
        Event::StartThread(st) => st.timestamp_ns,
        Event::EndThread(et) => et.timestamp_ns,
        Event::SetFunctionId(sf) => sf.timestamp_ns,
        Event::Heartbeat(hb) => hb.timestamp_ns,
    };
    let tid_of = |e: &Event| match e {
        Event::CallFunction(cf) => cf.thread_id,
        Event::EndFunction(ef) => ef.thread_id,
        Event::StartThread(st) => st.thread_id,
        Event::EndThread(et) => et.thread_id,
        Event::SetFunctionId(sf) => sf.thread_id,
        Event::Heartbeat(_) => 0,
    };
    let mut per_thread: HashMap<u64, Vec<&Event>> = HashMap::new();
    for e in events {
        per_thread.entry(tid_of(e)).or_default().push(e);
    }
    let mut threads: Vec<(u64, Vec<&Event>)> = per_thread.into_iter().collect();
    threads.sort_by_key(|(tid, _)| *tid);
    threads
        .into_iter()
        .map(|(_, mut es)| {
            es.sort_by_key(|e| ts_of(e));
            es.into_iter()
                .map(|e| match e {
                    Event::CallFunction(cf) => format!(
                        "call {} id={} parent={:?}",
                        fqn(cf.function_id),
                        cf.call_id,
                        cf.parent_call_id
                    ),
                    Event::EndFunction(ef) => {
                        format!("end id={} status={}", ef.call_id, ef.status)
                    }
                    Event::StartThread(st) => format!(
                        "start-thread parent={:?} pcall={:?}",
                        st.parent_thread_id, st.parent_call_id
                    ),
                    Event::EndThread(et) => format!("end-thread status={}", et.status),
                    Event::SetFunctionId(sf) => format!("set-id id={}", sf.call_id),
                    Event::Heartbeat(_) => "heartbeat".to_string(),
                })
                .collect()
        })
        .collect()
}

/// T20 port (JSONL-era `early_yield_resume_produces_identical_disk_stream`):
/// suspending and resuming the VM mid-run (GC park via `EarlyYield`) must
/// not duplicate, drop, or reorder `.bamlprof` records — the per-thread
/// stream is identical to an uninterrupted run, modulo timestamps. The two
/// runs differ only in their marker function's *name* (same position, so
/// the same-walk function ids are identical), which is what demuxes their
/// per-engine profiles in the shared directory.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn early_yield_resume_produces_identical_stream() {
    const N: i64 = 20_000;
    let _guard = test_lock().await;
    init_prof_env();
    let source = |pin: &str| {
        format!(
            r#"
        function {pin}() -> int {{ 0 }}
        function leaf(i: int) -> int {{
            i
        }}

        function spin(n: int) -> int {{
            {pin}();
            let i = 0;
            while (i < n) {{
                let _ = [leaf(i), i + 1];
                i += 1;
            }}
            i
        }}
    "#
        )
    };

    // Run 1: uninterrupted.
    let plain = {
        let program = compile_for_engine(&source("ey_plain_pin"));
        let engine = Arc::new(
            BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
                .expect("engine construction"),
        );
        let value = engine
            .call_function(
                "spin",
                vec![bex_engine::BexExternalValue::Int(N)],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true,
            )
            .await
            .expect("plain run completes");
        assert_eq!(value, BexExternalValue::Int(N));
        let (header, events) = load_profile("user.ey_plain_pin");
        assert_balance(&header, &events);
        normalized_per_thread_streams(&header, &events, "user.ey_plain_pin")
    };

    // Run 2: same program (marker renamed in place — identical ids), with a
    // GC park mid-flight.
    let parked = {
        let program = compile_for_engine(&source("ey_parked_pin"));
        let engine = Arc::new(
            BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
                .expect("engine construction"),
        );
        let call_handle = {
            let engine = Arc::clone(&engine);
            tokio::spawn(async move {
                engine
                    .call_function(
                        "spin",
                        vec![bex_engine::BexExternalValue::Int(N)],
                        FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                        true,
                    )
                    .await
            })
        };
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        engine
            .collect_garbage(bex_heap::CollectionLevel::Minor)
            .await;
        let value = call_handle
            .await
            .expect("task joins")
            .expect("parked run completes");
        assert_eq!(value, BexExternalValue::Int(N));
        let (header, events) = load_profile("user.ey_parked_pin");
        assert_balance(&header, &events);
        normalized_per_thread_streams(&header, &events, "user.ey_parked_pin")
    };

    let plain_streams = plain;
    let parked_streams = parked;
    assert_eq!(
        plain_streams.len(),
        parked_streams.len(),
        "thread count differs"
    );
    for (tid, (a, b)) in plain_streams.iter().zip(&parked_streams).enumerate() {
        assert_eq!(a.len(), b.len(), "thread #{tid}: record count differs");
        for (i, (ea, eb)) in a.iter().zip(b).enumerate() {
            assert_eq!(
                ea,
                eb,
                "thread #{tid}: first divergence at record {i} of {} \
                 (plain vs parked)",
                a.len()
            );
        }
    }
}

/// T22 port (documented-policy test): dropping the `call_function` future at
/// an await point truncates the `.bamlprof` stream — `StartThread`/
/// `CallFunction` are written, no `End*` ever arrives (the torn artifact is
/// what the reader's truncation tolerance exists for). This is the current,
/// intentional contract: hosts that abandon a call must cancel via its token
/// (or `cancel_function_call`) and await completion if they need a closed
/// trace. If a root drop-guard is added later, this test must flip to assert
/// `Cancelled` end records instead.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropped_call_future_truncates_stream_by_policy() {
    let _guard = test_lock().await;
    init_prof_env();
    let source = r#"
        function dft_pin() -> int { 0 }
        function main() -> int {
            dft_pin();
            baml.sys.sleep(baml.time.Duration.from_milliseconds(400n));
            1
        }
    "#;
    let program = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine construction"),
    );
    {
        let engine = Arc::clone(&engine);
        let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
        let fut = engine.call_function("main", vec![], call_ctx, true);
        // Poll long enough for StartThread/CallFunction to land, then drop
        // the future mid-sleep.
        let _ = tokio::time::timeout(Duration::from_millis(100), fut).await;
    }
    // Wait past the program's natural completion: if the drop did NOT
    // truncate execution, the 400ms sleep would finish and End records
    // would land inside this window.
    tokio::time::sleep(Duration::from_millis(800)).await;

    let (_header, events) = load_profile("user.dft_pin");
    assert!(
        events.iter().any(|e| matches!(e, Event::CallFunction(_))),
        "the call started: {events:#?}"
    );
    assert!(
        !events.iter().any(|e| matches!(e, Event::EndThread(_))),
        "documented policy: a dropped root future truncates the stream (no \
         EndThread). If End records now appear, a root drop-guard was added — \
         flip this test to assert Cancelled statuses instead: {events:#?}"
    );
}
