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
    AwaitDuration, CallPathId, ClockInstant, InvocationOutcome, allocate_telemetry_id,
};

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
    assert_eq!((first.chunks, first.records), (1, 1));
    assert!(!first.complete);
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    let second = processor.process_available();
    assert_eq!((second.chunks, second.records), (1, 3));
    assert_eq!(drops.load(Ordering::Relaxed), 3);
    assert_eq!(Arc::strong_count(&clock), references);
    assert_eq!(pool.stats().free_chunks, 2);
    pool.close_admission();
    assert!(!processor.process_available().complete);
    drop(producer);
    assert!(processor.process_available().complete);
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
        assert_eq!(stats.sealed_records, 124);
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
        assert_eq!(pool.stats().sealed_records, 1);
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
    let pool = ChunkPool::<TimingRecord, SpanRecord<(), BrokenCapture>>::new(config(1, 1)).unwrap();
    let processor = Processor::new(pool.bind_consumer().unwrap(), nz(8));
    let mut producer = pool.register_producer().unwrap();
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
