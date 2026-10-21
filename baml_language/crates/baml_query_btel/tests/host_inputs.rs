//! Actual host and BAML producers persist different input snapshot roots.
use std::sync::Arc;

use baml_query_btel::{Index, IndexOptions, QueryRequest, Status};
use bex_engine::{
    BexEngine, BexExternalValue, CallId, EngineConfig, FunctionCallContextBuilder, HostCallSite,
    HostCapture, HostDefinition, TelemetryRecording,
};
use bex_vm_types::trace::TraceOptionsData;
use btel_types::{InvocationMode, InvocationOutcome};
use serde_json::json;
use sys_native::SysOpsExt;

#[tokio::test]
async fn host_value_inputs_and_baml_argument_slots_are_queryable_in_both_views() {
    let project = tempfile::tempdir().unwrap();
    let engine = Arc::new(
        BexEngine::new_with_config(
            baml_test_support::compile_source("function echo(x: int) -> int { x }"),
            Arc::new(sys_native::SysOps::native()),
            vec![],
            EngineConfig {
                recording: Some(TelemetryRecording::local_files(
                    project.path(),
                    btel_recorder::RecordingConfig::default(),
                )),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let options = TraceOptionsData {
        mode: Some(InvocationMode::Span),
        inputs: Some(true),
        output: Some(true),
        ..Default::default()
    };
    for (language, inputs) in [
        (
            "python",
            HostCapture::Map(vec![("value".into(), HostCapture::Int(7))]),
        ),
        ("typescript", HostCapture::List(vec![HostCapture::Int(7)])),
    ] {
        let host = engine
            .begin_host_invocation(
                &HostDefinition {
                    language: language.into(),
                    module: "application".into(),
                    qualified_name: "request".into(),
                    source_file: "application".into(),
                    definition_line: 10,
                    wrapper_line: 9,
                    display_name: "custom request".into(),
                },
                None,
                &options,
                &HostCallSite {
                    source_file: "caller".into(),
                    line: 20,
                },
                Some(&inputs),
            )
            .unwrap();
        host.finish_with_value(InvocationOutcome::Ok, Some(&HostCapture::Int(8)));
    }
    assert_eq!(
        engine
            .call_function(
                "echo",
                vec![BexExternalValue::Int(7)],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_trace_options(options)
                    .build(),
                true,
            )
            .await
            .unwrap(),
        BexExternalValue::Int(7),
    );
    engine.record_process_exit(bex_engine::ProcessStatus::Success);
    engine.shutdown().await;
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    for relation in ["spans", "span_announcements"] {
        let result = index
            .refresh_and_query(&QueryRequest {
                sql: format!(
                    "SELECT input_args, input_args['value'], input_args[0],
                baml_value_state(input_args), input_args['x'] FROM {relation}
                WHERE span_type = 'function' ORDER BY span_name"
                ),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            result.outcome.status,
            Status::Complete,
            "{:?}",
            result.outcome.diagnostics
        );
        assert_eq!(
            result.rows,
            vec![
                vec![
                    json!({"value": 7}),
                    json!(7),
                    json!(null),
                    json!("present"),
                    json!(null)
                ],
                vec![
                    json!([7]),
                    json!(null),
                    json!(7),
                    json!("present"),
                    json!(null)
                ],
                vec![
                    json!({"x": 7}),
                    json!(null),
                    json!(7),
                    json!("present"),
                    json!(7)
                ],
            ]
        );
    }
}
