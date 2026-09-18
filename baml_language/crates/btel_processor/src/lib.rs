//! Chunk-backed telemetry processor. The initial processing behavior discards
//! records and returns their storage to the bounded pool; it does not export or
//! retain telemetry. Timing chunks recycle without per-record work. Span chunks
//! release owned captures and clock references without clearing record memory.
//!
//! Bind the consumer and construct the processor on its worker thread, before
//! enabling producers. `Processor` itself spawns no thread; native
//! `TelemetryRuntime` owns the worker and synchronous producer scopes. For
//! shutdown, close pool
//! admission, let existing producers finish/seal/release while this worker keeps
//! running, then join it. Closing admission alone cannot reclaim private chunks.
//!
//! Capture parameters must represent independently owned data before publication
//! from a VM. `Send` alone does not establish that ownership. This crate neither
//! accesses VM heaps nor implements capture snapshots.

use std::{
    fmt,
    num::NonZeroUsize,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
};

pub use bex_chunkedringbuffer::DrainStatus as Progress;
use bex_chunkedringbuffer::{Consumer, TransportFailed};
use btel_records::{SpanRecord, TimingRecord};

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
pub struct Processor<InputCapture: ?Sized, ValueCapture> {
    consumer: Consumer<TimingRecord, SpanRecord<InputCapture, ValueCapture>>,
    max_chunks: NonZeroUsize,
}

impl<InputCapture: ?Sized, ValueCapture> Processor<InputCapture, ValueCapture> {
    pub fn new(
        consumer: Consumer<TimingRecord, SpanRecord<InputCapture, ValueCapture>>,
        max_chunks: NonZeroUsize,
    ) -> Self {
        Self {
            consumer,
            max_chunks,
        }
    }

    /// Consume at most the configured chunk budget without waiting for work.
    /// `complete` means admission is closed, every producer has released, and
    /// all published records have been consumed. Empty alone is not completion.
    ///
    /// Like the underlying transport this raises `TransportFailed` on terminal
    /// failure. `run` converts that signal at its outer boundary. Other panics
    /// propagate, and the consumer's failure guard stops pending producers.
    pub fn process_available(&mut self) -> Progress {
        self.consumer.drain_chunks(
            self.max_chunks,
            |_, records| drop(records),
            |_, records| drop(records),
        )
    }

    /// Process until orderly shutdown. Only an idle processor waits; producers
    /// continue to use the pool's spin-only backpressure. Failure conversion is
    /// outside the loop, adding no per-record or per-batch unwind boundary.
    pub fn run(self) -> Result<(), ProcessorError> {
        // Nothing is reused after unwinding: the closure owns the entire
        // processor, and dropping its consumer marks an unfinished pool failed.
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
                self.consumer.wait();
            }
        }
    }
}

#[cfg(test)]
mod tests;
