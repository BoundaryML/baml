#![cfg(not(target_arch = "wasm32"))]

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use bex_engine::{
    BexEngine, BexExternalValue, FunctionCallContextBuilder, TelemetryRecording,
    logger::TraceLogger,
};
use btel_bcs::{CloudPublisherConfig, delivery::DeliveryConfig, proto::CloudUploadEnvelope};
use btel_reader::{
    cas::{CasLimits, CasStore},
    context::{ContextReference, reference},
};
use btel_recorder::{RecordingConfig, proto};
use btel_snapshot::{CasId, DecodedObject, DecodedRoot, DecodedSnapshot, DecodedValue};
use prost::Message as _;
use sys_native::SysOpsExt;
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

#[path = "../../btel_bcs/tests/support/mod.rs"]
mod cloud_protocol;

const SOURCE: &str = include_str!("../../baml_tests/baml_src/ns_trace_logs/logs.baml");

async fn execute(recording: TelemetryRecording, name: &str, live: bool) -> Arc<BexEngine> {
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            baml_test_support::compile_source(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            recording,
        )
        .unwrap(),
    );
    let logger = if live {
        TraceLogger::bounded(32)
    } else {
        TraceLogger::disabled()
    };
    let context = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_logger(logger.clone())
        .build();
    let result = tokio::time::timeout(
        Duration::from_secs(20),
        engine.call_function(
            "log_sequence",
            vec![BexExternalValue::String(name.into())],
            context,
            true,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result, BexExternalValue::Int(14));
    let output = logger.drain_rendered_logs();
    assert!(output.failures.is_empty(), "{:?}", output.failures);
    if live {
        assert_eq!(output.logs.len(), 14);
        assert_eq!(
            output.logs[0].metadata.event_name.as_deref(),
            Some("mutable")
        );
        assert_eq!(output.logs[0].body, "[1, 2]");
        assert_eq!(output.logs[2].metadata.event_name, None);
        assert_eq!(output.logs[4].metadata.event_name.as_deref(), Some(""));
    } else {
        assert!(output.logs.is_empty());
    }
    tokio::time::timeout(Duration::from_secs(20), engine.shutdown())
        .await
        .unwrap();
    engine
}

fn root(snapshot: &DecodedSnapshot) -> &DecodedValue {
    let DecodedRoot::Value(value) = &snapshot.root else {
        panic!("log snapshots are values")
    };
    value
}

fn object(snapshot: &DecodedSnapshot) -> &DecodedObject {
    let DecodedValue::Object(id) = root(snapshot) else {
        panic!("expected an object")
    };
    snapshot.object(*id)
}

fn fields(entries: &btel_snapshot::Entries) -> BTreeMap<&str, &DecodedValue> {
    entries
        .iter()
        .map(|(key, value)| (key.as_ref(), value))
        .collect()
}

