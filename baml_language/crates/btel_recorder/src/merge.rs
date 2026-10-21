//! Second-level accumulation for one recording and its sole sink.
//! Only processor deltas enter here; forwarded spans must not be counted again.

use std::collections::hash_map::Entry;

use btel_processor::AggregateDelta;
use btel_settings::publisher as settings;
use btel_types::{AwaitDuration, CallPathNodeId, ClockDuration};
use rustc_hash::FxHashMap;

use crate::proto;

/// Success stays implicit: `count - errored - cancelled`.
#[derive(Clone, Copy)]
struct Totals {
    count: u64,
    duration: ClockDuration,
    self_await: AwaitDuration,
    errored: u64,
    cancelled: u64,
    panicked: u64,
}

const _: () =
    assert!(std::mem::size_of::<Totals>() == btel_settings::layout::AGGREGATE_TOTALS_BYTES);

impl Totals {
    fn from_delta(delta: AggregateDelta) -> Self {
        Self {
            count: delta.count,
            duration: delta.total_duration,
            self_await: delta.total_self_await,
            errored: delta.errored,
            cancelled: delta.cancelled,
            panicked: delta.panicked,
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
                    .checked_add(delta.total_self_await.get().get())?,
            )),
            errored: self.errored.checked_add(delta.errored)?,
            cancelled: self.cancelled.checked_add(delta.cancelled)?,
            panicked: self.panicked.checked_add(delta.panicked)?,
        })
    }

    fn into_proto(self, node: CallPathNodeId) -> proto::AggregateDelta {
        proto::AggregateDelta {
            node: node.get(),
            count: self.count,
            total_duration_ticks: self.duration.get(),
            total_self_await_ticks: self.self_await.get().get(),
            // Present even when both are zero: presence means "all counted".
            outcomes: Some(proto::AggregateOutcomes {
                errored: self.errored,
                cancelled: self.cancelled,
                panicked: Some(self.panicked),
            }),
        }
    }
}

/// Sysop time per call path for one window. Rare next to completions, so a
/// plain map; an overflowing total spills as its own entry.
#[derive(Default)]
pub(crate) struct PendingSysOps {
    paths: FxHashMap<u32, (u64, u64)>,
}

impl PendingSysOps {
    /// Returns the encoded bytes this observation added.
    pub(crate) fn observe(
        &mut self,
        path: btel_types::CallPathId,
        elapsed: ClockDuration,
        output: &mut proto::AggregateBatch,
    ) -> usize {
        let entry = self.paths.entry(path.get()).or_insert((0, 0));
        let before = if entry.0 == 0 { 0 } else { SYSOP_ENTRY_BYTES };
        match entry.1.checked_add(elapsed.get()) {
            Some(total) => {
                entry.0 += 1;
                entry.1 = total;
            }
            None => {
                output.sysop_times.push(sysop_proto(path.get(), *entry));
                *entry = (1, elapsed.get());
            }
        }
        SYSOP_ENTRY_BYTES - before
    }

    pub(crate) fn drain_into(&mut self, output: &mut proto::AggregateBatch) {
        output.sysop_times.extend(
            self.paths
                .drain()
                .map(|(path, totals)| sysop_proto(path, totals)),
        );
    }
}

/// Upper bound of one encoded `SysOpTime`: tag, length, node, count, fixed64.
const SYSOP_ENTRY_BYTES: usize = 2 + 11 + 11 + 9;

fn sysop_proto(path: u32, (sysops, ticks): (u64, u64)) -> proto::SysOpTime {
    proto::SysOpTime {
        node: CallPathNodeId::new(
            btel_types::CallPathId::new_non_root(path).unwrap_or_default(),
            false,
        )
        .get(),
        sysops,
        total_ticks: ticks,
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
                    // Preserve all previous totals together, even when
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
