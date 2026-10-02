//! Real host producer → BAML child → persisted recording, without trace hooks.
#![cfg(not(target_arch = "wasm32"))]
mod common;

use std::{collections::HashMap, sync::Arc, time::Duration};

use bex_engine::{
    BexEngine, BexExternalValue, CallId, FunctionCallContextBuilder, HostDefinition,
    TelemetryRecording,
};
use btel_recorder::{RecordingConfig, proto};
use btel_types::{
    InvocationMode, InvocationOutcome,
    context::{ContextPatch, ContextValue},
};
use sys_native::SysOpsExt;

fn definition() -> HostDefinition {
    HostDefinition {
        language: "python".into(),
        module: "application".into(),
        qualified_name: "request".into(),
        source_file: "application.py".into(),
        definition_line: 10,
        wrapper_line: 9,
        display_name: "request".into(),
    }
}

#[tokio::test]
async fn host_context_and_recording_owner_have_independent_lifetimes() {
    let program = common::compile_for_engine(
        r#"
        function child() -> int {
            assert.equal(trace.current_context().metadata["request"], 7);
            7
        }
    "#,
    );
    let directory = tempfile::tempdir().unwrap();
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            program,
            Arc::new(sys_ops::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            TelemetryRecording::local_files_in(directory.path(), RecordingConfig::default()),
        )
        .unwrap(),
    );
    let host = engine
        .begin_host_invocation(
            &definition(),
            None,
            &bex_vm_types::trace::TraceOptionsData {
                context: Some(
                    ContextPatch {
                        metadata: [("request".into(), Some(ContextValue::Int(7)))].into(),
                        ..Default::default()
                    }
                    .into(),
                ),
                inputs: Some(true),
                output: Some(true),
                ..Default::default()
            },
            &bex_engine::HostCallSite {
                source_file: "caller.py".into(),
                line: 20,
            },
            Some(&bex_engine::HostCapture::Map(vec![(
                "value".into(),
                bex_engine::HostCapture::Int(7),
            )])),
        )
        .unwrap();
    let retained = host.inherited_state();
    let metadata = engine.program_metadata().await;
    let registered = metadata
        .function_table
        .functions
        .iter()
        .find(|function| function.display_name == "request")
        .unwrap();
    assert_eq!(
        engine.function_metadata(registered.function_id).await,
        Some(registered.clone())
    );
    host.finish_with_value(
        InvocationOutcome::Ok,
        Some(&bex_engine::HostCapture::Int(7)),
    );
    // A later child inherits a closed host parent, without keeping its work
    // registration, producer, or completion owner alive.
    assert_eq!(
        engine
            .call_function(
                "child",
                vec![],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_inherited_state(retained.clone())
                    .with_trace_options(bex_vm_types::trace::TraceOptionsData {
                        mode: Some(InvocationMode::Span),
                        ..Default::default()
                    })
                    .build(),
                true
            )
            .await
            .unwrap(),
        BexExternalValue::Int(7)
    );
    tokio::time::timeout(Duration::from_secs(5), engine.shutdown())
        .await
        .expect("retained host state must not block shutdown");
    assert_eq!(
        retained.trace_context().metadata()["request"],
        ContextValue::Int(7)
    );
    if let Some(path) = engine.telemetry_recording_directory() {
        assert_eq!(engine.telemetry_result(), Some(Ok(())));
        let mut functions = HashMap::new();
        let mut paths = HashMap::new();
        let mut threads = HashMap::new();
        let mut entries = vec![];
        let mut completions = vec![];
        for file in btel_file::read_directory(path).unwrap().files {
            let definitions = file.definitions.unwrap();
            for function in definitions.functions {
                if let Some(proto::function_definition::Resolution::Metadata(metadata)) =
                    function.resolution
                {
                    functions.insert(function.function_id, metadata.display_name);
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
        assert_eq!(entries.len(), 2);
        assert_eq!(completions.len(), 2);
        let (_, host) = entries
            .iter()
            .find(|(_, entry)| functions[&paths[&entry.call_path_id]] == "request")
            .expect("host definition metadata must reach the real recording");
        let (thread, child) = entries
            .iter()
            .find(|(_, entry)| functions[&paths[&entry.call_path_id]] == "child")
            .unwrap();
        assert_eq!(threads[thread], Some(host.id));
        assert!(host.inputs_cas_id.is_some());
        let parent_end = completions.iter().find(|done| done.id == host.id).unwrap();
        assert!(parent_end.value_cas_id.is_some());
        assert!(parent_end.exited_at_ticks <= child.entered_at_ticks);
    }
}

#[tokio::test]
async fn host_shutdown_waits_for_actual_exit() {
    let program = common::compile_for_engine("function child() -> int { 7 }");
    let engine =
        Arc::new(BexEngine::new(program, Arc::new(sys_ops::SysOps::native()), vec![]).unwrap());
    let host = engine
        .begin_host_invocation(
            &definition(),
            None,
            &bex_vm_types::trace::TraceOptionsData::default(),
            &bex_engine::HostCallSite {
                source_file: "caller.py".into(),
                line: 20,
            },
            None,
        )
        .unwrap();
    let retained = host.inherited_state();
    let runtime = Arc::clone(&engine);
    let mut shutdown = tokio::spawn(async move {
        runtime.shutdown().await;
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut shutdown)
            .await
            .is_err()
    );
    host.finish(InvocationOutcome::Cancelled);
    tokio::time::timeout(Duration::from_secs(5), shutdown)
        .await
        .unwrap()
        .unwrap();
    assert!(retained.trace_context().metadata().is_empty());
    assert!(retained.is_cancelled());
}

#[tokio::test]
async fn host_definitions_and_call_sites_are_structural_across_modes() {
    let program = common::compile_for_engine("function child() -> int { 7 }");
    let directory = tempfile::tempdir().unwrap();
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            program,
            Arc::new(sys_ops::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            TelemetryRecording::local_files_in(directory.path(), RecordingConfig::default()),
        )
        .unwrap(),
    );
    for (file, caller, line, display, mode) in [
        (
            "application.py",
            "caller.py",
            20,
            "request",
            InvocationMode::Span,
        ),
        (
            "application.py",
            "caller.py",
            20,
            "renamed",
            InvocationMode::Span,
        ),
        (
            "application.py",
            "other_caller.py",
            20,
            "request",
            InvocationMode::Span,
        ),
        (
            "other_application.py",
            "caller.py",
            20,
            "request",
            InvocationMode::Span,
        ),
        (
            "application.py",
            "caller.py",
            20,
            "request",
            InvocationMode::Timing,
        ),
        (
            "application.py",
            "caller.py",
            20,
            "request",
            InvocationMode::Hidden,
        ),
        (
            "application.py",
            "<native>",
            0,
            "request",
            InvocationMode::Span,
        ),
    ] {
        engine
            .begin_host_invocation(
                &HostDefinition {
                    source_file: file.into(),
                    display_name: display.into(),
                    ..definition()
                },
                None,
                &bex_vm_types::trace::TraceOptionsData {
                    mode: Some(mode),
                    ..Default::default()
                },
                &bex_engine::HostCallSite {
                    source_file: caller.into(),
                    line,
                },
                None,
            )
            .unwrap()
            .finish(InvocationOutcome::Ok);
    }
    let metadata = engine.program_metadata().await;
    let functions: Vec<_> = metadata
        .function_table
        .functions
        .iter()
        .filter(|function| function.fqn.starts_with("python:"))
        .collect();
    assert_eq!(
        functions.len(),
        2,
        "display labels and invocation modes must not create definitions"
    );
    assert_ne!(functions[0].function_id, functions[1].function_id);
    assert_ne!(functions[0].definition_key, functions[1].definition_key);
    tokio::time::timeout(Duration::from_secs(5), engine.shutdown())
        .await
        .unwrap();
    if let Some(path) = engine.telemetry_recording_directory() {
        assert_eq!(engine.telemetry_result(), Some(Ok(())));
        let mut paths = vec![];
        let mut spans = 0;
        for file in btel_file::read_directory(path).unwrap().files {
            paths.extend(file.definitions.unwrap().call_paths);
            for section in file.spans.unwrap().sections {
                spans += section
                    .events
                    .into_iter()
                    .filter(|event| {
                        matches!(
                            event.event,
                            Some(proto::span_event::Event::FunctionAnnouncement(_))
                        )
                    })
                    .count();
            }
        }
        assert_eq!(spans, 5, "Timing and Hidden must not emit function spans");
        assert_eq!(paths.len(), 6, "Hidden must not emit a host call path");
        let repeated: Vec<_> = paths
            .iter()
            .filter(|path| path.callee_function_id == functions[0].function_id.get())
            .collect();
        assert_eq!(repeated.len(), 5);
        assert_eq!(
            repeated.iter().filter(|path| path.caller_pc == 0).count(),
            1
        );
        let sites: std::collections::HashSet<_> =
            repeated.iter().map(|path| path.caller_pc).collect();
        assert_eq!(
            sites.len(),
            3,
            "same line in different caller files must be distinct"
        );
    }
}
