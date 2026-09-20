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
        &SpanRecord::ThreadSpanAnnouncement {
            id: thread,
            parent_id: None,
            spawn_call_path: CallPathId::ROOT,
            started_at: ClockInstant::from_ticks(1),
            clock,
        },
    );
    publisher.span(
        thread,
        &SpanRecord::CallPathDefined {
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
    p.span(thread, &completion(child, parent));
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
        &SpanRecord::FunctionSpanAnnouncement {
            id: parent,
            parent_id: thread,
            call_path: path,
            entered_at: ClockInstant::from_ticks(1),
            captured_inputs: Some(Box::new(CaptureDeferred)),
        },
    );
    p.span(thread, &completion(parent, thread));
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
fn immutable_bytes_are_shared_and_charged_until_the_last_destination_releases_them() {
    let files = RefCell::new(Vec::new());
    let (thread, function, path) = ids();
    let mut p = RecordingPublisher::new(RecordingId::generate(), config(), |f| {
        files.borrow_mut().push(f);
    })
    .unwrap();
    metadata(&mut p, thread, function, path);
    p.flush();
    let local = files.borrow_mut().pop().unwrap();
    let bcs = local.clone();
    assert_eq!(local.bytes().as_ptr(), bcs.bytes().as_ptr());
    assert_eq!(local.sequence(), bcs.sequence());
    let charged = p.retained.used.load(Ordering::Acquire);
    assert!(charged >= local.bytes().len());
    p.config.max_resident_bytes = nz(p.used());
    assert_eq!(p.check(1), Err(RecordingError::BudgetExceeded));
    drop(local);
    assert_eq!(p.retained.used.load(Ordering::Acquire), charged);
    drop(bcs);
    assert_eq!(p.retained.used.load(Ordering::Acquire), 0);
    assert_eq!(p.check(1), Ok(()));
}

#[test]
fn size_boundary_recycles_the_input_chunk_before_handoff() {
    let pool =
        ChunkPool::<btel_records::TimingRecord, SpanRecord<CaptureDeferred, CaptureDeferred>>::new(
            Config {
                chunk_capacity: nz(8),
                timing_chunks: nz(1),
                span_chunks: nz(1),
                max_producers: nz(1),
                preallocate: true,
            },
        )
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
    let deadline = p.deadline.unwrap();
    p.aggregate(delta(path));
    p.after_batch(1);
    assert_eq!(p.deadline, Some(deadline));
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
    p.next_sequence = NonZeroU64::new(u64::MAX);
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
        ChunkPool::<btel_records::TimingRecord, SpanRecord<CaptureDeferred, CaptureDeferred>>::new(
            Config {
                chunk_capacity: nz(8),
                timing_chunks: nz(1),
                span_chunks: nz(1),
                max_producers: nz(1),
                preallocate: true,
            },
        )
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
                flush_interval: Duration::from_millis(10),
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
        &SpanRecord::ThreadSpanAnnouncement {
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
        &SpanRecord::ThreadSpanCompletion {
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
fn failed_encoding_admission_keeps_the_unsealed_contributions_intact() {
    let files = RefCell::new(Vec::new());
    let (thread, function, path) = ids();
    let mut p = RecordingPublisher::new(RecordingId::generate(), config(), |f| {
        files.borrow_mut().push(f);
    })
    .unwrap();
    metadata(&mut p, thread, function, path);
    p.aggregate(delta(path));
    p.config.max_resident_bytes = nz(p.used());
    assert_eq!(p.seal(), Err(RecordingError::BudgetExceeded));
    assert!(files.borrow().is_empty());
    assert_eq!(p.buffer.pending.aggregates.entries[0].count, 2);
    p.config.max_resident_bytes = RecordingConfig::default().max_resident_bytes;
    p.seal().unwrap();
    assert_eq!(
        decode(&files.borrow()[0]).aggregates.unwrap().entries[0].count,
        2
    );
}

#[test]
fn retained_output_exhaustion_fails_the_transport_instead_of_dropping_records() {
    let pool =
        ChunkPool::<btel_records::TimingRecord, SpanRecord<CaptureDeferred, CaptureDeferred>>::new(
            Config {
                chunk_capacity: nz(8),
                timing_chunks: nz(1),
                span_chunks: nz(1),
                max_producers: nz(1),
                preallocate: true,
            },
        )
        .unwrap();
    let files = RefCell::new(Vec::new());
    let publisher = RecordingPublisher::new(
        RecordingId::generate(),
        RecordingConfig {
            target_bytes: nz(1),
            max_resident_bytes: nz(1024 * 1024),
            ..RecordingConfig::default()
        },
        |file| files.borrow_mut().push(file),
    )
    .unwrap();
    let mut processor = Processor::with_publisher(pool.bind_consumer().unwrap(), nz(8), publisher);
    let mut producer = pool.register_producer().unwrap();
    let thread = allocate_telemetry_id();
    producer.write_timing(btel_records::TimingRecord::ThreadSelected { thread_id: thread });
    producer.seal();
    processor.process_available();
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for id in 1..1024 {
            producer.write_timing(btel_records::TimingRecord::FunctionTimingCompletion {
                call_path: CallPathId::new_non_root(id).unwrap(),
                entered_at: ClockInstant::from_ticks(1),
                exited_at: ClockInstant::from_ticks(2),
                await_time: AwaitDuration::ZERO,
                outcome: InvocationOutcome::Ok,
                reentry: false,
            });
            producer.seal();
            processor.process_available();
            processor.flush();
        }
    }))
    .expect_err("bounded retained output must terminate");
    assert_eq!(
        failure.downcast_ref::<RecordingError>(),
        Some(&RecordingError::BudgetExceeded)
    );
    let write_failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        producer.write_timing(btel_records::TimingRecord::ThreadSelected { thread_id: thread });
    }))
    .expect_err("producer must observe terminal transport failure");
    assert!(write_failure.is::<bex_chunkedringbuffer::TransportFailed>());
}

