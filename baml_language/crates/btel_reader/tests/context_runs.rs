use std::{num::NonZeroUsize, sync::Arc};

use bex_chunkedringbuffer::{ChunkPool, Config};
use btel_clock::{ClockMode, ClockRuntime};
use btel_file::{LocalDelivery, LocalDeliveryConfig, LocalPublisher};
use btel_processor::Processor;
use btel_reader::{
    cas::{CasLimits, CasOutcome, CasStore},
    context::{ContextReference, reference},
};
use btel_recorder::{RecordingBuilder, RecordingConfig, RecordingId};
use btel_records::{SpanRecord, TimingRecord};
use btel_snapshot::{DecodedObject, DecodedRoot, DecodedValue, Limits, Snapshot, SnapshotPool};
use btel_types::{
    CallPathId, ClockInstant, allocate_telemetry_id,
    context::{Context, ContextPatch, ContextValue},
};
use prost::Message;

fn nz(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}

#[test]
fn context_runs_reach_local_recordings_and_verified_cas_through_the_span_buffer() {
    let root = tempfile::tempdir().unwrap();
    let recording = RecordingId::generate();
    let delivery =
        LocalDelivery::create(root.path(), recording, LocalDeliveryConfig::default()).unwrap();
    let directory = btel_file::recording_directory(root.path(), recording);
    let builder = RecordingBuilder::new(
        recording,
        RecordingConfig {
            target_bytes: nz(1),
            ..RecordingConfig::default()
        },
    )
    .unwrap();
    let publisher = LocalPublisher::new(builder, delivery);
    let delivery = Arc::clone(publisher.delivery().unwrap());
    let pool = ChunkPool::<TimingRecord, SpanRecord<Snapshot, Snapshot>>::new(Config {
        chunk_capacity: nz(8),
        timing_chunks: nz(1),
        span_chunks: nz(1),
        max_producers: nz(1),
        preallocate: true,
    })
    .unwrap();
    let mut processor = Processor::with_publisher(pool.bind_consumer().unwrap(), nz(8), publisher);
    let mut producer = pool.register_producer().unwrap();
    let thread = allocate_telemetry_id();
    let call_path = CallPathId::new_non_root(1).unwrap();
    let context = Context::default().with_patch(&ContextPatch {
        metadata: [("order_id".into(), Some(ContextValue::String("123".into())))].into(),
        distinct_id: Some("user-42".into()),
    });
    let snapshots = SnapshotPool::new(1, Limits::default());
    let snapshot = btel_snapshot::context::capture(&context, &snapshots).unwrap();
    let id = snapshot.id();
    producer.write_span(SpanRecord::ThreadSelected { thread_id: thread });
    producer.write_span(SpanRecord::ContextSelected {
        captured_context: Some(snapshot),
    });
    producer.write_span(SpanRecord::ThreadSpanAnnouncement {
        id: thread,
        parent_id: None,
        spawn_call_path: CallPathId::ROOT,
        started_at: ClockInstant::from_ticks(0),
        clock: ClockRuntime::new(ClockMode::Monotonic).start_run(),
        name: None,
    });
    producer.write_span(SpanRecord::CallPathDefined {
        call_path,
        parent_call_path: CallPathId::ROOT,
        visible_caller: None,
        caller_pc: 0,
        callee: btel_types::FunctionIdAllocator::default()
            .allocate()
            .unwrap(),
        edge: btel_types::CallPathEdge::Synchronous,
    });
    for _ in 0..2 {
        producer.write_span(SpanRecord::FunctionSpanAnnouncement {
            id: allocate_telemetry_id(),
            parent_id: thread,
            call_path,
            entered_at: ClockInstant::from_ticks(1),
            captured_inputs: None,
        });
    }
    producer.seal();
    processor.process_available();

    producer.write_span(SpanRecord::ThreadSelected { thread_id: thread });
    producer.write_span(SpanRecord::ContextReferenced { id });
    producer.write_span(SpanRecord::FunctionSpanAnnouncement {
        id: allocate_telemetry_id(),
        parent_id: thread,
        call_path,
        entered_at: ClockInstant::from_ticks(2),
        captured_inputs: None,
    });
    producer.seal();
    processor.process_available();

    // A new chunk without a context marker must not inherit the prior selection.
    producer.write_span(SpanRecord::ThreadSelected { thread_id: thread });
    producer.write_span(SpanRecord::FunctionSpanAnnouncement {
        id: allocate_telemetry_id(),
        parent_id: thread,
        call_path,
        entered_at: ClockInstant::from_ticks(2),
        captured_inputs: None,
    });
    producer.write_span(SpanRecord::ContextCleared);
    producer.write_span(SpanRecord::FunctionSpanAnnouncement {
        id: allocate_telemetry_id(),
        parent_id: thread,
        call_path,
        entered_at: ClockInstant::from_ticks(3),
        captured_inputs: None,
    });
    producer.seal();
    processor.process_available();
    drop(producer);
    pool.close_admission();
    assert!(processor.process_available().complete);
    delivery.finish().unwrap();

    let read = btel_file::read_directory(&directory).unwrap();
    assert!(read.issues.is_empty(), "{:?}", read.issues);
    let sections: Vec<_> = read
        .files
        .iter()
        .flat_map(|file| file.spans.iter().flat_map(|spans| &spans.sections))
        .collect();
    assert_eq!(sections.len(), 4);
    assert_eq!(reference(sections[0]), ContextReference::Snapshot(id));
    assert_eq!(sections[0].events.len(), 3);
    assert_eq!(reference(sections[1]), ContextReference::Snapshot(id));
    assert_eq!(reference(sections[2]), ContextReference::Unavailable);
    assert_eq!(reference(sections[3]), ContextReference::Empty);
    assert!(read.files.iter().all(|file| {
        file.header.as_ref().unwrap().format_minor
            == btel_settings::encoding::FORMAT_MINOR
                .max(btel_settings::encoding::CONTEXT_FORMAT_MINOR)
    }));

    let cas = CasStore::new(root.path().join("cas"), CasLimits::default());
    let CasOutcome::Available(snapshot) = cas.load(id).outcome else {
        panic!("context must reach the existing CAS delivery path");
    };
    assert_eq!(snapshot.id, id);
    let DecodedRoot::Value(DecodedValue::Object(root)) = snapshot.root else {
        panic!("context must be a map");
    };
    let DecodedObject::Map { entries, .. } = snapshot.object(root) else {
        panic!("context must be a map");
    };
    assert_eq!(
        entries[0],
        ("distinct_id".into(), DecodedValue::String("user-42".into()))
    );
    let DecodedValue::Object(metadata) = entries[1].1 else {
        panic!("metadata must be a map");
    };
    let DecodedObject::Map { entries, .. } = snapshot.object(metadata) else {
        panic!("metadata must be a map");
    };
    assert_eq!(
        entries,
        &vec![("order_id".into(), DecodedValue::String("123".into()))]
    );
    std::fs::remove_file(cas.path(id)).unwrap();
    assert!(matches!(cas.load(id).outcome, CasOutcome::Missing));
    assert_eq!(reference(sections[0]), ContextReference::Snapshot(id));

    let mut invalid = read.files[0].clone();
    invalid.spans.as_mut().unwrap().sections[0].context = Some(
        btel_recorder::proto::thread_section::Context::EmptyContext(false),
    );
    assert_eq!(
        btel_file::validate_file(&invalid.encode_to_vec(), recording, invalid.sequence)
            .unwrap_err(),
        "invalid empty context marker"
    );
}
