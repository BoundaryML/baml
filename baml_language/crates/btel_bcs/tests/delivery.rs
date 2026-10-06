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
use btel_snapshot::{Limits, Snapshot, SnapshotPool, SnapshotValue};
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
        max_pending_plans: 4,
        max_candidates: 8,
        max_targets: 8,
        max_recording_body_bytes: 4096,
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
            publisher.flush_recording().unwrap();
        }
    }
    assert_eq!(files.len(), count);
    files
}

fn scalar(pool: &SnapshotPool, value: i64) -> Snapshot {
    pool.try_acquire().unwrap().finish(
        SnapshotValue::Int(value),
        &mut btel_snapshot::Shaper::default(),
    )
}

fn large(pool: &SnapshotPool) -> Snapshot {
    let mut builder = pool.try_acquire().unwrap();
    let bytes = vec![1; 64 * 1024];
    let object = builder.bytes(&bytes);
    let id = builder.leaves().object(object).unwrap();
    builder.finish(
        SnapshotValue::Object(id),
        &mut btel_snapshot::Shaper::default(),
    )
}

fn proposed(id: u32, kind: UploadKind, members: &[u32]) -> ProposedUploadTarget {
    ProposedUploadTarget {
        client_target_id: id,
        kind,
        candidate_indices: members.to_vec(),
    }
}

#[tokio::test]
async fn refused_ingestion_disables_delivery_without_repeated_requests() {
    for status in [401, 403] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status))
            .expect(1)
            .mount(&server)
            .await;
        let disabled = Arc::new(AtomicBool::new(false));
        let callback = disabled.clone();
        let delivery = Arc::new(
            BcsDelivery::new(config(&server), move |error| {
                assert_eq!(error, DeliveryError::Unauthorized);
                callback.store(true, Ordering::SeqCst);
            })
            .unwrap(),
        );
        let handle = delivery.handle();
        handle
            .try_submit(
                files(1).pop().unwrap(),
                vec![],
                vec![proposed(0, UploadKind::Recording, &[])],
            )
            .unwrap();
        assert_eq!(finish(delivery).await, Err(DeliveryError::Unauthorized));
        assert!(handle.is_disabled());
        assert!(disabled.load(Ordering::SeqCst));
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
        handle.disable(fatal.clone());
        assert_eq!(
            handle.try_submit(
                files(1).pop().unwrap(),
                vec![],
                vec![proposed(0, UploadKind::Recording, &[])],
            ),
            Err(fatal.clone())
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
            s.root_blob()
                .write(&mut btel_snapshot::BlobScratch::default(), &mut bytes)
                .unwrap();
            (s.root_id().as_bytes().to_vec(), bytes)
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
            assert_eq!(object.snapshot_format_version, 3);
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
async fn response_and_recording_body_byte_limits_drop_only_affected_work() {
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
            .expect(0)
            .mount(&server)
            .await;
        let delivery = Arc::new(BcsDelivery::new(config(&server), |_| {}).unwrap());
        let pool = SnapshotPool::new(2, Limits::default());
        // A recording body has a limit, and a blob placed in it counts.
        delivery
            .handle()
            .try_submit(
                files(1).pop().unwrap(),
                vec![large(&pool)],
                vec![proposed(0, UploadKind::Recording, &[0])],
            )
            .unwrap();
        assert_eq!(finish(delivery).await, Err(DeliveryError::Capacity));
        assert_eq!(pool.stats().in_use, 0);
    }
}

/// The CAS blobs the server received, by upload path.
async fn uploaded_blobs(server: &MockServer) -> Vec<(String, Vec<u8>)> {
    let mut blobs = Vec::new();
    for request in server.received_requests().await.unwrap() {
        if request.method == "PUT" {
            let envelope = CloudUploadEnvelope::decode(request.body.as_slice()).unwrap();
            for object in envelope.cas_objects {
                assert_eq!(object.blob_sha256, Sha256::digest(&object.blob).to_vec());
                blobs.push((request.url.path().to_owned(), object.blob));
            }
        }
    }
    blobs
}

#[tokio::test]
async fn a_blob_larger_than_every_byte_limit_uploads_whole() {
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("POST"))
        .respond_with(move |r: &Request| {
            ResponseTemplate::new(200).set_body_json(response(r, &base, &[]))
        })
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&server)
        .await;
    let mut settings = config(&server);
    // Less than the blob, let alone its capture and both sides of its body.
    settings.cas_reserved_bytes = 4096;
    let delivery = Arc::new(BcsDelivery::new(settings, |_| {}).unwrap());
    let pool = SnapshotPool::new(1, Limits::default());
    let snapshot = large(&pool);
    let mut expected = Vec::new();
    snapshot
        .root_blob()
        .write(&mut btel_snapshot::BlobScratch::default(), &mut expected)
        .unwrap();
    assert!(expected.len() > 64 * 1024);
    delivery
        .handle()
        .try_submit(
            files(1).pop().unwrap(),
            vec![snapshot],
            vec![
                proposed(0, UploadKind::Recording, &[]),
                proposed(1, UploadKind::CasObject, &[0]),
            ],
        )
        .unwrap();
    assert_eq!(finish(delivery).await, Ok(()));
    assert_eq!(pool.stats().in_use, 0);
    assert_eq!(
        uploaded_blobs(&server).await,
        [("/put/1-1".to_owned(), expected)]
    );
}