#[test]
fn warmed_aggregate_capacity_stays_charged_and_is_released_under_pressure() {
    let mut p = RecordingPublisher::new(RecordingId::generate(), config(), |_| {}).unwrap();
    for id in 1..512 {
        p.aggregate(delta(CallPathId::new_non_root(id).unwrap()));
    }
    p.flush();
    let allocation = p.buffer.aggregates.allocation_charge();
    assert!(allocation > 0);
    assert_eq!(p.used(), BASE_CHARGE + allocation);
    p.aggregate(delta(CallPathId::new_non_root(1).unwrap()));
    p.flush();
    assert_eq!(p.buffer.aggregates.allocation_charge(), allocation);
    let reserve = p.batch_charge(1);
    p.config.max_resident_bytes = nz(p.used() + reserve - 1);
    p.before_batch(1);
    assert_eq!(p.buffer.aggregates.allocation_charge(), 0);
    assert_eq!(p.used(), BASE_CHARGE);
}

#[test]
fn encoded_sections_survive_growth_thread_switches_and_failed_sealing() {
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
            &SpanRecord::FunctionSpanCompletionOk {
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
    let capacity = p.buffer.spans.capacity();
    assert!(
        capacity > 4096,
        "exercise relocation with open message lengths"
    );
    let charged = p.used();
    p.config.max_resident_bytes = nz(charged);
    assert_eq!(p.seal(), Err(RecordingError::BudgetExceeded));
    assert!(files.borrow().is_empty());
    p.config.max_resident_bytes = RecordingConfig::default().max_resident_bytes;
    p.span(
        a,
        &SpanRecord::FunctionSpanAnnouncement {
            id: allocate_telemetry_id(),
            parent_id: a,
            call_path: path,
            entered_at: ClockInstant::from_ticks(0),
            captured_inputs: None,
        },
    );
    p.seal().unwrap();
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
        &SpanRecord::FunctionSpanAnnouncement {
            id: allocate_telemetry_id(),
            parent_id: a,
            call_path: path,
            entered_at: ClockInstant::from_ticks(0),
            captured_inputs: None,
        },
    );
    p.seal().unwrap();
    assert_eq!(
        decode(&files.borrow()[1]).spans.unwrap().sections[0].thread_id,
        a.get()
    );
}

