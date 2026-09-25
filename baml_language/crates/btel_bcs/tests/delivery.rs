use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use btel_bcs::{
    delivery::{BcsDelivery, DeliveryConfig, DeliveryError},
    proto::CloudUploadEnvelope,
    wire::{Disposition, PrepareUploadsRequest, ProposedUploadTarget, UploadKind},
};
use btel_processor::{AggregateDelta, Publisher};
use btel_recorder::{RecordingConfig, RecordingId, RecordingPublisher, SealedFile};
use btel_snapshot::{Limits, Snapshot, SnapshotObject, SnapshotPool, SnapshotValue};
use prost::Message;
use sha2::{Digest, Sha256};
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path},
};

#[path = "support/finish.rs"]
mod finish;
#[path = "support/prepare_requests.rs"]
mod prepare_requests;
mod support;
use finish::finish;
use prepare_requests::prepare_requests;
use support::response;

fn config(server: &MockServer) -> DeliveryConfig {
    DeliveryConfig {
        bearer_token: Some("prepare-only-secret".into()),
        allow_http: true,
        max_pending_plans: 4,
        max_candidates: 8,
        max_targets: 8,
        max_recording_body_bytes: 4096,
        max_cas_body_bytes: 4096,
        request_timeout: Duration::from_millis(200),
        retry_delay: Duration::from_millis(5),
        max_attempts: 2,
        ..DeliveryConfig::new(server.uri().parse().unwrap())
    }
}

fn files(count: usize) -> Vec<SealedFile> {
    let mut files = Vec::new();
    {
        let mut publisher = RecordingPublisher::new(
            RecordingId::from_bytes([7; 16]).unwrap(),
            RecordingConfig::default(),
            |file| files.push(file),
        )
        .unwrap();
        for _ in 0..count {
            publisher.aggregate(AggregateDelta {
                count: 1,
                ..AggregateDelta::default()
            });
            publisher.finish_recording().unwrap();
        }
    }
    assert_eq!(files.len(), count);
    files
}

fn scalar(pool: &SnapshotPool, value: i64) -> Snapshot {
    pool.try_acquire()
        .unwrap()
        .finish_value(SnapshotValue::Int(value))
}

fn large(pool: &SnapshotPool) -> Snapshot {
    let mut builder = pool.try_acquire().unwrap();
    let bytes = vec![1; 64 * 1024];
    let data = builder.copy_bytes(&bytes);
    let id = builder.reserve_object().unwrap();
    builder.set_object(
        id,
        SnapshotObject::Bytes {
            data,
            original_len: bytes.len(),
        },
    );
    builder.finish_value(SnapshotValue::Object(id))
}

fn proposed(id: u32, kind: UploadKind, members: &[u32]) -> ProposedUploadTarget {
    ProposedUploadTarget {
        client_target_id: id,
        kind,
        candidate_indices: members.to_vec(),
    }
}

#[tokio::test]
async fn failed_payload_releases_capacity_and_later_payload_uploads() {
    for fail_prepare in [true, false] {
        let server = MockServer::start().await;
        let base = server.uri();
        Mock::given(method("POST"))
            .respond_with(move |request: &Request| {
                let plan = response(request, &base, &[]);
                if fail_prepare && plan.plan_id == "plan-1" {
                    ResponseTemplate::new(503).set_delay(Duration::from_millis(20))
                } else {
                    ResponseTemplate::new(200).set_body_json(plan)
                }
            })
            .expect(if fail_prepare { 5 } else { 2 })
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .respond_with(|request: &Request| {
                ResponseTemplate::new(if request.url.path() == "/put/1-0" {
                    503
                } else {
                    200
                })
                .set_delay(Duration::from_millis(20))
            })
            .expect(if fail_prepare { 1 } else { 5 })
            .mount(&server)
            .await;
        let mut settings = config(&server);
        settings.max_pending_plans = 1;
        settings.max_attempts = DeliveryConfig::new(server.uri().parse().unwrap()).max_attempts;
        assert_eq!(settings.max_attempts, 4);
        let delivery = Arc::new(
            BcsDelivery::new(settings, |_| panic!("ordinary loss disabled telemetry")).unwrap(),
        );
        let handle = delivery.handle();
        let mut files = files(2).into_iter();
        handle
            .try_submit(
                files.next().unwrap(),
                vec![],
                vec![proposed(0, UploadKind::Recording, &[])],
            )
            .unwrap();
        let next = files.next().unwrap();
        let submitter = handle.clone();
        tokio::time::timeout(
            Duration::from_secs(2),
            tokio::task::spawn_blocking(move || {
                submitter.try_submit(next, vec![], vec![proposed(0, UploadKind::Recording, &[])])
            }),
        )
        .await
        .unwrap()
        .unwrap()
        .unwrap();
        assert!(!handle.is_disabled());
        assert_eq!(finish(delivery).await, Err(DeliveryError::Http));
        assert_eq!(handle.loss_count(), 1);
        assert_eq!(handle.last_error(), Some(DeliveryError::Http));
        let requests = server.received_requests().await.unwrap();
        for request in requests.iter().filter(|r| r.method == "POST") {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            assert!(matches!(
                body["state"].as_str(),
                Some("RUNNING" | "DRAINING")
            ));
        }
        assert!(
            requests
                .iter()
                .any(|r| r.method == "PUT" && r.url.path() == "/put/2-0")
        );
        let retries: Vec<_> = requests
            .iter()
            .filter(|r| r.method == "PUT" && r.url.path() == "/put/1-0")
            .collect();
        for pair in retries.windows(2) {
            assert_eq!(pair[0].body, pair[1].body);
            assert_eq!(pair[0].url, pair[1].url);
        }
    }
}

