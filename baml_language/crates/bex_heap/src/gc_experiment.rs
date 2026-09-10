//! Replaceable, opt-in policies for runtime experiments. Not a shipping default.
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use bex_vm_types::{Object, Value, types::AllocationAccount};

use crate::{BexHeap, CollectionLevel};

#[derive(Clone, Copy, Debug)]
pub struct GcExperimentConfig {
    /// None selects full-only; Some selects an independent young budget.
    pub young_budget: Option<usize>,
    pub full_budget_floor: usize,
    pub live_multiplier: usize,
    pub live_budget_uses_slots: bool,
    pub first_chunk: usize,
    pub max_chunk: usize,
    pub poll_interval: u64,
}

pub struct GcExperiment {
    pub config: GcExperimentConfig,
    pub pressure: Arc<AtomicBool>,
    young_spent: AtomicUsize,
    full_spent: AtomicUsize,
    full_budget: AtomicUsize,
    total_charged: AtomicUsize,
    last_live_bytes: AtomicUsize,
    collections: AtomicUsize,
}

#[derive(Clone, Copy, Debug)]
pub struct GcExperimentSnapshot {
    pub young_spent: usize,
    pub full_spent: usize,
    pub full_budget: usize,
    pub total_charged: usize,
    pub last_live_bytes: usize,
    pub collections: usize,
}

impl GcExperiment {
    pub(crate) fn new(config: GcExperimentConfig) -> Self {
        assert!(config.full_budget_floor > 0);
        assert!(config.live_multiplier > 0);
        assert!(config.young_budget.is_none_or(|n| n > 0));
        assert!(config.first_chunk > 0 && config.first_chunk <= config.max_chunk);
        assert!(config.poll_interval > 0);
        Self {
            config,
            pressure: Arc::new(AtomicBool::new(false)),
            young_spent: AtomicUsize::new(0),
            full_spent: AtomicUsize::new(0),
            full_budget: AtomicUsize::new(config.full_budget_floor),
            total_charged: AtomicUsize::new(0),
            last_live_bytes: AtomicUsize::new(0),
            collections: AtomicUsize::new(0),
        }
    }

    pub fn snapshot(&self) -> GcExperimentSnapshot {
        GcExperimentSnapshot {
            young_spent: self.young_spent.load(Ordering::Relaxed),
            full_spent: self.full_spent.load(Ordering::Relaxed),
            full_budget: self.full_budget.load(Ordering::Relaxed),
            total_charged: self.total_charged.load(Ordering::Relaxed),
            last_live_bytes: self.last_live_bytes.load(Ordering::Relaxed),
            collections: self.collections.load(Ordering::Relaxed),
        }
    }

    pub fn due(&self) -> Option<CollectionLevel> {
        if self.full_spent.load(Ordering::Relaxed) >= self.full_budget.load(Ordering::Relaxed) {
            Some(CollectionLevel::Major)
        } else if self
            .config
            .young_budget
            .is_some_and(|budget| self.young_spent.load(Ordering::Relaxed) >= budget)
        {
            Some(CollectionLevel::Minor)
        } else {
            None
        }
    }

    /// Called only while GC holds exclusive heap access. Copying is not charged.
    pub(crate) fn after_gc(
        &self,
        level: CollectionLevel,
        live_bytes: usize,
        live_slot_bytes: usize,
    ) {
        self.collections.fetch_add(1, Ordering::Relaxed);
        self.young_spent.store(0, Ordering::Relaxed);
        if level == CollectionLevel::Major {
            self.full_spent.store(0, Ordering::Relaxed);
            self.last_live_bytes.store(live_bytes, Ordering::Relaxed);
            let growth_basis = if self.config.live_budget_uses_slots {
                live_slot_bytes
            } else {
                live_bytes
            };
            self.full_budget.store(
                growth_basis
                    .saturating_mul(self.config.live_multiplier)
                    .max(self.config.full_budget_floor),
                Ordering::Relaxed,
            );
        }
        // An explicit minor must not satisfy a due full collection.
        self.pressure.store(self.due().is_some(), Ordering::Relaxed);
    }
}

fn saturating_add(counter: &AtomicUsize, amount: usize) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
        Some(old.saturating_add(amount))
    });
}

impl AllocationAccount for GcExperiment {
    fn charge(&self, bytes: usize) {
        if bytes == 0 {
            return;
        }
        saturating_add(&self.young_spent, bytes);
        saturating_add(&self.full_spent, bytes);
        saturating_add(&self.total_charged, bytes);
        if self.due().is_some() {
            // Request cooperation, never run moving GC while an allocation or
            // container mutation may still have unpublished roots.
            self.pressure.store(true, Ordering::Relaxed);
        }
    }
}