/// A server that takes every upload but those to `held`, which it answers
/// with 503 until released. The notifier fires when a held upload arrives.
async fn holding(
    held: &'static [&'static str],
) -> (MockServer, Arc<tokio::sync::Notify>, Arc<AtomicBool>) {
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
        .respond_with(move |request: &Request| {
            if !held.contains(&request.url.path()) || released.load(Ordering::Acquire) {
                return ResponseTemplate::new(200);
            }
            observed.notify_one();
            ResponseTemplate::new(503)
        })
        .mount(&server)
        .await;
    (server, entered, release)
}

/// The paths of the uploads the server has answered.
async fn upload_paths(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.method == "PUT")
        .map(|request| request.url.path().to_owned())
        .collect()
}

fn object(pool: &SnapshotPool, large_capture: bool) -> (Vec<Snapshot>, Vec<ProposedUploadTarget>) {
    let snapshot = if large_capture {
        large(pool)
    } else {
        scalar(pool, 1)
    };
    let targets = vec![
        proposed(0, UploadKind::Recording, &[]),
        proposed(1, UploadKind::CasObject, &[0]),
    ];
    (vec![snapshot], targets)
}

#[tokio::test]
async fn a_plan_over_the_cas_reservation_holds_up_only_another_such_plan() {
    let (server, entered, release) = holding(&["/put/1-1"]).await;
    let mut settings = config(&server);
    settings.max_attempts = 1000;
    settings.retry_delay = Duration::from_millis(10);
    // Room for a small capture's plan, not for a large one's.
    settings.cas_reserved_bytes = 32 * 1024;
    let delivery = Arc::new(BcsDelivery::new(settings, |_| {}).unwrap());
    let handle = delivery.handle();
    let pool = SnapshotPool::new(2, Limits::default());
    // A storage that held a large capture keeps its capacity, and is
    // charged for it: the small capture takes a storage of its own.
    let small = SnapshotPool::new(1, Limits::default());
    let mut files = files(4).into_iter();
    // Nothing else is held: the large plan goes at once, beside the budget.
    let (snapshots, targets) = object(&pool, true);
    handle
        .try_submit(files.next().unwrap(), snapshots, targets)
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    // While its upload is held back, plans that fit come and go: one with a
    // capture, and one with none.
    let (snapshots, targets) = object(&small, false);
    handle
        .try_submit(files.next().unwrap(), snapshots, targets)
        .unwrap();
    handle
        .try_submit(
            files.next().unwrap(),
            vec![],
            vec![proposed(0, UploadKind::Recording, &[])],
        )
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !upload_paths(&server)
            .await
            .iter()
            .any(|path| path == "/put/3-0")
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let paths = upload_paths(&server).await;
    assert!(paths.iter().any(|path| path == "/put/2-1"));
    // A second large plan waits for the first.
    let (snapshots, targets) = object(&pool, true);
    let last = files.next().unwrap();
    let producer = handle.clone();
    let mut waiting =
        tokio::task::spawn_blocking(move || producer.try_submit(last, snapshots, targets));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut waiting)
            .await
            .is_err()
    );
    assert_eq!(handle.loss_count(), 0);
    release.store(true, Ordering::Release);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), waiting)
            .await
            .unwrap()
            .unwrap(),
        Ok(())
    );
    assert_eq!(finish(delivery).await, Ok(()));
    assert_eq!(pool.stats().in_use + small.stats().in_use, 0);
    let paths = upload_paths(&server).await;
    for expected in ["/put/1-1", "/put/2-1", "/put/4-1"] {
        assert!(paths.iter().any(|path| path == expected), "{expected}");
    }
}

