//! Allocation spending requests a full collection at the next VM safe point.
//!
//! This is headroom between collections, not a hard heap or RSS limit. Charges
//! include reserved object slots and shallow estimates of known backing storage.
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use bex_vm_types::{
    Object, Value,
    types::{AllocationAccount, LockedWriteGuard},
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

impl AllocationAccount for AllocationBudget {
    fn charge(&self, bytes: usize) {
        self.charge(bytes);
    }
}

/// Shallow known backing storage. Shared strings are charged per reference;
/// slices count their retained parent. Map estimates exclude hash-index overhead
/// and out-of-line keys. Nested type metadata, opaque Rust/host data, futures,
/// and runtime packages are excluded. These estimates do not bound RSS.
pub(crate) fn payload_bytes(object: &Object) -> usize {
    match object {
        Object::String(s) => s.heap_size_estimate(),
        Object::Array(a) => a
            .lock()
            .capacity()
            .saturating_mul(size_of::<Value>())
            .saturating_add(size_of::<bex_vm_types::RealizedTy>()),
        Object::Uint8Array(a) => a.lock().capacity(),
        Object::Map(m) => map_capacity_bytes(&m.lock()),
        Object::Instance(i) => i
            .fields
            .capacity()
            .saturating_mul(size_of::<bex_vm_types::AtomicValueSlot>())
            .saturating_add(size_of_val(i.class_type_args.as_ref())),
        _ => 0,
    }
}

/// Estimate entry capacity, excluding the map's separate hash index.
pub fn map_capacity_bytes(map: &indexmap::IndexMap<bex_str::BexStr, Value>) -> usize {
    size_of_val(map).saturating_add(
        map.capacity()
            .saturating_mul(size_of::<(bex_str::BexStr, Value)>()),
    )
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

    /// Charge positive backing-capacity growth when the write guard is dropped.
    pub fn account_container_growth<'a, T>(
        &'a self,
        guard: LockedWriteGuard<'a, T>,
        estimate: fn(&T) -> usize,
    ) -> LockedWriteGuard<'a, T> {
        guard.with_allocation_accounting(&self.gc_policy, estimate)
    }
}

#[cfg(test)]
mod tests {
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
    fn few_large_payloads_and_container_growth_spend_budget() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new_empty(heap.clone());
        tlab.alloc_string("x".repeat(MIN_FULL_BUDGET));
        assert!(heap.should_gc());
        let container = bex_vm_types::types::LockedContainer::new(Vec::<u8>::new());
        let before = heap.gc_budget().bytes_since_full_gc;
        let capacity;
        {
            let mut guard = heap.account_container_growth(container.lock_mut(), Vec::capacity);
            guard.reserve(4096);
            capacity = guard.capacity();
        }
        assert_eq!(heap.gc_budget().bytes_since_full_gc, before + capacity);
        {
            let mut guard = heap.account_container_growth(container.lock_mut(), Vec::capacity);
            guard.push(1);
        }
        assert_eq!(heap.gc_budget().bytes_since_full_gc, before + capacity);
    }
}