#[tokio::test]
async fn failed_physical_target_does_not_cancel_either_lane() {
    for failed_target in [0, 1] {
        let server = MockServer::start().await;
        let base = server.uri();
        Mock::given(method("POST"))
            .respond_with(move |r: &Request| {
                ResponseTemplate::new(200).set_body_json(response(r, &base, &[]))
            })
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .respond_with(move |r: &Request| {
                if r.url.path() == format!("/put/1-{failed_target}") {
                    ResponseTemplate::new(403)
                } else {
                    ResponseTemplate::new(200).set_delay(Duration::from_millis(30))
                }
            })
            .expect(3)
            .mount(&server)
            .await;
        let delivery = Arc::new(
            BcsDelivery::new(config(&server), |_| {
                panic!("ordinary loss disabled telemetry")
            })
            .unwrap(),
        );
        let handle = delivery.handle();
        let pool = SnapshotPool::new(2, Limits::default());
        handle
            .try_submit(
                files(1).pop().unwrap(),
                vec![scalar(&pool, 1), scalar(&pool, 2)],
                vec![
                    proposed(0, UploadKind::Recording, &[]),
                    proposed(1, UploadKind::CasObject, &[0]),
                    proposed(2, UploadKind::CasObject, &[1]),
                ],
            )
            .unwrap();
        assert_eq!(finish(delivery).await, Err(DeliveryError::Http));
        assert_eq!(handle.loss_count(), 1);
        assert!(!handle.is_disabled());
        assert_eq!(pool.stats().in_use, 0);
    }
}

#[tokio::test]
async fn expired_target_drops_only_itself_and_later_work_is_accepted() {
    for expired_target in [0, 1] {
        let server = MockServer::start().await;
        let base = server.uri();
        Mock::given(method("POST"))
            .respond_with(move |request: &Request| {
                let mut plan = response(request, &base, &[]);
                if plan.plan_id == "plan-1" {
                    plan.uploads[expired_target].expires_at_unix_ms = 1;
                }
                ResponseTemplate::new(200).set_body_json(plan)
            })
            .expect(2)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(200))
            .expect(3)
            .mount(&server)
            .await;
        let delivery = Arc::new(BcsDelivery::new(config(&server), |_| {}).unwrap());
        let handle = delivery.handle();
        let pool = SnapshotPool::new(2, Limits::default());
        let mut files = files(2).into_iter();
        handle
            .try_submit(
                files.next().unwrap(),
                vec![scalar(&pool, 1), scalar(&pool, 2)],
                vec![
                    proposed(0, UploadKind::Recording, &[]),
                    proposed(1, UploadKind::CasObject, &[0]),
                    proposed(2, UploadKind::CasObject, &[1]),
                ],
            )
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while handle.loss_count() == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(!handle.is_disabled());
        handle
            .try_submit(
                files.next().unwrap(),
                vec![],
                vec![proposed(0, UploadKind::Recording, &[])],
            )
            .unwrap();
        assert_eq!(finish(delivery).await, Err(DeliveryError::Expired));
        assert_eq!(handle.loss_count(), 1);
        assert_eq!(pool.stats().in_use, 0);
        let requests = server.received_requests().await.unwrap();
        let paths: std::collections::HashSet<_> = requests
            .iter()
            .filter(|r| r.method == "PUT")
            .map(|r| r.url.path().to_owned())
            .collect();
        assert_eq!(paths.len(), 3);
        assert!(!paths.contains(&format!("/put/1-{expired_target}")));
        assert!(paths.contains("/put/1-2"));
        assert!(paths.contains("/put/2-0"));
    }
}

