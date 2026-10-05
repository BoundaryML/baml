//! Allocation spending requests a full collection at the next VM safe point.
//!
//! This is headroom between collections, not a hard heap or RSS limit. Spending
//! is the heap's net growth since the last full collection: reserved object
//! slots (including unused TLAB capacity) plus the memory objects keep alive
//! outside their slots, as reported by [`Object::measure`](bex_vm_types::Object::measure).
//! Each allocator keeps a local balance and settles it here in quanta, so a
//! running VM can hold a bounded amount of unsettled spending. Storage shared
//! between objects is charged by every object that references it, so spending
//! overestimates sharing; the census taken by a full collection does not.
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering},
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
    /// Net bytes the heap grew since the last full GC: slots reserved plus
    /// payload allocated, minus payload released. Excludes each allocator's
    /// unsettled balance. Negative when more was released than allocated.
    pub bytes_since_full_gc: isize,
    pub full_budget_bytes: usize,
    pub full_collections: usize,
}

pub(crate) struct AllocationBudget {
    spent: AtomicIsize,
    budget: AtomicUsize,
    full_collections: AtomicUsize,
    pressure: Arc<AtomicBool>,
}

impl AllocationBudget {
    pub(crate) fn new() -> Self {
        Self {
            spent: AtomicIsize::new(0),
            budget: AtomicUsize::new(MIN_FULL_BUDGET),
            full_collections: AtomicUsize::new(0),
            pressure: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Settle `delta` bytes of net growth. Returns whether this settlement
    /// took spending from below the budget to at or above it, so the caller
    /// that caused the crossing can reach a safe point promptly.
    pub(crate) fn adjust(&self, delta: isize) -> bool {
        if delta == 0 {
            return false;
        }
        let old = self
            .spent
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                Some(old.saturating_add(delta))
            })
            .unwrap_or_else(|_| unreachable!("the update never declines"));
        let new = old.saturating_add(delta);
        let budget = Self::budget_as_isize(self.budget.load(Ordering::Relaxed));
        // No collection here: allocation and mutation can hold unpublished
        // pointers or container locks. The VM cooperates at a safe point.
        // A release can also take spending back below the budget; the flag
        // follows it so no VM yields for a collection that is no longer due.
        self.pressure.store(new >= budget, Ordering::Relaxed);
        old < budget && new >= budget
    }

    pub(crate) fn due(&self) -> bool {
        self.spent.load(Ordering::Relaxed)
            >= Self::budget_as_isize(self.budget.load(Ordering::Relaxed))
    }

    fn budget_as_isize(budget: usize) -> isize {
        isize::try_from(budget).unwrap_or(isize::MAX)
    }

    /// Called with all mutators parked, after full collection has moved
    /// survivors. `live_bytes` is what the survivors occupy: their slots and
    /// what they keep alive outside them. Collector moves are not allocation
    /// spending. Minor GC never resets this.
    pub(crate) fn after_full(&self, live_bytes: usize) {
        self.spent.store(0, Ordering::Relaxed);
        self.budget.store(
            live_bytes
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

    pub fn gc_pressure(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.gc_policy.pressure)
    }
}

#[cfg(test)]
mod tests {
    use bex_vm_types::Object;

    use super::*;
    use crate::{CollectionLevel, Tlab};

    const BUDGET: isize = MIN_FULL_BUDGET as isize;

    #[test]
    fn full_budget_tracks_spending_and_live_headroom() {
        let p = AllocationBudget::new();
        assert!(!p.adjust(0));
        assert!(!p.due());
        assert!(!p.adjust(BUDGET - 1));
        assert!(!p.due());
        assert!(p.adjust(1), "this settlement crosses the budget");
        assert!(p.due());
        assert!(p.pressure.load(Ordering::Relaxed));
        assert!(!p.adjust(1), "spending was already over the budget");
        p.after_full(MIN_FULL_BUDGET);
        assert!(!p.due());
        assert!(!p.pressure.load(Ordering::Relaxed));
        assert_eq!(p.budget.load(Ordering::Relaxed), 4 * MIN_FULL_BUDGET);
        p.after_full(0);
        assert_eq!(p.budget.load(Ordering::Relaxed), MIN_FULL_BUDGET);
        p.adjust(isize::MAX);
        p.adjust(1);
        assert_eq!(p.spent.load(Ordering::Relaxed), isize::MAX);
        p.after_full(usize::MAX);
        assert_eq!(p.budget.load(Ordering::Relaxed), usize::MAX);
    }

    #[test]
    fn a_release_takes_spending_and_pressure_back_below_the_budget() {
        let p = AllocationBudget::new();
        assert!(p.adjust(BUDGET));
        assert!(p.due());
        assert!(!p.adjust(-1));
        assert!(!p.due(), "released bytes are no longer spent");
        assert!(!p.pressure.load(Ordering::Relaxed));
        assert!(p.adjust(1), "and spending them again is a fresh crossing");
        p.adjust(-3 * BUDGET);
        assert!(
            p.spent.load(Ordering::Relaxed) < 0,
            "net growth can be negative"
        );
        assert!(!p.adjust(2 * BUDGET));
        assert!(!p.due());
    }

