//! Chunk-backed processing: combine all function completions and forward spans.
//! Captures must be independently owned before publication; this crate never
//! accesses VM heaps. One bound worker scans original storage, with no event
//! cloning or dynamic dispatch. Producers keep spin-only backpressure.
//!
//! The default publisher has zero sinks: it merges bounded aggregate windows and
//! releases spans. It neither exports nor retains telemetry beyond those windows.
//! Close admission, let producers finish, then drain and explicitly flush before
//! joining the worker. No outcome-sensitive completion relies on Drop.

use std::{
    cell::RefCell,
    fmt,
    num::NonZeroUsize,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    time::{Duration, Instant},
};

pub use bex_chunkedringbuffer::DrainStatus as Progress;
use bex_chunkedringbuffer::{Consumer, ProducerId, SpanChunk, TransportFailed};
use btel_records::{SpanRecord, TimingRecord};
use btel_types::{AwaitDuration, CallPathId, CallPathNodeId, ClockInstant, TelemetryId};

mod aggregate;
mod publisher;
pub use aggregate::AggregateDelta;
use aggregate::CombiningCache;
pub use publisher::{NoSinkPublisher, ProcessedSpanBatch, Publisher, PublisherStats};

const FLUSH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Default)]
struct DecoderContext {
    timing: Option<TelemetryId>,
    span: Option<TelemetryId>,
}

const _: () = assert!(std::mem::size_of::<DecoderContext>() == 16);

struct Processing<P> {
    publisher: P,
    cache: CombiningCache,
    contexts: Box<[DecoderContext]>,
}

impl<P> Processing<P> {
    fn sample<I: ?Sized, V>(
        &mut self,
        path: CallPathId,
        entered: ClockInstant,
        exited: ClockInstant,
        io: AwaitDuration,
        reentry: bool,
    ) where
        P: Publisher<I, V>,
    {
        let delta = AggregateDelta {
            node: CallPathNodeId::new(path, reentry),
            count: 1,
            total_duration: entered.elapsed_until(exited),
            total_io_duration: io,
        };
        self.cache
            .observe(delta, |delta| self.publisher.aggregate(delta));
    }

    fn timing<I: ?Sized, V>(
        &mut self,
        producer: ProducerId,
        records: std::vec::Drain<'_, TimingRecord>,
    ) where
        P: Publisher<I, V>,
    {
        let mut thread = self.contexts[producer.index()].timing;
        for record in records.as_slice() {
            match *record {
                TimingRecord::ThreadSelected { thread_id } => thread = Some(thread_id),
                TimingRecord::FunctionTimingCompletion {
                    call_path,
                    entered_at,
                    exited_at,
                    await_time,
                    reentry,
                    ..
                } => {
                    assert!(
                        thread.is_some(),
                        "timing completion without thread selector"
                    );
                    self.sample::<I, V>(call_path, entered_at, exited_at, await_time, reentry);
                }
            }
        }
        self.contexts[producer.index()].timing = thread;
        drop(records);
    }

    fn spans<I: ?Sized, V>(
        &mut self,
        producer: ProducerId,
        records: SpanChunk<TimingRecord, SpanRecord<I, V>>,
    ) where
        P: Publisher<I, V>,
    {
        let initial_thread = self.contexts[producer.index()].span;
        let mut thread = initial_thread;
        for record in records.as_slice() {
            match record {
                SpanRecord::ThreadSelected { thread_id } => thread = Some(*thread_id),
                SpanRecord::FunctionSpanCompletion {
                    call_path,
                    entered_at,
                    exited_at,
                    await_time,
                    reentry,
                    ..
                }
                | SpanRecord::LateFunctionSpanCompletion {
                    call_path,
                    entered_at,
                    exited_at,
                    await_time,
                    reentry,
                    ..
                } => {
                    assert!(thread.is_some(), "span completion without thread selector");
                    self.sample::<I, V>(*call_path, *entered_at, *exited_at, *await_time, *reentry);
                }
                _ => assert!(thread.is_some(), "span record without thread selector"),
            }
        }
        self.contexts[producer.index()].span = thread;
        self.publisher.spans(ProcessedSpanBatch {
            initial_thread,
            records,
        });
    }

