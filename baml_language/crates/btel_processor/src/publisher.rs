//! Synchronous processor/publisher boundary. No sink I/O or outgoing queue yet.
use std::num::NonZeroUsize;

use btel_records::SpanRecord;
use btel_types::{CallPathNodeId, TelemetryId};
use rustc_hash::FxHashMap;

use crate::AggregateDelta;

/// Statically dispatched, worker-local receiver. The processor owns the chunk
/// throughout each callback and recycles it immediately afterward. A publisher
/// must finish reading a record synchronously; only independently owned output
/// may survive the callback. Selectors are consumed by the processor, so `thread`
/// is explicit on every callback and survives chunk boundaries and slot reuse.
///
/// Callbacks must not wait on VM/heap permits: producers can be spinning for the
/// input allocation. A panic fails the transport; partially accepted input is
/// never replayed. Aggregate deltas may precede supporting definitions. Flush
/// covers consumed input only, not private chunks or asynchronous delivery.
pub trait Publisher<I: ?Sized, V> {
    fn aggregate(&mut self, delta: AggregateDelta);
    fn span(&mut self, thread: TelemetryId, record: &SpanRecord<I, V>);
    fn flush(&mut self);
    /// Admission and delivery hooks run with no input chunk held.
    fn before_batch(&mut self, _max_records: usize) {}
    /// Actual span chunk length, including selectors, before visiting its records.
    /// Runs with the input chunk held; must not deliver files or block.
    fn before_span_chunk(&mut self, _records: usize) {}
    fn after_batch(&mut self, _records: usize) {}
    fn manages_flush_deadline(&self) -> bool {
        false
    }
    fn deadline(&self) -> Option<std::time::Instant> {
        None
    }
    fn max_chunks_per_batch(&self) -> usize {
        usize::MAX
    }
    fn finish(&mut self) {
        self.flush();
    }
}

/// Diagnostics across windows; temporal totals deliberately stay scoped to paths.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PublisherStats {
    pub aggregate_updates: u64,
    pub function_completions: u64,
    /// Non-selector records passed through the borrowed callback.
    pub span_records: u64,
    pub flushed_nodes: u64,
}

/// Real bounded downstream merging with zero configured sinks. Full windows are
/// discarded here; this is not exported telemetry or durable capture. Span records are
/// borrowed; the processor drops owned captures exactly once when recycling.
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
    fn span(&mut self, _: TelemetryId, _: &SpanRecord<I, V>) {
        self.stats.span_records = self.stats.span_records.saturating_add(1);
    }
    fn flush(&mut self) {
        self.flush_window();
    }
}
