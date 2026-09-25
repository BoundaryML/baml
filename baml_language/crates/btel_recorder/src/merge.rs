//! Second-level accumulation for one recording and its sole sink.
//! Only processor deltas enter here; forwarded spans must not be counted again.

use std::collections::hash_map::Entry;

use btel_processor::AggregateDelta;
use btel_settings::publisher as settings;
use btel_types::{AwaitDuration, CallPathNodeId, ClockDuration};
use rustc_hash::FxHashMap;

use crate::proto;

#[derive(Clone, Copy)]
struct Totals {
    count: u64,
    duration: ClockDuration,
    self_await: AwaitDuration,
}

const _: () =
    assert!(std::mem::size_of::<Totals>() == btel_settings::layout::AGGREGATE_TOTALS_BYTES);

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
/// publisher tracks encoded-size changes, including integer growth and overflow spills.
#[derive(Default)]
pub(crate) struct PendingAggregates {
    nodes: FxHashMap<CallPathNodeId, Totals>,
}

impl PendingAggregates {
    pub(crate) fn trim(&mut self) {
        if self.nodes.capacity() > settings::MAP_SHRINK_THRESHOLD {
            self.nodes.shrink_to(settings::MAP_RETAINED_CAPACITY);
        }
    }

    pub(crate) fn observe(
        &mut self,
        delta: AggregateDelta,
        output: &mut proto::AggregateBatch,
    ) -> usize {
        match self.nodes.entry(delta.node) {
            Entry::Vacant(entry) => {
                let totals = Totals::from_delta(delta);
                let bytes =
                    prost::encoding::message::encoded_len(1, &totals.into_proto(delta.node));
                entry.insert(totals);
                bytes
            }
            Entry::Occupied(mut entry) => {
                let current = entry.get_mut();
                if let Some(merged) = current.checked_add(delta) {
                    let before =
                        prost::encoding::message::encoded_len(1, &current.into_proto(delta.node));
                    let after =
                        prost::encoding::message::encoded_len(1, &merged.into_proto(delta.node));
                    *current = merged;
                    after - before
                } else {
                    // Preserve all three previous totals together, even when
                    // just one field overflows. Never flush from inside a span
                    // chunk callback or partially apply a failed addition.
                    let previous = std::mem::replace(current, Totals::from_delta(delta));
                    output.entries.push(previous.into_proto(delta.node));
                    prost::encoding::message::encoded_len(1, &current.into_proto(delta.node))
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
