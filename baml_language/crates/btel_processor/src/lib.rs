//! Chunk-backed processing: combine all function completions and forward spans.
//! Captures must be independently owned before publication; this crate never
//! accesses VM heaps. One bound worker scans original storage, with no event
//! cloning or dynamic dispatch. Producers never park and never wait forever:
//! out of storage, they probe for some to come back and then take more
//! ([`btel_settings::overage`]).
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
};

pub use bex_chunkedringbuffer::DrainStatus as Progress;
use bex_chunkedringbuffer::{Consumer, ProducerId, SpanChunk, TransportFailed};
use btel_records::{Capture, SpanRecord, TimingRecord};
use btel_types::{
    AwaitDuration, CallPathId, CallPathNodeId, ClockInstant, InvocationOutcome, TelemetryId,
};
use web_time::Instant;

mod capture;
pub use capture::CaptureProcessor;
mod aggregate;
mod publisher;
pub use aggregate::AggregateDelta;
use aggregate::CombiningCache;
use btel_settings::processor::CACHE_FLUSH_INTERVAL_DURATION;
pub use publisher::{NoSinkPublisher, Publisher, PublisherStats};

#[derive(Clone, Copy, Default)]
struct DecoderContext {
    timing: Option<TelemetryId>,
    span: Option<TelemetryId>,
}

const _: () =
    assert!(std::mem::size_of::<DecoderContext>() == btel_settings::layout::DECODER_CONTEXT_BYTES);

struct Processing<P> {
    publisher: P,
    cache: CombiningCache,
    contexts: Box<[DecoderContext]>,
}

impl<P> Processing<P> {
    fn sample<I, V>(
        &mut self,
        path: CallPathId,
        entered: ClockInstant,
        exited: ClockInstant,
        self_await: AwaitDuration,
        reentry: bool,
        outcome: InvocationOutcome,
    ) where
        P: Publisher<I, V>,
    {
        let delta = AggregateDelta {
            node: CallPathNodeId::new(path, reentry),
            count: 1,
            total_duration: entered.elapsed_until(exited),
            total_self_await: self_await,
            errored: u64::from(outcome.is_error()),
            cancelled: u64::from(outcome == InvocationOutcome::Cancelled),
            panicked: u64::from(outcome == InvocationOutcome::Panicked),
        };
        self.cache.observe(delta, |delta| {
            publisher::publish_aggregate(&mut self.publisher, delta);
        });
    }