#[test]
fn encoding_growth_is_admitted_before_consuming_a_span() {
    let mut p = RecordingPublisher::new(RecordingId::generate(), config(), |_| {}).unwrap();
    let (thread, _, path) = ids();
    // Admission must cover the new allocation while the old one remains live.
    p.buffer.spans.reserve(1024 * 1024);
    p.config.max_resident_bytes = nz(p.used() + 1);
    let bytes_before = p.buffer.spans.len();
    assert_eq!(
        p.reserve_encoding(2 * 1024 * 1024, 0),
        Err(RecordingError::BudgetExceeded)
    );
    assert_eq!(p.buffer.spans.len(), bytes_before);
    assert_eq!(p.buffer.spans.capacity(), 1024 * 1024);
    // Growth alone fits, but it must leave room for the record being admitted.
    p.config.max_resident_bytes = nz(p.used() + 2 * 1024 * 1024 + FILE_OVERHEAD);
    assert_eq!(
        p.reserve_encoding(2 * 1024 * 1024, CHUNK_HEADROOM_PER_RECORD),
        Err(RecordingError::BudgetExceeded)
    );
    assert_eq!(p.buffer.spans.capacity(), 1024 * 1024);
    p.config.max_resident_bytes = RecordingConfig::default().max_resident_bytes;
    p.span(
        thread,
        &SpanRecord::FunctionSpanAnnouncement {
            id: allocate_telemetry_id(),
            parent_id: thread,
            call_path: path,
            entered_at: ClockInstant::from_ticks(1),
            captured_inputs: None,
        },
    );
    p.finish_recording().unwrap();
}

#[test]
fn batch_reservation_is_lazy_and_sealing_invalidates_unused_credit() {
    let files = RefCell::new(Vec::new());
    let mut p = RecordingPublisher::new(RecordingId::generate(), RecordingConfig::default(), |f| {
        files.borrow_mut().push(f);
    })
    .unwrap();
    let (thread, _, path) = ids();
    let event = SpanRecord::FunctionSpanAnnouncement {
        id: allocate_telemetry_id(),
        parent_id: thread,
        call_path: path,
        entered_at: ClockInstant::from_ticks(1),
        captured_inputs: None,
    };
    p.before_batch(8);
    assert_eq!(
        p.buffer.spans.capacity(),
        0,
        "timing batches must not reserve span storage"
    );
    p.aggregate(delta(path));
    p.span(thread, &event);
    let capacity = p.buffer.spans.capacity();
    assert!(capacity >= 8 * crate::encoding::MAX_EVENT_BYTES);
    for _ in 1..8 {
        p.span(thread, &event);
        assert_eq!(p.buffer.spans.capacity(), capacity);
        assert!(p.used() <= p.config.max_resident_bytes.get());
    }
    p.after_batch(8);
    p.before_batch(8);
    p.span(thread, &event);
    assert!(p.span_credit > 0);
    p.flush();
    assert_eq!(p.span_credit, 0);
    assert_eq!(p.pending_span_reservation, 0);
    assert_eq!(p.buffer.spans.capacity(), 0);
    p.span(thread, &event); // Standalone callback must re-admit fresh storage.
    p.flush();
    p.before_batch(1024);
    p.before_span_chunk(1);
    p.span(thread, &event);
    assert_eq!(
        p.buffer.spans.capacity(),
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