#[tokio::test]
async fn canceled_submitters_do_not_count_as_payload_losses() {
    for fatal in [DeliveryError::Worker, DeliveryError::Capacity] {
        let server = MockServer::start().await;
        let delivery = Arc::new(BcsDelivery::new(config(&server), |_| {}).unwrap());
        let handle = delivery.handle();
        handle.disable(fatal);
        assert_eq!(
            handle.try_submit(
                files(1).pop().unwrap(),
                vec![],
                vec![proposed(0, UploadKind::Recording, &[])],
            ),
            Err(fatal)
        );
        assert_eq!(handle.loss_count(), 0);
        assert_eq!(finish(delivery).await, Err(fatal));
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn advisory_heartbeat_failure_does_not_interrupt_draining_uploads() {
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("POST"))
        .and(wiremock::matchers::path_regex("uploads:prepare$"))
        .respond_with(move |request: &Request| {
            let mut plan = response(request, &base, &[]);
            plan.heartbeat = Some(btel_bcs::wire::HeartbeatPolicy {
                interval_ms: 10,
                staleness_threshold_ms: 100,
            });
            ResponseTemplate::new(200).set_body_json(plan)
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(wiremock::matchers::path_regex("/heartbeat$"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(120)))
        .mount(&server)
        .await;
    let mut settings = config(&server);
    settings.request_timeout = Duration::from_secs(1);
    let delivery = Arc::new(BcsDelivery::new(settings, |_| {}).unwrap());
    delivery
        .handle()
        .try_submit(
            files(1).pop().unwrap(),
            vec![],
            vec![proposed(0, UploadKind::Recording, &[])],
        )
        .unwrap();
    assert_eq!(finish(Arc::clone(&delivery)).await, Ok(()));
    assert_eq!(
        delivery.heartbeat_error(),
        Some(btel_bcs::liveness::HeartbeatError::Status(503))
    );
    assert!(delivery.heartbeat_failure_count() > 0);
    let requests = server.received_requests().await.unwrap();
    let mut session = None;
    let mut sequences = std::collections::HashSet::new();
    let mut heartbeat_count = 0;
    let mut draining = false;
    for request in requests {
        if request.method == "PUT" {
            assert!(!request.headers.contains_key("authorization"));
            continue;
        }
        assert_eq!(
            request.headers["authorization"],
            "Bearer prepare-only-secret"
        );
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        let producer = body["producer_session_id"].as_str().unwrap();
        assert_eq!(session.get_or_insert_with(|| producer.to_owned()), producer);
        assert!(sequences.insert(body["liveness_sequence"].as_u64().unwrap()));
        if request.url.path().ends_with("/heartbeat") {
            heartbeat_count += 1;
            // A heartbeat started before finish closes admission can still be RUNNING.
            match body["state"].as_str() {
                Some("RUNNING") => assert!(!draining, "heartbeat state regressed"),
                Some("DRAINING") => draining = true,
                state => panic!("unexpected heartbeat state: {state:?}"),
            }
        }
    }
    assert!(heartbeat_count > 0);
}

#[tokio::test]
async fn serialized_snapshots_release_admission_slots_before_put_completion() {
    let server = MockServer::start().await;
    let second_prepared = Arc::new(AtomicBool::new(false));
    let prepared = Arc::clone(&second_prepared);
    let base = server.uri();
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            let plan = response(request, &base, &[]);
            if plan.plan_id == "plan-2" {
                prepared.store(true, Ordering::Release);
            }
            ResponseTemplate::new(200).set_body_json(plan)
        })
        .mount(&server)
        .await;
    let first_put = Arc::new(tokio::sync::Notify::new());
    let observed = Arc::clone(&first_put);
    Mock::given(method("PUT"))
        .respond_with(move |_: &Request| {
            observed.notify_one();
            ResponseTemplate::new(if second_prepared.load(Ordering::Acquire) {
                200
            } else {
                503
            })
        })
        .mount(&server)
        .await;
    let mut settings = config(&server);
    settings.max_pending_snapshots = 1;
    settings.max_candidates = 1;
    settings.max_targets = 2;
    settings.max_attempts = 50;
    settings.retry_delay = Duration::from_millis(10);
    let delivery = Arc::new(BcsDelivery::new(settings, |_| {}).unwrap());
    let handle = delivery.handle();
    let pool = SnapshotPool::new(1, Limits::default());
    let mut files = files(2).into_iter();
    handle
        .try_submit(
            files.next().unwrap(),
            vec![scalar(&pool, 1)],
            vec![proposed(0, UploadKind::Recording, &[0])],
        )
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), first_put.notified())
        .await
        .unwrap();
    assert_eq!(pool.stats().in_use, 0);
    handle
        .try_submit(
            files.next().unwrap(),
            vec![scalar(&pool, 2)],
            vec![proposed(0, UploadKind::Recording, &[0])],
        )
        .unwrap();
    assert_eq!(finish(delivery).await, Ok(()));
    assert_eq!(pool.stats().in_use, 0);
}

