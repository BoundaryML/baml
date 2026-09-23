#![cfg(not(target_arch = "wasm32"))]

use std::{
    collections::BTreeSet,
    fmt::Write as _,
    sync::{Arc, Mutex},
    time::Duration,
};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, TelemetryRecording};
use btel_bcs::{delivery::DeliveryConfig, proto::CloudUploadEnvelope};
use btel_publisher::{RecordingConfig, proto};
use prost::Message as _;
use serde_json::{Value, json};
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

#[path = "../../btel_bcs/tests/support/mod.rs"]
mod cloud_protocol;
#[path = "../../btel_bcs/tests/support/prepare_requests.rs"]
mod prepare_requests;
#[path = "support/telemetry.rs"]
mod telemetry;

const SOURCE: &str =
    include_str!("../../baml_tests/baml_src/ns_cloud_telemetry/cloud_telemetry.baml");

fn engine(server: &MockServer, flush_interval_duration: Duration) -> Arc<BexEngine> {
    telemetry::recording_engine(
        SOURCE,
        &["leaf"],
        TelemetryRecording::cloud(
            RecordingConfig {
                flush_interval_duration,
                ..RecordingConfig::default()
            },
            btel_bcs::PublisherConfig {
                inline_target_bytes: 1024 * 1024,
                ..btel_bcs::PublisherConfig::default()
            },
            DeliveryConfig {
                prepare_base_url: server.uri(),
                bearer_token: Some("prepare-only-token".into()),
                allow_http: true,
                max_attempts: 1,
                request_timeout: Duration::from_secs(2),
                retry_delay: Duration::ZERO,
                ..DeliveryConfig::default()
            },
        ),
    )
}

async fn execute(engine: &Arc<BexEngine>) {
    let context = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    assert_eq!(
        engine
            .call_function("main", vec![], context, true)
            .await
            .unwrap(),
        BexExternalValue::Int(14)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capture_pressure_cannot_deadlock_vm_or_fail_execution() {
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            ResponseTemplate::new(200).set_body_json(cloud_protocol::response(request, &base, &[]))
        })
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(2)))
        .mount(&server)
        .await;
    let engine = telemetry::recording_engine(
        include_str!(
            "../../baml_tests/baml_src/ns_telemetry_performance/telemetry_performance.baml"
        ),
        &["capture"],
        TelemetryRecording::cloud(
            RecordingConfig::default(),
            btel_bcs::PublisherConfig {
                snapshot_target: 2,
                max_pending_snapshots: 4,
                ..btel_bcs::PublisherConfig::default()
            },
            DeliveryConfig {
                prepare_base_url: server.uri(),
                allow_http: true,
                max_pending_plans: 1,
                max_pending_snapshots: 4,
                max_candidates: 4,
                max_targets: 5,
                ..DeliveryConfig::default()
            },
        ),
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        for _ in 0..2 {
            let context = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
            assert_eq!(
                engine
                    .call_function("many_captures", vec![], context, true)
                    .await
                    .unwrap(),
                BexExternalValue::Int(32640)
            );
        }
        engine.shutdown().await;
    })
    .await
    .expect("backpressure must release snapshot owners and let VM capture proceed");
    engine.telemetry_result().unwrap().unwrap();
}

fn hex(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut result, "{byte:02x}").unwrap();
    }
    result
}