#[tokio::test]
async fn a_long_cas_upload_does_not_keep_its_plan_pending() {
    let (server, entered, release) = holding(&["/put/1-1"]).await;
    let mut settings = config(&server);
    settings.max_attempts = 1000;
    settings.retry_delay = Duration::from_millis(10);
    // The next plan is admitted only when this one is no longer pending.
    settings.max_pending_plans = 1;
    let delivery = Arc::new(BcsDelivery::new(settings, |_| {}).unwrap());
    let handle = delivery.handle();
    let pool = SnapshotPool::new(1, Limits::default());
    let mut files = files(2).into_iter();
    let (snapshots, targets) = object(&pool, false);
    handle
        .try_submit(files.next().unwrap(), snapshots, targets)
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    let second = files.next().unwrap();
    let producer = handle.clone();
    let waiting = tokio::task::spawn_blocking(move || {
        producer.try_submit(
            second,
            vec![],
            vec![proposed(0, UploadKind::Recording, &[])],
        )
    });
    // The first plan's recording is uploaded; its blob is not.
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), waiting)
            .await
            .unwrap()
            .unwrap(),
        Ok(())
    );
    assert!(!release.load(Ordering::Acquire));
    release.store(true, Ordering::Release);
    assert_eq!(finish(delivery).await, Ok(()));
    assert_eq!(handle.loss_count(), 0);
}

#[tokio::test]
async fn cas_bodies_upload_side_by_side_up_to_the_slot_count() {
    for slots in [1, 2] {
        let (server, entered, release) = holding(&["/put/1-1"]).await;
        let mut settings = config(&server);
        settings.max_attempts = 1000;
        settings.retry_delay = Duration::from_millis(10);
        settings.max_cas_uploads = slots;
        let delivery = Arc::new(BcsDelivery::new(settings, |_| {}).unwrap());
        let pool = SnapshotPool::new(2, Limits::default());
        delivery
            .handle()
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
        tokio::time::timeout(Duration::from_secs(2), entered.notified())
            .await
            .unwrap();
        // The first blob's upload is held back. With a second slot the
        // other goes meanwhile; with one it waits its turn.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let passed = upload_paths(&server)
            .await
            .iter()
            .any(|path| path == "/put/1-2");
        assert_eq!(passed, slots == 2);
        release.store(true, Ordering::Release);
        assert_eq!(finish(delivery).await, Ok(()));
        assert!(
            upload_paths(&server)
                .await
                .iter()
                .any(|path| path == "/put/1-2")
        );
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

/// A plan with one file and one 64 KiB blob uploaded on its own, against a
/// server that takes `delay` to accept the blob and lets its URL live for
/// `lifetime`.
async fn large_upload(
    delay: Duration,
    lifetime: Duration,
    min_upload_bytes_per_second: usize,
) -> (Result<(), DeliveryError>, Vec<String>) {
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            let mut plan = response(request, &base, &[]);
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            let expiry = u64::try_from((now + lifetime).as_millis()).unwrap();
            for target in &mut plan.uploads {
                if target.kind == UploadKind::CasObject {
                    target.expires_at_unix_ms = expiry;
                }
            }
            ResponseTemplate::new(200).set_body_json(plan)
        })
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/put/1-0"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/put/1-1"))
        .respond_with(ResponseTemplate::new(200).set_delay(delay))
        .mount(&server)
        .await;
    let mut settings = config(&server);
    settings.request_timeout = Duration::from_millis(100);
    settings.min_upload_bytes_per_second = min_upload_bytes_per_second;
    let delivery = Arc::new(BcsDelivery::new(settings, |_| {}).unwrap());
    let pool = SnapshotPool::new(1, Limits::default());
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
    let result = finish(delivery).await;
    assert_eq!(pool.stats().in_use, 0);
    let puts = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.method == "PUT")
        .map(|request| request.url.path().to_owned())
        .collect();
    (result, puts)
}

#[tokio::test]
async fn a_slow_upload_is_given_time_for_its_body_and_no_more() {
    let hour = Duration::from_secs(3600);
    let slow = Duration::from_millis(500);
    // 64 KiB at 64 KiB a second: a second on top of the request timeout.
    let (result, puts) = large_upload(slow, hour, 64 * 1024).await;
    assert_eq!(result, Ok(()));
    assert_eq!(puts.len(), 2);
    // At a rate that gives the body no time of its own, each attempt at the
    // blob times out; the recording still arrives.
    let (result, puts) = large_upload(slow, hour, usize::MAX).await;
    assert_eq!(result, Err(DeliveryError::Http));
    assert_eq!(
        puts.iter().filter(|path| *path == "/put/1-1").count(),
        2,
        "one attempt and one retry"
    );
    assert!(puts.contains(&"/put/1-0".to_owned()));
}

#[tokio::test]
async fn an_upload_url_must_outlive_the_window_of_its_body() {
    let minute = Duration::from_secs(60);
    // Two attempts of 64 KiB at 1 KiB a second can hold the lane for over
    // two minutes: the URL is refused before anything is sent to it.
    let (result, puts) = large_upload(Duration::ZERO, minute, 1024).await;
    assert_eq!(result, Err(DeliveryError::Expired));
    assert_eq!(puts, ["/put/1-0"]);
    // At a megabyte a second the same URL lives long enough.
    let (result, puts) = large_upload(Duration::ZERO, minute, 1 << 20).await;
    assert_eq!(result, Ok(()));
    assert_eq!(puts.len(), 2);
}

