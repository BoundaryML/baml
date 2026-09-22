use std::time::Duration;

use btel_bcs::{
    delivery::{BcsDelivery, DeliveryConfig, DeliveryError},
    publisher::{CloudPublisher, PublisherConfig},
    wire::PrepareUploadsRequest,
};
use btel_processor::Publisher;
use btel_publisher::{RecordingConfig, RecordingId};
use btel_records::SpanRecord;
use btel_snapshot::{Limits, Snapshot, SnapshotPool, SnapshotValue};
use btel_types::{CallPathId, ClockInstant, allocate_telemetry_id};
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

mod support;

fn capture(publisher: &mut CloudPublisher, pool: &SnapshotPool, value: i64) {
    let thread = allocate_telemetry_id();
    let snapshot = pool
        .try_acquire()
        .unwrap()
        .finish_value(SnapshotValue::Int(value));
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

async fn finish(delivery: BcsDelivery) -> Result<(), DeliveryError> {
    tokio::task::spawn_blocking(move || delivery.finish())
        .await
        .unwrap()
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
    let prepares: Vec<PrepareUploadsRequest> = requests
        .iter()
        .filter(|request| request.method == "POST")
        .map(|request| serde_json::from_slice(&request.body).unwrap())
        .collect();
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
    let prepares: Vec<PrepareUploadsRequest> = requests
        .iter()
        .filter(|request| request.method == "POST")
        .map(|request| serde_json::from_slice(&request.body).unwrap())
        .collect();
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
