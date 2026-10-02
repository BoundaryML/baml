use std::time::Duration;

use btel_bcs::{
    delivery::{BcsDelivery, DeliveryConfig, DeliveryError},
    proto::CloudUploadEnvelope,
    publisher::{CloudPublisher, CloudPublisherConfig},
};
use btel_processor::{AggregateDelta, Publisher};
use btel_recorder::{RecordingConfig, RecordingId};
use btel_records::SpanRecord;
use btel_snapshot::{Limits, Snapshot, SnapshotPool, SnapshotValue};
use btel_types::{CallPathId, ClockInstant, allocate_telemetry_id};
use prost::Message;
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

#[path = "support/finish.rs"]
mod finish;
#[path = "support/prepare_requests.rs"]
mod prepare_requests;
mod support;
use finish::finish;
use prepare_requests::prepare_requests;

fn capture(publisher: &mut CloudPublisher, pool: &SnapshotPool, value: i64) {
    capture_at(
        publisher,
        pool,
        value,
        allocate_telemetry_id(),
        CallPathId::ROOT,
    );
}

fn capture_at(
    publisher: &mut CloudPublisher,
    pool: &SnapshotPool,
    value: i64,
    thread: btel_types::TelemetryId,
    call_path: CallPathId,
) {
    let snapshot = pool.try_acquire().unwrap().finish(
        SnapshotValue::Int(value),
        &mut btel_snapshot::Shaper::default(),
    );
    publisher.span(
        thread,
        &mut SpanRecord::<Snapshot, Snapshot>::FunctionSpanAnnouncement {
            id: allocate_telemetry_id(),
            parent_id: thread,
            call_path,
            entered_at: ClockInstant::from_ticks(1),
            captured_inputs: Some(snapshot),
        },
    );
}

async fn setup(
    config: CloudPublisherConfig,
) -> (MockServer, BcsDelivery, CloudPublisher, SnapshotPool) {
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            ResponseTemplate::new(200).set_body_json(support::response(request, &base, &[0, 1, 2]))
        })
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let delivery = BcsDelivery::new(
        DeliveryConfig {
            allow_http: true,
            ..DeliveryConfig::new(server.uri().parse().unwrap())
        },
        |_| {},
    )
    .unwrap();
    let publisher = CloudPublisher::new(
        RecordingId::generate(),
        RecordingConfig {
            flush_interval_duration: Duration::from_secs(60),
            ..RecordingConfig::default()
        },
        config,
        delivery.handle(),
    )
    .unwrap();
    (
        server,
        delivery,
        publisher,
        SnapshotPool::new(
            btel_settings::snapshot::MIN_SNAPSHOT_SLOTS,
            Limits::default(),
        ),
    )
}

