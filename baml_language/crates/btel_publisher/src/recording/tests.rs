use std::cell::RefCell;

use bex_chunkedringbuffer::{ChunkPool, Config};
use btel_clock::{ClockMode, ClockRuntime};
use btel_processor::Processor;
use btel_types::{
    AwaitDuration, CallPathId, CallPathNodeId, ClockDuration, ClockInstant, FunctionIdAllocator,
    InvocationOutcome, allocate_telemetry_id,
};

use super::*;

fn nz(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}
fn config() -> RecordingConfig {
    RecordingConfig {
        target_bytes: nz(1),
        ..RecordingConfig::default()
    }
}

#[test]
fn recording_rejects_unrepresentable_deadline() {
    let config = RecordingConfig {
        flush_interval_duration: std::time::Duration::MAX,
        ..config()
    };
    assert!(matches!(
        RecordingBuilder::new(RecordingId::from_bytes([1; 16]).unwrap(), config),
        Err(RecordingError::InvalidConfig)
    ));
}

#[test]
fn span_reservation_rejects_overflow_and_encoder_limit_without_allocating() {
    for records in [
        encoding::MAX_BUFFER_BYTES / encoding::MAX_EVENT_BYTES + 1,
        usize::MAX / encoding::MAX_EVENT_BYTES + 1,
        usize::MAX,
    ] {
        let mut builder =
            RecordingBuilder::new(RecordingId::from_bytes([1; 16]).unwrap(), config()).unwrap();
        builder.pending_span_reservation = records;
        assert_eq!(builder.admit_spans(), Err(RecordingError::EncodingTooLarge));
        assert_eq!(builder.buffer.spans.capacity(), 0);
        assert_eq!(builder.span_credit, 0);
    }
}

#[test]
fn encoding_reservation_rejects_addition_overflow() {
    let mut builder =
        RecordingBuilder::new(RecordingId::from_bytes([1; 16]).unwrap(), config()).unwrap();
    builder.admit_spans().unwrap();
    builder.buffer.spans.event(
        allocate_telemetry_id(),
        proto::span_event::Event::FunctionCompletion(proto::FunctionCompletion::default()),
    );
    assert_eq!(
        builder.reserve_encoding(usize::MAX),
        Err(RecordingError::EncodingTooLarge)
    );
    assert_eq!(
        builder.reserve_encoding(encoding::MAX_BUFFER_BYTES),
        Err(RecordingError::EncodingTooLarge)
    );
}

fn ids() -> (TelemetryId, btel_types::FunctionId, CallPathId) {
    (
        allocate_telemetry_id(),
        FunctionIdAllocator::default().allocate().unwrap(),
        CallPathId::new_non_root(7).unwrap(),
    )
}
fn metadata(
    publisher: &mut RecordingPublisher<impl FnMut(SealedFile)>,
    thread: TelemetryId,
    function: btel_types::FunctionId,
    path: CallPathId,
) {
    let clock = ClockRuntime::new(ClockMode::Monotonic).start_run();
    publisher.span(
        thread,
        &mut SpanRecord::ThreadSpanAnnouncement {
            id: thread,
            parent_id: None,
            spawn_call_path: CallPathId::ROOT,
            started_at: ClockInstant::from_ticks(1),
            clock,
        },
    );
    publisher.span(
        thread,
        &mut SpanRecord::CallPathDefined {
            call_path: path,
            parent_call_path: CallPathId::ROOT,
            visible_caller: None,
            caller_pc: 3,
            callee: function,
            edge: btel_types::CallPathEdge::Synchronous,
        },
    );
}
fn delta(path: CallPathId) -> AggregateDelta {
    AggregateDelta {
        node: CallPathNodeId::new(path, false),
        count: 2,
        total_duration: ClockDuration::from_ticks(9),
        total_io_duration: AwaitDuration::ZERO,
    }
}
fn decode(file: &SealedFile) -> proto::RecordingFile {
    proto::RecordingFile::decode(file.bytes()).unwrap()
}

