//! Reusable byte batches and ownership transfer after the round-robin drainer.
//!
//! Neither path decodes markers. Each committed source range is copied whole,
//! then the source ring can reuse it. Only this stage may allocate more memory
//! during a downstream backlog; the VM producer is unchanged.
use std::{
    collections::VecDeque,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, Thread},
    time::Duration,
};

use btel_core::stage::MarkerRange;
use crossbeam_queue::ArrayQueue;

use crate::DrainTarget;

#[derive(Clone, Copy, Debug)]
pub struct BatchConfig {
    pub payload_bytes: usize,
    pub source_ranges: usize,
    pub queue_batches: usize,
    pub retained_batches: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            payload_bytes: 256 * 1024,
            source_ranges: 256,
            queue_batches: 64,
            retained_batches: 8,
        }
    }
}

impl BatchConfig {
    pub fn validate(self) -> Result<(), &'static str> {
        if self.payload_bytes == 0
            || self.source_ranges == 0
            || self.queue_batches == 0
            || self.retained_batches == 0
        {
            return Err(
                "batch payload, descriptor, queue and retention capacities must be positive",
            );
        }
        Ok(())
    }
}

#[derive(Debug)]
struct SourceRange {
    source_id: u64,
    start: usize,
    end: usize,
}

/// The allocation belongs to the transport. Builders borrow ranges until their
/// processing call returns; they cannot retain a reference to recycled memory.
#[derive(Debug)]
pub struct SharedMarkerBatch {
    bytes: Vec<u8>,
    sources: Vec<SourceRange>,
}

impl SharedMarkerBatch {
    fn new(config: BatchConfig) -> Self {
        Self {
            bytes: Vec::with_capacity(config.payload_bytes),
            sources: Vec::with_capacity(config.source_ranges),
        }
    }

    pub fn ranges(&self) -> impl ExactSizeIterator<Item = MarkerRange<'_>> {
        self.sources.iter().map(|source| MarkerRange {
            source_id: source.source_id,
            bytes: &self.bytes[source.start..source.end],
        })
    }

    fn needs_flush(&self, range: MarkerRange<'_>) -> bool {
        !self.sources.is_empty()
            && (self.sources.len() == self.sources.capacity()
                || range.bytes.len() > self.bytes.capacity() - self.bytes.len())
    }

    fn append(&mut self, range: MarkerRange<'_>) {
        let start = self.bytes.len();
        // An oversized range can grow an empty batch. It is never split into
        // partial records and does not cause an endless empty-batch handoff.
        self.bytes.extend_from_slice(range.bytes);
        self.sources.push(SourceRange {
            source_id: range.source_id,
            start,
            end: self.bytes.len(),
        });
    }

    fn clear(&mut self) {
        self.bytes.clear();
        self.sources.clear();
    }
}

pub trait BatchProcessor {
    type Output;
    fn process(&mut self, batch: &SharedMarkerBatch);
    fn finish(self) -> Self::Output;
}

/// Copy into one reusable allocation and process on the drainer thread.
/// This isolates byte-copy cost before adding cross-thread handoff.
pub struct CopyLocal<P> {
    batch: SharedMarkerBatch,
    processor: P,
}

impl<P: BatchProcessor> CopyLocal<P> {
    pub fn new(config: BatchConfig, processor: P) -> Result<Self, &'static str> {
        config.validate()?;
        Ok(Self {
            batch: SharedMarkerBatch::new(config),
            processor,
        })
    }

    fn flush(&mut self) {
        if !self.batch.sources.is_empty() {
            self.processor.process(&self.batch);
            self.batch.clear();
        }
    }
}

impl<P: BatchProcessor> DrainTarget for CopyLocal<P> {
    type Output = P::Output;
    fn accept(&mut self, range: MarkerRange<'_>) {
        if range.bytes.is_empty() {
            return;
        }
        if self.batch.needs_flush(range) {
            self.flush();
        }
        self.batch.append(range);
    }
    fn end_step(&mut self) {
        self.flush();
    }
    fn finish(mut self) -> Self::Output {
        self.flush();
        self.processor.finish()
    }
}