#[tokio::test]
async fn windows_cross_batches_deduplicate_and_finish_drains_the_final_partial_file() {
    let (server, delivery, mut publisher, pool) = setup(CloudPublisherConfig {
        candidate_target: 3,
        ..CloudPublisherConfig::default()
    })
    .await;
    for values in [&[1, 2][..], &[3, 4, 5], &[1, 6], &[7]] {
        publisher.before_batch(values.len());
        publisher.before_span_chunk(values.len());
        for &value in values {
            capture(&mut publisher, &pool, value);
        }
        publisher.after_batch(values.len());
    }
    assert!(publisher.deadline().is_some());
    publisher.finish();
    assert!(publisher.deadline().is_none());
    publisher.finish();
    finish(delivery).await.unwrap();
    assert_eq!(pool.stats().in_use, 0);
    let requests = server.received_requests().await.unwrap();
    let prepares = prepare_requests(&requests);
    assert_eq!(
        prepares
            .iter()
            .map(|request| (
                request.recording.recording_file_sequence,
                request.candidates.len()
            ))
            .collect::<Vec<_>>(),
        [(1, 3), (2, 3), (3, 1)]
    );
    // The final partial window is the terminal file; finishing twice adds nothing.
    assert_eq!(
        recording_ends(&requests),
        [(1, false), (2, false), (3, true)]
    );
    assert_eq!(
        prepares
            .iter()
            .flat_map(|request| &request.candidates)
            .map(|candidate| &candidate.snapshot_id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        7
    );
}

#[tokio::test]
async fn one_chunk_can_stage_multiple_windows_without_publishing_while_borrowed() {
    let (server, delivery, mut publisher, pool) = setup(CloudPublisherConfig {
        candidate_target: 2,
        ..CloudPublisherConfig::default()
    })
    .await;
    publisher.before_batch(6);
    publisher.before_span_chunk(6);
    for value in 0..6 {
        capture(&mut publisher, &pool, value);
    }
    assert!(server.received_requests().await.unwrap().is_empty());
    assert_eq!(pool.stats().in_use, 6);
    publisher.after_batch(6);
    publisher.finish();
    finish(delivery).await.unwrap();
    assert_eq!(pool.stats().in_use, 0);
    let requests = server.received_requests().await.unwrap();
    let prepares = prepare_requests(&requests);
    // Every window was already sealed: the terminal file carries only the end.
    assert_eq!(
        prepares
            .iter()
            .map(|request| request.candidates.len())
            .collect::<Vec<_>>(),
        [2, 2, 2, 0]
    );
    assert_eq!(
        recording_ends(&requests),
        [(1, false), (2, false), (3, false), (4, true)]
    );
}

/// `(sequence, carries RecordingEnd)` for each uploaded recording file.
fn recording_ends(requests: &[Request]) -> Vec<(u64, bool)> {
    let mut ends: Vec<_> = requests
        .iter()
        .filter(|request| request.method.as_str() == "PUT")
        .filter_map(|request| {
            let envelope = CloudUploadEnvelope::decode(request.body.as_slice()).unwrap();
            let file =
                btel_recorder::proto::RecordingFile::decode(envelope.recording_file?.as_slice())
                    .unwrap();
            Some((file.sequence, file.end.is_some()))
        })
        .collect();
    ends.sort_unstable();
    ends
}

#[tokio::test]
async fn delivery_failure_clears_the_open_window_and_returns_pool_owners() {
    let (server, delivery, mut publisher, pool) = setup(CloudPublisherConfig::default()).await;
    capture(&mut publisher, &pool, 1);
    publisher.after_batch(1);
    assert_eq!(pool.stats().in_use, 1);
    delivery.handle().disable(DeliveryError::Capacity);
    publisher.after_batch(0);
    assert_eq!(pool.stats().in_use, 0);
    assert!(publisher.deadline().is_none());
    publisher.finish();
    assert_eq!(finish(delivery).await, Err(DeliveryError::Capacity));
    // A failed recording is never sealed.
    assert!(recording_ends(&server.received_requests().await.unwrap()).is_empty());
}

#[tokio::test]
async fn lost_recording_replays_metadata_and_reoffers_cas_without_replaying_events() {
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            ResponseTemplate::new(200).set_body_json(support::response(request, &base, &[]))
        })
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(|request: &Request| {
            ResponseTemplate::new(if request.url.path() == "/put/1-0" {
                503
            } else {
                200
            })
        })
        .mount(&server)
        .await;
    let delivery = BcsDelivery::new(
        DeliveryConfig {
            allow_http: true,
            retry_delay: Duration::from_millis(1),
            ..DeliveryConfig::new(server.uri().parse().unwrap())
        },
        |_| panic!("ordinary loss must not disable delivery"),
    )
    .unwrap();
    let handle = delivery.handle();
    let mut publisher = CloudPublisher::new(
        RecordingId::generate(),
        RecordingConfig::default(),
        CloudPublisherConfig::default(),
        handle.clone(),
    )
    .unwrap();
    let pool = SnapshotPool::new(4, Limits::default());
    let thread = allocate_telemetry_id();
    let call_path = CallPathId::new_non_root(1).unwrap();
    let clock = btel_clock::ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run();
    publisher.span(
        thread,
        &mut SpanRecord::ThreadSpanAnnouncement {
            id: thread,
            parent_id: None,
            spawn_call_path: CallPathId::ROOT,
            started_at: ClockInstant::from_ticks(1),
            clock: clock.clone(),
            name: None,
        },
    );
    publisher.span(
        thread,
        &mut SpanRecord::CallPathDefined {
            call_path,
            parent_call_path: CallPathId::ROOT,
            visible_caller: None,
            caller_pc: 0,
            callee: btel_types::FunctionIdAllocator::default()
                .allocate()
                .unwrap(),
            edge: btel_types::CallPathEdge::Synchronous,
        },
    );
    capture_at(&mut publisher, &pool, 42, thread, call_path);
    publisher.aggregate(AggregateDelta {
        count: 7,
        ..AggregateDelta::default()
    });
    publisher.flush();
    tokio::time::timeout(Duration::from_secs(5), async {
        while handle.loss_count() == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    assert!(!handle.is_disabled());
    assert_eq!(pool.stats().in_use, 0);
    capture_at(&mut publisher, &pool, 42, thread, call_path);
    publisher.aggregate(AggregateDelta {
        count: 2,
        ..AggregateDelta::default()
    });
    publisher.flush();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .any(|request| request.method == "PUT" && request.url.path() == "/put/2-0")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    capture_at(&mut publisher, &pool, 42, thread, call_path);
    publisher.flush();
    assert_eq!(finish(delivery).await, Err(DeliveryError::Http));
    assert_eq!(pool.stats().in_use, 0);
    assert_eq!(handle.loss_count(), 1);
    assert_eq!(handle.metadata_replay_evictions(), 0);

    let requests = server.received_requests().await.unwrap();
    let prepares = prepare_requests(&requests);
    assert_eq!(prepares.len(), 3);
    assert_eq!(prepares[0].candidates.len(), 1);
    assert_eq!(prepares[1].candidates.len(), 1);
    assert_eq!(
        prepares[0].candidates[0].snapshot_id,
        prepares[1].candidates[0].snapshot_id
    );
    assert!(
        prepares[2].candidates.is_empty(),
        "successful recent ID is not serialized again"
    );
    let failed_puts: Vec<_> = requests
        .iter()
        .filter(|request| request.method == "PUT" && request.url.path() == "/put/1-0")
        .collect();
    assert_eq!(failed_puts.len(), 4);
    assert!(
        failed_puts
            .iter()
            .all(|request| request.body == failed_puts[0].body)
    );
    let recovered_put = requests
        .iter()
        .find(|request| request.method == "PUT" && request.url.path() == "/put/2-0")
        .unwrap();
    let envelope =
        btel_bcs::proto::CloudUploadEnvelope::decode(recovered_put.body.as_slice()).unwrap();
    assert_eq!(envelope.cas_objects.len(), 1);
    let recovered =
        btel_recorder::proto::RecordingFile::decode(envelope.recording_file.as_deref().unwrap())
            .unwrap();
    let definitions = recovered.definitions.unwrap();
    assert_eq!(definitions.functions.len(), 1);
    assert_eq!(definitions.call_paths.len(), 1);
    assert_eq!(definitions.call_paths[0].call_path_id, call_path.get());
    assert_eq!(definitions.threads.len(), 1);
    assert_eq!(definitions.threads[0].thread_id, thread.get());
    assert_eq!(definitions.clock_epochs.len(), 1);
    assert_eq!(recovered.clock_states.unwrap().states.len(), 1);
    assert_eq!(
        recovered
            .spans
            .unwrap()
            .sections
            .iter()
            .map(|section| section.events.len())
            .sum::<usize>(),
        1
    );
    assert_eq!(recovered.aggregates.unwrap().entries[0].count, 2);
}

/// A list of `count` distinct strings, each in a blob of its own.
fn cut_list(pool: &SnapshotPool, count: usize) -> Snapshot {
    let mut b = pool.try_acquire().unwrap();
    let element_type = b.leaves().ty(btel_snapshot::OwnedType::string());
    let list = b.list(element_type, 0..count, |leaves, index| {
        let text = format!("{index}-{}", "x".repeat(100));
        leaves.string_value(&text.as_str().into())
    });
    let list = b.leaves().object(list).unwrap();
    let mut shaper = btel_snapshot::Shaper::new(btel_snapshot::ShapePolicy::Split {
        unit_bytes: 1 << 20,
        leaf_bytes: 64,
    });
    b.finish(SnapshotValue::Object(list), &mut shaper)
}

#[tokio::test]
async fn a_capture_spanning_plans_delivers_every_blob_once_and_ends_last() {
    let (server, delivery, mut publisher, pool) = setup(CloudPublisherConfig {
        candidate_target: 4,
        ..CloudPublisherConfig::default()
    })
    .await;
    // `setup` uses the default delivery limits.
    let plan = DeliveryConfig::new(server.uri().parse().unwrap()).max_candidates;
    let snapshot = cut_list(&pool, 2 * plan + 2);
    let expected: std::collections::HashSet<_> = snapshot.blobs().map(|blob| blob.id()).collect();
    assert_eq!(expected.len(), 2 * plan + 3);
    publisher.before_batch(1);
    publisher.before_span_chunk(1);
    let thread = allocate_telemetry_id();
    publisher.span(
        thread,
        &mut SpanRecord::<Snapshot, Snapshot>::FunctionSpanAnnouncement {
            id: allocate_telemetry_id(),
            parent_id: thread,
            call_path: CallPathId::ROOT,
            entered_at: ClockInstant::from_ticks(1),
            captured_inputs: Some(snapshot),
        },
    );
    publisher.after_batch(1);
    publisher.finish();
    finish(delivery).await.unwrap();
    assert_eq!(
        pool.stats().in_use,
        0,
        "the capture is freed with its last plan"
    );
    let requests = server.received_requests().await.unwrap();
    let prepares = prepare_requests(&requests);
    // Two full plans, then the end file with the remaining three blobs.
    assert_eq!(
        prepares
            .iter()
            .map(|request| (
                request.recording.recording_file_sequence,
                request.candidates.len()
            ))
            .collect::<Vec<_>>(),
        [(1, plan), (2, plan), (3, 3)]
    );
    assert_eq!(
        recording_ends(&requests),
        [(1, false), (2, false), (3, true)]
    );
    // Every blob is offered once; the mock server has the first three of
    // each plan already, and the rest are uploaded once.
    let mut offered = std::collections::HashSet::new();
    let mut wanted = std::collections::HashSet::new();
    for request in &prepares {
        for (index, candidate) in request.candidates.iter().enumerate() {
            let id = btel_snapshot::CasId::from_bytes(
                hex::decode(&candidate.snapshot_id)
                    .unwrap()
                    .try_into()
                    .unwrap(),
            );
            assert!(offered.insert(id), "a blob is offered once");
            if index >= 3 {
                wanted.insert(id);
            }
        }
    }
    assert_eq!(offered, expected);
    let mut uploaded = std::collections::HashSet::new();
    for request in requests.iter().filter(|r| r.method == "PUT") {
        let envelope = CloudUploadEnvelope::decode(request.body.as_slice()).unwrap();
        for object in envelope.cas_objects {
            let id = btel_snapshot::CasId::from_bytes(object.snapshot_id.try_into().unwrap());
            assert!(uploaded.insert(id), "a blob is uploaded once");
        }
    }
    assert_eq!(uploaded, wanted);
}

#[tokio::test]
async fn blobs_a_sealed_file_left_queued_are_delivered_without_another_record() {
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            ResponseTemplate::new(200).set_body_json(support::response(request, &base, &[]))
        })
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let config = DeliveryConfig {
        allow_http: true,
        ..DeliveryConfig::new(server.uri().parse().unwrap())
    };
    let plan = config.max_candidates;
    let delivery = BcsDelivery::new(config, |_| {}).unwrap();
    let mut publisher = CloudPublisher::new(
        RecordingId::generate(),
        RecordingConfig {
            flush_interval_duration: Duration::from_millis(50),
            ..RecordingConfig::default()
        },
        CloudPublisherConfig {
            candidate_target: 4,
            ..CloudPublisherConfig::default()
        },
        delivery.handle(),
    )
    .unwrap();
    let pool = SnapshotPool::new(2, Limits::default());
    let snapshot = cut_list(&pool, 2 * plan + 2);
    let root = snapshot.root_id();
    publisher.before_batch(1);
    publisher.before_span_chunk(1);
    let thread = allocate_telemetry_id();
    publisher.span(
        thread,
        &mut SpanRecord::<Snapshot, Snapshot>::FunctionSpanAnnouncement {
            id: allocate_telemetry_id(),
            parent_id: thread,
            call_path: CallPathId::ROOT,
            entered_at: ClockInstant::from_ticks(1),
            captured_inputs: Some(snapshot),
        },
    );
    publisher.after_batch(1);
    // Two files took a plan's worth each. The last three blobs, the root
    // among them, wait: nothing else is recorded, and the processor only
    // wakes for the deadline.
    let deadline = publisher.deadline().expect("a deadline for queued blobs");
    tokio::time::sleep(deadline.saturating_duration_since(std::time::Instant::now())).await;
    publisher.after_batch(0);
    assert!(publisher.deadline().is_none());
    let uploaded = async {
        while pool.stats().in_use != 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), uploaded)
        .await
        .expect("the capture is let go without waiting for shutdown");
    let requests = server.received_requests().await.unwrap();
    let prepares = prepare_requests(&requests);
    assert_eq!(
        prepares
            .iter()
            .map(|request| (
                request.recording.recording_file_sequence,
                request.candidates.len()
            ))
            .collect::<Vec<_>>(),
        [(1, plan), (2, plan), (3, 3)]
    );
    assert_eq!(
        prepares[2].candidates.last().unwrap().snapshot_id,
        hex::encode(root.as_bytes())
    );
    assert_eq!(
        recording_ends(&requests),
        [(1, false), (2, false), (3, false)]
    );
    publisher.finish();
    finish(delivery).await.unwrap();
}