    fn flush<I: ?Sized, V>(&mut self)
    where
        P: Publisher<I, V>,
    {
        self.cache.flush(|delta| self.publisher.aggregate(delta));
        self.publisher.flush();
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod runtime;
#[cfg(not(target_arch = "wasm32"))]
pub use runtime::{ExecutionScope, TelemetryRuntime};

#[derive(Debug, Eq, PartialEq)]
pub enum ProcessorError {
    /// The pool is terminal; producers must stop rather than omit telemetry.
    TransportFailed,
}

impl fmt::Display for ProcessorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TransportFailed => f.write_str("telemetry chunk transport failed"),
        }
    }
}

impl std::error::Error for ProcessorError {}

/// One exclusive consumer of a chunk pool, bound to the current OS thread.
/// No SPSC backend, heap access, record cloning, or dynamic dispatch.
pub struct Processor<InputCapture: ?Sized, ValueCapture, P = NoSinkPublisher> {
    consumer: Consumer<TimingRecord, SpanRecord<InputCapture, ValueCapture>>,
    max_chunks: NonZeroUsize,
    processing: Processing<P>,
    next_flush: Instant,
    finished: bool,
}

impl<I: ?Sized, V> Processor<I, V> {
    pub fn new(
        consumer: Consumer<TimingRecord, SpanRecord<I, V>>,
        max_chunks: NonZeroUsize,
    ) -> Self {
        Self::with_publisher(consumer, max_chunks, NoSinkPublisher::default())
    }
}

impl<I: ?Sized, V, P: Publisher<I, V>> Processor<I, V, P> {
    pub fn with_publisher(
        consumer: Consumer<TimingRecord, SpanRecord<I, V>>,
        max_chunks: NonZeroUsize,
        publisher: P,
    ) -> Self {
        let contexts =
            vec![DecoderContext::default(); consumer.producer_capacity()].into_boxed_slice();
        Self {
            consumer,
            max_chunks,
            processing: Processing {
                publisher,
                cache: CombiningCache::default(),
                contexts,
            },
            next_flush: Instant::now() + FLUSH_INTERVAL,
            finished: false,
        }
    }

    pub fn publisher(&self) -> &P {
        &self.processing.publisher
    }

    /// Flush already consumed input. This is not a barrier for private producer
    /// chunks or unread input. Deadlines are checked per processing batch, never
    /// per function or record. Final drain invokes this before returning complete.
    pub fn flush(&mut self) {
        self.consumer
            .guard_processing(|| self.processing.flush::<I, V>());
        self.next_flush = Instant::now() + FLUSH_INTERVAL;
    }

    /// Consume at most the configured chunk budget without waiting. A callback
    /// panic fails the transport and prevents producers from silently proceeding.
    /// No partially accepted batch is retried after failure.
    pub fn process_available(&mut self) -> Progress {
        // Both callbacks share worker-local state. Borrow once per chunk, not
        // once per record, and never across a callback or user suspension.
        let processing = RefCell::new(&mut self.processing);
        let progress = self.consumer.drain_chunks(
            self.max_chunks,
            |id, records| processing.borrow_mut().timing::<I, V>(id, records),
            |id, records| processing.borrow_mut().spans(id, records),
        );
        if progress.complete {
            if !self.finished {
                self.flush();
                self.processing.contexts.fill(DecoderContext::default());
                self.finished = true;
            }
        } else if Instant::now() >= self.next_flush {
            self.flush();
        }
        progress
    }

    /// Only idle consumers wait. Failure conversion is outside the run loop.
    pub fn run(self) -> Result<(), ProcessorError> {
        match catch_unwind(AssertUnwindSafe(move || self.run_loop())) {
            Ok(()) => Ok(()),
            Err(error) if error.is::<TransportFailed>() => Err(ProcessorError::TransportFailed),
            Err(error) => resume_unwind(error),
        }
    }

    fn run_loop(mut self) {
        loop {
            let progress = self.process_available();
            if progress.complete {
                return;
            }
            if progress.chunks == 0 {
                self.consumer.wait_until(Some(self.next_flush));
            }
        }
    }
}

#[cfg(test)]
mod tests;
