use std::time::Duration;

use btel_bcs::{
    delivery::{BcsDelivery, DeliveryConfig, DeliveryError},
    publisher::{CloudPublisher, PublisherConfig},
};
use btel_processor::{AggregateDelta, Publisher};
use btel_publisher::{RecordingConfig, RecordingId};
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
    let snapshot = pool
        .try_acquire()
        .unwrap()
        .finish_value(SnapshotValue::Int(value));
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

async fn setup(config: PublisherConfig) -> (MockServer, BcsDelivery, CloudPublisher, SnapshotPool) {
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
            prepare_base_url: server.uri(),
            allow_http: true,
            ..DeliveryConfig::default()
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
    let (server, delivery, mut publisher, pool) = setup(PublisherConfig {
        snapshot_target: 3,
        ..PublisherConfig::default()
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
    let (server, delivery, mut publisher, pool) = setup(PublisherConfig {
        snapshot_target: 2,
        ..PublisherConfig::default()
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
    assert_eq!(prepares.len(), 3);
    assert!(prepares.iter().all(|request| request.candidates.len() == 2));
}

#[tokio::test]
async fn delivery_failure_clears_the_open_window_and_returns_pool_owners() {
    let (_server, delivery, mut publisher, pool) = setup(PublisherConfig::default()).await;
    capture(&mut publisher, &pool, 1);
    publisher.after_batch(1);
    assert_eq!(pool.stats().in_use, 1);
    delivery.handle().disable(DeliveryError::Capacity);
    publisher.after_batch(0);
    assert_eq!(pool.stats().in_use, 0);
    assert!(publisher.deadline().is_none());
    publisher.finish();
    assert_eq!(finish(delivery).await, Err(DeliveryError::Capacity));
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
            prepare_base_url: server.uri(),
            allow_http: true,
            retry_delay: Duration::from_millis(1),
            ..DeliveryConfig::default()
        },
        |_| panic!("ordinary loss must not disable delivery"),
    )
    .unwrap();
    let handle = delivery.handle();
    let mut publisher = CloudPublisher::new(
        RecordingId::generate(),
        RecordingConfig::default(),
        PublisherConfig::default(),
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
        btel_publisher::proto::RecordingFile::decode(envelope.recording_file.as_deref().unwrap())
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