/// Lists of two strings of `length`, distinct per `seed`: two leaf blobs and
/// a root.
fn two_strings(pool: &SnapshotPool, seed: usize, length: usize) -> Snapshot {
    let mut b = pool.try_acquire().unwrap();
    let element_type = b.leaves().ty(btel_snapshot::OwnedType::string());
    let list = b.list(element_type, 0..2, |leaves, index| {
        let text = format!("{seed}-{index}-{}", "x".repeat(length));
        leaves.string_value(&text.as_str().into())
    });
    let list = b.leaves().object(list).unwrap();
    b.finish(
        SnapshotValue::Object(list),
        &mut btel_snapshot::Shaper::default(),
    )
}

#[tokio::test]
async fn captures_over_every_byte_budget_are_sent_whole_and_none_is_dropped() {
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            ResponseTemplate::new(200).set_body_json(support::response(request, &base, &[]))
        })
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let delivery = BcsDelivery::new(
        DeliveryConfig {
            allow_http: true,
            // Less than the strings of any capture below: every plan that
            // carries them is admitted alone.
            cas_reserved_bytes: 16 * 1024,
            ..DeliveryConfig::new(server.uri().parse().unwrap())
        },
        |_| panic!("a large capture must not disable delivery"),
    )
    .unwrap();
    let mut publisher = CloudPublisher::new(
        RecordingId::generate(),
        RecordingConfig {
            flush_interval_duration: Duration::from_secs(60),
            ..RecordingConfig::default()
        },
        CloudPublisherConfig {
            // Less than any capture retains.
            max_retained_bytes: 1,
            retained_bytes_target: 1,
            ..CloudPublisherConfig::default()
        },
        delivery.handle(),
    )
    .unwrap();
    let pool = SnapshotPool::new(
        btel_settings::snapshot::MIN_SNAPSHOT_SLOTS,
        Limits::default(),
    );
    let mut expected = std::collections::HashMap::new();
    let mut scratch = btel_snapshot::BlobScratch::default();
    let mut captures: Vec<_> = (0..3)
        .map(|seed| two_strings(&pool, seed, 64 * 1024))
        .collect();
    // A bigint's memory is an estimate; its capture is offered all the same.
    let mut b = pool.try_acquire().unwrap();
    let big = b.leaves().bigint(&std::sync::Arc::new(
        num_bigint::BigInt::from(7) << 4096_u32,
    ));
    captures.push(b.finish(big, &mut btel_snapshot::Shaper::default()));
    for snapshot in &captures {
        for blob in snapshot.blobs() {
            let mut bytes = Vec::new();
            blob.write(&mut scratch, &mut bytes).unwrap();
            expected.insert(blob.id(), bytes);
        }
    }
    assert_eq!(expected.len(), 3 * 3 + 1);
    // As the processor hands over detached records: the publisher may send
    // what it holds before each.
    publisher.before_batch(captures.len());
    publisher.before_span_chunk(captures.len());
    let thread = allocate_telemetry_id();
    for snapshot in captures {
        let mut record = SpanRecord::<Snapshot, Snapshot>::FunctionSpanAnnouncement {
            id: allocate_telemetry_id(),
            parent_id: thread,
            call_path: CallPathId::ROOT,
            entered_at: ClockInstant::from_ticks(1),
            captured_inputs: Some(snapshot),
        };
        publisher.before_detached_span(&record);
        publisher.span(thread, &mut record);
        publisher.after_detached_span();
    }
    publisher.after_batch(4);
    publisher.finish();
    let handle = delivery.handle();
    assert_eq!(finish(delivery).await, Ok(()));
    assert_eq!(handle.loss_count(), 0);
    assert_eq!(pool.stats().in_use, 0);
    let requests = server.received_requests().await.unwrap();
    let mut uploaded = std::collections::HashMap::new();
    for request in requests.iter().filter(|r| r.method == "PUT") {
        let envelope = CloudUploadEnvelope::decode(request.body.as_slice()).unwrap();
        for object in envelope.cas_objects {
            let id = btel_snapshot::CasId::from_bytes(object.snapshot_id.try_into().unwrap());
            assert!(
                uploaded.insert(id, object.blob).is_none(),
                "a blob is uploaded once"
            );
        }
    }
    assert_eq!(uploaded, expected);
    // A string is sent from where it is held, so its length cuts no
    // window: each capture went whole, in a plan of its own.
    let mut plans: Vec<_> = prepare_requests(&requests)
        .iter()
        .map(|request| request.candidates.len())
        .filter(|candidates| *candidates > 0)
        .collect();
    plans.sort_unstable();
    assert_eq!(plans, [1, 3, 3, 3]);
}

