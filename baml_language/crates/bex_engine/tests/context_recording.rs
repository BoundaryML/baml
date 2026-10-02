#![cfg(not(target_arch = "wasm32"))]

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, TelemetryRecording};
use btel_reader::{
    cas::{CasLimits, CasOutcome, CasStore},
    context::{ContextReference, reference},
};
use btel_recorder::{RecordingConfig, proto};
use sys_native::SysOpsExt;

#[path = "support/trace_context.rs"]
mod trace_context;

fn context() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn baml_context_reaches_local_files_and_cas_at_entry_and_completion() {
    let program = baml_db::testing::compile_source(trace_context::SOURCE);
    let root = tempfile::tempdir().unwrap();
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            program,
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            TelemetryRecording::local_files_in(root.path(), RecordingConfig::default()),
        )
        .unwrap(),
    );
    assert_eq!(
        engine
            .call_function(
                "context_record",
                vec![BexExternalValue::Bool(false)],
                context(),
                true
            )
            .await
            .unwrap(),
        BexExternalValue::Int(14)
    );
    assert!(matches!(
        engine
            .call_function("context_failure", vec![], context(), true)
            .await,
        Err(bex_engine::EngineError::UnhandledThrow { .. })
    ));
    assert_eq!(
        engine
            .call_function("context_many", vec![], context(), true)
            .await
            .unwrap(),
        BexExternalValue::Int(28672)
    );
    tokio::time::timeout(Duration::from_secs(20), engine.shutdown())
        .await
        .unwrap();
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
    let files = btel_file::read_directory(engine.telemetry_recording_directory().unwrap()).unwrap();
    assert!(files.issues.is_empty(), "{:?}", files.issues);
    let cas = CasStore::new(root.path().join("cas"), CasLimits::default());
    let mut snapshots = BTreeMap::new();
    for section in files
        .files
        .iter()
        .flat_map(|file| file.spans.iter().flat_map(|spans| &spans.sections))
    {
        if let ContextReference::Snapshot(id) = reference(section) {
            let CasOutcome::Available(snapshot) = cas.load(id).outcome else {
                panic!("context CAS reference must resolve after shutdown");
            };
            snapshots.insert(*id.as_bytes(), snapshot.as_ref().clone());
        }
        for event in &section.events {
            match &event.event {
                Some(proto::span_event::Event::FunctionAnnouncement(entry)) => {
                    assert!(entry.inputs_cas_id.is_none());
                }
                Some(proto::span_event::Event::FunctionCompletion(done)) => {
                    assert!(done.value_cas_id.is_none());
                }
                _ => {}
            }
        }
    }
    trace_context::assert_leaf_contexts(&files.files, &snapshots, 4099);
}