fn assert_logs(
    files: &[proto::RecordingFile],
    name: &str,
    mut load: impl FnMut(CasId) -> DecodedSnapshot,
) {
    use proto::span_event::Event;
    let functions: BTreeMap<_, _> = files
        .iter()
        .flat_map(|file| file.definitions.iter().flat_map(|defs| &defs.functions))
        .filter_map(|definition| match &definition.resolution {
            Some(proto::function_definition::Resolution::Metadata(metadata)) => {
                Some((definition.function_id, metadata))
            }
            _ => None,
        })
        .collect();
    let spans: BTreeSet<_> = files
        .iter()
        .flat_map(|file| file.spans.iter().flat_map(|spans| &spans.sections))
        .flat_map(|section| &section.events)
        .filter_map(|event| match &event.event {
            Some(Event::FunctionAnnouncement(entry)) => Some(entry.id),
            _ => None,
        })
        .collect();
    let mut logs = Vec::new();
    for file in files {
        for section in file.spans.iter().flat_map(|spans| &spans.sections) {
            for event in &section.events {
                let Some(Event::Log(log)) = &event.event else {
                    continue;
                };
                assert!(
                    file.header.as_ref().unwrap().format_minor
                        >= btel_settings::encoding::LOG_FORMAT_MINOR
                );
                let ContextReference::Snapshot(context) = reference(section) else {
                    panic!("missing log context")
                };
                let context = load(context);
                let DecodedObject::Map { entries, .. } = object(&context) else {
                    panic!("context map")
                };
                let context_fields = fields(entries);
                assert_eq!(
                    context_fields["distinct_id"],
                    &DecodedValue::String("log-user".into())
                );
                let DecodedValue::Object(metadata) = context_fields["metadata"] else {
                    panic!("metadata map")
                };
                let DecodedObject::Map { entries, .. } = context.object(*metadata) else {
                    panic!("metadata map")
                };
                let phase = match log.event_name.as_deref() {
                    Some("mutable" | "structured") => "before",
                    Some("hidden") => "hidden",
                    _ => "after",
                };
                assert_eq!(
                    fields(entries)["phase"],
                    &DecodedValue::String(phase.into())
                );
                let function = functions[&log.function_id];
                assert!(
                    function
                        .source_map
                        .as_ref()
                        .is_some_and(|map| log.pc < map.code_bytes)
                );
                if log.event_name.as_deref() == Some("span") {
                    assert!(spans.contains(&log.parent_id));
                } else {
                    assert_eq!(
                        log.parent_id, section.thread_id,
                        "logging must not force a span"
                    );
                }
                if log.event_name.as_deref() == Some("hidden") {
                    assert!(function.fqn.ends_with(".hidden_log"));
                }
                logs.push((
                    log,
                    load(
                        log.data_cas_id
                            .expect("null data is captured, not absent")
                            .into(),
                    ),
                ));
            }
        }
    }
    assert_eq!(logs.len(), 14);
    let named = |name: &str| {
        &logs
            .iter()
            .find(|(log, _)| log.event_name.as_deref() == Some(name))
            .unwrap()
            .1
    };
    let DecodedObject::List { items, .. } = object(named("mutable")) else {
        panic!("list")
    };
    assert_eq!(items, &vec![DecodedValue::Int(1), DecodedValue::Int(2)]);
    let DecodedObject::Map { entries, .. } = object(named("structured")) else {
        panic!("map")
    };
    assert_eq!(
        fields(entries)["level"],
        &DecodedValue::String("user".into())
    );
    assert_eq!(fields(entries)["data"], &DecodedValue::Int(42));
    assert_eq!(
        fields(entries)["event_name"],
        &DecodedValue::String("payload".into())
    );
    let unnamed: Vec<_> = logs
        .iter()
        .filter(|(log, _)| log.event_name.is_none())
        .collect();
    assert_eq!(unnamed.len(), 2);
    assert!(
        unnamed
            .iter()
            .any(|(log, data)| log.level == proto::LogLevel::Debug as i32
                && root(data) == &DecodedValue::Null)
    );
    assert!(
        unnamed
            .iter()
            .any(|(log, _)| log.level == proto::LogLevel::Warn as i32)
    );
    let empty = logs
        .iter()
        .find(|(log, _)| log.event_name.as_deref() == Some(""))
        .unwrap();
    assert_eq!(empty.0.level, proto::LogLevel::Error as i32);
    assert_eq!(root(&empty.1), &DecodedValue::String("empty name".into()));
    let DecodedObject::Instance { fields: values, .. } = object(named("class")) else {
        panic!("class")
    };
    assert_eq!(fields(values)["value"], &DecodedValue::Int(7));
    assert!(
        matches!(root(named("enum")), DecodedValue::Enum { variant: 0, name, .. } if name.as_ref() == "First")
    );
    let DecodedObject::Map { entries, .. } = object(named("cycle")) else {
        panic!("cycle map")
    };
    assert_eq!(fields(entries)["self"], root(named("cycle")));
    assert_eq!(root(named("ordered")), &DecodedValue::Int(99));
    assert_eq!(root(named(name)), &DecodedValue::String("long".into()));
    assert_eq!(root(named("last")), &DecodedValue::Int(7));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn log_data_and_event_time_context_reach_local_cas() {
    let directory = tempfile::tempdir().unwrap();
    let name = "long-event-".repeat(8192);
    let engine = execute(
        TelemetryRecording::local_files_in(directory.path(), RecordingConfig::default()),
        &name,
        false,
    )
    .await;
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
    let read = btel_file::read_directory(engine.telemetry_recording_directory().unwrap()).unwrap();
    assert!(read.issues.is_empty(), "{:?}", read.issues);
    let cas = CasStore::new(directory.path().join("cas"), CasLimits::default());
    assert_logs(&read.files, &name, |id| {
        let Ok(snapshot) = cas.load(id).snapshot else {
            panic!("log CAS was not persisted")
        };
        snapshot.as_ref().clone()
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn log_data_and_context_use_existing_cloud_uploads() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(wiremock::matchers::path_regex(r"/heartbeat$"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let base = server.uri();
    Mock::given(method("POST"))
        .and(wiremock::matchers::path_regex(r"/uploads:prepare$"))
        .respond_with(move |request: &Request| {
            ResponseTemplate::new(200).set_body_json(cloud_protocol::response(request, &base, &[]))
        })
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let name = "cloud-event-".repeat(2048);
    let engine = execute(
        TelemetryRecording::cloud(
            RecordingConfig::default(),
            CloudPublisherConfig {
                candidate_target: 1,
                max_pending_snapshots: 1,
                inline_target_bytes: 1024 * 1024,
                ..CloudPublisherConfig::default()
            },
            DeliveryConfig {
                max_pending_snapshots: 1,
                max_candidates: 1,
                max_targets: 2,
                max_pending_plans: 1,
                ..DeliveryConfig::new(server.uri().parse().unwrap())
            },
        ),
        &name,
        true,
    )
    .await;
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
    let mut files = Vec::new();
    let mut snapshots = BTreeMap::new();
    for request in server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.method == "PUT")
    {
        let envelope = CloudUploadEnvelope::decode(request.body.as_slice()).unwrap();
        if let Some(bytes) = envelope.recording_file {
            files.push(proto::RecordingFile::decode(bytes.as_slice()).unwrap());
        }
        for object in envelope.cas_objects {
            let snapshot =
                btel_snapshot::decode_blob(&object.blob, &btel_snapshot::DecodeLimits::default())
                    .unwrap();
            assert_eq!(snapshot.id.as_bytes().as_slice(), object.snapshot_id);
            snapshots.insert(*snapshot.id.as_bytes(), snapshot);
        }
    }
    assert_logs(&files, &name, |id| snapshots[id.as_bytes()].clone());
}

#[test]
fn off_keeps_live_logs_without_persisting_files() {
    const CHILD: &str = "BAML_TEST_LOG_RECORDING_OFF";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "off_keeps_live_logs_without_persisting_files",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("BAML_TELEMETRY", "off")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let engine = execute(
            TelemetryRecording::local_files_in(directory.path(), RecordingConfig::default()),
            "off",
            true,
        )
        .await;
        assert!(engine.telemetry_recording_directory().is_none());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    });
}
