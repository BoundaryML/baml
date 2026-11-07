//! Fixed-size combining only; complete aggregate state belongs downstream.
use btel_records::AggregateDelta;

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