#[test]
fn aggregates_seal_before_definitions_without_rewriting_earlier_files() {
    let files = RefCell::new(Vec::new());
    let (thread, function, path) = ids();
    let id = RecordingId::generate();
    let mut p = RecordingPublisher::new(id, config(), |f| files.borrow_mut().push(f)).unwrap();
    p.aggregate(delta(path));
    p.flush();
    let first = decode(&files.borrow()[0]);
    assert_eq!(first.sequence, 1);
    assert_eq!(first.header.unwrap().recording_id, id.as_bytes());
    assert_eq!(first.aggregates.unwrap().entries[0].count, 2);
    assert!(first.definitions.unwrap().call_paths.is_empty());
    let original = files.borrow()[0].bytes().to_vec();
    metadata(&mut p, thread, function, path);
    p.flush();
    let second = decode(&files.borrow()[1]);
    assert_eq!(second.sequence, 2);
    assert_eq!(
        second.definitions.unwrap().call_paths[0].call_path_id,
        path.get()
    );
    assert!(second.aggregates.unwrap().entries.is_empty());
    assert_eq!(files.borrow()[0].bytes(), original);
    p.finish_recording().unwrap();
    assert_eq!(
        files.borrow().len(),
        2,
        "no empty flush or false final marker"
    );
}

#[test]
fn spans_and_announcements_forward_without_parent_or_definition_lookups() {
    let files = RefCell::new(Vec::new());
    let (thread, _, path) = ids();
    let mut p = RecordingPublisher::new(RecordingId::generate(), config(), |f| {
        files.borrow_mut().push(f);
    })
    .unwrap();
    let parent = allocate_telemetry_id();
    let child = allocate_telemetry_id();
    let completion = |id, parent_id| SpanRecord::FunctionSpanCompletionOk {
        id,
        parent_id,
        call_path: path,
        entered_at: ClockInstant::from_ticks(1),
        exited_at: ClockInstant::from_ticks(2),
        await_time: AwaitDuration::ZERO,
        captured_value: None,
    };
    p.span(thread, &mut completion(child, parent));
    p.flush();
    let first = decode(&files.borrow()[0]);
    assert!(first.definitions.unwrap().threads.is_empty());
    let events = first.spans.unwrap().sections.remove(0).events;
    let Some(proto::span_event::Event::FunctionCompletion(child_event)) = &events[0].event else {
        panic!()
    };
    assert_eq!(
        (child_event.id, child_event.parent_id),
        (child.get(), parent.get())
    );
    p.span(
        thread,
        &mut SpanRecord::FunctionSpanAnnouncement {
            id: parent,
            parent_id: thread,
            call_path: path,
            entered_at: ClockInstant::from_ticks(1),
            captured_inputs: Some(snapshot()),
        },
    );
    p.span(thread, &mut completion(parent, thread));
    p.finish_recording().unwrap();
    let events = decode(&files.borrow()[1])
        .spans
        .unwrap()
        .sections
        .remove(0)
        .events;
    assert_eq!(events.len(), 2);
    assert!(
        matches!(&events[0].event, Some(proto::span_event::Event::FunctionAnnouncement(f)) if f.id == parent.get())
    );
    assert!(
        matches!(&events[1].event, Some(proto::span_event::Event::FunctionCompletion(f)) if f.id == parent.get())
    );
    assert_eq!(
        files.borrow().len(),
        2,
        "unresolved thread/path references do not prevent finish"
    );
}

