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
fn metadata_prefix_merges_before_newer_resolution_and_clock_observations() {
    use proto::function_definition::Resolution;

    let mut builder = RecordingBuilder::new(
        RecordingId::from_bytes([1; 16]).unwrap(),
        RecordingConfig::default(),
    )
    .unwrap()
    .with_metadata_replay();
    builder
        .buffer
        .pending
        .definitions
        .functions
        .push(proto::FunctionDefinition {
            function_id: 1,
            resolution: Some(Resolution::Unavailable(proto::MetadataUnavailable {})),
        });
    builder
        .buffer
        .pending
        .clock_states
        .states
        .push(proto::ClockEpochState {
            epoch_id: 1,
            status: proto::TimingStatus::Valid as i32,
            r#final: false,
        });
    builder.aggregate(AggregateDelta {
        count: 7,
        ..AggregateDelta::default()
    });
    let first = builder.flush_recording().unwrap().unwrap();
    let metadata = proto::RecordingFile::decode(first.metadata_bytes()).unwrap();
    assert!(metadata.header.is_none());
    assert_eq!(metadata.sequence, 0);
    assert!(metadata.spans.is_none());
    assert!(metadata.aggregates.is_none());

    builder
        .buffer
        .pending
        .definitions
        .functions
        .push(proto::FunctionDefinition {
            function_id: 1,
            resolution: Some(Resolution::Metadata(proto::FunctionMetadata {
                fqn: "resolved".into(),
                ..Default::default()
            })),
        });
    builder
        .buffer
        .pending
        .clock_states
        .states
        .push(proto::ClockEpochState {
            epoch_id: 1,
            status: proto::TimingStatus::Discontinuity as i32,
            r#final: true,
        });
    builder.aggregate(AggregateDelta {
        count: 2,
        ..AggregateDelta::default()
    });
    let mut second = builder.flush_recording().unwrap().unwrap();
    let fresh = second.metadata_bytes().to_vec();
    second.prepend_metadata(std::iter::once(first.metadata_bytes()));
    assert_eq!(second.metadata_bytes(), fresh);
    let decoded = proto::RecordingFile::decode(second.bytes()).unwrap();
    let definitions = decoded.definitions.unwrap().functions;
    assert!(matches!(
        definitions[0].resolution,
        Some(Resolution::Unavailable(_))
    ));
    assert!(matches!(
        definitions[1].resolution,
        Some(Resolution::Metadata(_))
    ));
    let states = decoded.clock_states.unwrap().states;
    assert_eq!(states.len(), 2);
    assert_eq!(states[0].status, proto::TimingStatus::Valid as i32);
    assert_eq!(states[1].status, proto::TimingStatus::Discontinuity as i32);
    assert!(states[1].r#final);
    assert_eq!(decoded.aggregates.unwrap().entries[0].count, 2);
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
        errored: 0,
        cancelled: 0,
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
    p.flush_recording().unwrap();
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
    p.flush_recording().unwrap();
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
    let early = deadline.checked_sub(Duration::from_nanos(1)).unwrap();
    for (now, expected) in [
        (early, 0),
        (deadline, 1),
        (deadline + Duration::from_secs(30), 1),
    ] {
        let file = p.builder.flush_if_due(now).unwrap();
        p.deliver(file);
        assert_eq!(files.borrow().len(), expected);
    }
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
    p.flush_recording().unwrap();
    assert_eq!(files.borrow()[0].sequence().get(), u64::MAX);
    p.aggregate(delta(CallPathId::new_non_root(999).unwrap()));
    assert_eq!(p.flush_recording(), Err(RecordingError::SequenceExhausted));
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
    let clock = ClockRuntime::new(ClockMode::Monotonic).start_run();
    clock.attach_thread();
    producer.write_span(SpanRecord::ThreadSelected { thread_id: thread });
    producer.write_span(SpanRecord::ThreadSpanAnnouncement {
        id: thread,
        parent_id: None,
        spawn_call_path: CallPathId::ROOT,
        started_at: ClockInstant::from_ticks(1),
        clock: Arc::clone(&clock),
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
    let first = decode(&file);
    assert_eq!(first.sequence, 1);
    assert!(first.end.is_none(), "a deadline seal is not the end");
    assert_eq!(pool.stats().free_chunks, 2);
    drop(producer);
    clock.finish_thread();
    pool.close_admission();
    assert_eq!(worker.join().unwrap(), Ok(()));
    // Nothing else was pending: the shutdown file carries only the settled
    // clock and the end.
    let last = decode(&recv.try_recv().expect("terminal file"));
    assert_eq!(last.sequence, 2);
    assert!(last.end.is_some());
    assert!(last.spans.unwrap().sections.is_empty());
    let states = last.clock_states.unwrap().states;
    assert_eq!(states.len(), 1);
    assert!(states[0].r#final);
    assert!(recv.try_recv().is_err(), "one terminal file");
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
    p.flush_recording().unwrap();
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
    p.flush_recording().unwrap();
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
    p.flush_recording().unwrap();
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
            // Cross count and errored varint boundaries, introduce previously
            // omitted fixed64 and cancelled fields, then overflow: both
            // aggregate rows must survive.
            for (count, duration, io, errored, cancelled) in [
                (127, 0, 0, 127, 0),
                (1, 9, 7, 1, 0),
                (2, 0, 0, 0, 2),
                (u64::MAX, 1, 0, 0, u64::MAX),
            ] {
                p.aggregate(AggregateDelta {
                    node: CallPathNodeId::new(path, true),
                    count,
                    total_duration: ClockDuration::from_ticks(duration),
                    total_io_duration: AwaitDuration::ZERO
                        .saturating_add(ClockDuration::from_ticks(io)),
                    errored,
                    cancelled,
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
            assert_eq!(entries[0].count, 130);
            assert_eq!(entries[0].total_duration_ticks, 9);
            assert_eq!(entries[0].total_self_await_ticks, 7);
            assert_eq!(
                entries[0].outcomes,
                Some(proto::AggregateOutcomes {
                    errored: 128,
                    cancelled: 2
                })
            );
            assert_eq!(entries[1].count, u64::MAX);
            assert_eq!(
                entries[1].outcomes,
                Some(proto::AggregateOutcomes {
                    errored: 0,
                    cancelled: u64::MAX
                })
            );
        }
    }
}

#[test]
fn population_outcomes_include_timing_only_calls_and_count_forwarded_spans_once() {
    let pool =
        ChunkPool::<btel_records::TimingRecord, SpanRecord<Snapshot, Snapshot>>::new(Config {
            chunk_capacity: nz(16),
            timing_chunks: nz(1),
            span_chunks: nz(1),
            max_producers: nz(1),
            preallocate: true,
        })
        .unwrap();
    let files = RefCell::new(Vec::new());
    let publisher =
        RecordingPublisher::new(RecordingId::generate(), RecordingConfig::default(), |f| {
            files.borrow_mut().push(f);
        })
        .unwrap();
    let mut processor = Processor::with_publisher(pool.bind_consumer().unwrap(), nz(8), publisher);
    let mut producer = pool.register_producer().unwrap();
    let thread = allocate_telemetry_id();
    let path = CallPathId::new_non_root(7).unwrap();
    producer.write_timing(btel_records::TimingRecord::ThreadSelected { thread_id: thread });
    for (outcome, reentry) in [
        (InvocationOutcome::Ok, false),
        (InvocationOutcome::Errored, false),
        (InvocationOutcome::Cancelled, false),
        (InvocationOutcome::Errored, false),
        (InvocationOutcome::Cancelled, true),
    ] {
        producer.write_timing(btel_records::TimingRecord::FunctionTimingCompletion {
            call_path: path,
            entered_at: ClockInstant::from_ticks(1),
            exited_at: ClockInstant::from_ticks(2),
            await_time: AwaitDuration::ZERO,
            outcome,
            reentry,
        });
    }
    producer.write_span(SpanRecord::ThreadSelected { thread_id: thread });
    producer.write_span(SpanRecord::FunctionSpanCompletionErrored {
        id: allocate_telemetry_id(),
        parent_id: thread,
        call_path: path,
        entered_at: ClockInstant::from_ticks(1),
        exited_at: ClockInstant::from_ticks(2),
        await_time: AwaitDuration::ZERO,
        captured_value: None,
    });
    producer.write_span(SpanRecord::LateFunctionSpanCompletionOkReentry {
        id: allocate_telemetry_id(),
        parent_id: thread,
        call_path: path,
        entered_at: ClockInstant::from_ticks(1),
        exited_at: ClockInstant::from_ticks(2),
        await_time: AwaitDuration::ZERO,
        captured_value: None,
    });
    // A cancelled thread is not a cancelled function invocation.
    producer.write_span(SpanRecord::ThreadSpanCompletion {
        id: thread,
        parent_id: None,
        spawn_call_path: CallPathId::ROOT,
        started_at: ClockInstant::from_ticks(1),
        completed_at: ClockInstant::from_ticks(3),
        outcome: InvocationOutcome::Cancelled,
        clock: ClockRuntime::new(ClockMode::Monotonic).start_run(),
    });
    drop(producer);
    pool.close_admission();
    // The recording publisher admits one chunk per batch.
    assert!((0..8).any(|_| processor.process_available().complete));
    drop(processor);
    let mut totals = std::collections::BTreeMap::<u64, [u64; 3]>::new();
    let mut events = 0;
    for file in files.borrow().iter() {
        let file = decode(file);
        for entry in file.aggregates.unwrap_or_default().entries {
            let outcomes = entry
                .outcomes
                .expect("new producers always report outcomes");
            let total = totals.entry(entry.node).or_default();
            total[0] += entry.count;
            total[1] += outcomes.errored;
            total[2] += outcomes.cancelled;
        }
        events += file
            .spans
            .unwrap_or_default()
            .sections
            .iter()
            .map(|section| section.events.len())
            .sum::<usize>();
    }
    assert_eq!(
        totals,
        [
            (CallPathNodeId::new(path, false).get(), [5, 3, 1]),
            (CallPathNodeId::new(path, true).get(), [2, 0, 1]),
        ]
        .into()
    );
    assert_eq!(
        events, 3,
        "forwarded spans are encoded, not aggregated again"
    );
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

#[test]
fn known_function_metadata_is_published_once_and_unknown_functions_stay_placeholders() {
    use proto::function_definition::Resolution;
    let files = RefCell::new(Vec::new());
    let allocator = FunctionIdAllocator::default();
    let (known, unknown) = (allocator.allocate().unwrap(), allocator.allocate().unwrap());
    let metadata_for = |function_id| btel_types::FunctionMetadata {
        function_id,
        fqn: "user.Extract".into(),
        display_name: "Extract".into(),
        source_file: Some("main.baml".into()),
        source_span: None,
        kind: btel_types::RuntimeFunctionKind::Bytecode,
        origin: btel_types::RuntimeFunctionOrigin::UserDefined,
        owner_type: None,
        parent_function: None,
        lambda_path: None,
        definition_key: None,
        package_name: Some("user".into()),
        namespace: Vec::new(),
        argument_layout: Some(btel_types::ArgumentLayout {
            slots: vec![btel_types::ArgumentSlot {
                name: Some("customer".into()),
                receiver: false,
            }],
        }),
        source_map: None,
    };
    let table = std::sync::Arc::new(btel_types::FunctionMetadataTable {
        functions: vec![metadata_for(known)],
    });
    let mut p = RecordingPublisher::new(RecordingId::generate(), config(), |f| {
        files.borrow_mut().push(f);
    })
    .unwrap()
    .with_function_metadata(table);
    let thread = allocate_telemetry_id();
    let define = |p: &mut RecordingPublisher<_>, path: u32, callee| {
        p.span(
            thread,
            &mut SpanRecord::CallPathDefined {
                call_path: CallPathId::new_non_root(path).unwrap(),
                parent_call_path: CallPathId::ROOT,
                visible_caller: None,
                caller_pc: 0,
                callee,
                edge: btel_types::CallPathEdge::Synchronous,
            },
        );
    };
    define(&mut p, 1, known);
    define(&mut p, 2, unknown);
    p.flush();
    define(&mut p, 3, known);
    define(&mut p, 4, unknown);
    p.flush_recording().unwrap();
    let definitions = |index: usize| {
        decode(&files.borrow()[index])
            .definitions
            .unwrap()
            .functions
    };
    let first = definitions(0);
    assert_eq!(first.len(), 2);
    let Some(Resolution::Metadata(m)) = &first[0].resolution else {
        panic!("known function must carry metadata: {first:?}")
    };
    assert_eq!(
        (first[0].function_id, m.fqn.as_str()),
        (known.get(), "user.Extract")
    );
    let layout = m.argument_layout.as_ref().unwrap();
    assert_eq!(layout.slots[0].name.as_deref(), Some("customer"));
    assert!(matches!(
        first[1].resolution,
        Some(Resolution::Unavailable(_))
    ));
    // The second file resolves `known` through the first file's definition.
    let second = definitions(1);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].function_id, unknown.get());
    assert!(matches!(
        second[0].resolution,
        Some(Resolution::Unavailable(_))
    ));
}

fn thread_record(
    thread: TelemetryId,
    clock: &Arc<btel_clock::ClockEpoch>,
    completed: bool,
) -> SpanRecord<Snapshot, Snapshot> {
    if completed {
        SpanRecord::ThreadSpanCompletion {
            id: thread,
            parent_id: None,
            spawn_call_path: CallPathId::ROOT,
            started_at: ClockInstant::from_ticks(1),
            completed_at: ClockInstant::from_ticks(2),
            outcome: InvocationOutcome::Ok,
            clock: Arc::clone(clock),
        }
    } else {
        SpanRecord::ThreadSpanAnnouncement {
            id: thread,
            parent_id: None,
            spawn_call_path: CallPathId::ROOT,
            started_at: ClockInstant::from_ticks(1),
            clock: Arc::clone(clock),
        }
    }
}

fn clock_states(file: &proto::RecordingFile) -> Vec<(u64, i32, bool)> {
    file.clock_states
        .iter()
        .flat_map(|batch| &batch.states)
        .map(|state| (state.epoch_id, state.status, state.r#final))
        .collect()
}

#[test]
fn flush_continues_the_recording_and_end_seals_it_once() {
    let files = RefCell::new(Vec::new());
    let mut p = RecordingPublisher::new(RecordingId::generate(), config(), |f| {
        files.borrow_mut().push(f);
    })
    .unwrap();
    let runtime = ClockRuntime::new(ClockMode::Monotonic);
    let clock = runtime.start_run();
    let epoch = clock.metadata().epoch.get();
    clock.attach_thread();
    let thread = allocate_telemetry_id();
    p.span(thread, &mut thread_record(thread, &clock, false));
    p.flush_recording().unwrap();
    // The run's thread completes and detaches before its completion is read.
    let mut completion = thread_record(thread, &clock, true);
    clock.finish_thread();
    p.span(thread, &mut completion);
    drop(completion);
    p.finish();
    p.finish();
    p.flush_recording().unwrap();
    assert!(p.builder.buffer.unsettled_clocks.is_empty());
    drop(p);
    let files = files.into_inner();
    assert_eq!(
        files.len(),
        2,
        "one data file, then exactly one terminal file"
    );
    let (first, last) = (decode(&files[0]), decode(&files[1]));
    assert!(first.end.is_none(), "a forced flush is not the end");
    let status = clock_states(&first)[0].1;
    assert_eq!(clock_states(&first), [(epoch, status, false)]);
    assert_eq!(last.sequence, 2);
    assert!(last.end.is_some());
    assert_eq!(clock_states(&last), [(epoch, status, true)]);
    assert_eq!(Arc::strong_count(&clock), 1, "settled epochs are released");
}

#[test]
fn an_empty_recording_ends_with_a_header_only_file() {
    let id = RecordingId::generate();
    let mut builder = RecordingBuilder::new(id, config()).unwrap();
    let file = decode(&builder.end_recording().unwrap().unwrap());
    assert_eq!(file.sequence, 1);
    assert!(file.end.is_some());
    assert!(clock_states(&file).is_empty());
    assert_eq!(file.header.unwrap().recording_id, id.as_bytes());
    assert!(file.aggregates.unwrap().entries.is_empty());
    assert!(file.spans.unwrap().sections.is_empty());
    assert!(builder.end_recording().unwrap().is_none());
}

#[test]
fn already_sealed_data_is_followed_by_an_end_only_file() {
    let (_, _, path) = ids();
    let mut builder = RecordingBuilder::new(RecordingId::generate(), config()).unwrap();
    builder.aggregate(delta(path));
    let data = decode(&builder.flush_recording().unwrap().unwrap());
    assert!(data.end.is_none());
    assert!(builder.flush_recording().unwrap().is_none());
    let last = decode(&builder.end_recording().unwrap().unwrap());
    assert_eq!(last.sequence, 2);
    assert!(last.end.is_some());
    assert!(last.aggregates.unwrap().entries.is_empty());
}

#[test]
fn input_after_the_end_is_rejected_by_both_flush_and_end() {
    let (_, _, path) = ids();
    let mut builder = RecordingBuilder::new(RecordingId::generate(), config()).unwrap();
    builder.aggregate(delta(path));
    assert!(
        decode(&builder.end_recording().unwrap().unwrap())
            .end
            .is_some()
    );
    assert!(builder.end_recording().unwrap().is_none());
    builder.aggregate(delta(path));
    assert!(matches!(
        builder.end_recording(),
        Err(RecordingError::InputAfterEnd)
    ));
    assert!(matches!(
        builder.flush_recording(),
        Err(RecordingError::InputAfterEnd)
    ));
}

#[test]
fn an_unsettled_clock_keeps_the_recording_unsealed_until_it_settles() {
    let mut builder = RecordingBuilder::new(RecordingId::generate(), config()).unwrap();
    let runtime = ClockRuntime::new(ClockMode::Monotonic);
    let clock = runtime.start_run();
    let epoch = clock.metadata().epoch.get();
    clock.attach_thread();
    let thread = allocate_telemetry_id();
    builder.span(thread, &mut thread_record(thread, &clock, false));
    // Input is exhausted but the run is still attached: no end claim. The
    // attempt adds the latest status to the announcement's (no deduplication).
    let first = decode(&builder.end_recording().unwrap().unwrap());
    assert!(first.end.is_none());
    let status = clock_states(&first)[0].1;
    assert_eq!(clock_states(&first), [(epoch, status, false); 2]);
    // The unsettled run is still exposed to invalidation, and each attempt
    // reports its latest status.
    runtime.reset_after_restore();
    let restored = proto::TimingStatus::Restored as i32;
    let second = decode(&builder.end_recording().unwrap().unwrap());
    assert!(second.end.is_none());
    assert_eq!(clock_states(&second), [(epoch, restored, false)]);
    clock.finish_thread();
    let last = decode(&builder.end_recording().unwrap().unwrap());
    assert_eq!(last.sequence, 3);
    assert!(last.end.is_some());
    assert_eq!(clock_states(&last), [(epoch, restored, true)]);
    assert!(builder.end_recording().unwrap().is_none());
}

#[test]
fn clock_bookkeeping_holds_only_unsettled_runs_and_releases_them_once_final() {
    let mut builder = RecordingBuilder::new(RecordingId::generate(), config()).unwrap();
    let runtime = ClockRuntime::new(ClockMode::Monotonic);
    let (_, _, path) = ids();
    // Runs that settle before conversion are reported final inline and never kept.
    for _ in 0..64 {
        let clock = runtime.start_run();
        clock.attach_thread();
        let thread = allocate_telemetry_id();
        let mut completion = thread_record(thread, &clock, true);
        clock.finish_thread();
        builder.span(thread, &mut completion);
        assert_eq!(Arc::strong_count(&clock), 2, "only the record holds it");
    }
    assert!(builder.buffer.unsettled_clocks.is_empty());
    let live: Vec<_> = (0..8)
        .map(|_| {
            let clock = runtime.start_run();
            clock.attach_thread();
            let thread = allocate_telemetry_id();
            builder.span(thread, &mut thread_record(thread, &clock, false));
            clock
        })
        .collect();
    assert_eq!(builder.buffer.unsettled_clocks.len(), 8);
    let first = decode(&builder.flush_recording().unwrap().unwrap());
    assert_eq!(clock_states(&first).iter().filter(|s| s.2).count(), 64);
    for clock in &live {
        clock.finish_thread();
    }
    // Settlement alone does not produce a file; the next sealed file reports it.
    assert!(builder.flush_recording().unwrap().is_none());
    builder.aggregate(delta(path));
    let second = decode(&builder.flush_recording().unwrap().unwrap());
    let finals: Vec<_> = clock_states(&second).iter().map(|s| (s.0, s.2)).collect();
    assert_eq!(
        finals,
        live.iter()
            .map(|clock| (clock.metadata().epoch.get(), true))
            .collect::<Vec<_>>()
    );
    assert!(builder.buffer.unsettled_clocks.is_empty());
    assert!(live.iter().all(|clock| Arc::strong_count(clock) == 1));
    assert!(
        decode(&builder.end_recording().unwrap().unwrap())
            .end
            .is_some()
    );
}

#[test]
fn processor_ends_the_recording_only_after_producers_stop_writing() {
    use proto::span_event::Event;

    let pool =
        ChunkPool::<btel_records::TimingRecord, SpanRecord<Snapshot, Snapshot>>::new(Config {
            chunk_capacity: nz(8),
            timing_chunks: nz(4),
            span_chunks: nz(4),
            max_producers: nz(2),
            preallocate: true,
        })
        .unwrap();
    let (send, recv) = std::sync::mpsc::channel();
    let copy = pool.clone();
    let worker = std::thread::spawn(move || {
        let p = RecordingPublisher::new(RecordingId::generate(), config(), move |file| {
            send.send(file).unwrap();
        })
        .unwrap();
        Processor::with_publisher(copy.bind_consumer().unwrap(), nz(8), p).run()
    });
    let runtime = Arc::new(ClockRuntime::new(ClockMode::Monotonic));
    let path = CallPathId::new_non_root(7).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let producers: Vec<_> = (0..2)
        .map(|_| {
            let (pool, runtime, barrier) = (pool.clone(), runtime.clone(), barrier.clone());
            std::thread::spawn(move || {
                let clock = runtime.start_run();
                clock.attach_thread();
                let thread = allocate_telemetry_id();
                let mut producer = pool.register_producer().unwrap();
                producer.write_span(SpanRecord::ThreadSelected { thread_id: thread });
                producer.write_span(thread_record(thread, &clock, false));
                barrier.wait(); // Admission closes while this producer is bound.
                barrier.wait();
                for n in 0..100 {
                    producer.write_span(SpanRecord::FunctionSpanCompletionOk {
                        id: allocate_telemetry_id(),
                        parent_id: thread,
                        call_path: path,
                        entered_at: ClockInstant::from_ticks(n),
                        exited_at: ClockInstant::from_ticks(n + 1),
                        await_time: AwaitDuration::ZERO,
                        captured_value: None,
                    });
                }
                producer.write_span(thread_record(thread, &clock, true));
                clock.finish_thread();
                drop(producer);
            })
        })
        .collect();
    barrier.wait();
    pool.close_admission();
    barrier.wait();
    for producer in producers {
        producer.join().unwrap();
    }
    assert_eq!(worker.join().unwrap(), Ok(()));
    let files: Vec<_> = recv.try_iter().map(|file| decode(&file)).collect();
    let (last, data) = files.split_last().unwrap();
    assert!(last.end.is_some(), "the last file ends the recording");
    assert!(data.iter().all(|file| file.end.is_none()));
    assert_eq!(last.sequence, files.len() as u64);
    let events = |kind: fn(&Event) -> bool| {
        files
            .iter()
            .flat_map(|file| &file.spans.as_ref().unwrap().sections)
            .flat_map(|section| &section.events)
            .filter(|event| event.event.as_ref().is_some_and(kind))
            .count()
    };
    assert_eq!(
        events(|e| matches!(e, Event::FunctionCompletion(_))),
        200,
        "writes after admission closed are recorded before the end"
    );
    assert_eq!(events(|e| matches!(e, Event::ThreadCompletion(_))), 2);
    let finals = files
        .iter()
        .flat_map(clock_states)
        .filter(|state| state.2)
        .map(|state| state.0)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(finals.len(), 2, "both runs are final by the end");
}
