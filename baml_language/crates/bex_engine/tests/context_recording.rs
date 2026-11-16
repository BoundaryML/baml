#![cfg(not(target_arch = "wasm32"))]

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, TelemetryRecording};
use btel_reader::{
    cas::{CasLimits, CasStore},
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
async fn differently_versioned_programs_share_process_context_but_keep_their_own_versions() {
    let program = baml_test_support::compile_source(
        r#"
function version() -> string {
    match (trace.current_context().metadata.get("baml.ctx.app_version")) {
        let value: string => value,
        _ => "missing",
    }
}
function main() -> string {
    version($trace = trace.span(inputs = false, output = false, error = false))
}
"#,
    );
    let root = tempfile::tempdir().unwrap();
    let engines: Vec<_> = ["1.2.3", "4.5.6"]
        .into_iter()
        .map(|version| {
            let mut program = program.clone();
            program.app_version = Some(version.to_owned());
            let directory = root.path().join(version);
            let engine = Arc::new(
                BexEngine::new_with_telemetry_recording(
                    program,
                    Arc::new(sys_native::SysOps::native()),
                    vec![],
                    None,
                    btel_clock::ClockMode::Monotonic,
                    TelemetryRecording::local_files_in(&directory, RecordingConfig::default()),
                )
                .unwrap(),
            );
            (version, directory, engine)
        })
        .collect();
    let mut shared_process = None;
    for (version, directory, engine) in engines {
        assert_eq!(
            engine
                .call_function("main", vec![], context(), true)
                .await
                .unwrap(),
            BexExternalValue::String(version.into())
        );
        engine.shutdown().await;
        assert_eq!(engine.telemetry_result(), Some(Ok(())));
        let files =
            btel_file::read_directory(engine.telemetry_recording_directory().unwrap()).unwrap();
        assert!(files.issues.is_empty());
        let cas = CasStore::new(directory.join("cas"), CasLimits::default());
        let mut saw_program_context = false;
        for file in files.files {
            let header = file.header.unwrap();
            let id: btel_snapshot::CasId = header.initial_context_cas_id.unwrap().into();
            let identity = (header.process_id.unwrap(), *id.as_bytes());
            if let Some(expected) = &shared_process {
                assert_eq!(&identity, expected);
            } else {
                shared_process = Some(identity);
            }
            let snapshot = cas.load(id).snapshot.unwrap();
            assert!(
                !trace_context::metadata(&snapshot)
                    .1
                    .contains_key("baml.ctx.app_version")
            );
            for section in file.spans.unwrap_or_default().sections {
                if !section.events.iter().any(|event| {
                    matches!(
                        event.event,
                        Some(
                            proto::span_event::Event::FunctionAnnouncement(_)
                                | proto::span_event::Event::FunctionCompletion(_)
                        )
                    )
                }) {
                    continue;
                }
                let ContextReference::Snapshot(id) = reference(&section) else {
                    panic!("recorded functions must retain their program context");
                };
                let snapshot = cas.load(id).snapshot.unwrap();
                let (_, metadata) = trace_context::metadata(&snapshot);
                assert_eq!(
                    metadata["baml.ctx.app_version"],
                    &btel_snapshot::DecodedValue::String(version.into())
                );
                saw_program_context = true;
            }
        }
        assert!(
            saw_program_context,
            "the recorded invocation retains its program version"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn baml_context_reaches_local_files_and_cas_at_entry_and_completion() {
    let mut program = baml_test_support::compile_source(trace_context::SOURCE);
    program.app_version = Some("5.6.7+local".into());
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
            let Ok(snapshot) = cas.load(id).snapshot else {
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
    for file in &files.files {
        let id: btel_snapshot::CasId = file
            .header
            .as_ref()
            .unwrap()
            .initial_context_cas_id
            .unwrap()
            .into();
        let snapshot = cas.load(id).snapshot.unwrap();
        let (_, metadata) = trace_context::metadata(&snapshot);
        assert_eq!(
            metadata["baml.ctx.baml_version"],
            &btel_snapshot::DecodedValue::String(baml_version::CANONICAL_VERSION.into())
        );
        assert!(
            !metadata.contains_key("baml.ctx.app_version"),
            "program version is not process-shared metadata"
        );
    }
    trace_context::assert_leaf_contexts(&files.files, &snapshots, 4099, Some("5.6.7+local"));
}