#[tokio::test]
async fn a_long_string_is_held_until_its_upload_ends_and_the_capture_only_until_assembly() {
    let server = MockServer::start().await;
    let base = server.uri();
    // The server has the first string already.
    Mock::given(method("POST"))
        .respond_with(move |r: &Request| {
            ResponseTemplate::new(200).set_body_json(response(r, &base, &[0]))
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
    settings.max_attempts = 1000;
    settings.retry_delay = Duration::from_millis(10);
    let delivery = Arc::new(BcsDelivery::new(settings, |_| {}).unwrap());
    let pool = SnapshotPool::new(1, Limits::default());
    let texts = ["a", "b"].map(|letter| btel_snapshot::BexStr::from(letter.repeat(10_000)));
    let backing = texts.each_ref().map(|text| match text {
        btel_snapshot::BexStr::Flat(flat) => Arc::downgrade(flat),
        other => panic!("expected a heap-backed string, got {other:?}"),
    });
    let mut b = pool.try_acquire().unwrap();
    let element_type = b.leaves().ty(btel_snapshot::OwnedType::string());
    let list = b.list(element_type, texts.iter(), |leaves, text| {
        leaves.string_value(text)
    });
    let list = b.leaves().object(list).unwrap();
    let mut shaper = btel_snapshot::Shaper::new(btel_snapshot::ShapePolicy::Split {
        unit_bytes: 1 << 20,
        leaf_bytes: 64,
    });
    let snapshot = b.finish(SnapshotValue::Object(list), &mut shaper);
    drop(texts);
    assert_eq!(snapshot.blobs().len(), 3);
    let mut wanted = Vec::new();
    snapshot
        .blobs()
        .nth(1)
        .unwrap()
        .write(&mut btel_snapshot::BlobScratch::default(), &mut wanted)
        .unwrap();
    delivery
        .handle()
        .try_submit(
            files(1).pop().unwrap(),
            vec![snapshot],
            vec![
                proposed(0, UploadKind::Recording, &[2]),
                proposed(1, UploadKind::CasObject, &[0]),
                proposed(2, UploadKind::CasObject, &[1]),
            ],
        )
        .unwrap();
    // The first upload is under way and held back: every body is built.
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    assert!(backing[0].upgrade().is_none(), "the server had it");
    assert!(backing[1].upgrade().is_some(), "its body sends it");
    assert_eq!(pool.stats().in_use, 0, "nothing more is written from it");
    release.store(true, Ordering::Release);
    assert_eq!(finish(delivery).await, Ok(()));
    assert!(backing[1].upgrade().is_none(), "its upload is over");
    let uploaded = uploaded_blobs(&server).await;
    assert!(uploaded.iter().all(|(path, _)| path != "/put/1-1"));
    // Sent in pieces, received as the blob it is.
    assert!(
        uploaded
            .iter()
            .any(|(path, blob)| path == "/put/1-2" && *blob == wanted)
    );
}

#[tokio::test]
async fn a_short_part_of_a_long_string_goes_with_its_recording() {
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("POST"))
        .respond_with(move |r: &Request| {
            ResponseTemplate::new(200).set_body_json(response(r, &base, &[]))
        })
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&server)
        .await;
    let mut settings = config(&server);
    // Room for a recording, not for what the long string retains.
    settings.recording_reserved_bytes = 5 * 1024 * 1024;
    let delivery = Arc::new(BcsDelivery::new(settings, |_| {}).unwrap());
    let handle = delivery.handle();
    let pool = SnapshotPool::new(1, Limits::default());
    let document = btel_snapshot::BexStr::from("d".repeat(8 * 1024 * 1024));
    // One part short enough to be copied into the body, one sent from the
    // document's own memory: each holds all of it.
    let mut sent = Vec::new();
    for (file, end) in files(2).into_iter().zip([1000, 3000]) {
        let part = document.substring(0, end);
        let mut b = pool.try_acquire().unwrap();
        let value = b.leaves().string_value(&part);
        let snapshot = b.finish(value, &mut btel_snapshot::Shaper::default());
        let mut blob = Vec::new();
        snapshot
            .root_blob()
            .write(&mut btel_snapshot::BlobScratch::default(), &mut blob)
            .unwrap();
        sent.push(blob);
        handle
            .try_submit(
                file,
                vec![snapshot],
                vec![proposed(0, UploadKind::Recording, &[0])],
            )
            .unwrap();
    }
    assert_eq!(finish(delivery).await, Ok(()));
    assert_eq!(handle.loss_count(), 0);
    let uploaded: Vec<_> = uploaded_blobs(&server)
        .await
        .into_iter()
        .map(|(_, blob)| blob)
        .collect();
    assert_eq!(uploaded, sent);
}
