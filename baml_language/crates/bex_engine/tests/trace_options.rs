#![cfg(not(target_arch = "wasm32"))]

use std::{collections::BTreeSet, sync::Arc, time::Duration};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, TelemetryRecording};
use btel_bcs::{delivery::DeliveryConfig, proto::CloudUploadEnvelope};
use btel_publisher::{RecordingConfig, proto};
use prost::Message as _;
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

#[path = "../../btel_bcs/tests/support/mod.rs"]
mod cloud_protocol;
#[path = "support/telemetry.rs"]
mod telemetry;

const SOURCE: &str =
    include_str!("../../baml_tests/baml_src/ns_trace_invocations/trace_invocations.baml");

async fn execute(engine: &Arc<BexEngine>) {
    let context = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    assert_eq!(
        engine
            .call_function("capture_main", vec![], context, true)
            .await
            .unwrap(),
        BexExternalValue::Int(85)
    );
}

fn snapshot_bytes(id: proto::SnapshotId) -> [u8; 16] {
    let mut bytes = [0; 16];
    bytes[..8].copy_from_slice(&id.low.to_le_bytes());
    bytes[8..].copy_from_slice(&id.high.to_le_bytes());
    bytes
}

fn assert_captures(files: &[proto::RecordingFile]) -> BTreeSet<[u8; 16]> {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut started = BTreeSet::new();
    let mut finished = BTreeSet::new();
    for file in files {
        for section in file.spans.iter().flat_map(|spans| &spans.sections) {
            for event in &section.events {
                match event.event.as_ref().unwrap() {
                    proto::span_event::Event::FunctionAnnouncement(entry) => {
                        started.insert(entry.id);
                        if let Some(id) = entry.inputs_cas_id {
                            inputs.push(snapshot_bytes(id));
                        }
                    }
                    proto::span_event::Event::FunctionCompletion(exit) => {
                        finished.insert(exit.id);
                        if let Some(id) = exit.value_cas_id {
                            outputs.push(snapshot_bytes(id));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    assert_eq!(
        started.len(),
        2,
        "both explicitly requested calls must have spans"
    );
    assert_eq!(started, finished);
    assert_eq!(inputs.len(), 1, "only the first call requests inputs");
    assert_eq!(outputs.len(), 1, "only the first call requests output");
    inputs.into_iter().chain(outputs).collect()
}

#[tokio::test]
async fn explicit_captures_reach_local_recordings() {
    let directory = tempfile::tempdir().unwrap();
    let engine = telemetry::recording_engine(
        SOURCE,
        &[],
        TelemetryRecording::local_files_in(directory.path(), RecordingConfig::default()),
    );
    let recording_directory = engine.telemetry_recording_directory().unwrap().to_owned();
    execute(&engine).await;
    engine.shutdown().await;
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
    let recording = btel_file::read_directory(&recording_directory).unwrap();
    assert!(recording.issues.is_empty(), "{:?}", recording.issues);
    for id in assert_captures(&recording.files) {
        let id = btel_snapshot::SnapshotId::from_bytes(id);
        assert!(btel_file::cas_path(&directory.path().join("cas"), id).is_file());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_captures_reach_cloud_recordings() {
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            ResponseTemplate::new(200).set_body_json(cloud_protocol::response(request, &base, &[]))
        })
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let engine = telemetry::recording_engine(
        SOURCE,
        &[],
        TelemetryRecording::cloud(
            RecordingConfig::default(),
            btel_bcs::PublisherConfig::default(),
            DeliveryConfig {
                prepare_base_url: server.uri(),
                allow_http: true,
                max_attempts: 1,
                request_timeout: Duration::from_secs(2),
                ..DeliveryConfig::default()
            },
        ),
    );
    execute(&engine).await;
    engine.shutdown().await;
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
    let mut files = Vec::new();
    let mut uploaded = BTreeSet::new();
    for request in server.received_requests().await.unwrap() {
        if request.method != "PUT" {
            continue;
        }
        let envelope = CloudUploadEnvelope::decode(request.body.as_slice()).unwrap();
        if let Some(bytes) = envelope.recording_file {
            files.push(proto::RecordingFile::decode(bytes.as_slice()).unwrap());
        }
        for blob in envelope.cas_objects {
            uploaded.insert(<[u8; 16]>::try_from(blob.snapshot_id.as_slice()).unwrap());
        }
    }
    assert!(assert_captures(&files).is_subset(&uploaded));
}