struct Shared {
    // Fixed-capacity atomic queues, used strictly one sender/one receiver.
    // Allocation-free push/pop; no new unsafe queue implementation to audit.
    ready: ArrayQueue<SharedMarkerBatch>,
    recycled: ArrayQueue<SharedMarkerBatch>,
    closed: AtomicBool,
    receiver_alive: AtomicBool,
    sleeping: AtomicBool,
    worker: OnceLock<Thread>,
}

impl Shared {
    fn notify(&self) {
        if self.sleeping.load(Ordering::Acquire) {
            if let Some(worker) = self.worker.get() {
                worker.unpark();
            }
        }
    }
}

/// Sole sender, owned by the drainer. Saturated ready queues spill into an
/// ordered, sender-local backlog rather than blocking OS-ring drainage or
/// dropping markers. Its memory can grow without a fixed limit in this phase.
pub struct CopyHandoff {
    shared: Arc<Shared>,
    current: SharedMarkerBatch,
    pending: VecDeque<SharedMarkerBatch>,
    config: BatchConfig,
}

pub struct BatchReceiver {
    shared: Arc<Shared>,
}

impl CopyHandoff {
    pub fn new(config: BatchConfig) -> Result<(Self, BatchReceiver), &'static str> {
        config.validate()?;
        let shared = Arc::new(Shared {
            ready: ArrayQueue::new(config.queue_batches),
            recycled: ArrayQueue::new(config.retained_batches),
            closed: AtomicBool::new(false),
            receiver_alive: AtomicBool::new(true),
            sleeping: AtomicBool::new(false),
            worker: OnceLock::new(),
        });
        for _ in 0..config.retained_batches {
            shared
                .recycled
                .push(SharedMarkerBatch::new(config))
                .unwrap();
        }
        let receiver = BatchReceiver {
            shared: shared.clone(),
        };
        Ok((
            Self {
                shared,
                current: SharedMarkerBatch::new(config),
                pending: VecDeque::new(),
                config,
            },
            receiver,
        ))
    }

    fn check_receiver(&self) {
        assert!(
            self.shared.receiver_alive.load(Ordering::Acquire),
            "batch receiver exited before transport completion"
        );
    }

    fn pump(&mut self) {
        self.check_receiver();
        // Bound this work even when the worker concurrently empties the queue.
        for _ in 0..self.config.queue_batches {
            let Some(batch) = self.pending.pop_front() else {
                break;
            };
            if let Err(batch) = self.shared.ready.push(batch) {
                self.pending.push_front(batch);
                break;
            }
            self.shared.notify();
        }
        if self.pending.is_empty() && self.pending.capacity() > self.config.queue_batches {
            self.pending.shrink_to(self.config.queue_batches);
        }
    }

    fn flush(&mut self) {
        self.pump();
        if self.current.sources.is_empty() {
            return;
        }
        let next = self
            .shared
            .recycled
            .pop()
            .unwrap_or_else(|| SharedMarkerBatch::new(self.config));
        let batch = std::mem::replace(&mut self.current, next);
        if self.pending.is_empty() {
            match self.shared.ready.push(batch) {
                Ok(()) => self.shared.notify(),
                Err(batch) => self.pending.push_back(batch),
            }
        } else {
            self.pending.push_back(batch);
        }
    }
}

impl DrainTarget for CopyHandoff {
    type Output = ();
    fn accept(&mut self, range: MarkerRange<'_>) {
        if range.bytes.is_empty() {
            return;
        }
        if self.current.needs_flush(range) {
            self.flush();
        }
        self.current.append(range);
    }
    fn end_step(&mut self) {
        self.flush();
    }
    fn finish(mut self) {
        self.flush();
        while !self.pending.is_empty() {
            self.pump();
            if !self.pending.is_empty() {
                thread::yield_now();
            }
        }
        // Drop publishes closure only after the final pending batch is queued.
    }
}