#[tokio::test]
async fn strings_sent_with_one_plan_are_let_go_while_their_capture_waits_for_the_next() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    let server = MockServer::start().await;
    let base = server.uri();
    // The server has the first sixteen blobs of every plan.
    let skips: Vec<u32> = (0..16).collect();
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            ResponseTemplate::new(200).set_body_json(support::response(request, &base, &skips))
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
    let delivery = BcsDelivery::new(
        DeliveryConfig {
            allow_http: true,
            // Many short attempts: the upload is retried until released, and
            // its URL outlives them all.
            max_attempts: 1000,
            request_timeout: Duration::from_millis(200),
            retry_delay: Duration::from_millis(10),
            ..DeliveryConfig::new(server.uri().parse().unwrap())
        },
        |_| {},
    )
    .unwrap();
    let plan = delivery.handle();
    let mut publisher = CloudPublisher::new(
        RecordingId::generate(),
        RecordingConfig {
            flush_interval_duration: Duration::from_secs(60),
            ..RecordingConfig::default()
        },
        CloudPublisherConfig::default(),
        delivery.handle(),
    )
    .unwrap();
    let pool = SnapshotPool::new(1, Limits::default());
    // Forty strings, each a blob of its own, and the list that names them:
    // more than the thirty-two blobs one plan takes.
    let texts: Vec<_> = (0..40)
        .map(|index| btel_snapshot::BexStr::from(format!("{index}-{}", "x".repeat(200))))
        .collect();
    let backing: Vec<_> = texts
        .iter()
        .map(|text| match text {
            btel_snapshot::BexStr::Flat(flat) => Arc::downgrade(flat),
            other => panic!("expected a heap-backed string, got {other:?}"),
        })
        .collect();
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
    assert_eq!(snapshot.blobs().len(), 41);
    publisher.before_batch(1);
    publisher.before_span_chunk(1);
    let thread = allocate_telemetry_id();
    publisher.span(
        thread,
        &mut SpanRecord::<Snapshot, Snapshot>::FunctionSpanAnnouncement {
            id: allocate_telemetry_id(),
            parent_id: thread,
            call_path: CallPathId::ROOT,
            entered_at: ClockInstant::from_ticks(1),
            captured_inputs: Some(snapshot),
        },
    );
    publisher.after_batch(1);
    // The first plan is assembled and its upload is held back. The second
    // has not been sealed: the capture waits with eight strings and its root.
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    let alive = |range: std::ops::Range<usize>| {
        backing[range]
            .iter()
            .filter(|backing| backing.upgrade().is_some())
            .count()
    };
    assert_eq!(alive(0..16), 0, "the server had these");
    assert_eq!(alive(16..32), 0, "their blobs were written");
    assert_eq!(alive(32..40), 8, "these wait for the next file");
    assert_eq!(pool.stats().in_use, 1, "the root is still to be written");
    assert_eq!(plan.loss_count(), 0);
    release.store(true, Ordering::Release);
    publisher.finish();
    assert_eq!(finish(delivery).await, Ok(()));
    assert_eq!(alive(0..40), 0);
    assert_eq!(pool.stats().in_use, 0);
}
