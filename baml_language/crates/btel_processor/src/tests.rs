use std::{
    num::NonZeroUsize,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use bex_chunkedringbuffer::{ChunkPool, Config};
use btel_types::{
    AwaitDuration, CallPathId, CallPathNodeId, ClockDuration, ClockInstant, InvocationOutcome,
    allocate_telemetry_id,
};
use rustc_hash::FxHashMap;

use super::*;

fn nz(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

fn config(capacity: usize, producers: usize) -> Config {
    Config {
        chunk_capacity: nz(capacity),
        timing_chunks: nz(producers),
        span_chunks: nz(producers),
        max_producers: nz(producers),
        preallocate: true,
    }
}

fn timing() -> TimingRecord {
    TimingRecord::FunctionTimingCompletion {
        call_path: CallPathId::new_non_root(1).unwrap(),
        entered_at: ClockInstant::from_ticks(1),
        exited_at: ClockInstant::from_ticks(2),
        await_time: AwaitDuration::ZERO,
        outcome: InvocationOutcome::Ok,
        reentry: false,
    }
}

struct Capture(Arc<AtomicUsize>);
impl Drop for Capture {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

fn span(capture: Capture) -> SpanRecord<[Capture], Capture> {
    SpanRecord::LateFunctionSpanCompletion {
        id: allocate_telemetry_id(),
        parent_id: allocate_telemetry_id(),
        call_path: CallPathId::new_non_root(1).unwrap(),
        entered_at: ClockInstant::from_ticks(1),
        exited_at: ClockInstant::from_ticks(2),
        await_time: AwaitDuration::ZERO,
        outcome: InvocationOutcome::Ok,
        reentry: false,
        captured_value: Some(Box::new(capture)),
    }
}

// Bound threaded tests so lost wakeups/backpressure failures report rather than
// hanging the suite. Propagate the original assertion/panic to the test thread.
fn checked(test: impl FnOnce() + Send + 'static) {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || tx.send(catch_unwind(AssertUnwindSafe(test))).unwrap());
    if let Err(error) = rx
        .recv_timeout(Duration::from_secs(if cfg!(miri) { 120 } else { 15 }))
        .expect("processor test timed out")
    {
        std::panic::resume_unwind(error);
    }
}

#[test]
fn bounded_processing_releases_captures_and_clock_ownership() {
    let drops = Arc::new(AtomicUsize::new(0));
    let clock = btel_clock::ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run();
    let references = Arc::strong_count(&clock);
    let pool = ChunkPool::new(config(8, 1)).unwrap();
    let mut processor = Processor::new(pool.bind_consumer().unwrap(), nz(1));
    let mut producer = pool.register_producer().unwrap();
    let thread_id = allocate_telemetry_id();
    producer.write_timing(TimingRecord::ThreadSelected { thread_id });
    producer.write_span(SpanRecord::ThreadSelected { thread_id });
    producer.write_timing(timing());
    producer.write_span(SpanRecord::ThreadSpanCompletion {
        id: allocate_telemetry_id(),
        parent_id: None,
        spawn_call_path: CallPathId::ROOT,
        started_at: ClockInstant::from_ticks(1),
        completed_at: ClockInstant::from_ticks(2),
        outcome: InvocationOutcome::Ok,
        clock: clock.clone(),
    });
    producer.write_span(SpanRecord::FunctionSpanAnnouncement {
        id: allocate_telemetry_id(),
        parent_id: allocate_telemetry_id(),
        call_path: CallPathId::new_non_root(1).unwrap(),
        entered_at: ClockInstant::from_ticks(1),
        captured_inputs: Some(
            vec![Capture(drops.clone()), Capture(drops.clone())].into_boxed_slice(),
        ),
    });
    producer.write_span(span(Capture(drops.clone())));
    let private = processor.process_available();
    assert_eq!(private.records, 0);
    assert!(!private.complete);
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    producer.seal();
    let first = processor.process_available();
    assert_eq!((first.chunks, first.records), (1, 2));
    assert!(!first.complete);
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    let second = processor.process_available();
    assert_eq!((second.chunks, second.records), (1, 4));
    assert_eq!(drops.load(Ordering::Relaxed), 3);
    assert_eq!(Arc::strong_count(&clock), references);
    assert_eq!(pool.stats().free_chunks, 2);
    pool.close_admission();
    assert!(!processor.process_available().complete);
    drop(producer);
    assert!(processor.process_available().complete);
    assert_eq!(processor.publisher().stats().function_completions, 2);
    drop(processor);
    assert!(!pool.is_failed());
}

#[test]
fn run_recycles_tiny_pools_under_multiple_producers() {
    checked(|| {
        let drops = Arc::new(AtomicUsize::new(0));
        let pool = ChunkPool::new(config(2, 2)).unwrap();
        let (ready, bound) = mpsc::channel();
        let copy = pool.clone();
        let worker = thread::spawn(move || {
            let processor = Processor::new(copy.bind_consumer().unwrap(), nz(8));
            ready.send(()).unwrap();
            processor.run()
        });
        bound.recv().unwrap();
        let writers: Vec<_> = (0..2)
            .map(|_| {
                let copy = pool.clone();
                let drops = drops.clone();
                thread::spawn(move || {
                    let mut producer = copy.register_producer().unwrap();
                    let thread_id = allocate_telemetry_id();
                    producer.write_timing(TimingRecord::ThreadSelected { thread_id });
                    producer.write_span(SpanRecord::ThreadSelected { thread_id });
                    for _ in 0..31 {
                        producer.write_timing(timing());
                        producer.write_span(span(Capture(drops.clone())));
                    }
                    assert_eq!(producer.stats().allocations, 0);
                    // Drop seals the final partial chunks of both lanes.
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        pool.close_admission();
        assert_eq!(worker.join().unwrap(), Ok(()));
        assert_eq!(drops.load(Ordering::Relaxed), 62);
        let stats = pool.stats();
        assert_eq!(stats.sealed_records, 128);
        assert_eq!(stats.free_chunks, 4);
        assert_eq!(stats.ready_chunks, 0);
        assert!(!pool.is_failed());
    });
}

#[test]
fn idle_run_wakes_on_final_partial_release_after_admission_closes() {
    checked(|| {
        let pool = ChunkPool::<TimingRecord, SpanRecord<(), ()>>::new(config(4, 1)).unwrap();
        let mut producer = pool.register_producer().unwrap();
        producer.write_timing(TimingRecord::ThreadSelected {
            thread_id: allocate_telemetry_id(),
        });
        producer.write_timing(timing());
        let (ready, bound) = mpsc::channel();
        let (done, finished) = mpsc::channel();
        let copy = pool.clone();
        let worker = thread::spawn(move || {
            let mut processor = Processor::new(copy.bind_consumer().unwrap(), nz(8));
            assert_eq!(processor.process_available().records, 0);
            ready.send(()).unwrap();
            done.send(processor.run()).unwrap();
        });
        bound.recv().unwrap();
        pool.close_admission();
        assert!(finished.recv_timeout(Duration::from_millis(10)).is_err());
        drop(producer);
        assert_eq!(finished.recv().unwrap(), Ok(()));
        worker.join().unwrap();
        assert_eq!(pool.stats().free_chunks, 2);
        assert_eq!(pool.stats().sealed_records, 2);
    });
}

#[test]
fn failure_wakes_idle_processor_and_is_reported_at_run_boundary() {
    checked(|| {
        let pool = ChunkPool::<TimingRecord, SpanRecord<(), ()>>::new(config(1, 1)).unwrap();
        let (ready, bound) = mpsc::channel();
        let copy = pool.clone();
        let worker = thread::spawn(move || {
            let processor = Processor::new(copy.bind_consumer().unwrap(), nz(8));
            ready.send(()).unwrap();
            processor.run()
        });
        bound.recv().unwrap();
        pool.fail();
        assert_eq!(worker.join().unwrap(), Err(ProcessorError::TransportFailed));
    });
}

#[test]
fn destructor_panic_is_preserved_and_stops_producers() {
    struct BrokenCapture;
    impl Drop for BrokenCapture {
        fn drop(&mut self) {
            std::panic::panic_any(42_u32);
        }
    }
    let pool = ChunkPool::<TimingRecord, SpanRecord<(), BrokenCapture>>::new(config(2, 1)).unwrap();
    let processor = Processor::new(pool.bind_consumer().unwrap(), nz(8));
    let mut producer = pool.register_producer().unwrap();
    producer.write_span(SpanRecord::ThreadSelected {
        thread_id: allocate_telemetry_id(),
    });
    producer.write_span(SpanRecord::FunctionSpanCompletion {
        id: allocate_telemetry_id(),
        parent_id: allocate_telemetry_id(),
        call_path: CallPathId::new_non_root(1).unwrap(),
        entered_at: ClockInstant::from_ticks(1),
        exited_at: ClockInstant::from_ticks(2),
        await_time: AwaitDuration::ZERO,
        outcome: InvocationOutcome::Ok,
        reentry: false,
        captured_value: Some(Box::new(BrokenCapture)),
    });
    let panic = catch_unwind(AssertUnwindSafe(|| processor.run())).unwrap_err();
    assert_eq!(*panic.downcast::<u32>().unwrap(), 42);
    assert!(pool.is_failed());
    let panic = catch_unwind(AssertUnwindSafe(|| producer.write_timing(timing()))).unwrap_err();
    assert!(panic.is::<TransportFailed>());
}

#[derive(Default)]
struct RecordingPublisher {
    deltas: Vec<AggregateDelta>,
    spans: Vec<ProcessedSpanBatch<[Capture], Capture>>,
    initial_threads: Vec<Option<btel_types::TelemetryId>>,
    flushes: usize,
}
impl Publisher<[Capture], Capture> for RecordingPublisher {
    fn aggregate(&mut self, delta: AggregateDelta) {
        self.deltas.push(delta);
    }
    fn spans(&mut self, batch: ProcessedSpanBatch<[Capture], Capture>) {
        self.initial_threads.push(batch.initial_thread());
        self.spans.push(batch);
    }
    fn flush(&mut self) {
        self.flushes += 1;
    }
}

#[test]
fn all_completions_combine_once_and_spans_move_without_copying_captures() {
    let pool = ChunkPool::new(config(16, 2)).unwrap();
    let mut processor = Processor::with_publisher(
        pool.bind_consumer().unwrap(),
        nz(8),
        RecordingPublisher::default(),
    );
    let mut producer = pool.register_producer().unwrap();
    let thread_id = allocate_telemetry_id();
    let path = CallPathId::new_non_root(7).unwrap();
    let io = AwaitDuration::ZERO.saturating_add(ClockDuration::from_ticks(3));
    producer.write_timing(TimingRecord::ThreadSelected { thread_id });
    for reentry in [false, true] {
        producer.write_timing(TimingRecord::FunctionTimingCompletion {
            call_path: path,
            entered_at: ClockInstant::from_ticks(10),
            exited_at: ClockInstant::from_ticks(20),
            await_time: io,
            outcome: InvocationOutcome::Errored,
            reentry,
        });
    }
    // Consume Timing before any Span metadata exists. Do not block recycling.
    producer.seal();
    processor.process_available();
    assert!(
        processor.publisher().deltas.is_empty(),
        "cache survives chunk boundary"
    );
    let drops = Arc::new(AtomicUsize::new(0));
    let captured = Box::new(Capture(drops.clone()));
    let address = &raw const *captured;
    producer.write_span(SpanRecord::ThreadSelected { thread_id });
    producer.write_span(SpanRecord::FunctionSpanCompletion {
        id: allocate_telemetry_id(),
        parent_id: thread_id,
        call_path: path,
        entered_at: ClockInstant::from_ticks(10),
        exited_at: ClockInstant::from_ticks(20),
        await_time: io,
        outcome: InvocationOutcome::Ok,
        reentry: false,
        captured_value: Some(captured),
    });
    producer.seal();
    processor.process_available();
    // A later chunk deliberately has no selector: context survives chunk reuse.
    producer.write_span(SpanRecord::LateFunctionSpanCompletion {
        id: allocate_telemetry_id(),
        parent_id: thread_id,
        call_path: path,
        entered_at: ClockInstant::from_ticks(20),
        exited_at: ClockInstant::from_ticks(10),
        await_time: io,
        outcome: InvocationOutcome::Cancelled,
        reentry: false,
        captured_value: None,
    });
    let clock = btel_clock::ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run();
    producer.write_span(SpanRecord::ThreadSpanCompletion {
        id: thread_id,
        parent_id: None,
        spawn_call_path: CallPathId::ROOT,
        started_at: ClockInstant::from_ticks(1),
        completed_at: ClockInstant::from_ticks(30),
        outcome: InvocationOutcome::Ok,
        clock: clock.clone(),
    });
    pool.close_admission();
    drop(producer);
    assert!(processor.process_available().complete);
    let receiver = processor.publisher();
    let nodes: FxHashMap<_, _> = receiver.deltas.iter().map(|d| (d.node, *d)).collect();
    assert_eq!(nodes.len(), 2);
    assert_eq!(
        nodes[&CallPathNodeId::new(path, false)],
        AggregateDelta {
            node: CallPathNodeId::new(path, false),
            count: 3,
            total_duration: ClockDuration::from_ticks(20),
            total_io_duration: AwaitDuration::ZERO.saturating_add(ClockDuration::from_ticks(9)),
        }
    );
    assert_eq!(
        nodes[&CallPathNodeId::new(path, true)],
        AggregateDelta {
            node: CallPathNodeId::new(path, true),
            count: 1,
            total_duration: ClockDuration::from_ticks(10),
            total_io_duration: AwaitDuration::ZERO.saturating_add(ClockDuration::from_ticks(3)),
        }
    );
    assert_eq!(receiver.deltas.iter().map(|d| d.count).sum::<u64>(), 4);
    assert_eq!(
        receiver
            .deltas
            .iter()
            .map(|d| d.inclusive_duration().get())
            .sum::<u64>(),
        20
    );
    assert_eq!(
        receiver
            .deltas
            .iter()
            .map(|d| d.total_io_duration.get().get())
            .sum::<u64>(),
        12
    );
    assert_eq!(receiver.initial_threads, [None, Some(thread_id)]);
    assert_eq!(receiver.spans.len(), 2); // Two original, retained chunks.
    assert_eq!(
        receiver
            .spans
            .iter()
            .map(|b| b.records().len())
            .sum::<usize>(),
        4
    );
    assert_eq!(pool.stats().free_chunks, 2, "only Timing recycled");
    let SpanRecord::FunctionSpanCompletion {
        captured_value: Some(value),
        ..
    } = &receiver.spans[0].records()[1]
    else {
        panic!()
    };
    assert_eq!(&raw const **value, address);
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    assert_eq!(receiver.flushes, 1);
    assert!(processor.process_available().complete);
    assert_eq!(processor.publisher().flushes, 1, "final flush happens once");
    assert!(
        processor
            .processing
            .contexts
            .iter()
            .all(|c| c.timing.is_none() && c.span.is_none())
    );
    drop(processor);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    assert_eq!(Arc::strong_count(&clock), 1);
    assert_eq!(
        pool.stats().free_chunks,
        4,
        "publisher released both span chunks"
    );
}

#[test]
fn retained_span_chunks_keep_context_after_switches_and_producer_retirement() {
    let pool = ChunkPool::new(config(4, 4)).unwrap();
    let mut processor = Processor::with_publisher(
        pool.bind_consumer().unwrap(),
        nz(8),
        RecordingPublisher::default(),
    );
    let a = allocate_telemetry_id();
    let b = allocate_telemetry_id();
    let announcement = |thread_id| SpanRecord::FunctionSpanAnnouncement {
        id: allocate_telemetry_id(),
        parent_id: thread_id,
        call_path: CallPathId::new_non_root(1).unwrap(),
        entered_at: ClockInstant::from_ticks(1),
        captured_inputs: None,
    };
    let mut producer = pool.register_producer().unwrap();
    producer.write_span(SpanRecord::ThreadSelected { thread_id: a });
    producer.write_span(announcement(a));
    producer.seal();
    processor.process_available();
    producer.write_span(announcement(a)); // Next chunk has no selector.
    producer.seal();
    processor.process_available();
    producer.write_span(SpanRecord::ThreadSelected { thread_id: b });
    producer.write_span(announcement(b));
    producer.write_span(SpanRecord::ThreadSelected { thread_id: a });
    producer.write_span(announcement(a));
    drop(producer);
    processor.process_available();
    // Reuse the OS thread with a distinct producer identity.
    let mut producer = pool.register_producer().unwrap();
    producer.write_span(SpanRecord::ThreadSelected { thread_id: b });
    producer.write_span(announcement(b));
    drop(producer);
    pool.close_admission();
    assert!(processor.process_available().complete);
    assert!(
        processor
            .processing
            .contexts
            .iter()
            .all(|c| c.timing.is_none() && c.span.is_none())
    );
    let publisher = processor.publisher();
    assert!(
        publisher.deltas.is_empty(),
        "announcements do not aggregate"
    );
    assert_eq!(publisher.initial_threads, [None, Some(a), Some(a), Some(a)]);
    for (batch, expected) in publisher
        .spans
        .iter()
        .zip([vec![a], vec![a], vec![b, a], vec![b]])
    {
        let mut current = batch.initial_thread();
        let mut observed = Vec::new();
        for record in batch.records() {
            match record {
                SpanRecord::ThreadSelected { thread_id } => current = Some(*thread_id),
                SpanRecord::FunctionSpanAnnouncement { parent_id, .. } => {
                    assert_eq!(current, Some(*parent_id));
                    observed.push(current.unwrap());
                }
                _ => unreachable!(),
            }
        }
        assert_eq!(observed, expected);
    }
}

#[test]
fn decoder_slots_stay_fixed_and_live_selectors_survive_other_slot_reuse() {
    let pool = ChunkPool::<TimingRecord, SpanRecord<(), ()>>::new(config(8, 2)).unwrap();
    let mut processor = Processor::new(pool.bind_consumer().unwrap(), nz(8));
    let mut live = pool.register_producer().unwrap();
    live.write_timing(TimingRecord::ThreadSelected {
        thread_id: allocate_telemetry_id(),
    });
    live.seal();
    processor.process_available();
    // A second OS thread repeatedly registers new producer identities.
    for _ in 0..40 {
        let copy = pool.clone();
        thread::spawn(move || {
            let mut producer = copy.register_producer().unwrap();
            producer.write_timing(TimingRecord::ThreadSelected {
                thread_id: allocate_telemetry_id(),
            });
            producer.write_timing(timing());
        })
        .join()
        .unwrap();
        processor.process_available();
        assert_eq!(processor.processing.contexts.len(), 2);
    }
    live.write_timing(timing()); // No selector after repeated reuse of the other slot.
    drop(live);
    pool.close_admission();
    processor.process_available();
    assert_eq!(processor.publisher().stats().function_completions, 41);
    assert!(
        processor
            .processing
            .contexts
            .iter()
            .all(|c| c.timing.is_none() && c.span.is_none())
    );
}

#[test]
fn combiner_collisions_and_overflow_preserve_exact_totals() {
    let mut cache = CombiningCache::default();
    let mut actual: FxHashMap<CallPathNodeId, (u128, u128, u128)> = FxHashMap::default();
    let mut expected = actual.clone();
    let mut accept = |d: AggregateDelta| {
        let total = actual.entry(d.node).or_default();
        total.0 += u128::from(d.count);
        total.1 += u128::from(d.total_duration.get());
        total.2 += u128::from(d.total_io_duration.get().get());
    };
    for i in 0..400_u32 {
        let path = CallPathId::new_non_root((1 + (i % 65) * 64) | ((i % 2) << 31)).unwrap();
        let amount = if i % 7 == 0 { u64::MAX } else { u64::from(i) };
        let d = AggregateDelta {
            node: CallPathNodeId::new(path, i % 3 == 0),
            count: amount.max(1),
            total_duration: ClockDuration::from_ticks(amount),
            total_io_duration: AwaitDuration::ZERO
                .saturating_add(ClockDuration::from_ticks(amount)),
        };
        let total = expected.entry(d.node).or_default();
        total.0 += u128::from(d.count);
        total.1 += u128::from(amount);
        total.2 += u128::from(amount);
        cache.observe(d, &mut accept);
    }
    // Force same-key overflow without relying on a particular cache mapping.
    for _ in 0..2 {
        let path = CallPathId::new_non_root(1).unwrap();
        let d = AggregateDelta {
            node: CallPathNodeId::new(path, false),
            count: u64::MAX,
            total_duration: ClockDuration::from_ticks(u64::MAX),
            total_io_duration: AwaitDuration::ZERO,
        };
        let total = expected.entry(d.node).or_default();
        total.0 += u128::from(u64::MAX);
        total.1 += u128::from(u64::MAX);
        cache.observe(d, &mut accept);
    }
    cache.flush(&mut accept);
    cache.flush(&mut accept); // Empty flush contributes nothing.
    assert_eq!(actual, expected);
}

#[test]
fn no_sink_publisher_bounds_unique_paths_and_flushes_before_overflow() {
    let mut publisher = NoSinkPublisher::new(nz(2));
    for raw in [1, 1, 2, 3] {
        <NoSinkPublisher as Publisher<(), ()>>::aggregate(
            &mut publisher,
            AggregateDelta {
                node: CallPathNodeId::new(CallPathId::new_non_root(raw).unwrap(), false),
                count: 1,
                total_duration: ClockDuration::from_ticks(u64::MAX),
                total_io_duration: AwaitDuration::ZERO,
            },
        );
        assert!(publisher.pending().len() <= 2);
    }
    assert_eq!(publisher.stats().function_completions, 4);
    assert_eq!(publisher.stats().flushed_nodes, 3);
    <NoSinkPublisher as Publisher<(), ()>>::flush(&mut publisher);
    assert!(publisher.pending().is_empty());
}

#[test]
fn idle_deadline_flushes_without_new_input() {
    checked(|| {
        struct Receiver(mpsc::Sender<u64>, u64);
        impl Publisher<(), ()> for Receiver {
            fn aggregate(&mut self, d: AggregateDelta) {
                self.1 += d.count;
            }
            fn spans(&mut self, _: ProcessedSpanBatch<(), ()>) {}
            fn flush(&mut self) {
                if self.1 != 0 {
                    self.0.send(std::mem::take(&mut self.1)).unwrap();
                }
            }
        }
        let pool = ChunkPool::<TimingRecord, SpanRecord<(), ()>>::new(config(8, 1)).unwrap();
        let mut producer = pool.register_producer().unwrap();
        producer.write_timing(TimingRecord::ThreadSelected {
            thread_id: allocate_telemetry_id(),
        });
        producer.write_timing(timing());
        producer.seal();
        let (tx, rx) = mpsc::channel();
        let copy = pool.clone();
        let worker = thread::spawn(move || {
            let mut processor =
                Processor::with_publisher(copy.bind_consumer().unwrap(), nz(8), Receiver(tx, 0));
            processor.next_flush = Instant::now() + Duration::from_millis(10);
            processor.run()
        });
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
        drop(producer);
        pool.close_admission();
        assert_eq!(worker.join().unwrap(), Ok(()));
    });
}

#[test]
fn publisher_flush_failure_is_terminal_even_after_the_last_chunk() {
    struct BrokenPublisher;
    impl Publisher<(), ()> for BrokenPublisher {
        fn aggregate(&mut self, _: AggregateDelta) {}
        fn spans(&mut self, _: ProcessedSpanBatch<(), ()>) {}
        fn flush(&mut self) {
            std::panic::panic_any(43_u32);
        }
    }
    let pool = ChunkPool::<TimingRecord, SpanRecord<(), ()>>::new(config(4, 1)).unwrap();
    let mut processor =
        Processor::with_publisher(pool.bind_consumer().unwrap(), nz(8), BrokenPublisher);
    let mut producer = pool.register_producer().unwrap();
    producer.write_timing(TimingRecord::ThreadSelected {
        thread_id: allocate_telemetry_id(),
    });
    producer.write_timing(timing());
    drop(producer);
    pool.close_admission();
    let panic = catch_unwind(AssertUnwindSafe(|| processor.process_available())).unwrap_err();
    assert_eq!(*panic.downcast::<u32>().unwrap(), 43);
    assert!(pool.is_failed());
    let retry = catch_unwind(AssertUnwindSafe(|| processor.process_available())).unwrap_err();
    assert!(
        retry.is::<TransportFailed>(),
        "no replay after partial acceptance"
    );
}