    #[test]
    fn minor_does_not_erase_full_spending() {
        let heap = BexHeap::new(vec![]);
        heap.gc_policy.adjust(BUDGET);
        // SAFETY: standalone heap, no concurrent users or retained pointers.
        unsafe {
            heap.collect_garbage_minor(&[]);
        }
        assert_eq!(heap.gc_budget().bytes_since_full_gc, BUDGET);
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
                (slots * size_of::<Object>()) as isize
            );
        }
        tlab.alloc_string("inline");
        assert_eq!(
            heap.gc_budget().bytes_since_full_gc,
            (5120 * size_of::<Object>()) as isize
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
            (128 * size_of::<Object>()) as isize
        );
    }

    /// Payload spends the budget as soon as it is allocated: one buffer the
    /// size of the whole allowance is due on its own, and every handle to a
    /// shared buffer is charged in full (an overestimate), while a slice
    /// charges only what it views.
    #[test]
    fn backing_storage_spends_the_budget() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new_empty(heap.clone());
        tlab.alloc_uint8array(vec![0; MIN_FULL_BUDGET]);
        assert!(heap.should_gc(), "one full-budget buffer is due on its own");
        assert!(
            tlab.take_budget_crossed(),
            "the allocator that crossed the budget is told so"
        );
        assert!(!tlab.take_budget_crossed(), "and told once");

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new_empty(heap.clone());
        let text = bex_str::BexStr::from("x".repeat(1 << 20));
        let whole = text.unshared_heap_bytes() as isize;
        for _ in 0..8 {
            tlab.alloc_string(text.clone());
            tlab.alloc_string(text.substring(1, 128));
        }
        tlab.flush_alloc_debt();
        let spent = heap.gc_budget().bytes_since_full_gc;
        let slots = (32 * size_of::<Object>()) as isize;
        assert_eq!(spent, slots + 8 * whole + 8 * 127);
        assert!(!heap.should_gc());
    }

    /// A payload too small to settle on its own is settled when the TLAB
    /// refills, when its owner asks, and when the TLAB is dropped. The TLAB
    /// starts with a chunk, so the first allocation does not refill.
    #[test]
    fn small_payloads_settle_at_refill_on_request_and_on_drop() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(heap.clone());
        tlab.alloc_uint8array(vec![0; 1000]);
        let slots = (32 * size_of::<Object>()) as isize;
        assert_eq!(heap.gc_budget().bytes_since_full_gc, slots);
        assert_eq!(tlab.alloc_debt().balance(), 1000);

        tlab.flush_alloc_debt();
        assert_eq!(heap.gc_budget().bytes_since_full_gc, slots + 1000);
        assert_eq!(tlab.alloc_debt().balance(), 0);

        for _ in 0..32 {
            tlab.alloc_uint8array(vec![0; 10]);
        }
        // The 33rd allocation refills, settling the 320 bytes with the slots.
        assert_eq!(
            heap.gc_budget().bytes_since_full_gc,
            slots + 1000 + (32 * size_of::<Object>()) as isize + 320
        );

        tlab.alloc_uint8array(vec![0; 10]);
        drop(tlab);
        assert_eq!(
            heap.gc_budget().bytes_since_full_gc,
            slots + 1000 + (32 * size_of::<Object>()) as isize + 330
        );
    }

    /// A full collection makes every outstanding balance stale: what it
    /// counted is in the census now. A minor collection does not.
    #[test]
    fn invalidation_discards_the_balance_only_after_a_full_collection() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(heap.clone());
        tlab.alloc_uint8array(vec![0; 1000]);
        // SAFETY: single-threaded heap with no surviving roots.
        unsafe {
            heap.collect_garbage_minor(&[]);
        }
        tlab.invalidate();
        assert_eq!(tlab.alloc_debt().balance(), 1000);
        // SAFETY: as above.
        unsafe {
            heap.collect_garbage(&[]);
        }
        tlab.invalidate();
        assert_eq!(tlab.alloc_debt().balance(), 0);
        assert_eq!(heap.gc_budget().bytes_since_full_gc, 0);
    }

    /// The budget after a full collection covers the survivors' payload, not
    /// just their slots.
    #[test]
    fn the_budget_after_a_full_collection_includes_live_payload() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new_empty(heap.clone());
        let live = tlab.alloc_uint8array(vec![0; 16 * MIN_FULL_BUDGET]);
        // SAFETY: single-threaded heap; `live` is the only root.
        unsafe {
            heap.collect_garbage(&[live]);
        }
        let budget = heap.gc_budget().full_budget_bytes;
        assert!(
            budget >= 4 * 16 * MIN_FULL_BUDGET,
            "{budget} bytes of headroom for a 16-budget live buffer"
        );
    }
}