#[test]
fn size_boundary_recycles_the_input_chunk_before_handoff() {
    let pool =
        ChunkPool::<btel_records::TimingRecord, SpanRecord<Snapshot, Snapshot>>::new(Config {
            chunk_capacity: nz(8),
            timing_chunks: nz(1),
            span_chunks: nz(1),
            max_producers: nz(1),
            preallocate: true,
        })
        .unwrap();
    let files = RefCell::new(Vec::new());
    let copy = pool.clone();
    let publisher = RecordingPublisher::new(RecordingId::generate(), config(), |f| {
        assert_eq!(copy.stats().free_chunks, 2);
        files.borrow_mut().push(f);
    })
    .unwrap();
    let mut processor = Processor::with_publisher(pool.bind_consumer().unwrap(), nz(8), publisher);
    let mut producer = pool.register_producer().unwrap();
    let thread = allocate_telemetry_id();
    producer.write_span(SpanRecord::ThreadSelected { thread_id: thread });
    producer.write_span(SpanRecord::ThreadSpanCompletion {
        id: thread,
        parent_id: None,
        spawn_call_path: CallPathId::ROOT,
        started_at: ClockInstant::from_ticks(1),
        completed_at: ClockInstant::from_ticks(2),
        outcome: InvocationOutcome::Ok,
        clock: ClockRuntime::new(ClockMode::Monotonic).start_run(),
    });
    producer.seal();
    processor.process_available();
    assert_eq!(files.borrow().len(), 1);
    drop(producer);
    pool.close_admission();
    assert!(processor.process_available().complete);
}

#[test]
fn deadlines_start_once_do_not_slide_and_do_not_emit_empty_files() {
    let files = RefCell::new(Vec::new());
    let (thread, function, path) = ids();
    let mut p = RecordingPublisher::new(RecordingId::generate(), RecordingConfig::default(), |f| {
        files.borrow_mut().push(f);
    })
    .unwrap();
    metadata(&mut p, thread, function, path);
    p.after_batch(2);
    let deadline = p.builder.deadline.unwrap();
    p.aggregate(delta(path));
    p.after_batch(1);
    assert_eq!(p.builder.deadline, Some(deadline));
    p.service(
        deadline.checked_sub(Duration::from_nanos(1)).unwrap(),
        false,
    )
    .unwrap();
    assert!(files.borrow().is_empty());
    p.service(deadline, false).unwrap();
    assert_eq!(files.borrow().len(), 1);
    p.service(deadline + Duration::from_secs(30), false)
        .unwrap();
    assert_eq!(files.borrow().len(), 1);
}

#[test]
fn sequence_exhaustion_never_wraps_or_reuses_a_file_number() {
    let files = RefCell::new(Vec::new());
    let mut p = RecordingPublisher::new(RecordingId::generate(), config(), |f| {
        files.borrow_mut().push(f);
    })
    .unwrap();
    p.builder.next_sequence = NonZeroU64::new(u64::MAX);
    p.aggregate(delta(CallPathId::new_non_root(999).unwrap()));
    p.finish_recording().unwrap();
    assert_eq!(files.borrow()[0].sequence().get(), u64::MAX);
    p.aggregate(delta(CallPathId::new_non_root(999).unwrap()));
    assert_eq!(p.finish_recording(), Err(RecordingError::SequenceExhausted));
    assert_eq!(files.borrow().len(), 1);
}

#[test]
fn idle_processor_seals_on_the_recording_deadline_without_more_input() {
    let pool =
        ChunkPool::<btel_records::TimingRecord, SpanRecord<Snapshot, Snapshot>>::new(Config {
            chunk_capacity: nz(8),
            timing_chunks: nz(1),
            span_chunks: nz(1),
            max_producers: nz(1),
            preallocate: true,
        })
        .unwrap();
    let mut producer = pool.register_producer().unwrap();
    let thread = allocate_telemetry_id();
    producer.write_span(SpanRecord::ThreadSelected { thread_id: thread });
    producer.write_span(SpanRecord::ThreadSpanAnnouncement {
        id: thread,
        parent_id: None,
        spawn_call_path: CallPathId::ROOT,
        started_at: ClockInstant::from_ticks(1),
        clock: ClockRuntime::new(ClockMode::Monotonic).start_run(),
    });
    producer.seal();
    let (send, recv) = std::sync::mpsc::channel();
    let copy = pool.clone();
    let worker = std::thread::spawn(move || {
        let p = RecordingPublisher::new(
            RecordingId::generate(),
            RecordingConfig {
                flush_interval_duration: Duration::from_millis(10),
                ..RecordingConfig::default()
            },
            move |file| {
                send.send(file).unwrap();
            },
        )
        .unwrap();
        Processor::with_publisher(copy.bind_consumer().unwrap(), nz(8), p).run()
    });
    let file = recv
        .recv_timeout(Duration::from_secs(5))
        .expect("idle publisher deadline must wake the processor");
    assert_eq!(decode(&file).sequence, 1);
    assert_eq!(pool.stats().free_chunks, 2);
    drop(producer);
    pool.close_admission();
    assert_eq!(worker.join().unwrap(), Ok(()));
    assert!(recv.try_recv().is_err(), "no empty shutdown file");
}

