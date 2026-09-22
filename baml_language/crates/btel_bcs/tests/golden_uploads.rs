use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::Duration,
};

use btel_bcs::{
    delivery::{BcsDelivery, DeliveryConfig, DeliveryError},
    proto::CloudUploadEnvelope,
    wire::{PrepareUploadsRequest, PrepareUploadsResponse, UploadKind},
};
use btel_processor::AggregateDelta;
use btel_publisher::{RecordingBuilder, RecordingConfig, RecordingId, SealedFile};
use btel_records::SpanRecord;
use btel_snapshot::{Limits, Snapshot, SnapshotPool, SnapshotValue};
use btel_types::{AwaitDuration, CallPathId, ClockInstant, TelemetryId, allocate_telemetry_id};
use prost::Message;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path},
};

const CASES: &[&str] = &[
    "inline",
    "batch",
    "standalone",
    "all-skip",
    "mixed",
    "expired",
    "recording-only",
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cloud-v1")
}

fn json_file(relative: &str) -> Value {
    serde_json::from_slice(&std::fs::read(root().join(relative)).unwrap()).unwrap()
}

fn bytes_file(relative: &str) -> Vec<u8> {
    hex::decode(
        std::fs::read_to_string(root().join(relative))
            .unwrap()
            .trim(),
    )
    .unwrap()
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn sources(pool: &SnapshotPool, count: usize) -> (SealedFile, Vec<Snapshot>) {
    // This integration-test process is the sole telemetry ID allocator. Cache
    // the first range once so parallel tests still use the same numeric IDs.
    static IDS: OnceLock<[TelemetryId; 6]> = OnceLock::new();
    let ids = IDS.get_or_init(|| {
        let ids = std::array::from_fn(|_| allocate_telemetry_id());
        assert_eq!(ids.map(TelemetryId::get), [1, 2, 3, 4, 5, 6]);
        ids
    });
    let mut builder = RecordingBuilder::new(
        RecordingId::from_bytes([7; 16]).unwrap(),
        RecordingConfig::default(),
    )
    .unwrap();
    builder.aggregate(AggregateDelta {
        count: 1,
        ..AggregateDelta::default()
    });
    for index in 0..count {
        let snapshot = pool
            .try_acquire()
            .unwrap()
            .finish_value(SnapshotValue::Int(10 + i64::try_from(index).unwrap()));
        builder.span(
            ids[0],
            &mut SpanRecord::FunctionSpanCompletionOk {
                id: ids[index + 1],
                parent_id: ids[0],
                call_path: CallPathId::new_non_root(7).unwrap(),
                entered_at: ClockInstant::from_ticks(100),
                exited_at: ClockInstant::from_ticks(200),
                await_time: AwaitDuration::ZERO,
                captured_value: Some(snapshot),
            },
        );
    }
    let snapshots = builder.take_snapshots().collect();
    let file = builder.finish_recording().unwrap().unwrap();
    (file, snapshots)
}

fn source_manifest() -> Value {
    let pool = SnapshotPool::new(5, Limits::default());
    let mut recordings = Vec::new();
    let mut blobs = Vec::new();
    for count in [0, 1, 2, 5] {
        let (file, snapshots) = sources(&pool, count);
        recordings.push(json!({
            "candidate_count": count,
            "hex": hex::encode(file.bytes()),
            "length": file.bytes().len(),
            "sha256": digest(file.bytes()),
        }));
        if count == 5 {
            for (index, snapshot) in snapshots.iter().enumerate() {
                let mut blob = Vec::new();
                snapshot.write_blob(&mut blob).unwrap();
                blobs.push(json!({
                    "value": 10 + index,
                    "snapshot_id": hex::encode(snapshot.id().as_bytes()),
                    "snapshot_format_version": btel_settings::snapshot::BLOB_VERSION,
                    "hex": hex::encode(&blob),
                    "length": blob.len(),
                    "sha256": digest(&blob),
                }));
            }
        }
    }
    assert_eq!(pool.stats().in_use, 0);
    json!({"recordings": recordings, "snapshots": blobs})
}

#[test]
fn source_encoding_goldens() {
    assert_eq!(source_manifest(), json_file("sources/manifest.json"));
}

/// Prints initial source encodings for manual contract review, never overwrites goldens.
#[test]
#[ignore]
#[expect(
    clippy::print_stdout,
    reason = "explicit manual fixture inspection command"
)]
fn print_source_encodings() {
    println!(
        "{}",
        serde_json::to_string_pretty(&source_manifest()).unwrap()
    );
}