#[tokio::test]
async fn mixed_plan_prunes_without_serializing_and_uploads_canonical_envelopes() {
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("POST"))
        .respond_with(move |r: &Request| {
            ResponseTemplate::new(200).set_body_json(response(r, &base, &[2, 4]))
        })
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .expect(3)
        .mount(&server)
        .await;
    let pool = SnapshotPool::new(8, Limits::default());
    let snapshots = vec![
        scalar(&pool, 10),
        scalar(&pool, 11),
        large(&pool),
        scalar(&pool, 13),
        scalar(&pool, 14),
    ];
    let expected: Vec<_> = snapshots
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 2 && *i != 4)
        .map(|(_, s)| {
            let mut bytes = Vec::new();
            s.write_blob(&mut bytes).unwrap();
            (s.id().as_bytes().to_vec(), bytes)
        })
        .collect();
    let file = files(1).pop().unwrap();
    let recording_bytes = file.bytes().to_vec();
    let delivery =
        Arc::new(BcsDelivery::new(config(&server), |_| panic!("unexpected failure")).unwrap());
    let handle = delivery.handle();
    handle
        .try_submit(
            file,
            snapshots,
            vec![
                proposed(0, UploadKind::Recording, &[0]),
                proposed(1, UploadKind::CasBatch, &[1, 2]),
                proposed(2, UploadKind::CasObject, &[3]),
                proposed(3, UploadKind::CasObject, &[4]),
            ],
        )
        .unwrap();
    finish(delivery.clone()).await.unwrap();
    assert_eq!(delivery.result(), Some(Ok(())));
    assert!(!handle.is_disabled());
    assert_eq!(pool.stats().in_use, 0);
    let requests = server.received_requests().await.unwrap();
    let post = requests.iter().find(|r| r.method == "POST").unwrap();
    assert_eq!(post.headers["authorization"], "Bearer prepare-only-secret");
    let prepare: PrepareUploadsRequest = serde_json::from_slice(&post.body).unwrap();
    assert_eq!(
        prepare.recording.encoded_length,
        recording_bytes.len() as u64
    );
    assert_eq!(
        prepare.recording.sha256,
        hex::encode(Sha256::digest(&recording_bytes))
    );
    assert_eq!(
        post.headers["idempotency-key"],
        format!("{}:1", prepare.recording.recording_id)
    );
    let mut uploaded = Vec::new();
    for request in requests.iter().filter(|r| r.method == "PUT") {
        assert!(!request.headers.contains_key("authorization"));
        assert!(!request.headers.contains_key("cookie"));
        assert_eq!(request.headers["x-required"], "signed-value");
        let body = CloudUploadEnvelope::decode(request.body.as_slice()).unwrap();
        assert_eq!(body.format_version, 1);
        assert_eq!(body.plan_id, "plan-1");
        assert_eq!(
            body.recording_file.as_ref(),
            (body.upload_id == "1-0").then_some(&recording_bytes)
        );
        for object in body.cas_objects {
            assert_eq!(object.snapshot_format_version, 2);
            assert_eq!(object.blob_sha256, Sha256::digest(&object.blob).to_vec());
            uploaded.push((object.snapshot_id, object.blob));
        }
    }
    uploaded.sort();
    let mut expected = expected;
    expected.sort();
    assert_eq!(uploaded, expected);
    assert_eq!(
        handle.try_submit(
            files(1).pop().unwrap(),
            vec![],
            vec![proposed(0, UploadKind::Recording, &[]),]
        ),
        Err(DeliveryError::Closed)
    );
}

