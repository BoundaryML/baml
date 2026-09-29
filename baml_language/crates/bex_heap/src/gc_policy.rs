//! Allocation spending requests a full collection at the next VM safe point.
//!
//! This is headroom between collections, not a hard heap or RSS limit. Charges
//! count reserved object slots, including unused TLAB capacity, plus the backing
//! buffers of newly allocated strings and byte arrays that the new object owns
//! exclusively. Shared buffers (clones, slices), deferred concatenations,
//! container growth and other indirect storage do not spend this budget.
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use crate::BexHeap;

pub(crate) const MIN_FULL_BUDGET: usize = 32 * 1024 * 1024;
const LIVE_HEADROOM: usize = 4;
pub(crate) const FIRST_TLAB_SLOTS: usize = 32;
pub(crate) const MAX_TLAB_SLOTS: usize = 1024;
/// Number of existing VM control-flow checks between pressure/park polls.
pub const POLL_INTERVAL: u64 = 4096;

/// Diagnostic snapshot. Concurrent allocation may advance spending while read.
#[derive(Debug, Clone, Copy)]
pub struct GcBudgetSnapshot {
    /// Bytes of object slots reserved plus fresh string/byte-array buffers
    /// allocated since the last full GC.
    pub bytes_since_full_gc: usize,
    pub full_budget_bytes: usize,
    pub full_collections: usize,
}

pub(crate) struct AllocationBudget {
    spent: AtomicUsize,
    budget: AtomicUsize,
    full_collections: AtomicUsize,
    pressure: Arc<AtomicBool>,
}

impl AllocationBudget {
    pub(crate) fn new() -> Self {
        Self {
            spent: AtomicUsize::new(0),
            budget: AtomicUsize::new(MIN_FULL_BUDGET),
            full_collections: AtomicUsize::new(0),
            pressure: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn charge(&self, bytes: usize) {
        if bytes == 0 {
            return;
        }
        let old = self
            .spent
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                Some(old.saturating_add(bytes))
            })
            .expect("update always succeeds");
        if old.saturating_add(bytes) >= self.budget.load(Ordering::Relaxed) {
            // No collection here: allocation and mutation can hold unpublished
            // pointers or container locks. The VM cooperates at a safe point.
            self.pressure.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn due(&self) -> bool {
        self.spent.load(Ordering::Relaxed) >= self.budget.load(Ordering::Relaxed)
    }

    /// Called with all mutators parked, after full collection has moved survivors.
    /// Collector copies are not allocation spending. Minor GC never resets this.
    pub(crate) fn after_full(&self, live_slot_bytes: usize) {
        self.spent.store(0, Ordering::Relaxed);
        self.budget.store(
            live_slot_bytes
                .saturating_mul(LIVE_HEADROOM)
                .max(MIN_FULL_BUDGET),
            Ordering::Relaxed,
        );
        self.pressure.store(false, Ordering::Relaxed);
        self.full_collections.fetch_add(1, Ordering::Relaxed);
    }
}

impl BexHeap {
    pub fn gc_budget(&self) -> GcBudgetSnapshot {
        GcBudgetSnapshot {
            bytes_since_full_gc: self.gc_policy.spent.load(Ordering::Relaxed),
            full_budget_bytes: self.gc_policy.budget.load(Ordering::Relaxed),
            full_collections: self.gc_policy.full_collections.load(Ordering::Relaxed),
        }
    }

    /// Spend budget on a backing buffer owned by an object being allocated.
    /// Object slots are charged separately when a TLAB reserves them.
    #[inline]
    pub(crate) fn charge_backing_bytes(&self, bytes: usize) {
        self.gc_policy.charge(bytes);
    }

    pub fn gc_pressure(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.gc_policy.pressure)
    }
}

#[cfg(test)]
mod tests {
    use bex_vm_types::Object;

    use super::*;
    use crate::{CollectionLevel, Tlab};