    fn timing<I, V>(&mut self, producer: ProducerId, records: std::vec::Drain<'_, TimingRecord>)
    where
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
                    outcome,
                    reentry,
                } => {
                    assert!(
                        thread.is_some(),
                        "timing completion without thread selector"
                    );
                    self.sample::<I, V>(
                        call_path, entered_at, exited_at, await_time, reentry, outcome,
                    );
                }
                TimingRecord::SysOpTime { call_path, elapsed } => {
                    assert!(thread.is_some(), "sysop time without thread selector");
                    self.publisher.sysop_time(call_path, elapsed);
                }
            }
        }
        self.contexts[producer.index()].timing = thread;
        drop(records);
    }

    fn spans<I: Capture, V: Capture>(
        &mut self,
        producer: ProducerId,
        mut records: SpanChunk<TimingRecord, SpanRecord<I, V>>,
        detached: &mut Vec<SpanRecord<I, V>>,
    ) where
        P: Publisher<I, V>,
    {
        self.publisher.before_span_chunk(records.as_slice().len());
        // The chunk is the processor's now, its captures with it: they stop
        // counting against the producers' budget before anything downstream
        // can hold them up.
        for record in records.as_slice() {
            record.uncharge_captures();
        }
        let initial_thread = self.contexts[producer.index()].span;
        let mut thread = initial_thread;
        if P::DETACH_RECORDS {
            assert!(detached.is_empty());
            assert!(records.as_slice().len() <= detached.capacity());
            detached.extend(records.drain());
            drop(records);
            for mut record in detached.drain(..) {
                self.publisher.before_detached_span(&record);
                self.span(&mut thread, &mut record);
                self.publisher.after_detached_span();
            }
        } else {
            records.consume_in_place(|record| self.span(&mut thread, record));
            drop(records);
        }
        self.contexts[producer.index()].span = thread;
    }

    fn span<I, V>(&mut self, thread: &mut Option<TelemetryId>, record: &mut SpanRecord<I, V>)
    where
        P: Publisher<I, V>,
    {
        match record {
            SpanRecord::ThreadSelected { thread_id } => {
                *thread = Some(*thread_id);
                return;
            }
            SpanRecord::ManualSpanCompletion(done) => {
                // Neutral closure measures resource lifetime, not a completed
                // request. Include only terminal success/error in latency stats.
                if let Some(outcome) = done.outcome {
                    self.sample::<I, V>(
                        done.call_path,
                        done.entered_at,
                        done.exited_at,
                        done.await_time,
                        false,
                        outcome,
                    );
                }
            }
            // Every function completion form is one population invocation. Its
            // variant fixes the outcome; thread completions are not invocations.
            SpanRecord::FunctionSpanCompletionOk {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            }
            | SpanRecord::FunctionSpanCompletionOkNeedsAnnouncement {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            }
            | SpanRecord::LateFunctionSpanCompletionOk {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            } => {
                assert!(thread.is_some(), "span completion without thread selector");
                self.sample::<I, V>(
                    *call_path,
                    *entered_at,
                    *exited_at,
                    *await_time,
                    false,
                    InvocationOutcome::Ok,
                );
            }
            SpanRecord::FunctionSpanCompletionErrored {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            }
            | SpanRecord::FunctionSpanCompletionErroredNeedsAnnouncement {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            }
            | SpanRecord::LateFunctionSpanCompletionErrored {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            } => {
                assert!(thread.is_some(), "span completion without thread selector");
                self.sample::<I, V>(
                    *call_path,
                    *entered_at,
                    *exited_at,
                    *await_time,
                    false,
                    InvocationOutcome::Errored,
                );
            }
            SpanRecord::FunctionSpanCompletionCancelled {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            }
            | SpanRecord::FunctionSpanCompletionCancelledNeedsAnnouncement {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            }
            | SpanRecord::LateFunctionSpanCompletionCancelled {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            } => {
                assert!(thread.is_some(), "span completion without thread selector");
                self.sample::<I, V>(
                    *call_path,
                    *entered_at,
                    *exited_at,
                    *await_time,
                    false,
                    InvocationOutcome::Cancelled,
                );
            }
            SpanRecord::FunctionSpanCompletionPanicked {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            }
            | SpanRecord::FunctionSpanCompletionPanickedNeedsAnnouncement {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            }
            | SpanRecord::LateFunctionSpanCompletionPanicked {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            } => {
                assert!(thread.is_some(), "span completion without thread selector");
                self.sample::<I, V>(
                    *call_path,
                    *entered_at,
                    *exited_at,
                    *await_time,
                    false,
                    InvocationOutcome::Panicked,
                );
            }
            SpanRecord::FunctionSpanCompletionOkReentry {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            }
            | SpanRecord::FunctionSpanCompletionOkReentryNeedsAnnouncement {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            }
            | SpanRecord::LateFunctionSpanCompletionOkReentry {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            } => {
                assert!(thread.is_some(), "span completion without thread selector");
                self.sample::<I, V>(
                    *call_path,
                    *entered_at,
                    *exited_at,
                    *await_time,
                    true,
                    InvocationOutcome::Ok,
                );
            }
            SpanRecord::FunctionSpanCompletionErroredReentry {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            }
            | SpanRecord::FunctionSpanCompletionErroredReentryNeedsAnnouncement {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            }
            | SpanRecord::LateFunctionSpanCompletionErroredReentry {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            } => {
                assert!(thread.is_some(), "span completion without thread selector");
                self.sample::<I, V>(
                    *call_path,
                    *entered_at,
                    *exited_at,
                    *await_time,
                    true,
                    InvocationOutcome::Errored,
                );
            }
            SpanRecord::FunctionSpanCompletionCancelledReentry {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            }
            | SpanRecord::FunctionSpanCompletionCancelledReentryNeedsAnnouncement {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            }
            | SpanRecord::LateFunctionSpanCompletionCancelledReentry {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            } => {
                assert!(thread.is_some(), "span completion without thread selector");
                self.sample::<I, V>(
                    *call_path,
                    *entered_at,
                    *exited_at,
                    *await_time,
                    true,
                    InvocationOutcome::Cancelled,
                );
            }
            SpanRecord::FunctionSpanCompletionPanickedReentry {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            }
            | SpanRecord::FunctionSpanCompletionPanickedReentryNeedsAnnouncement {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            }
            | SpanRecord::LateFunctionSpanCompletionPanickedReentry {
                call_path,
                entered_at,
                exited_at,
                await_time,
                ..
            } => {
                assert!(thread.is_some(), "span completion without thread selector");
                self.sample::<I, V>(
                    *call_path,
                    *entered_at,
                    *exited_at,
                    *await_time,
                    true,
                    InvocationOutcome::Panicked,
                );
            }
            _ => {}
        }
        self.publisher
            .span(thread.expect("span record without thread selector"), record);
    }

    fn flush<I, V>(&mut self)
    where
        P: Publisher<I, V>,
    {
        self.cache
            .flush(|delta| publisher::publish_aggregate(&mut self.publisher, delta));
        self.publisher.flush();
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod runtime;
#[cfg(not(target_arch = "wasm32"))]
pub use runtime::{
    ExecutionScope, FutureErrorLinks, RecordingControl, RuntimeError, SpanOwner, TelemetryRuntime,
};

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

type DeferredSpans<I, V> = std::sync::Arc<
    std::sync::Mutex<
        Vec<(
            TelemetryId,
            btel_records::ContextReference,
            SpanRecord<I, V>,
        )>,
    >,
>;

/// One exclusive consumer of a chunk pool, bound to the current OS thread.
/// No SPSC backend, heap access, record cloning, or dynamic dispatch.
pub struct Processor<InputCapture, ValueCapture, P = NoSinkPublisher> {
    consumer: Consumer<TimingRecord, SpanRecord<InputCapture, ValueCapture>>,
    max_chunks: NonZeroUsize,
    processing: Processing<P>,
    // Two capacity-sized buffers for opted-in publishers, with one live chunk
    // at a time. Their captures are the processor's, uncharged from the
    // producers' budget, until the publisher takes or drops them.
    detached: Vec<SpanRecord<InputCapture, ValueCapture>>,
    detached_timing: Vec<TimingRecord>,
    next_flush: Instant,
    finished: bool,
    deferred_spans: Option<DeferredSpans<InputCapture, ValueCapture>>,
}

impl<I: Capture, V: Capture> Processor<I, V> {
    pub fn new(
        consumer: Consumer<TimingRecord, SpanRecord<I, V>>,
        max_chunks: NonZeroUsize,
    ) -> Self {
        Self::with_publisher(consumer, max_chunks, NoSinkPublisher::default())
    }
}

impl<I: Capture, V: Capture, P: Publisher<I, V>> Processor<I, V, P> {
    pub fn with_publisher(
        consumer: Consumer<TimingRecord, SpanRecord<I, V>>,
        max_chunks: NonZeroUsize,
        publisher: P,
    ) -> Self {
        let contexts =
            vec![DecoderContext::default(); consumer.producer_capacity()].into_boxed_slice();
        let max_chunks = NonZeroUsize::new(max_chunks.get().min(if P::DETACH_RECORDS {
            1
        } else {
            publisher.max_chunks_per_batch()
        }))
        .expect("publisher batch limit must be nonzero");
        let detached = if P::DETACH_RECORDS {
            Vec::with_capacity(consumer.chunk_capacity())
        } else {
            Vec::new()
        };
        let detached_timing = if P::DETACH_RECORDS {
            Vec::with_capacity(consumer.chunk_capacity())
        } else {
            Vec::new()
        };
        Self {
            consumer,
            max_chunks,
            detached,
            detached_timing,
            processing: Processing {
                publisher,
                cache: CombiningCache::default(),
                contexts,
            },
            next_flush: Instant::now() + CACHE_FLUSH_INTERVAL_DURATION,
            finished: false,
            deferred_spans: None,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn with_deferred_spans(mut self, spans: DeferredSpans<I, V>) -> Self {
        self.deferred_spans = Some(spans);
        self
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
        self.next_flush = Instant::now() + CACHE_FLUSH_INTERVAL_DURATION;
    }

    /// Consume at most the configured chunk budget without waiting. A callback
    /// panic fails the transport and prevents producers from silently proceeding.
    /// No partially accepted batch is retried after failure.
    pub fn process_available(&mut self) -> Progress {
        if self.consumer.is_disabled() {
            // Abandon pending reductions/encoding; shutdown must not manufacture
            // successful completion after a storage failure.
            return Progress {
                complete: true,
                ..Progress::default()
            };
        }
        self.consumer.guard_processing(|| {
            if self.consumer.has_ready_chunks() {
                self.processing.publisher.before_batch(
                    self.max_chunks
                        .get()
                        .saturating_mul(self.consumer.chunk_capacity()),
                );
            }
        });
        // Both callbacks share worker-local state. Borrow once per chunk, not
        // once per record, and never across a callback or user suspension.
        let processing = RefCell::new(&mut self.processing);
        let mut timing_producer = None;
        let progress = self.consumer.drain_chunks(
            self.max_chunks,
            |id, records| {
                if P::DETACH_RECORDS {
                    assert!(self.detached_timing.is_empty());
                    assert!(records.len() <= self.detached_timing.capacity());
                    self.detached_timing.extend(records);
                    timing_producer = Some(id);
                } else {
                    processing.borrow_mut().timing::<I, V>(id, records);
                }
            },
            |id, records| {
                processing
                    .borrow_mut()
                    .spans(id, records, &mut self.detached);
            },
        );
        if let Some(id) = timing_producer {
            self.consumer.guard_processing(|| {
                self.processing
                    .timing::<I, V>(id, self.detached_timing.drain(..));
            });
        }
        if self.consumer.is_disabled() {
            return Progress {
                complete: true,
                ..progress
            };
        }
        self.consumer.guard_processing(|| {
            if progress.complete {
                if !self.finished {
                    self.processing.cache.flush(|delta| {
                        publisher::publish_aggregate(&mut self.processing.publisher, delta);
                    });
                    // Destructors cannot bind producers under GC locks, and
                    // shutdown must not reserve another OS-worker slot. Once
                    // producers have drained, publish the remaining owned
                    // completions directly on the consumer before sealing.
                    if let Some(deferred) = &self.deferred_spans {
                        let pending = std::mem::take(
                            &mut *deferred
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner),
                        );
                        for (thread, context, record) in pending {
                            use btel_records::ContextReference;
                            // Deferred records never went through a chunk.
                            record.uncharge_captures();
                            let marker = match context {
                                ContextReference::Empty => SpanRecord::ContextCleared,
                                ContextReference::Unavailable => SpanRecord::ContextSelected {
                                    captured_context: None,
                                },
                                ContextReference::Snapshot(id) => {
                                    SpanRecord::ContextReferenced { id }
                                }
                            };
                            self.processing.publisher.before_batch(2);
                            let mut selected = Some(thread);
                            for mut record in [marker, record] {
                                if P::DETACH_RECORDS {
                                    self.processing.publisher.before_detached_span(&record);
                                }
                                self.processing.span(&mut selected, &mut record);
                                if P::DETACH_RECORDS {
                                    self.processing.publisher.after_detached_span();
                                }
                            }
                            self.processing.publisher.after_batch(2);
                        }
                    }
                    self.processing.publisher.finish();
                    self.processing.contexts.fill(DecoderContext::default());
                    self.finished = true;
                }
            } else {
                let now = Instant::now();
                if now >= self.next_flush
                    || self
                        .processing
                        .publisher
                        .deadline()
                        .is_some_and(|d| now >= d)
                {
                    self.processing.cache.flush(|delta| {
                        publisher::publish_aggregate(&mut self.processing.publisher, delta);
                    });
                    self.next_flush = now + CACHE_FLUSH_INTERVAL_DURATION;
                    // Publishers without their own deadline retain the original
                    // periodic flush behavior (including zero-sink windows).
                    if !self.processing.publisher.manages_flush_deadline() {
                        self.processing.publisher.flush();
                    }
                }
                self.processing.publisher.after_batch(progress.records);
            }
        });
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
                let wake = self
                    .processing
                    .publisher
                    .deadline()
                    .map_or(self.next_flush, |d| d.min(self.next_flush));
                self.consumer.wait_until(Some(wake));
            }
        }
    }
}

#[cfg(test)]
mod tests;