fn checked_body(case: &str, expected: &Value) -> Vec<u8> {
    let body = bytes_file(&format!(
        "{case}/{}",
        expected["body_file"].as_str().unwrap()
    ));
    assert_eq!(body.len() as u64, expected["length"].as_u64().unwrap());
    assert_eq!(digest(&body), expected["sha256"].as_str().unwrap());
    body
}

#[test]
fn fixture_envelopes_embed_exact_sources_and_ordered_membership() {
    let sources = json_file("sources/manifest.json");
    for case in CASES {
        let request: PrepareUploadsRequest =
            serde_json::from_value(json_file(&format!("{case}/prepare.json"))).unwrap();
        let response: PrepareUploadsResponse =
            serde_json::from_value(json_file(&format!("{case}/response.json"))).unwrap();
        let puts = json_file(&format!("{case}/puts.json"));
        for target in &response.uploads {
            let expected = if *case == "expired" {
                &puts["forbidden_body"]
            } else {
                puts["uploads"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|entry| entry["upload_id"] == target.upload_id)
                    .unwrap()
            };
            let body = checked_body(case, expected);
            let envelope = CloudUploadEnvelope::decode(body.as_slice()).unwrap();
            assert_eq!(envelope.format_version, 1);
            assert_eq!(envelope.plan_id, response.plan_id);
            assert_eq!(envelope.upload_id, target.upload_id);
            if target.kind == UploadKind::Recording {
                let recording = envelope.recording_file.as_ref().unwrap();
                assert_eq!(recording.len() as u64, request.recording.encoded_length);
                assert_eq!(digest(recording), request.recording.sha256);
                let source = sources["recordings"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|entry| {
                        entry["candidate_count"].as_u64().unwrap()
                            == request.candidates.len() as u64
                    })
                    .unwrap();
                assert_eq!(hex::encode(recording), source["hex"].as_str().unwrap());
            } else {
                assert!(envelope.recording_file.is_none());
            }
            assert_eq!(envelope.cas_objects.len(), target.candidate_indices.len());
            for (object, &index) in envelope.cas_objects.iter().zip(&target.candidate_indices) {
                let candidate = &request.candidates[index as usize];
                assert_eq!(hex::encode(&object.snapshot_id), candidate.snapshot_id);
                assert_eq!(
                    object.snapshot_format_version,
                    candidate.snapshot_format_version
                );
                assert_eq!(hex::encode(&object.blob_sha256), digest(&object.blob));
                let source = sources["snapshots"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|entry| entry["snapshot_id"] == candidate.snapshot_id)
                    .unwrap();
                assert_eq!(hex::encode(&object.blob), source["hex"].as_str().unwrap());
                assert_eq!(object.blob.len() as u64, source["length"].as_u64().unwrap());
            }
        }
    }
}