#[test]
fn thread_lifecycle_and_clock_observations_are_forwarded_without_deduplication() {
    let files = RefCell::new(Vec::new());
    let mut p = RecordingPublisher::new(RecordingId::generate(), config(), |f| {
        files.borrow_mut().push(f);
    })
    .unwrap();
    let runtime = ClockRuntime::new(ClockMode::Monotonic);
    let epoch = runtime.start_run();
    epoch.attach_thread();
    let thread = allocate_telemetry_id();
    p.span(
        thread,
        &mut SpanRecord::ThreadSpanAnnouncement {
            id: thread,
            parent_id: None,
            spawn_call_path: CallPathId::ROOT,
            started_at: ClockInstant::from_ticks(1),
            clock: Arc::clone(&epoch),
        },
    );
    runtime.reset_after_restore();
    p.span(
        thread,
        &mut SpanRecord::ThreadSpanCompletion {
            id: thread,
            parent_id: None,
            spawn_call_path: CallPathId::ROOT,
            started_at: ClockInstant::from_ticks(1),
            completed_at: ClockInstant::from_ticks(2),
            outcome: InvocationOutcome::Ok,
            clock: Arc::clone(&epoch),
        },
    );
    p.finish_recording().unwrap();
    let file = decode(&files.borrow()[0]);
    let definitions = file.definitions.unwrap();
    assert_eq!(definitions.threads.len(), 2);
    assert_eq!(definitions.clock_epochs.len(), 2);
    let states = file.clock_states.unwrap().states;
    assert_eq!(states.len(), 2);
    assert_eq!(states[0].status, proto::TimingStatus::Valid as i32);
    assert_eq!(states[1].status, proto::TimingStatus::Restored as i32);
    assert!(states.iter().all(|s| !s.r#final));
    let events = file.spans.unwrap().sections.remove(0).events;
    assert!(matches!(
        events[0].event,
        Some(proto::span_event::Event::ThreadAnnouncement(_))
    ));
    assert!(matches!(
        events[1].event,
        Some(proto::span_event::Event::ThreadCompletion(_))
    ));
    epoch.finish_thread();
}

#[test]
fn encoded_sections_survive_growth_and_thread_switches() {
    let files = RefCell::new(Vec::new());
    let mut p = RecordingPublisher::new(RecordingId::generate(), RecordingConfig::default(), |f| {
        files.borrow_mut().push(f);
    })
    .unwrap();
    let a = allocate_telemetry_id();
    let b = allocate_telemetry_id();
    let path = CallPathId::new_non_root(u32::MAX).unwrap();
    for n in 0..2048 {
        let thread = if !(1024..1536).contains(&n) { a } else { b };
        p.span(
            thread,
            &mut SpanRecord::FunctionSpanCompletionOk {
                id: allocate_telemetry_id(),
                parent_id: thread,
                call_path: path,
                entered_at: ClockInstant::from_ticks(u64::MAX - n),
                exited_at: ClockInstant::from_ticks(n),
                await_time: AwaitDuration::ZERO.saturating_add(ClockDuration::from_ticks(u64::MAX)),
                captured_value: None,
            },
        );
    }
    let capacity = p.builder.buffer.spans.capacity();
    assert!(
        capacity > 4096,
        "exercise relocation with open message lengths"
    );
    p.span(
        a,
        &mut SpanRecord::FunctionSpanAnnouncement {
            id: allocate_telemetry_id(),
            parent_id: a,
            call_path: path,
            entered_at: ClockInstant::from_ticks(0),
            captured_inputs: None,
        },
    );
    p.finish_recording().unwrap();
    let file = decode(&files.borrow()[0]);
    let sections = file.spans.unwrap().sections;
    assert_eq!(
        sections.iter().map(|s| s.thread_id).collect::<Vec<_>>(),
        [a.get(), b.get(), a.get()]
    );
    assert_eq!(
        sections.iter().map(|s| s.events.len()).collect::<Vec<_>>(),
        [1024, 512, 513]
    );
    for (n, event) in sections
        .iter()
        .flat_map(|s| &s.events)
        .take(2048)
        .enumerate()
    {
        let Some(proto::span_event::Event::FunctionCompletion(event)) = &event.event else {
            panic!()
        };
        assert_eq!(event.entered_at_ticks, u64::MAX - n as u64);
        assert_eq!(event.exited_at_ticks, n as u64);
        assert_eq!(event.self_await_ticks, u64::MAX);
        assert_eq!(event.node, u64::from(u32::MAX) << 1);
    }
    // Subsequent files must open a new section even for the same selected thread.
    p.span(
        a,
        &mut SpanRecord::FunctionSpanAnnouncement {
            id: allocate_telemetry_id(),
            parent_id: a,
            call_path: path,
            entered_at: ClockInstant::from_ticks(0),
            captured_inputs: None,
        },
    );
    p.finish_recording().unwrap();
    assert_eq!(
        decode(&files.borrow()[1]).spans.unwrap().sections[0].thread_id,
        a.get()
    );
}

#[test]
fn batch_reservation_is_lazy_and_sealing_invalidates_unused_credit() {
    let files = RefCell::new(Vec::new());
    let mut p = RecordingPublisher::new(RecordingId::generate(), RecordingConfig::default(), |f| {
        files.borrow_mut().push(f);
    })
    .unwrap();
    let (thread, _, path) = ids();
    let mut event = SpanRecord::FunctionSpanAnnouncement {
        id: allocate_telemetry_id(),
        parent_id: thread,
        call_path: path,
        entered_at: ClockInstant::from_ticks(1),
        captured_inputs: None,
    };
    p.before_batch(8);
    assert_eq!(
        p.builder.buffer.spans.capacity(),
        0,
        "timing batches must not reserve span storage"
    );
    p.aggregate(delta(path));
    p.span(thread, &mut event);
    let capacity = p.builder.buffer.spans.capacity();
    assert!(capacity >= 8 * btel_settings::encoding::MAX_EVENT_BYTES);
    for _ in 1..8 {
        p.span(thread, &mut event);
        assert_eq!(p.builder.buffer.spans.capacity(), capacity);
    }
    p.after_batch(8);
    p.before_batch(8);
    p.span(thread, &mut event);
    assert!(p.builder.span_credit > 0);
    p.flush();
    assert_eq!(p.builder.span_credit, 0);
    assert_eq!(p.builder.pending_span_reservation, 0);
    assert_eq!(p.builder.buffer.spans.capacity(), 0);
    p.span(thread, &mut event); // Standalone callback must re-admit fresh storage.
    p.flush();
    p.before_batch(1024);
    p.before_span_chunk(1);
    p.span(thread, &mut event);
    assert_eq!(
        p.builder.buffer.spans.capacity(),
        256,
        "reserve for actual records, not chunk capacity"
    );
    p.after_batch(1);
    p.flush();
    assert_eq!(
        decode(&files.borrow()[0]).spans.unwrap().sections[0]
            .events
            .len(),
        9
    );
    assert_eq!(
        decode(&files.borrow()[1]).spans.unwrap().sections[0]
            .events
            .len(),
        1
    );
}

#[test]
fn size_hint_tracks_encoded_bodies_including_merge_growth_and_overflow() {
    let files = RefCell::new(Vec::new());
    let (thread, function, path) = ids();
    let mut p = RecordingPublisher::new(RecordingId::generate(), config(), |f| {
        files.borrow_mut().push(f);
    })
    .unwrap()
    .with_source_snapshot(Some([255; 32]));
    // Cover a span-only file, a mixed file and an aggregate-only file. In each,
    // compare the cached hint with actual output, including protobuf wrappers.
    for round in 0..3 {
        if round == 1 {
            metadata(&mut p, thread, function, path);
        }
        if round < 2 {
            p.span(
                thread,
                &mut SpanRecord::FunctionSpanCompletionOk {
                    id: allocate_telemetry_id(),
                    parent_id: thread,
                    call_path: path,
                    entered_at: ClockInstant::from_ticks(u64::MAX),
                    exited_at: ClockInstant::from_ticks(0),
                    await_time: AwaitDuration::ZERO,
                    captured_value: None,
                },
            );
        }
        if round > 0 {
            // Cross a count varint boundary, introduce previously omitted
            // fixed64 fields, then overflow: both aggregate rows must survive.
            for (count, duration, io) in [(127, 0, 0), (1, 9, 7), (u64::MAX, 1, 0)] {
                p.aggregate(AggregateDelta {
                    node: CallPathNodeId::new(path, true),
                    count,
                    total_duration: ClockDuration::from_ticks(duration),
                    total_io_duration: AwaitDuration::ZERO
                        .saturating_add(ClockDuration::from_ticks(io)),
                });
            }
        }
        let hint = p.builder.buffer.encoded_size_hint();
        p.flush();
        let files = files.borrow();
        let file = files.last().unwrap();
        assert!(hint >= file.bytes().len());
        assert!(hint - file.bytes().len() <= settings::FILE_ENVELOPE_BYTES);
        assert_eq!(p.builder.buffer.encoded_size_hint(), 0);
        if round > 0 {
            let entries = decode(file).aggregates.unwrap().entries;
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].count, 128);
            assert_eq!(entries[0].total_duration_ticks, 9);
            assert_eq!(entries[0].total_self_await_ticks, 7);
            assert_eq!(entries[1].count, u64::MAX);
        }
    }
}