impl Drop for CopyHandoff {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Release);
        self.shared.notify();
    }
}

impl BatchReceiver {
    /// Process every queued batch before observing end-of-input. Extra buffers
    /// from a spike are freed here when the bounded return pool is full.
    pub fn run<P: BatchProcessor>(self, mut processor: P) -> P::Output {
        let _ = self.shared.worker.set(thread::current());
        loop {
            if let Some(mut batch) = self.shared.ready.pop() {
                self.shared.sleeping.store(false, Ordering::Release);
                processor.process(&batch);
                batch.clear();
                // A full recycle queue discards only empty storage, never data.
                drop(self.shared.recycled.push(batch));
                continue;
            }
            if self.shared.closed.load(Ordering::Acquire) {
                // Recheck after acquiring closure: final enqueue precedes it.
                if self.shared.ready.is_empty() {
                    break;
                }
                continue;
            }
            self.shared.sleeping.store(true, Ordering::Release);
            // Arm before checking, so a concurrent publish either appears in
            // the queue or leaves an unpark token. Timeout is a fallback.
            if self.shared.ready.is_empty() && !self.shared.closed.load(Ordering::Acquire) {
                thread::park_timeout(Duration::from_micros(200));
            }
            self.shared.sleeping.store(false, Ordering::Release);
        }
        processor.finish()
    }
}