fn snapshot_hex(id: proto::SnapshotId) -> String {
    hex(&[id.low.to_le_bytes(), id.high.to_le_bytes()].concat())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cloud_shutdown_drains_recordings_and_respects_cas_plan() {
    let server = MockServer::start().await;
    let uri = server.uri();
    let skipped_id = Arc::new(Mutex::new(None::<String>));
    let plan_skipped_id = Arc::clone(&skipped_id);
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            let prepare: Value = serde_json::from_slice(&request.body).unwrap();
            let sequence = prepare["recording"]["recording_file_sequence"]
                .as_u64()
                .unwrap();
            let candidates = prepare["candidates"].as_array().unwrap();
            let mut skipped_id = plan_skipped_id.lock().unwrap();
            if skipped_id.is_none() {
                *skipped_id = candidates
                    .first()
                    .map(|candidate| candidate["snapshot_id"].as_str().unwrap().to_owned());
            }
            let inline: Vec<_> = candidates
                .iter()
                .enumerate()
                .filter(|(_, candidate)| candidate["snapshot_id"].as_str() != skipped_id.as_deref())
                .map(|(index, _)| index)
                .collect();
            let target = prepare["proposed_uploads"]
                .as_array()
                .unwrap()
                .iter()
                .find(|target| target["kind"] == "recording")
                .unwrap();
            let upload_id = format!("recording-{sequence}");
            let cas: Vec<_> = candidates
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    json!({
                        "candidate_index": index,
                        "disposition": if !inline.contains(&index) {
                            json!({"kind": "already_available"})
                        } else {
                            json!({"kind": "inline_with_recording", "upload_id": upload_id})
                        },
                    })
                })
                .collect();
            ResponseTemplate::new(200).set_body_json(json!({
                "plan_id": format!("plan-{sequence}"),
                "expires_at_unix_ms": 4_102_444_800_000_u64,
                "uploads": [{
                    "upload_id": upload_id,
                    "client_target_id": target["client_target_id"],
                    "object_key": format!("recordings/{sequence}"),
                    "presigned_put_url": format!("{uri}/objects/{sequence}"),
                    "expires_at_unix_ms": 4_102_444_800_000_u64,
                    "required_headers": {"x-upload-plan": "required"},
                    "kind": "recording",
                    "candidate_indices": inline,
                }],
                "cas": cas,
            }))
        })
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let engine = engine(&server, Duration::from_secs(3600));
    let recording_id = engine.telemetry_recording_id().unwrap();
    assert!(engine.telemetry_recording_directory().is_none());
    execute(&engine).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(engine.shutdown(), engine.shutdown());
    })
    .await
    .expect("shutdown must drain cloud delivery");
    assert_eq!(engine.telemetry_result(), Some(Ok(())));

    let requests = server.received_requests().await.unwrap();
    let skipped_id = skipped_id.lock().unwrap().clone().unwrap();
    let prepares: Vec<_> = requests.iter().filter(|r| r.method == "POST").collect();
    let puts: Vec<_> = requests.iter().filter(|r| r.method == "PUT").collect();
    assert!(!prepares.is_empty());
    assert_eq!(prepares.len(), puts.len());
    let mut references = BTreeSet::new();
    let mut offered = BTreeSet::new();
    let (mut announcements, mut completions, mut threads, mut uploaded, mut skipped) =
        (0, 0, 0, 0, 0);
    for (index, request) in prepares.iter().enumerate() {
        assert_eq!(
            request.headers["authorization"],
            "Bearer prepare-only-token"
        );
        let prepare: Value = serde_json::from_slice(&request.body).unwrap();
        let sequence = prepare["recording"]["recording_file_sequence"]
            .as_u64()
            .unwrap();
        assert_eq!(sequence, index as u64 + 1);
        let id = hex(recording_id.as_bytes());
        assert_eq!(
            request.url.path(),
            format!("/v1/recordings/{id}/uploads:prepare")
        );
        assert_eq!(
            request.headers["idempotency-key"],
            format!("{id}:{sequence}")
        );
        assert_eq!(
            prepare["recording"]["recording_id"],
            hex(recording_id.as_bytes())
        );
        assert_eq!(prepare["recording"]["sha256"].as_str().unwrap().len(), 64);
        let put = puts
            .iter()
            .find(|r| r.url.path() == format!("/objects/{sequence}"))
            .unwrap();
        assert!(!put.headers.contains_key("authorization"));
        assert_eq!(put.headers["x-upload-plan"], "required");
        let envelope = CloudUploadEnvelope::decode(put.body.as_slice()).unwrap();
        assert_eq!(envelope.format_version, 1);
        assert_eq!(envelope.plan_id, format!("plan-{sequence}"));
        assert_eq!(envelope.upload_id, format!("recording-{sequence}"));
        let bytes = envelope.recording_file.unwrap();
        assert_eq!(prepare["recording"]["encoded_length"], bytes.len());
        let file = proto::RecordingFile::decode(bytes.as_slice()).unwrap();
        assert_eq!(file.sequence, sequence);
        assert_eq!(file.header.unwrap().recording_id, recording_id.as_bytes());
        assert!(file.end.is_none());
        for section in file.spans.unwrap().sections {
            for event in section.events {
                match event.event.unwrap() {
                    proto::span_event::Event::FunctionAnnouncement(entry) => {
                        announcements += 1;
                        references.insert(snapshot_hex(entry.inputs_cas_id.unwrap()));
                    }
                    proto::span_event::Event::FunctionCompletion(done) => {
                        completions += 1;
                        references.insert(snapshot_hex(done.value_cas_id.unwrap()));
                    }
                    proto::span_event::Event::ThreadCompletion(done) => {
                        threads += 1;
                        assert_eq!(done.outcome, proto::InvocationOutcome::Ok as i32);
                    }
                    _ => {}
                }
            }
        }
        let candidates = prepare["candidates"].as_array().unwrap();
        let actual: BTreeSet<_> = envelope
            .cas_objects
            .iter()
            .map(|c| hex(&c.snapshot_id))
            .collect();
        let expected: BTreeSet<_> = candidates
            .iter()
            .map(|c| c["snapshot_id"].as_str().unwrap().to_owned())
            .filter(|id| id != &skipped_id)
            .collect();
        assert_eq!(actual, expected);
        assert_eq!(envelope.cas_objects.len(), expected.len());
        if candidates
            .iter()
            .any(|candidate| candidate["snapshot_id"] == skipped_id)
        {
            skipped += 1;
        }
        assert!(!actual.contains(&skipped_id));
        for candidate in candidates {
            assert_eq!(candidate["snapshot_format_version"], 1);
            offered.insert(candidate["snapshot_id"].as_str().unwrap().to_owned());
        }
        for object in envelope.cas_objects {
            uploaded += 1;
            assert_eq!(object.snapshot_format_version, 1);
            assert_eq!(object.snapshot_id.len(), 16);
            assert_eq!(object.blob_sha256.len(), 32);
            assert!(object.blob.len() >= 35);
            assert_eq!(&object.blob[..8], b"BTELCAS\0");
            assert_eq!(&object.blob[12..28], object.snapshot_id);
        }
    }
    assert_eq!((announcements, completions), (2, 2));
    assert!(threads >= 2);
    assert!(uploaded > 0 && skipped > 0);
    assert!(
        references.is_subset(&offered),
        "captured span values must reach prepare"
    );
    engine.shutdown().await;
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        requests.len()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cloud_delivery_failure_does_not_change_execution() {
    let server = MockServer::start().await;
    let healthy = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let responding = healthy.clone();
    let base = server.uri();
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            if responding.load(std::sync::atomic::Ordering::Acquire) {
                ResponseTemplate::new(200).set_body_json(cloud_protocol::response(
                    request,
                    &base,
                    &[],
                ))
            } else {
                ResponseTemplate::new(503)
            }
        })
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let engine = engine(&server, Duration::from_millis(10));
    execute(&engine).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while engine
            .telemetry_result()
            .is_none_or(|result| result.is_ok())
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("delivery failure must be observable without another invocation");
    let failure = engine.telemetry_result().unwrap().unwrap_err();
    assert!(engine.telemetry_delivery_loss_count() > 0);
    healthy.store(true, std::sync::atomic::Ordering::Release);
    execute(&engine).await;
    tokio::time::timeout(Duration::from_secs(10), engine.shutdown())
        .await
        .unwrap();
    assert_eq!(engine.telemetry_result(), Some(Err(failure.clone())));
    engine.shutdown().await;
    assert_eq!(engine.telemetry_result(), Some(Err(failure)));
    let requests = server.received_requests().await.unwrap();
    assert!(requests.iter().any(|request| request.method == "PUT"));
    let prepares = prepare_requests::prepare_requests(&requests);
    let initial_ids: BTreeSet<_> = prepares[0]
        .candidates
        .iter()
        .map(|candidate| &candidate.snapshot_id)
        .collect();
    assert!(!initial_ids.is_empty());
    assert!(prepares.iter().skip(1).any(|request| {
        request
            .candidates
            .iter()
            .any(|candidate| initial_ids.contains(&candidate.snapshot_id))
    }));
}