#[test]
fn thread_heavy_files_seal_near_the_encoded_target() {
    let files = RefCell::new(Vec::new());
    let target = 64 * 1024;
    let mut p = RecordingPublisher::new(
        RecordingId::generate(),
        RecordingConfig {
            target_bytes: nz(target),
            ..RecordingConfig::default()
        },
        |f| {
            files.borrow_mut().push(f);
        },
    )
    .unwrap();
    let clock = ClockRuntime::new(ClockMode::Monotonic).start_run();
    let mut bytes_before_last_batch = 0;
    for _ in 0..1024 {
        bytes_before_last_batch = p.builder.buffer.encoded_size_hint();
        for _ in 0..8 {
            let thread = allocate_telemetry_id();
            p.span(
                thread,
                &mut SpanRecord::ThreadSpanAnnouncement {
                    id: thread,
                    parent_id: None,
                    spawn_call_path: CallPathId::ROOT,
                    started_at: ClockInstant::from_ticks(1),
                    clock: Arc::clone(&clock),
                },
            );
        }
        p.after_batch(8);
        if !files.borrow().is_empty() {
            break;
        }
    }
    let files = files.borrow();
    assert_eq!(files.len(), 1);
    assert!(bytes_before_last_batch < target);
    assert!(files[0].bytes().len() >= target - settings::FILE_ENVELOPE_BYTES);
    // A full batch can cross the soft threshold, but not by a factor of four.
    assert!(files[0].bytes().len() < target + 4096);
}

