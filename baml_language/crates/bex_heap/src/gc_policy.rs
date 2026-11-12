//! Allocation spending requests a full collection at the next VM safe point.
//!
//! This is headroom between collections, not a hard heap or RSS limit. Spending
//! is the heap's net growth since the last full collection: the cells
//! allocated (at their size class) and the memory objects keep alive outside
//! their cells, as reported by [`Object::measure`](bex_vm_types::Object::measure).
//! Each allocator keeps a local balance and settles it here in quanta, so a
//! running VM can hold a bounded amount of unsettled spending. Storage shared
//! between objects is charged by every object that references it, so spending
//! overestimates sharing; the census taken by a full collection does not.
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering},
};

use crate::BexHeap;

/// The budget floor. Small programs collect their garbage (outgrown backings
/// included) about as often as the enum VM freed it, rather than
/// accumulating up to a large fixed floor. 8 MiB rather than Go's 4 MiB: at
/// 4 MiB allocation-high-retention ran 3.4% slower than at 32 MiB, at 8 MiB
/// no row did, and peak RSS stays at or below the enum VM's either way.
pub(crate) const MIN_FULL_BUDGET: usize = 8 * 1024 * 1024;
const LIVE_HEADROOM: usize = 4;
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

    /// Full collections so far: the epoch every per-mutator cache of heap
    /// state (a TLAB's block, its type objects) is keyed by.
    #[inline]
    pub(crate) fn full_collections(&self) -> usize {
        self.full_collections.load(Ordering::Relaxed)
    }

    /// Called with all mutators parked, after a full collection. `live_bytes`
    /// is what the survivors occupy: their cells and what they keep alive
    /// outside them.
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
    use super::*;
    use crate::{Tlab, descriptor::OBJECT_CELL_BYTES, space::block::USABLE_BLOCK_BYTES};
    use bex_vm_types::Object;

    const BUDGET: isize = MIN_FULL_BUDGET as isize;
    /// What one `Object` cell spends.
    const CELL: isize = OBJECT_CELL_BYTES as isize;

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

    /// A full collection settles what was spent: the budget restarts from
    /// zero, the collection is counted, and nothing is due any more.
    #[test]
    fn a_full_collection_resets_spending() {
        let heap = BexHeap::new(vec![]);
        heap.gc_policy.adjust(BUDGET);
        assert!(heap.should_gc());
        assert!(heap.gc_pressure().load(Ordering::Relaxed));
        // SAFETY: standalone heap, no concurrent users or retained pointers.
        unsafe {
            heap.collect_garbage(&[]);
        }
        assert_eq!(heap.gc_budget().bytes_since_full_gc, 0);
        assert_eq!(heap.gc_budget().full_collections, 1);
        assert!(!heap.should_gc());
    }

    /// A full collection makes every outstanding balance stale: what it
    /// counted is in the census now, so invalidating the allocator discards
    /// it rather than settling it a second time.
    #[test]
    fn invalidation_discards_the_balance_after_a_full_collection() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(heap.clone());
        tlab.alloc_uint8array(vec![0; 1000]);
        assert_eq!(tlab.alloc_debt().balance(), CELL + 1000);
        // SAFETY: single-threaded heap with no surviving roots.
        unsafe {
            heap.collect_garbage(&[]);
        }
        tlab.invalidate();
        assert_eq!(tlab.alloc_debt().balance(), 0);
        assert_eq!(heap.gc_budget().bytes_since_full_gc, 0);
    }

    /// Every cell spends its class size; taking a block settles the balance.
    #[test]
    fn cells_spend_their_size() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new_empty(heap.clone());
        tlab.alloc(Object::String(("inline").into()));
        assert_eq!(
            heap.gc_budget().bytes_since_full_gc,
            CELL,
            "the first cell took a block, which settled it"
        );
        for _ in 0..10 {
            tlab.alloc(Object::String(("inline").into()));
        }
        assert_eq!(heap.gc_budget().bytes_since_full_gc, CELL);
        tlab.flush_alloc_debt();
        assert_eq!(heap.gc_budget().bytes_since_full_gc, 11 * CELL);
    }

    /// A collection stales the allocator's block and zeroes the budget; the
    /// next allocation takes a fresh block and is the only spend since.
    #[test]
    fn a_collection_stales_the_block_and_the_next_cell_starts_the_budget() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new_empty(heap.clone());
        for _ in 0..65 {
            tlab.alloc(Object::String(("inline").into()));
        }
        // SAFETY: single-threaded heap with no surviving roots.
        unsafe {
            heap.collect_garbage(&[]);
        }
        tlab.invalidate();
        assert_eq!(tlab.remaining(), 0, "the block is stale");
        tlab.alloc(Object::String(("inline").into()));
        assert_eq!(tlab.remaining(), USABLE_BLOCK_BYTES - OBJECT_CELL_BYTES);
        drop(tlab);
        assert_eq!(heap.gc_budget().bytes_since_full_gc, CELL);
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
        // 8 copies stay under the floor.
        let text = bex_str::BexStr::from("x".repeat(1 << 18));
        let whole = text.unshared_heap_bytes() as isize;
        for _ in 0..8 {
            tlab.alloc(Object::String((text.clone()).into()));
            tlab.alloc(Object::String((text.byte_slice(1..128)).into()));
        }
        tlab.flush_alloc_debt();
        let spent = heap.gc_budget().bytes_since_full_gc;
        assert_eq!(spent, 16 * CELL + 8 * whole + 8 * 127);
        assert!(!heap.should_gc());
    }

    /// A balance too small to settle on its own is settled when the TLAB
    /// takes a block, when its owner asks, and when the TLAB is dropped. The
    /// TLAB starts with a block, so the first allocation does not take one.
    #[test]
    fn small_payloads_settle_at_refill_on_request_and_on_drop() {
        const SMALL: isize = CELL + 10;
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(heap.clone());
        tlab.alloc_uint8array(vec![0; 1000]);
        assert_eq!(heap.gc_budget().bytes_since_full_gc, 0);
        assert_eq!(tlab.alloc_debt().balance(), CELL + 1000);

        tlab.flush_alloc_debt();
        assert_eq!(heap.gc_budget().bytes_since_full_gc, CELL + 1000);
        assert_eq!(tlab.alloc_debt().balance(), 0);

        let mut filled = 0;
        while tlab.is_valid() {
            tlab.alloc_uint8array(vec![0; 10]);
            filled += 1;
        }
        assert_eq!(heap.gc_budget().bytes_since_full_gc, CELL + 1000);
        // This allocation takes a new block, settling everything with it.
        tlab.alloc_uint8array(vec![0; 10]);
        let settled = CELL + 1000 + (filled + 1) * SMALL;
        assert_eq!(heap.gc_budget().bytes_since_full_gc, settled);

        tlab.alloc_uint8array(vec![0; 10]);
        drop(tlab);
        assert_eq!(heap.gc_budget().bytes_since_full_gc, settled + SMALL);
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