#[tokio::test]
async fn prepare_retries_preserve_identity_and_put_retries_preserve_exact_bytes() {
    let server = MockServer::start().await;
    let base = server.uri();
    let posts = Arc::new(AtomicUsize::new(0));
    let puts = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .respond_with(move |r: &Request| {
            if posts.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(503)
            } else {
                ResponseTemplate::new(200).set_body_json(response(r, &base, &[]))
            }
        })
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(move |_: &Request| {
            ResponseTemplate::new(if puts.fetch_add(1, Ordering::SeqCst) == 0 {
                503
            } else {
                200
            })
        })
        .expect(2)
        .mount(&server)
        .await;
    let pool = SnapshotPool::new(2, Limits::default());
    let delivery = Arc::new(BcsDelivery::new(config(&server), |_| {}).unwrap());
    delivery
        .handle()
        .try_submit(
            files(1).pop().unwrap(),
            vec![scalar(&pool, 1)],
            vec![proposed(0, UploadKind::Recording, &[0])],
        )
        .unwrap();
    finish(delivery).await.unwrap();
    assert_eq!(pool.stats().in_use, 0);
    let requests = server.received_requests().await.unwrap();
    for verb in ["POST", "PUT"] {
        let requests: Vec<_> = requests.iter().filter(|r| r.method == verb).collect();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].url, requests[1].url);
        if verb == "PUT" {
            assert_eq!(requests[0].body, requests[1].body);
            assert_eq!(requests[0].headers, requests[1].headers);
        } else {
            let mut first: PrepareUploadsRequest =
                serde_json::from_slice(&requests[0].body).unwrap();
            let mut second: PrepareUploadsRequest =
                serde_json::from_slice(&requests[1].body).unwrap();
            assert!(first.producer_session_id.is_some());
            assert_eq!(first.producer_session_id, second.producer_session_id);
            assert!(first.liveness_sequence.unwrap() < second.liveness_sequence.unwrap());
            first.liveness_sequence = None;
            second.liveness_sequence = None;
            first.state = None;
            second.state = None;
            assert_eq!(first, second);
            for name in ["idempotency-key", "authorization"] {
                assert_eq!(requests[0].headers[name], requests[1].headers[name]);
            }
        }
    }
}