fn snapshot() -> btel_snapshot::Snapshot {
    let pool = btel_snapshot::SnapshotPool::new(1, btel_snapshot::Limits::default());
    let b = pool.try_acquire().unwrap();
    b.finish_value(btel_snapshot::SnapshotValue::Int(42))
}

#[test]
fn captures_move_after_chunk_recycle_and_duplicates_release_before_file_flush() {
    use btel_snapshot::{Limits, SnapshotPool, SnapshotValue};
    let snapshots = SnapshotPool::new(2, Limits::default());
    let make = || {
        snapshots
            .try_acquire()
            .unwrap()
            .finish_value(SnapshotValue::Int(42))
    };
    let first = make();
    let second = make();
    let expected = crate::snapshot_id(&first);
    let chunks =
        ChunkPool::<btel_records::TimingRecord, SpanRecord<Snapshot, Snapshot>>::new(Config {
            chunk_capacity: nz(4),
            timing_chunks: nz(1),
            span_chunks: nz(1),
            max_producers: nz(1),
            preallocate: true,
        })
        .unwrap();
    let mut producer = chunks.register_producer().unwrap();
    let thread = allocate_telemetry_id();
    producer.write_span(SpanRecord::ThreadSelected { thread_id: thread });
    for captured in [first, second] {
        producer.write_span(SpanRecord::FunctionSpanAnnouncement {
            id: allocate_telemetry_id(),
            parent_id: thread,
            call_path: CallPathId::new_non_root(1).unwrap(),
            entered_at: ClockInstant::from_ticks(1),
            captured_inputs: Some(captured),
        });
    }
    producer.seal();
    let received = RefCell::new(Vec::new());
    let files = RefCell::new(Vec::new());
    let publisher = RecordingPublisher::with_snapshot_receiver(
        RecordingId::generate(),
        RecordingConfig::default(),
        |file| files.borrow_mut().push(file),
        |snapshot| {
            // Input allocation is already back in the bounded pool.
            assert_eq!(chunks.stats().free_chunks, 2);
            assert_eq!(snapshots.stats().in_use, 1, "duplicate was recycled");
            received.borrow_mut().push(snapshot);
        },
    )
    .unwrap();
    let mut processor =
        Processor::with_publisher(chunks.bind_consumer().unwrap(), nz(8), publisher);
    processor.process_available();
    assert!(files.borrow().is_empty(), "captures cannot wait on sealing");
    assert_eq!(received.borrow().len(), 1);
    let snapshot = received.borrow_mut().pop().unwrap();
    std::thread::spawn(move || assert!(matches!(snapshot.value(), Some(SnapshotValue::Int(42)))))
        .join()
        .unwrap();
    assert_eq!(snapshots.stats().in_use, 0);
    drop(producer);
    chunks.close_admission();
    processor.process_available();
    let file = decode(&files.borrow()[0]);
    for event in &file.spans.unwrap().sections[0].events {
        let proto::span_event::Event::FunctionAnnouncement(entry) = event.event.unwrap() else {
            panic!()
        };
        assert_eq!(entry.inputs_cas_id, Some(expected));
    }
}

#[test]
fn snapshot_receiver_panic_releases_pending_owners() {
    use btel_snapshot::{Limits, SnapshotPool, SnapshotValue};
    let pool = SnapshotPool::new(2, Limits::default());
    let mut p = RecordingPublisher::with_snapshot_receiver(
        RecordingId::generate(),
        RecordingConfig::default(),
        |_| {},
        |_: Snapshot| panic!("receiver failed"),
    )
    .unwrap();
    let thread = allocate_telemetry_id();
    for n in [1, 2] {
        let snapshot = pool
            .try_acquire()
            .unwrap()
            .finish_value(SnapshotValue::Int(n));
        p.span(
            thread,
            &mut SpanRecord::FunctionSpanAnnouncement {
                id: allocate_telemetry_id(),
                parent_id: thread,
                call_path: CallPathId::new_non_root(1).unwrap(),
                entered_at: ClockInstant::from_ticks(1),
                captured_inputs: Some(snapshot),
            },
        );
    }
    assert_eq!(pool.stats().in_use, 2);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| p.after_batch(2))).is_err());
    assert_eq!(pool.stats().in_use, 0);
}
