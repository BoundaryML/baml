//! Real VM → V2 callback frame → retained handle → new VM → recording files.
//! No trace hooks or host instrumentation participate in these tests.
#![cfg(not(target_arch = "wasm32"))]
#![allow(unsafe_code)]

mod common;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use bex_engine::{
    BexEngine, BexExternalValue, CallId, CancellationToken, EngineError,
    FunctionCallContextBuilder, InheritedInvocationState, TelemetryRecording,
};
use bex_resource_types::{HostValueArc, HostValueKind};
use bridge_ctypes::{
    CffiHandleTableEntry, HANDLE_TABLE, OwnedHostInvocation, baml_bridge::cffi::HostInvocation,
};
use btel_recorder::{RecordingConfig, proto};
use btel_types::{InvocationMode, context::ContextValue};
use prost::Message;
use sys_native::SysOpsExt;

static CAPTURED: OnceLock<Mutex<HashMap<u64, OwnedHostInvocation>>> = OnceLock::new();

extern "C" fn dispatch(request: *const u8, length: usize) {
    // SAFETY: bytes are borrowed for the native trampoline; decode before returning.
    let bytes = unsafe { std::slice::from_raw_parts(request, length) };
    let frame = OwnedHostInvocation(HostInvocation::decode(bytes).unwrap());
    let callback_id = frame.0.callback_id;
    CAPTURED
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap()
        .insert(frame.0.host_value_key, frame);
    // The body has actually returned. Retaining its context must not retain execution.
    sys_native::host_dispatch::complete_with_value(callback_id, BexExternalValue::Int(7));
}

fn captured(key: u64) -> (OwnedHostInvocation, InheritedInvocationState) {
    let frame = CAPTURED
        .get()
        .unwrap()
        .lock()
        .unwrap()
        .remove(&key)
        .unwrap();
    let entry = HANDLE_TABLE.resolve(frame.0.effective_state).unwrap();
    let CffiHandleTableEntry::InvocationState(state) = &*entry else {
        panic!("effective_state must be a typed invocation-state handle")
    };
    (frame, state.state.clone())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retained_callback_context_and_ancestry_survive_parent_completion() {
    sys_native::host_dispatch::set_dispatch_v2(dispatch);
    let program = common::compile_for_engine(
        r#"
        function parent(callback: (int) -> int) -> int {
            inner(callback, $trace = trace.hidden().context(
                distinct_id = "inner-id", metadata = { "phase": "inner" }
            ))
        }
        function inner(callback: (int) -> int) -> int { callback(1) }
        function child() -> int {
            assert.equal(trace.current_context().distinct_id, "inner-id");
            assert.equal(trace.current_context().metadata["phase"], "inner");
            7
        }
    "#,
    );
    let directory = tempfile::tempdir().unwrap();
    let recording =
        TelemetryRecording::local_files_in(directory.path(), RecordingConfig::default());
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            program,
            Arc::new(sys_ops::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            recording,
        )
        .unwrap(),
    );
    let input = CancellationToken::new();
    let id = CallId::next();
    let result = engine
        .call_function(
            "parent",
            vec![BexExternalValue::HostValue(HostValueArc::new(
                1,
                HostValueKind::Callable,
            ))],
            FunctionCallContextBuilder::new(id)
                .with_cancel_token(input.clone())
                .with_host_environment(41)
                .with_trace_options(bex_vm_types::trace::TraceOptionsData {
                    mode: Some(InvocationMode::Span),
                    inputs: Some(true),
                    output: Some(true),
                    ..Default::default()
                })
                .build(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(result, BexExternalValue::Int(7));
    assert!(engine.invocation_state(id).is_err());
    let (frame, state) = captured(1);
    assert_eq!(frame.0.host_environment, 41);
    assert_eq!(state.trace_context().distinct_id(), Some("inner-id"));
    assert_eq!(
        state.trace_context().metadata()["phase"],
        ContextValue::String("inner".into())
    );
    assert!(!state.is_cancelled());
    // New execution uses the retained environment, not an active parent lookup.
    assert_eq!(
        engine
            .call_function(
                "child",
                vec![],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_inherited_state(state.clone())
                    .with_trace_options(bex_vm_types::trace::TraceOptionsData {
                        mode: Some(InvocationMode::Span),
                        inputs: Some(false),
                        output: Some(false),
                        ..Default::default()
                    })
                    .build(),
                true
            )
            .await
            .unwrap(),
        BexExternalValue::Int(7)
    );
    input.cancel();
    assert!(state.is_cancelled());
    tokio::time::timeout(Duration::from_secs(5), engine.shutdown())
        .await
        .expect("retained handles must not hold active execution or recording open");

    // Inspect actual persisted records, including the child's inherited parent.
    if let Some(path) = engine.telemetry_recording_directory() {
        assert_eq!(engine.telemetry_result(), Some(Ok(())));
        let mut entries = Vec::new();
        let mut completions = Vec::new();
        let mut functions = HashMap::new();
        let mut paths = HashMap::new();
        let mut threads = HashMap::new();
        for file in btel_file::read_directory(path).unwrap().files {
            let definitions = file.definitions.unwrap();
            for function in definitions.functions {
                if let Some(proto::function_definition::Resolution::Metadata(metadata)) =
                    function.resolution
                {
                    functions.insert(function.function_id, metadata.fqn);
                }
            }
            for path in definitions.call_paths {
                paths.insert(path.call_path_id, path.callee_function_id);
            }
            for thread in definitions.threads {
                threads.insert(thread.thread_id, thread.parent_id);
            }
            for section in file.spans.unwrap().sections {
                for event in section.events {
                    match event.event.unwrap() {
                        proto::span_event::Event::FunctionAnnouncement(entry) => {
                            entries.push((section.thread_id, entry));
                        }
                        proto::span_event::Event::FunctionCompletion(done) => {
                            completions.push(done);
                        }
                        _ => {}
                    }
                }
            }
        }
        assert_eq!(
            entries.len(),
            2,
            "only the two explicitly selected BAML spans"
        );
        assert_eq!(completions.len(), entries.len());
        let find = |name: &str| {
            entries
                .iter()
                .find(|(_, entry)| functions[&paths[&entry.call_path_id]].ends_with(name))
                .unwrap()
        };
        let (_, parent) = find("parent");
        let (thread, child) = find("child");
        assert_eq!(threads[thread], Some(parent.id));
        assert!(parent.inputs_cas_id.is_some());
        assert!(child.inputs_cas_id.is_none());
        let completion = |id| completions.iter().find(|done| done.id == id).unwrap();
        assert!(completion(parent.id).value_cas_id.is_some());
        assert!(completion(child.id).value_cas_id.is_none());
    }
    // Reading a retained immutable frame still works after shutdown; invoking
    // its closed runtime is a separate operation and must fail normally.
    assert_eq!(state.trace_context().distinct_id(), Some("inner-id"));
    assert!(matches!(
        tokio::time::timeout(
            Duration::from_secs(5),
            engine.call_function(
                "child",
                vec![],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_inherited_state(state)
                    .build(),
                true,
            ),
        )
        .await
        .expect("a retained handle must not let execution enter a closed runtime"),
        Err(EngineError::ShuttingDown)
    ));
    drop(frame);
}
