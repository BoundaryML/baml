//! Fixed-size combining only; complete aggregate state belongs downstream.
use btel_types::{AwaitDuration, CallPathNodeId, ClockDuration};

/// Raw sums retain the call path's immutable clock context. Never add these
/// across paths/epochs before conversion. Missing clock metadata is unresolved,
/// not a zero duration. Counts remain usable if the epoch is later invalidated.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct AggregateDelta {
    pub node: CallPathNodeId,
    pub count: u64,
    pub total_duration: ClockDuration,
    pub total_io_duration: AwaitDuration,
}

const _: () =
    assert!(std::mem::size_of::<AggregateDelta>() == btel_settings::layout::AGGREGATE_DELTA_BYTES);

impl AggregateDelta {
    /// Contribution to the base path's inclusive duration. The node's actual
    /// measured duration remains available in `total_duration`, including reentry.
    pub fn inclusive_duration(&self) -> ClockDuration {
        if self.node.is_reentry() {
            ClockDuration::ZERO
        } else {
            self.total_duration
        }
    }

    /// A failed addition leaves both operands intact for separate publication.
    pub(crate) fn checked_merge(self, other: Self) -> Option<Self> {
        debug_assert_eq!(self.node, other.node);
        Some(Self {
            node: self.node,
            count: self.count.checked_add(other.count)?,
            total_duration: ClockDuration::from_ticks(
                self.total_duration
                    .get()
                    .checked_add(other.total_duration.get())?,
            ),
            total_io_duration: AwaitDuration::ZERO.saturating_add(ClockDuration::from_ticks(
                self.total_io_duration
                    .get()
                    .get()
                    .checked_add(other.total_io_duration.get().get())?,
            )),
        })
    }
}

// Keep lookup to one mixed index and one full-node comparison; collisions emit
// a delta. Fully associative scalar/SIMD probes regressed high-cardinality input.
use btel_settings::{identity::HASH_MULTIPLIER, processor::COMBINING_SLOTS as SLOTS};

pub(crate) struct CombiningCache {
    slots: [AggregateDelta; SLOTS],
    dirty: bool,
}

impl Default for CombiningCache {
    fn default() -> Self {
        Self {
            slots: [AggregateDelta::default(); SLOTS],
            dirty: false,
        }
    }
}

impl CombiningCache {
    pub(crate) fn observe(&mut self, sample: AggregateDelta, mut emit: impl FnMut(AggregateDelta)) {
        let hash = sample.node.get().wrapping_mul(HASH_MULTIPLIER);
        let index =
            usize::try_from(hash >> (u64::BITS - SLOTS.ilog2())).expect("cache index fits usize");
        let slot = &mut self.slots[index];
        if slot.count != 0 {
            if slot.node == sample.node {
                if let Some(merged) = slot.checked_merge(sample) {
                    *slot = merged;
                    return;
                }
            }
            // Collision or overflow: acceptance precedes overwrite. A panic
            // terminates processing; partially accepted input is never retried.
            emit(*slot);
        }
        *slot = sample;
        self.dirty = true;
    }

    pub(crate) fn flush(&mut self, mut emit: impl FnMut(AggregateDelta)) {
        if !self.dirty {
            return;
        }
        for slot in &mut self.slots {
            if slot.count != 0 {
                emit(*slot);
                slot.count = 0;
            }
        }
        self.dirty = false;
    }
}