impl Drop for BatchReceiver {
    fn drop(&mut self) {
        self.shared.receiver_alive.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Records(Vec<(u64, Vec<u8>)>);
    impl BatchProcessor for Records {
        type Output = Vec<(u64, Vec<u8>)>;
        fn process(&mut self, batch: &SharedMarkerBatch) {
            self.0
                .extend(batch.ranges().map(|r| (r.source_id, r.bytes.to_vec())));
        }
        fn finish(self) -> Self::Output {
            self.0
        }
    }

    fn config() -> BatchConfig {
        BatchConfig {
            payload_bytes: 16,
            source_ranges: 2,
            queue_batches: 2,
            retained_batches: 2,
        }
    }

    #[test]
    fn local_copy_preserves_sources_whole_ranges_and_oversized_input() {
        let mut target = CopyLocal::new(config(), Records::default()).unwrap();
        let expected = [
            (1, vec![1; 9]),
            (2, vec![2; 9]),
            (3, vec![3; 65]),
            (4, vec![4; 1]),
        ];
        for (source_id, bytes) in &expected {
            target.accept(MarkerRange {
                source_id: *source_id,
                bytes,
            });
        }
        assert_eq!(target.finish(), expected);
    }

    #[test]
    fn descriptor_exhaustion_flushes_without_reallocating_the_local_buffer() {
        let mut target = CopyLocal::new(config(), Records::default()).unwrap();
        let ptr = target.batch.bytes.as_ptr();
        for source_id in 0..101 {
            target.accept(MarkerRange {
                source_id,
                bytes: &[7],
            });
            assert_eq!(target.batch.bytes.as_ptr(), ptr);
            assert!(target.batch.sources.len() <= 2);
        }
        let records = target.finish();
        assert_eq!(
            records,
            (0..101).map(|id| (id, vec![7])).collect::<Vec<_>>()
        );
    }

    #[test]
    fn saturated_handoff_grows_without_losing_or_reordering_bytes_and_shrinks_afterward() {
        let (mut sender, receiver) = CopyHandoff::new(config()).unwrap();
        let shared = sender.shared.clone();
        // Withhold the downstream worker until far beyond queue/pool capacity.
        // Acceptance must still return, because draining cannot block on it.
        for seq in 0u64..1000 {
            sender.accept(MarkerRange {
                source_id: seq % 3,
                bytes: &seq.to_le_bytes(),
            });
            sender.end_step();
        }
        assert!(sender.pending.len() > config().queue_batches);
        let worker = thread::spawn(move || receiver.run(Records::default()));
        // Do not explicitly flush the last partial batch: finish must deliver it.
        sender.accept(MarkerRange {
            source_id: 9,
            bytes: &[42],
        });
        sender.finish();
        let actual = worker.join().unwrap();
        let mut expected = (0u64..1000)
            .map(|seq| (seq % 3, seq.to_le_bytes().to_vec()))
            .collect::<Vec<_>>();
        expected.push((9, vec![42]));
        assert_eq!(actual, expected);
        assert!(shared.ready.is_empty());
        assert_eq!(shared.recycled.len(), config().retained_batches);
        assert!(!shared.receiver_alive.load(Ordering::Acquire));
    }

    #[test]
    fn returned_buffers_reuse_the_preallocated_payload_memory() {
        struct Ack(std::sync::mpsc::Sender<()>);
        impl BatchProcessor for Ack {
            type Output = ();
            fn process(&mut self, _: &SharedMarkerBatch) {
                self.0.send(()).unwrap();
            }
            fn finish(self) {}
        }
        let (mut sender, receiver) = CopyHandoff::new(config()).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut pointers = std::collections::HashSet::new();
        pointers.insert(sender.current.bytes.as_ptr() as usize);
        // All initial allocations can be inspected here without timing counters.
        let first = sender.shared.recycled.pop().unwrap();
        let second = sender.shared.recycled.pop().unwrap();
        pointers.insert(first.bytes.as_ptr() as usize);
        pointers.insert(second.bytes.as_ptr() as usize);
        sender.shared.recycled.push(first).unwrap();
        sender.shared.recycled.push(second).unwrap();
        let worker = thread::spawn(move || receiver.run(Ack(tx)));
        for _ in 0..100 {
            sender.accept(MarkerRange {
                source_id: 1,
                bytes: &[1, 2, 3],
            });
            sender.end_step();
            rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(pointers.contains(&(sender.current.bytes.as_ptr() as usize)));
        }
        sender.finish();
        worker.join().unwrap();
    }

    #[test]
    fn receiver_failure_is_reported_instead_of_spinning_forever() {
        let (mut sender, receiver) = CopyHandoff::new(config()).unwrap();
        drop(receiver);
        sender.accept(MarkerRange {
            source_id: 1,
            bytes: &[1],
        });
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sender.finish())).is_err()
        );
    }

    #[test]
    fn real_rings_finish_every_source_through_the_downstream_worker() {
        let (factory, mut drainer) = crate::transport(crate::TransportConfig {
            segment_bytes: 64,
            memory_bytes: 32 * 1024 * 1024,
            freelist_segments: 2,
        })
        .unwrap();
        let (mut target, receiver) = CopyHandoff::new(BatchConfig {
            payload_bytes: 128,
            ..config()
        })
        .unwrap();
        let worker = thread::spawn(move || receiver.run(Records::default()));
        let writers: Vec<_> = (1..=3)
            .map(|source_id| {
                let factory = factory.clone();
                thread::spawn(move || {
                    let mut producer = factory.producer(source_id).unwrap();
                    for seq in 0u64..4097 {
                        assert!(producer.write(&seq.to_le_bytes()));
                    }
                })
            })
            .collect();
        while writers.iter().any(|w| !w.is_finished()) {
            drainer.drain(&mut target, 1);
            thread::yield_now();
        }
        for writer in writers {
            writer.join().unwrap();
        }
        drainer.finish(target).unwrap();
        let records = worker.join().unwrap();
        for source in 1..=3 {
            let actual: Vec<_> = records
                .iter()
                .filter(|(id, _)| *id == source)
                .flat_map(|(_, bytes)| {
                    bytes
                        .as_chunks::<8>()
                        .0
                        .iter()
                        .map(|bytes| u64::from_le_bytes(*bytes))
                })
                .collect();
            assert_eq!(actual, (0..4097).collect::<Vec<_>>());
        }
    }
}