#[tokio::test]
async fn rejects_malformed_plans_before_any_put_and_releases_every_owner() {
    for mutation in 0..17 {
        let server = MockServer::start().await;
        let base = server.uri();
        Mock::given(method("POST"))
            .respond_with(move |r: &Request| {
                let mut plan = response(r, &base, &[]);
                match mutation {
                    0 => plan.cas.clear(),
                    1 => plan.cas.push(plan.cas[0].clone()),
                    2 => plan.cas[0].candidate_index = 99,
                    3 => plan.uploads[0].candidate_indices.reverse(),
                    4 => plan.uploads[0].client_target_id = 99,
                    5 => plan.uploads[0].kind = UploadKind::CasBatch,
                    6 => {
                        plan.uploads[0]
                            .required_headers
                            .insert("Authorization".into(), "secret".into());
                    }
                    7 => plan.expires_at_unix_ms = 1,
                    8 => plan.uploads[0].expires_at_unix_ms = 1,
                    9 => {
                        plan.uploads[0].presigned_put_url =
                            "https://user:secret@example.com/".into();
                    }
                    10 => plan.uploads.clear(),
                    11 => plan.uploads.push(plan.uploads[0].clone()),
                    12 => plan.uploads[0].candidate_indices = vec![0, 0],
                    13 => plan.uploads[0].candidate_indices = vec![0, 99],
                    14 => plan.cas[0].disposition = Disposition::AlreadyAvailable {},
                    15 => {
                        plan.cas[0].disposition = Disposition::InlineWithRecording {
                            upload_id: "unknown".into(),
                        }
                    }
                    16 => {
                        plan.uploads[0]
                            .required_headers
                            .insert("content-length".into(), "1".into());
                    }
                    _ => unreachable!(),
                }
                ResponseTemplate::new(200).set_body_json(plan)
            })
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        let failures = Arc::new(AtomicUsize::new(0));
        let callback = failures.clone();
        let delivery = Arc::new(
            BcsDelivery::new(config(&server), move |_| {
                callback.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap(),
        );
        let handle = delivery.handle();
        let pool = SnapshotPool::new(3, Limits::default());
        handle
            .try_submit(
                files(1).pop().unwrap(),
                vec![scalar(&pool, 1), scalar(&pool, 2)],
                vec![proposed(0, UploadKind::Recording, &[0, 1])],
            )
            .unwrap();
        assert!(
            finish(delivery.clone()).await.is_err(),
            "mutation {mutation}"
        );
        assert!(!handle.is_disabled());
        assert_eq!(pool.stats().in_use, 0);
        assert_eq!(failures.load(Ordering::SeqCst), 0);
        assert!(delivery.finish().is_err());
        assert_eq!(failures.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn redirects_are_terminal_and_never_forward_credentials_or_capabilities() {
    for redirect_prepare in [true, false] {
        let server = MockServer::start().await;
        let destination = MockServer::start().await;
        let base = server.uri();
        let redirect = destination.uri();
        Mock::given(method("POST"))
            .respond_with(move |r: &Request| {
                if redirect_prepare {
                    ResponseTemplate::new(307).insert_header("location", redirect.as_str())
                } else {
                    ResponseTemplate::new(200).set_body_json(response(r, &base, &[]))
                }
            })
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .respond_with(
                ResponseTemplate::new(307).insert_header("location", destination.uri().as_str()),
            )
            .expect(u64::from(!redirect_prepare))
            .mount(&server)
            .await;
        let delivery = Arc::new(BcsDelivery::new(config(&server), |_| {}).unwrap());
        delivery
            .handle()
            .try_submit(
                files(1).pop().unwrap(),
                vec![],
                vec![proposed(0, UploadKind::Recording, &[])],
            )
            .unwrap();
        assert_eq!(finish(delivery).await, Err(DeliveryError::Http));
        assert!(destination.received_requests().await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn timeout_retries_are_bounded_and_never_fit_work_is_rejected() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(200)))
        .expect(2)
        .mount(&server)
        .await;
    let mut settings = config(&server);
    settings.request_timeout = Duration::from_millis(20);
    let delivery = Arc::new(BcsDelivery::new(settings, |_| {}).unwrap());
    delivery
        .handle()
        .try_submit(
            files(1).pop().unwrap(),
            vec![],
            vec![proposed(0, UploadKind::Recording, &[])],
        )
        .unwrap();
    assert_eq!(finish(delivery).await, Err(DeliveryError::Http));

    let mut settings = config(&server);
    settings.recording_reserved_bytes = 1;
    let delivery = BcsDelivery::new(settings, |_| {}).unwrap();
    let handle = delivery.handle();
    assert_eq!(
        handle.try_submit(
            files(1).pop().unwrap(),
            vec![],
            vec![proposed(0, UploadKind::Recording, &[])]
        ),
        Err(DeliveryError::Capacity)
    );
    assert!(!handle.is_disabled());
    assert_eq!(delivery.finish(), Err(DeliveryError::Capacity));
}

#[tokio::test]
async fn saturated_admission_waits_and_resumes_or_wakes_on_terminal_state() {
    for mode in 0..4 {
        let server = MockServer::start().await;
        let base = server.uri();
        Mock::given(method("POST"))
            .respond_with(move |r: &Request| {
                ResponseTemplate::new(200).set_body_json(response(r, &base, &[]))
            })
            .mount(&server)
            .await;
        let entered = Arc::new(tokio::sync::Notify::new());
        let observed = entered.clone();
        let release = Arc::new(AtomicBool::new(false));
        let released = release.clone();
        Mock::given(method("PUT"))
            .respond_with(move |_: &Request| {
                observed.notify_one();
                ResponseTemplate::new(if released.load(Ordering::Acquire) {
                    200
                } else {
                    503
                })
            })
            .mount(&server)
            .await;
        let mut settings = config(&server);
        settings.max_attempts = 100;
        settings.retry_delay = Duration::from_millis(10);
        if mode == 1 {
            // One reservation fits, but two do not, despite spare plan slots.
            settings.recording_reserved_bytes = 5 * 1024 * 1024;
        } else {
            settings.max_pending_plans = 1;
        }
        let delivery = Arc::new(BcsDelivery::new(settings, |_| {}).unwrap());
        let handle = delivery.handle();
        let mut files = files(2).into_iter();
        handle
            .try_submit(
                files.next().unwrap(),
                vec![],
                vec![proposed(0, UploadKind::Recording, &[])],
            )
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), entered.notified())
            .await
            .unwrap();
        let second = files.next().unwrap();
        let producer = handle.clone();
        let mut waiting = tokio::task::spawn_blocking(move || {
            producer.try_submit(
                second,
                vec![],
                vec![proposed(0, UploadKind::Recording, &[])],
            )
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(30), &mut waiting)
                .await
                .is_err()
        );
        assert!(!handle.is_disabled());
        match mode {
            2 => handle.disable(DeliveryError::Http),
            3 => {
                let draining = delivery.clone();
                let finish = tokio::task::spawn_blocking(move || draining.finish());
                assert_eq!(
                    tokio::time::timeout(Duration::from_secs(2), waiting)
                        .await
                        .unwrap()
                        .unwrap(),
                    Err(DeliveryError::Closed)
                );
                release.store(true, Ordering::Release);
                assert_eq!(finish.await.unwrap(), Ok(()));
                continue;
            }
            _ => release.store(true, Ordering::Release),
        }
        let expected = if mode == 2 {
            Err(DeliveryError::Http)
        } else {
            Ok(())
        };
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), waiting)
                .await
                .unwrap()
                .unwrap(),
            expected
        );
        assert_eq!(finish(delivery).await, expected);
    }
}

