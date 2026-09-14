//! Allocation spending requests a collection at the next VM safe point.
//!
//! This is headroom between collections, not a hard heap or RSS limit. Charges
//! count reserved object slots only, including unused TLAB capacity. Indirect
//! backing storage (strings, containers, images, etc.) does not spend this budget.
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use crate::{BexHeap, CollectionLevel};

pub(crate) const MIN_MINOR_BUDGET: usize = 16 * 1024 * 1024;
pub(crate) const MIN_FULL_BUDGET: usize = 32 * 1024 * 1024;
const LIVE_HEADROOM: usize = 4;
/// High survival makes copying young generations poor value. Retry periodically
/// so a later low-survival workload can restore minor collection automatically.
const HIGH_SURVIVAL_BACKOFF_FULL_CYCLES: usize = 4;
pub(crate) const FIRST_TLAB_SLOTS: usize = 32;
pub(crate) const MAX_TLAB_SLOTS: usize = 1024;
/// Number of existing VM control-flow checks between pressure/park polls.
pub const POLL_INTERVAL: u64 = 4096;

/// Diagnostic snapshot. Concurrent allocation may advance spending while read.
#[derive(Debug, Clone, Copy)]
pub struct GcBudgetSnapshot {
    /// Bytes of object slots reserved since the last minor or full GC.
    pub bytes_since_minor_gc: usize,
    /// Bytes of object slots reserved since the last full GC; excludes payloads.
    pub bytes_since_full_gc: usize,
    pub minor_budget_bytes: usize,
    pub full_budget_bytes: usize,
    /// Remaining full collections before probing minor GC after high survival.
    pub minor_backoff_full_collections: usize,
    pub minor_collections: usize,
    pub full_collections: usize,
}

pub(crate) struct AllocationBudget {
    full_spent: AtomicUsize,
    next_collection_spent: AtomicUsize,
    pressure: Arc<AtomicBool>,
    cold: Box<AllocationBudgetCold>,
}

struct AllocationBudgetCold {
    minor_checkpoint: AtomicUsize,
    full_budget: AtomicUsize,
    minor_backoff_full_collections: AtomicUsize,
    minor_collections: AtomicUsize,
    full_collections: AtomicUsize,
}

fn add_saturating(counter: &AtomicUsize, bytes: usize) -> usize {
    let old = counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
            Some(old.saturating_add(bytes))
        })
        .expect("update always succeeds");
    old.saturating_add(bytes)
}

fn minor_budget_for(full_budget: usize) -> usize {
    (full_budget / 2).max(MIN_MINOR_BUDGET)
}

impl AllocationBudget {
    pub(crate) fn new() -> Self {
        Self {
            full_spent: AtomicUsize::new(0),
            next_collection_spent: AtomicUsize::new(MIN_MINOR_BUDGET),
            pressure: Arc::new(AtomicBool::new(false)),
            cold: Box::new(AllocationBudgetCold {
                minor_checkpoint: AtomicUsize::new(0),
                full_budget: AtomicUsize::new(MIN_FULL_BUDGET),
                minor_backoff_full_collections: AtomicUsize::new(0),
                minor_collections: AtomicUsize::new(0),
                full_collections: AtomicUsize::new(0),
            }),
        }
    }