    #[test]
    fn full_budget_tracks_spending_and_live_headroom() {
        let p = AllocationBudget::new();
        p.charge(0);
        assert!(!p.due());
        p.charge(MIN_FULL_BUDGET - 1);
        assert!(!p.due());
        p.charge(1);
        assert!(p.due());
        assert!(p.pressure.load(Ordering::Relaxed));
        p.after_full(MIN_FULL_BUDGET);
        assert!(!p.due());
        assert!(!p.pressure.load(Ordering::Relaxed));
        assert_eq!(p.budget.load(Ordering::Relaxed), 4 * MIN_FULL_BUDGET);
        p.after_full(0);
        assert_eq!(p.budget.load(Ordering::Relaxed), MIN_FULL_BUDGET);
        p.charge(usize::MAX);
        p.charge(1);
        assert_eq!(p.spent.load(Ordering::Relaxed), usize::MAX);
        p.after_full(usize::MAX);
        assert_eq!(p.budget.load(Ordering::Relaxed), usize::MAX);
    }

    #[test]
    fn minor_does_not_erase_full_spending() {
        let heap = BexHeap::new(vec![]);
        heap.gc_policy.charge(MIN_FULL_BUDGET);
        // SAFETY: standalone heap, no concurrent users or retained pointers.
        unsafe {
            heap.collect_garbage_minor(&[]);
        }
        assert_eq!(heap.gc_budget().bytes_since_full_gc, MIN_FULL_BUDGET);
        assert_eq!(heap.should_collect(), Some(CollectionLevel::Major));
        assert!(heap.gc_pressure().load(Ordering::Relaxed));
        // SAFETY: standalone heap, no concurrent users or retained pointers.
        unsafe {
            heap.collect_garbage(&[]);
        }
        assert_eq!(heap.gc_budget().bytes_since_full_gc, 0);
        assert_eq!(heap.gc_budget().full_collections, 1);
        assert_eq!(heap.should_collect(), None);
    }

    #[test]
    fn reservations_grow_without_wasting_alignment_slots() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new_empty(heap.clone());
        for i in 1usize..=4096 {
            tlab.alloc_string("inline");
            let slots = if i <= 1024 {
                i.next_power_of_two().max(32)
            } else {
                i.div_ceil(1024) * 1024
            };
            assert_eq!(
                heap.gc_budget().bytes_since_full_gc,
                slots * size_of::<Object>()
            );
        }
        tlab.alloc_string("inline");
        assert_eq!(
            heap.gc_budget().bytes_since_full_gc,
            5120 * size_of::<Object>()
        );
    }

    #[test]
    fn collection_invalidates_capacity_but_preserves_reservation_growth() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new_empty(heap.clone());
        for _ in 0..65 {
            tlab.alloc_string("inline");
        }
        assert_eq!(tlab.remaining(), 63);
        // SAFETY: single-threaded heap with no surviving roots.
        unsafe {
            heap.collect_garbage(&[]);
        }
        tlab.invalidate();
        tlab.alloc_string("inline");
        assert_eq!(tlab.remaining(), 127);
        assert_eq!(
            heap.gc_budget().bytes_since_full_gc,
            128 * size_of::<Object>()
        );
    }

    #[test]
    fn shared_backing_storage_only_spends_object_slots() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new_empty(heap.clone());
        let text = bex_str::BexStr::from("x".repeat(MIN_FULL_BUDGET));
        for _ in 0..1000 {
            tlab.alloc_string(text.clone());
            tlab.alloc_string(text.substring(1, 128));
        }
        // These 2000 objects reserve 2048 slots; none owns its backing buffer.
        assert_eq!(
            heap.gc_budget().bytes_since_full_gc,
            2048 * size_of::<Object>()
        );
        assert!(!heap.should_gc());
    }

    #[test]
    fn fresh_backing_storage_spends_the_budget() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new_empty(heap.clone());
        let slots = FIRST_TLAB_SLOTS * size_of::<Object>();
        tlab.alloc_string("x".repeat(1000));
        assert_eq!(heap.gc_budget().bytes_since_full_gc, slots + 1000);
        tlab.alloc_uint8array(Vec::with_capacity(2000));
        assert_eq!(heap.gc_budget().bytes_since_full_gc, slots + 3000);
        assert!(!heap.should_gc());

        // Short-lived large strings request a collection once their bytes
        // exceed the allowance, even though they reserve few object slots.
        let chunk = 64 * 1024;
        for _ in 0..MIN_FULL_BUDGET / chunk {
            tlab.alloc_string("x".repeat(chunk));
        }
        assert!(heap.should_gc());
        assert!(heap.gc_pressure().load(Ordering::Relaxed));
        // SAFETY: standalone heap, no concurrent users or retained pointers.
        unsafe {
            heap.collect_garbage(&[]);
        }
        assert!(!heap.should_gc());
    }
}