#[tokio::test]
async fn snapshot_owner_saturation_waits_until_planning_releases_owners() {
    let server = MockServer::start().await;
    let base = server.uri();
    let prepared = Arc::new(tokio::sync::Notify::new());
    let observed = prepared.clone();
    let release = Arc::new(AtomicBool::new(false));
    let released = release.clone();
    Mock::given(method("POST"))
        .respond_with(move |r: &Request| {
            observed.notify_one();
            if released.load(Ordering::Acquire) {
                ResponseTemplate::new(200).set_body_json(response(r, &base, &[0]))
            } else {
                ResponseTemplate::new(503)
            }
        })
        .expect(2..)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&server)
        .await;
    let mut settings = config(&server);
    settings.max_pending_snapshots = 1;
    settings.max_candidates = 1;
    settings.max_targets = 2;
    settings.max_attempts = 100;
    settings.retry_delay = Duration::from_millis(10);
    settings.request_timeout = Duration::from_secs(2);
    let delivery = Arc::new(BcsDelivery::new(settings, |_| {}).unwrap());
    let handle = delivery.handle();
    let pool = SnapshotPool::new(2, Limits::default());
    let mut files = files(2).into_iter();
    handle
        .try_submit(
            files.next().unwrap(),
            vec![scalar(&pool, 1)],
            vec![proposed(0, UploadKind::Recording, &[0])],
        )
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), prepared.notified())
        .await
        .unwrap();
    let second = files.next().unwrap();
    let snapshot = scalar(&pool, 2);
    let mut waiting = tokio::task::spawn_blocking(move || {
        handle.try_submit(
            second,
            vec![snapshot],
            vec![proposed(0, UploadKind::Recording, &[0])],
        )
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(30), &mut waiting)
            .await
            .is_err()
    );
    assert_eq!(pool.stats().in_use, 2);
    release.store(true, Ordering::Release);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), waiting)
            .await
            .unwrap()
            .unwrap(),
        Ok(())
    );
    assert_eq!(finish(delivery).await, Ok(()));
    assert_eq!(pool.stats().in_use, 0);
}

#[tokio::test]
async fn drain_finishes_multiple_plans_without_cas_blocking_the_recording_lane() {
    let server = MockServer::start().await;
    let base = server.uri();
    let second_recording = Arc::new(AtomicBool::new(false));
    Mock::given(method("POST"))
        .respond_with(move |r: &Request| {
            ResponseTemplate::new(200).set_body_json(response(r, &base, &[]))
        })
        .expect(2)
        .mount(&server)
        .await;
    let observed = second_recording.clone();
    Mock::given(method("PUT"))
        .and(path("/put/1-1"))
        .respond_with(move |_: &Request| {
            if observed.load(Ordering::SeqCst) {
                ResponseTemplate::new(200)
            } else {
                ResponseTemplate::new(503).set_delay(Duration::from_millis(100))
            }
        })
        .expect(1..=2)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/put/1-0"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/put/2-0"))
        .respond_with(move |_: &Request| {
            second_recording.store(true, Ordering::SeqCst);
            ResponseTemplate::new(200)
        })
        .expect(1)
        .mount(&server)
        .await;
    let delivery = Arc::new(BcsDelivery::new(config(&server), |_| {}).unwrap());
    let handle = delivery.handle();
    let mut files = files(2).into_iter();
    let pool = SnapshotPool::new(2, Limits::default());
    handle
        .try_submit(
            files.next().unwrap(),
            vec![scalar(&pool, 1)],
            vec![
                proposed(0, UploadKind::Recording, &[]),
                proposed(1, UploadKind::CasObject, &[0]),
            ],
        )
        .unwrap();
    handle
        .try_submit(
            files.next().unwrap(),
            vec![],
            vec![proposed(0, UploadKind::Recording, &[])],
        )
        .unwrap();
    finish(delivery.clone()).await.unwrap();
    assert_eq!(pool.stats().in_use, 0);
    assert_eq!(delivery.result(), Some(Ok(())));
    let requests = server.received_requests().await.unwrap();
    let sequences: Vec<_> = prepare_requests(&requests)
        .iter()
        .map(|request| request.recording.recording_file_sequence)
        .collect();
    assert_eq!(sequences, [1, 2]);
}

#[tokio::test]
async fn response_and_serialization_byte_limits_drop_only_affected_work() {
    for oversized_response in [true, false] {
        let server = MockServer::start().await;
        let base = server.uri();
        Mock::given(method("POST"))
            .respond_with(move |r: &Request| {
                if oversized_response {
                    ResponseTemplate::new(200).set_body_bytes(vec![b' '; 128 * 1024])
                } else {
                    ResponseTemplate::new(200).set_body_json(response(r, &base, &[]))
                }
            })
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(200))
            .expect(u64::from(!oversized_response))
            .mount(&server)
            .await;
        let delivery = Arc::new(BcsDelivery::new(config(&server), |_| {}).unwrap());
        let pool = SnapshotPool::new(2, Limits::default());
        delivery
            .handle()
            .try_submit(
                files(1).pop().unwrap(),
                vec![large(&pool)],
                vec![
                    proposed(0, UploadKind::Recording, &[]),
                    proposed(1, UploadKind::CasObject, &[0]),
                ],
            )
            .unwrap();
        let error = finish(delivery).await.unwrap_err();
        assert_eq!(
            error,
            if oversized_response {
                DeliveryError::Capacity
            } else {
                DeliveryError::Encoding
            }
        );
        assert_eq!(pool.stats().in_use, 0);
    }
}