async fn check_case(case: &str) {
    let expected_prepare = json_file(&format!("{case}/prepare.json"));
    let request: PrepareUploadsRequest = serde_json::from_value(expected_prepare.clone()).unwrap();
    let manifest = json_file(&format!("{case}/puts.json"));
    let mut response: PrepareUploadsResponse =
        serde_json::from_value(json_file(&format!("{case}/response.json"))).unwrap();
    let server = MockServer::start().await;
    for target in &mut response.uploads {
        let suffix = target
            .presigned_put_url
            .strip_prefix("https://uploads.invalid")
            .unwrap();
        assert!(suffix.starts_with('/'));
        target.presigned_put_url = format!("{}{suffix}", server.uri());
    }
    let observed = Arc::new(tokio::sync::Notify::new());
    let notify = Arc::clone(&observed);
    Mock::given(method("POST"))
        .and(path(manifest["prepare_path"].as_str().unwrap()))
        .respond_with(move |_: &Request| {
            notify.notify_one();
            ResponseTemplate::new(200).set_body_json(&response)
        })
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .expect(u64::try_from(manifest["uploads"].as_array().unwrap().len()).unwrap())
        .mount(&server)
        .await;
    let delivery = Arc::new(
        BcsDelivery::new(
            DeliveryConfig {
                prepare_base_url: server.uri(),
                bearer_token: Some("golden-prepare-token".into()),
                allow_http: true,
                max_candidates: 8,
                max_targets: 8,
                max_recording_body_bytes: 4096,
                max_cas_body_bytes: 4096,
                request_timeout: Duration::from_secs(2),
                max_attempts: 1,
                ..DeliveryConfig::default()
            },
            |_| {},
        )
        .unwrap(),
    );
    let pool = SnapshotPool::new(5, Limits::default());
    let (file, snapshots) = sources(&pool, request.candidates.len());
    delivery
        .handle()
        .try_submit(file, snapshots, request.proposed_uploads)
        .unwrap();
    // Observe the prepare before finish changes RUNNING to DRAINING.
    tokio::time::timeout(Duration::from_secs(5), observed.notified())
        .await
        .unwrap();
    let result = tokio::task::spawn_blocking(move || delivery.finish())
        .await
        .unwrap();
    assert_eq!(
        result,
        if case == "expired" {
            Err(DeliveryError::Expired)
        } else {
            Ok(())
        },
        "{case}"
    );
    assert_eq!(pool.stats().in_use, 0, "{case}");
    let received = server.received_requests().await.unwrap();
    let prepares: Vec<_> = received.iter().filter(|r| r.method == "POST").collect();
    assert_eq!(prepares.len(), 1);
    let prepare = prepares[0];
    assert_eq!(
        prepare.url.path(),
        manifest["prepare_path"].as_str().unwrap()
    );
    assert!(prepare.url.query().is_none());
    for (name, value) in manifest["prepare_headers"].as_object().unwrap() {
        assert_eq!(prepare.headers.get_all(name).iter().count(), 1);
        assert_eq!(prepare.headers[name], value.as_str().unwrap(), "{case}");
    }
    let mut actual: Value = serde_json::from_slice(&prepare.body).unwrap();
    let session = uuid::Uuid::parse_str(actual["producer_session_id"].as_str().unwrap()).unwrap();
    assert_eq!(session.get_version_num(), 4);
    assert!(actual["liveness_sequence"].as_u64().unwrap() > 0);
    assert_eq!(actual["state"], "RUNNING");
    for field in ["producer_session_id", "liveness_sequence"] {
        actual[field] = expected_prepare[field].clone();
    }
    assert_eq!(actual, expected_prepare, "{case}");
    let puts: Vec<_> = received.iter().filter(|r| r.method == "PUT").collect();
    let expected_puts = manifest["uploads"].as_array().unwrap();
    assert_eq!(puts.len(), expected_puts.len(), "{case}");
    for expected in expected_puts {
        let matches: Vec<_> = puts
            .iter()
            .filter(|r| r.url.path() == expected["path"].as_str().unwrap())
            .collect();
        assert_eq!(matches.len(), 1, "{case}: {}", expected["upload_id"]);
        let put = matches[0];
        assert_eq!(put.url.query(), expected["query"].as_str());
        assert!(!put.headers.contains_key("authorization"));
        assert!(!put.headers.contains_key("idempotency-key"));
        for (name, value) in expected["headers"].as_object().unwrap() {
            assert_eq!(put.headers.get_all(name).iter().count(), 1);
            assert_eq!(put.headers[name], value.as_str().unwrap(), "{case}");
        }
        let body = checked_body(case, expected);
        assert_eq!(put.body, body, "{case}: {}", expected["upload_id"]);
        assert_eq!(put.headers.get_all("content-length").iter().count(), 1);
        assert_eq!(
            put.headers["content-length"],
            expected["length"].as_u64().unwrap().to_string()
        );
        let decoded = CloudUploadEnvelope::decode(body.as_slice()).unwrap();
        assert_eq!(decoded.upload_id, expected["upload_id"].as_str().unwrap());
    }
}

#[tokio::test]
async fn fixed_upload_contract_on_http_wire() {
    for case in CASES {
        check_case(case).await;
    }
}

#[test]
fn golden_bytes_detect_blob_and_membership_corruption() {
    let bytes = bytes_file("mixed/recording.hex");
    let expected = CloudUploadEnvelope::decode(bytes.as_slice()).unwrap();
    let mut corrupt = expected.clone();
    corrupt.cas_objects[0].blob[0] ^= 1;
    assert_ne!(corrupt.encode_to_vec(), bytes);
    assert_ne!(
        Sha256::digest(&corrupt.cas_objects[0].blob).as_slice(),
        corrupt.cas_objects[0].blob_sha256
    );
    let mut wrong_membership = expected;
    wrong_membership.cas_objects.clear();
    assert_ne!(wrong_membership.encode_to_vec(), bytes);
}