    pub(crate) fn charge(&self, bytes: usize) {
        if bytes == 0 {
            return;
        }
        let full_spent = add_saturating(&self.full_spent, bytes);
        if full_spent >= self.next_collection_spent.load(Ordering::Relaxed) {
            // No collection here: allocation and mutation can hold unpublished
            // pointers or container locks. The VM cooperates at a safe point.
            self.pressure.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn due(&self) -> Option<CollectionLevel> {
        self.due_at(self.full_spent.load(Ordering::Relaxed))
    }

    fn due_at(&self, full_spent: usize) -> Option<CollectionLevel> {
        if full_spent >= self.cold.full_budget.load(Ordering::Relaxed) {
            Some(CollectionLevel::Major)
        } else if full_spent >= self.next_collection_spent.load(Ordering::Relaxed) {
            Some(CollectionLevel::Minor)
        } else {
            None
        }
    }

    /// Called with all mutators parked after a minor collection. Young debt is
    /// paid, but cumulative debt remains so repeated minors cannot postpone a full GC.
    pub(crate) fn after_minor(&self, young_slots_before: usize, young_live_slots: usize) {
        let full_spent = self.full_spent.load(Ordering::Relaxed);
        self.cold
            .minor_checkpoint
            .store(full_spent, Ordering::Relaxed);
        self.cold.minor_collections.fetch_add(1, Ordering::Relaxed);
        if young_slots_before > 0 && young_live_slots.saturating_mul(2) >= young_slots_before {
            self.cold
                .minor_backoff_full_collections
                .store(HIGH_SURVIVAL_BACKOFF_FULL_CYCLES, Ordering::Relaxed);
            self.next_collection_spent.store(
                self.cold.full_budget.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
        } else {
            let full_budget = self.cold.full_budget.load(Ordering::Relaxed);
            self.next_collection_spent.store(
                full_spent
                    .saturating_add(minor_budget_for(full_budget))
                    .min(full_budget),
                Ordering::Relaxed,
            );
        }
        self.pressure.store(self.due().is_some(), Ordering::Relaxed);
    }

    /// Called with all mutators parked, after full collection has moved survivors.
    /// Collector copies are not allocation spending.
    pub(crate) fn after_full(&self, live_slot_bytes: usize) {
        let full_budget = live_slot_bytes
            .saturating_mul(LIVE_HEADROOM)
            .max(MIN_FULL_BUDGET);
        self.full_spent.store(0, Ordering::Relaxed);
        self.cold.minor_checkpoint.store(0, Ordering::Relaxed);
        let minor_budget = minor_budget_for(full_budget);
        self.cold.full_budget.store(full_budget, Ordering::Relaxed);
        let previous_backoff = self
            .cold
            .minor_backoff_full_collections
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                Some(remaining.saturating_sub(1))
            })
            .expect("update always succeeds");
        let remaining_backoff = previous_backoff.saturating_sub(1);
        self.next_collection_spent.store(
            if remaining_backoff == 0 {
                minor_budget
            } else {
                full_budget
            },
            Ordering::Relaxed,
        );
        self.pressure.store(false, Ordering::Relaxed);
        self.cold.full_collections.fetch_add(1, Ordering::Relaxed);
    }
}

impl BexHeap {
    pub fn gc_budget(&self) -> GcBudgetSnapshot {
        let full_spent = self.gc_policy.full_spent.load(Ordering::Relaxed);
        GcBudgetSnapshot {
            bytes_since_minor_gc: full_spent
                .saturating_sub(self.gc_policy.cold.minor_checkpoint.load(Ordering::Relaxed)),
            bytes_since_full_gc: full_spent,
            minor_budget_bytes: minor_budget_for(
                self.gc_policy.cold.full_budget.load(Ordering::Relaxed),
            ),
            full_budget_bytes: self.gc_policy.cold.full_budget.load(Ordering::Relaxed),
            minor_backoff_full_collections: self
                .gc_policy
                .cold
                .minor_backoff_full_collections
                .load(Ordering::Relaxed),
            minor_collections: self
                .gc_policy
                .cold
                .minor_collections
                .load(Ordering::Relaxed),
            full_collections: self.gc_policy.cold.full_collections.load(Ordering::Relaxed),
        }
    }

    pub fn gc_pressure(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.gc_policy.pressure)
    }