#[tokio::test]
async fn permanent_put_failure_is_not_retried_and_draining_releases_queued_owners() {
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("POST"))
        .respond_with(move |r: &Request| {
            ResponseTemplate::new(200).set_body_json(response(r, &base, &[]))
        })
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    let failures = Arc::new(AtomicUsize::new(0));
    let callback = failures.clone();
    let delivery = Arc::new(
        BcsDelivery::new(config(&server), move |_| {
            callback.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap(),
    );
    let pool = SnapshotPool::new(4, Limits::default());
    for (index, file) in files(3).into_iter().enumerate() {
        let result = delivery.handle().try_submit(
            file,
            vec![scalar(&pool, i64::try_from(index).unwrap())],
            vec![proposed(0, UploadKind::Recording, &[0])],
        );
        result.unwrap();
    }
    assert_eq!(finish(delivery).await, Err(DeliveryError::Http));
    assert_eq!(pool.stats().in_use, 0);
    assert_eq!(failures.load(Ordering::SeqCst), 0);
    let requests = server.received_requests().await.unwrap();
    let mut paths = std::collections::HashSet::new();
    for request in requests.iter().filter(|r| r.method == "PUT") {
        assert!(paths.insert(request.url.path()));
    }
    assert_eq!(paths.len(), 3);
}

#[tokio::test]
async fn disabling_cancels_a_blocked_prepare_and_releases_queued_owners_promptly() {
    let server = MockServer::start().await;
    let started = Arc::new(AtomicBool::new(false));
    let observed = started.clone();
    Mock::given(method("POST"))
        .respond_with(move |_: &Request| {
            observed.store(true, Ordering::SeqCst);
            ResponseTemplate::new(200).set_delay(Duration::from_secs(30))
        })
        .expect(1)
        .mount(&server)
        .await;
    let mut settings = config(&server);
    settings.request_timeout = Duration::from_secs(10);
    let delivery = Arc::new(BcsDelivery::new(settings, |_| {}).unwrap());
    let handle = delivery.handle();
    let pool = SnapshotPool::new(4, Limits::default());
    for (index, file) in files(3).into_iter().enumerate() {
        handle
            .try_submit(
                file,
                vec![scalar(&pool, i64::try_from(index).unwrap())],
                vec![proposed(0, UploadKind::Recording, &[0])],
            )
            .unwrap();
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        while !started.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    handle.disable(DeliveryError::Capacity);
    let result = tokio::time::timeout(Duration::from_secs(2), finish(delivery))
        .await
        .unwrap();
    assert_eq!(result, Err(DeliveryError::Capacity));
    assert_eq!(pool.stats().in_use, 0);
}

#[tokio::test]
async fn required_content_type_is_sent_once_and_short_recording_url_uses_actual_lane_work() {
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            let mut plan = response(request, &base, &[]);
            let expiry = u64::try_from(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis(),
            )
            .unwrap()
                + 300_000;
            plan.expires_at_unix_ms = expiry;
            for target in &mut plan.uploads {
                target.expires_at_unix_ms = expiry;
                target.required_headers.insert(
                    "Content-Type".into(),
                    "application/vnd.btel.protobuf".into(),
                );
                target
                    .required_headers
                    .insert("x-amz-content-sha256".into(), "UNSIGNED-PAYLOAD".into());
            }
            ResponseTemplate::new(200).set_body_json(plan)
        })
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let settings = DeliveryConfig {
        allow_http: true,
        ..DeliveryConfig::new(server.uri().parse().unwrap())
    };
    let delivery = Arc::new(BcsDelivery::new(settings, |_| {}).unwrap());
    delivery
        .handle()
        .try_submit(
            files(1).pop().unwrap(),
            vec![],
            vec![proposed(0, UploadKind::Recording, &[])],
        )
        .unwrap();
    finish(delivery).await.unwrap();
    let requests = server.received_requests().await.unwrap();
    let put = requests.iter().find(|r| r.method == "PUT").unwrap();
    let values: Vec<_> = put.headers.get_all("content-type").iter().collect();
    assert_eq!(values.len(), 1);
    assert_eq!(values[0], "application/vnd.btel.protobuf");
    assert_eq!(put.headers["x-amz-content-sha256"], "UNSIGNED-PAYLOAD");
}
