//! Synchronous processor/publisher boundary. No sink I/O or outgoing queue yet.
use std::{num::NonZeroUsize, vec::Drain};

use bex_chunkedringbuffer::SpanChunk;
use btel_records::{SpanRecord, TimingRecord};
use btel_types::{CallPathNodeId, TelemetryId};
use rustc_hash::FxHashMap;

use crate::AggregateDelta;

/// Only the processor constructs this, after every completion was aggregated.
/// Owns the original allocation, counted against the bounded input pool until
/// this batch drops. Retaining or transferring it does not copy any records.
/// `initial_thread` applies before the first selector in `records`.
/// Each batch carries that context independently, including after producer
/// retirement. A publisher need not maintain a second per-producer selector map.
pub struct ProcessedSpanBatch<I: ?Sized, V> {
    pub(crate) initial_thread: Option<TelemetryId>,
    pub(crate) records: SpanChunk<TimingRecord, SpanRecord<I, V>>,
}

impl<I: ?Sized, V> ProcessedSpanBatch<I, V> {
    pub fn initial_thread(&self) -> Option<TelemetryId> {
        self.initial_thread
    }
    pub fn records(&self) -> &[SpanRecord<I, V>] {
        self.records.as_slice()
    }
    /// Move individual records if needed, keeping the allocation with this batch.
    pub fn drain(&mut self) -> Drain<'_, SpanRecord<I, V>> {
        self.records.drain()
    }
}

/// Statically dispatched, worker-local receiver. Callbacks must accept ownership
/// synchronously (including accepting an owned batch for later use) or panic
/// terminally; a partial batch is never replayed. They must
/// not wait on VM/heap permits (producers may be spinning for these allocations).
///
/// Aggregate deltas may precede definitions. A sink must associate each path with
/// the defining thread and retained clock epoch before interpreting raw tick sums.
/// Call-path IDs must not be interned across epochs without changing that key.
/// Span batches retain their original order and selector context. Retained
/// batches consume the pool's quota and must eventually be released so producers
/// can progress. Input completion/flush is not an asynchronous delivery barrier.
pub trait Publisher<I: ?Sized, V> {
    fn aggregate(&mut self, delta: AggregateDelta);
    fn spans(&mut self, batch: ProcessedSpanBatch<I, V>);
    fn flush(&mut self);
}

/// Diagnostics across windows; temporal totals deliberately stay scoped to paths.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PublisherStats {
    pub aggregate_updates: u64,
    pub function_completions: u64,
    pub span_records: u64,
    pub flushed_nodes: u64,
}

/// Real bounded downstream merging with zero configured sinks. Full windows are
/// discarded here; this is not exported telemetry or durable capture. Spans are
/// accepted as their original batch and owned captures are dropped exactly once.
/// Ordinary and reentry nodes retain separate, unmodified measurements. An
/// inclusive rollup uses `AggregateDelta::inclusive_duration`; it does not erase
/// the reentry node. No metadata is retained because no sink interprets raw times.
pub struct NoSinkPublisher {
    aggregates: FxHashMap<CallPathNodeId, AggregateDelta>,
    max_nodes: NonZeroUsize,
    stats: PublisherStats,
}

impl Default for NoSinkPublisher {
    fn default() -> Self {
        Self::new(NonZeroUsize::new(4096).unwrap())
    }
}

impl NoSinkPublisher {
    pub fn new(max_nodes: NonZeroUsize) -> Self {
        Self {
            aggregates: FxHashMap::default(),
            max_nodes,
            stats: PublisherStats::default(),
        }
    }
    pub fn stats(&self) -> PublisherStats {
        self.stats
    }
    pub fn pending(&self) -> &FxHashMap<CallPathNodeId, AggregateDelta> {
        &self.aggregates
    }

    fn flush_window(&mut self) {
        self.stats.flushed_nodes = self
            .stats
            .flushed_nodes
            .saturating_add(self.aggregates.len() as u64);
        self.aggregates.clear();
    }

    fn accept(&mut self, delta: AggregateDelta) {
        self.stats.aggregate_updates = self.stats.aggregate_updates.saturating_add(1);
        self.stats.function_completions =
            self.stats.function_completions.saturating_add(delta.count);
        if let Some(current) = self.aggregates.get_mut(&delta.node) {
            if let Some(merged) = current.checked_merge(delta) {
                *current = merged;
                return;
            }
            // Finish the existing window before arithmetic can overflow.
            self.flush_window();
        } else if self.aggregates.len() == self.max_nodes.get() {
            self.flush_window();
        }
        self.aggregates.insert(delta.node, delta);
    }
}

impl<I: ?Sized, V> Publisher<I, V> for NoSinkPublisher {
    fn aggregate(&mut self, delta: AggregateDelta) {
        self.accept(delta);
    }
    fn spans(&mut self, batch: ProcessedSpanBatch<I, V>) {
        self.stats.span_records = self
            .stats
            .span_records
            .saturating_add(batch.records().len() as u64);
        drop(batch);
    }
    fn flush(&mut self) {
        self.flush_window();
    }
}
