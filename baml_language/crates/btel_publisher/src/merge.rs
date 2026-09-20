//! Second-level accumulation shared by all destinations of one recording.
//! Only processor deltas enter here; forwarded spans must not be counted again.

use std::collections::hash_map::Entry;

use btel_processor::AggregateDelta;
use btel_types::{AwaitDuration, CallPathNodeId, ClockDuration};
use rustc_hash::FxHashMap;

use crate::proto;

#[derive(Clone, Copy)]
struct Totals {
    count: u64,
    duration: ClockDuration,
    self_await: AwaitDuration,
}

const _: () = assert!(std::mem::size_of::<Totals>() == 24);

impl Totals {
    fn from_delta(delta: AggregateDelta) -> Self {
        Self {
            count: delta.count,
            duration: delta.total_duration,
            self_await: delta.total_io_duration,
        }
    }

    fn checked_add(self, delta: AggregateDelta) -> Option<Self> {
        Some(Self {
            count: self.count.checked_add(delta.count)?,
            duration: ClockDuration::from_ticks(
                self.duration
                    .get()
                    .checked_add(delta.total_duration.get())?,
            ),
            self_await: AwaitDuration::ZERO.saturating_add(ClockDuration::from_ticks(
                self.self_await
                    .get()
                    .get()
                    .checked_add(delta.total_io_duration.get().get())?,
            )),
        })
    }

    fn into_proto(self, node: CallPathNodeId) -> proto::AggregateDelta {
        proto::AggregateDelta {
            node: node.get(),
            count: self.count,
            total_duration_ticks: self.duration.get(),
            total_self_await_ticks: self.self_await.get().get(),
        }
    }
}

/// One map per publisher window, independent of sink count. The recording
/// publisher charges new entries, overflow spills and retained map capacity.
#[derive(Default)]
pub(crate) struct PendingAggregates {
    nodes: FxHashMap<CallPathNodeId, Totals>,
}

impl PendingAggregates {
    pub(crate) fn allocation_charge(&self) -> usize {
        // Hash-table buckets, occupancy bytes, and spare capacity. The factor
        // covers the table's load factor; BASE_CHARGE covers allocation headers.
        self.nodes.capacity().saturating_mul(
            2 * (std::mem::size_of::<CallPathNodeId>() + std::mem::size_of::<Totals>() + 1),
        )
    }
    pub(crate) fn release_empty_capacity(&mut self) {
        assert!(self.nodes.is_empty());
        self.nodes = FxHashMap::default();
    }
    pub(crate) fn trim(&mut self) {
        if self.nodes.capacity() > 8192 {
            self.nodes.shrink_to(4096);
        }
    }

    pub(crate) fn observe(
        &mut self,
        delta: AggregateDelta,
        output: &mut proto::AggregateBatch,
    ) -> usize {
        match self.nodes.entry(delta.node) {
            Entry::Vacant(entry) => {
                entry.insert(Totals::from_delta(delta));
                1
            }
            Entry::Occupied(mut entry) => {
                let current = entry.get_mut();
                if let Some(merged) = current.checked_add(delta) {
                    *current = merged;
                    0
                } else {
                    // Preserve all three previous totals together, even when
                    // just one field overflows. Never flush from inside a span
                    // chunk callback or partially apply a failed addition.
                    let previous = std::mem::replace(current, Totals::from_delta(delta));
                    output.entries.push(previous.into_proto(delta.node));
                    1
                }
            }
        }
    }

    pub(crate) fn drain_into(&mut self, output: &mut proto::AggregateBatch) {
        output.entries.reserve(self.nodes.len());
        output.entries.extend(
            self.nodes
                .drain()
                .map(|(node, totals)| totals.into_proto(node)),
        );
    }
}