    /// Skip automatic minor collection until the current full-GC interval
    /// ends. Returns false if a major collection is already due.
    pub fn suppress_minor_collection(&self) -> bool {
        if self.gc_policy.due() == Some(CollectionLevel::Major) {
            return false;
        }
        self.gc_policy.next_collection_spent.store(
            self.gc_policy.cold.full_budget.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        self.gc_policy
            .pressure
            .store(self.gc_policy.due().is_some(), Ordering::Relaxed);
        // Do not lose a concurrent charge that crossed the full threshold
        // between the preceding due check and pressure update.
        if self.gc_policy.due().is_some() {
            self.gc_policy.pressure.store(true, Ordering::Relaxed);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use bex_vm_types::Object;

    use super::*;
    use crate::{CollectionLevel, Tlab};

    #[test]
    fn minor_and_full_budgets_track_independent_spending() {
        let p = AllocationBudget::new();
        p.charge(0);
        assert_eq!(p.due(), None);
        p.charge(MIN_MINOR_BUDGET - 1);
        assert_eq!(p.due(), None);
        p.charge(1);
        assert_eq!(p.due(), Some(CollectionLevel::Minor));
        assert!(p.pressure.load(Ordering::Relaxed));
        p.after_minor(100, 0);
        assert_eq!(p.due(), None);
        assert!(!p.pressure.load(Ordering::Relaxed));
        assert_eq!(p.full_spent.load(Ordering::Relaxed), MIN_MINOR_BUDGET);
        assert_eq!(p.cold.minor_collections.load(Ordering::Relaxed), 1);
        p.charge(MIN_FULL_BUDGET - MIN_MINOR_BUDGET);
        assert_eq!(p.due(), Some(CollectionLevel::Major));
        p.after_full(MIN_FULL_BUDGET);
        assert_eq!(p.due(), None);
        assert!(!p.pressure.load(Ordering::Relaxed));
        assert_eq!(
            p.cold.full_budget.load(Ordering::Relaxed),
            4 * MIN_FULL_BUDGET
        );
        assert_eq!(
            minor_budget_for(p.cold.full_budget.load(Ordering::Relaxed)),
            2 * MIN_FULL_BUDGET
        );
        p.after_full(0);
        assert_eq!(p.cold.full_budget.load(Ordering::Relaxed), MIN_FULL_BUDGET);
        assert_eq!(
            minor_budget_for(p.cold.full_budget.load(Ordering::Relaxed)),
            MIN_MINOR_BUDGET
        );
        p.charge(usize::MAX);
        p.charge(1);
        assert_eq!(p.full_spent.load(Ordering::Relaxed), usize::MAX);
        assert_eq!(
            p.full_spent
                .load(Ordering::Relaxed)
                .saturating_sub(p.cold.minor_checkpoint.load(Ordering::Relaxed)),
            usize::MAX
        );
        p.after_full(usize::MAX);
        assert_eq!(p.cold.full_budget.load(Ordering::Relaxed), usize::MAX);
        assert_eq!(
            minor_budget_for(p.cold.full_budget.load(Ordering::Relaxed)),
            usize::MAX / 2
        );
    }

    #[test]
    fn high_minor_survival_temporarily_falls_back_to_full_collections() {
        let p = AllocationBudget::new();
        p.charge(MIN_MINOR_BUDGET);
        assert_eq!(p.due(), Some(CollectionLevel::Minor));
        p.after_minor(100, 75);
        assert_eq!(
            p.cold
                .minor_backoff_full_collections
                .load(Ordering::Relaxed),
            HIGH_SURVIVAL_BACKOFF_FULL_CYCLES
        );
        p.charge(MIN_FULL_BUDGET - MIN_MINOR_BUDGET);
        for cycle in 0..HIGH_SURVIVAL_BACKOFF_FULL_CYCLES {
            assert_eq!(p.due(), Some(CollectionLevel::Major));
            p.after_full(0);
            if cycle + 1 < HIGH_SURVIVAL_BACKOFF_FULL_CYCLES {
                p.charge(MIN_FULL_BUDGET);
            }
        }
        p.charge(MIN_MINOR_BUDGET);
        assert_eq!(p.due(), Some(CollectionLevel::Minor));
    }

    #[test]
    fn deferred_minor_preserves_full_collection_debt() {
        let heap = BexHeap::new(vec![]);
        heap.gc_policy.charge(MIN_MINOR_BUDGET);
        assert_eq!(heap.should_collect(), Some(CollectionLevel::Minor));
        assert!(heap.suppress_minor_collection());
        assert_eq!(heap.should_collect(), None);
        assert!(!heap.gc_pressure().load(Ordering::Relaxed));
        heap.gc_policy.charge(MIN_FULL_BUDGET - MIN_MINOR_BUDGET);
        assert_eq!(heap.should_collect(), Some(CollectionLevel::Major));
        assert!(heap.gc_pressure().load(Ordering::Relaxed));
        assert!(!heap.suppress_minor_collection());
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
        assert_eq!(heap.gc_budget().bytes_since_minor_gc, 0);
        assert_eq!(heap.gc_budget().minor_collections, 1);
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
    fn large_and_shared_backing_storage_only_spends_object_slots() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new_empty(heap.clone());
        let text = bex_str::BexStr::from("x".repeat(MIN_FULL_BUDGET));
        tlab.alloc_string(text.clone());
        for _ in 0..1000 {
            tlab.alloc_string(text.clone());
            tlab.alloc_string(text.substring(1, 128));
        }
        tlab.alloc_uint8array(vec![0; MIN_FULL_BUDGET]);
        // These 2002 objects reserve 2048 slots, regardless of backing size or sharing.
        assert_eq!(
            heap.gc_budget().bytes_since_full_gc,
            2048 * size_of::<Object>()
        );
        assert!(!heap.should_gc());
    }
}