/// Shallow known backing storage. Shared strings are charged per reference;
/// survivor estimates use the same convention. Map capacity estimates include
/// entry storage, but not hash-index overhead or out-of-line key growth.
/// Nested type metadata, opaque Rust/host data, futures and runtime packages
/// remain unaccounted. This is explicitly experimental, not an RSS bound.
pub fn payload_bytes(object: &Object) -> usize {
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

pub fn map_capacity_bytes(map: &indexmap::IndexMap<bex_str::BexStr, Value>) -> usize {
    size_of_val(map).saturating_add(
        map.capacity()
            .saturating_mul(size_of::<(bex_str::BexStr, Value)>()),
    )
}

impl BexHeap {
    /// Configure once, before starting any calls. Available only in experiment builds.
    pub fn configure_gc_experiment(&self, config: GcExperimentConfig) {
        assert_eq!(
            self.stats().runtime_objects,
            0,
            "configure before allocation"
        );
        assert!(
            self.gc_experiment.set(GcExperiment::new(config)).is_ok(),
            "already configured"
        );
    }

    pub fn gc_experiment(&self) -> Option<&GcExperiment> {
        self.gc_experiment.get()
    }

    pub(crate) fn charge_gc_bytes(&self, bytes: usize) {
        if let Some(policy) = self.gc_experiment() {
            policy.charge(bytes);
        }
    }

    pub fn account_container_growth<'a, T>(
        &'a self,
        guard: bex_vm_types::types::LockedWriteGuard<'a, T>,
        estimate: fn(&T) -> usize,
    ) -> bex_vm_types::types::LockedWriteGuard<'a, T> {
        match self.gc_experiment() {
            Some(policy) => guard.with_allocation_accounting(policy, estimate),
            None => guard,
        }
    }

    /// # Safety
    /// Requires exclusive GC access, after forwarding and reclamation.
    pub(crate) unsafe fn finish_gc_experiment(&self, level: CollectionLevel) {
        if let Some(policy) = self.gc_experiment() {
            let mut live_bytes = 0usize;
            let mut live_slot_bytes = 0usize;
            if level == CollectionLevel::Major {
                // SAFETY: all mutators are parked and all survivors are in Gen2.
                let old = unsafe { self.gen2_ref() };
                live_slot_bytes = old.len().saturating_mul(size_of::<Object>());
                live_bytes = live_slot_bytes;
                for i in 0..old.len() {
                    live_bytes = live_bytes.saturating_add(payload_bytes(old.get(i)));
                }
            }
            policy.after_gc(level, live_bytes, live_slot_bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tlab;

    #[test]
    fn minor_collection_does_not_erase_full_gc_spending() {
        let heap = BexHeap::new(vec![]);
        heap.configure_gc_experiment(GcExperimentConfig {
            young_budget: Some(512),
            full_budget_floor: 1024,
            live_multiplier: 1,
            live_budget_uses_slots: false,
            first_chunk: 32,
            max_chunk: 1024,
            poll_interval: 64,
        });
        let mut tlab = Tlab::new_empty(heap.clone());
        tlab.alloc_string("garbage");
        assert_eq!(heap.should_collect(), Some(CollectionLevel::Major));
        let spent = heap.gc_experiment().unwrap().snapshot().full_spent;
        // SAFETY: standalone heap; this thread has exclusive access.
        unsafe {
            heap.collect_garbage_minor(&[]);
        }
        assert_eq!(heap.gc_experiment().unwrap().snapshot().full_spent, spent);
        assert_eq!(heap.should_collect(), Some(CollectionLevel::Major));
        // SAFETY: standalone heap; no retained raw pointers or concurrent users.
        unsafe {
            heap.collect_garbage(&[]);
        }
        assert!(heap.should_collect().is_none());
        assert_eq!(heap.gc_experiment().unwrap().snapshot().full_spent, 0);
    }

    #[test]
    fn growing_reservations_do_not_waste_a_chunk_at_power_of_two_boundaries() {
        let heap = BexHeap::new(vec![]);
        heap.configure_gc_experiment(GcExperimentConfig {
            young_budget: None,
            full_budget_floor: 1024 * 1024,
            live_multiplier: 1,
            live_budget_uses_slots: false,
            first_chunk: 32,
            max_chunk: 1024,
            poll_interval: 64,
        });
        let mut tlab = Tlab::new_empty(heap.clone());
        for _ in 0..4096 {
            tlab.alloc_string("inline");
        }
        // Inline payloads cost no backing storage. Debug canaries are not
        // charged; this verifies the actual reservation sequence in both builds.
        assert_eq!(
            heap.gc_experiment().unwrap().snapshot().total_charged,
            4096 * size_of::<Object>()
        );
        tlab.alloc_string("inline");
        assert_eq!(
            heap.gc_experiment().unwrap().snapshot().total_charged,
            5120 * size_of::<Object>()
        );
    }
}
