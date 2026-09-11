//! Replaceable, opt-in policies for runtime experiments. Not a shipping default.
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use bex_vm_types::{Object, Value, types::AllocationAccount};
use web_time::{Duration, Instant};

use crate::{
    BexHeap, CollectionLevel,
    gc_adaptive::{AdaptiveConfig, AdaptivePolicy, AdaptiveSnapshot},
};

#[derive(Clone, Copy, Debug)]
pub struct GcExperimentConfig {
    /// Three-generation feedback controller; None preserves earlier baselines.
    pub adaptive: Option<AdaptiveConfig>,
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
    adaptive: Option<AdaptivePolicy>,
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
    pub adaptive: Option<AdaptiveSnapshot>,
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
        assert!(
            config
                .adaptive
                .is_none_or(|a| a.gen1_budget_floor <= config.full_budget_floor)
        );
        Self {
            adaptive: config.adaptive.map(|a| {
                AdaptivePolicy::new(
                    a,
                    config.young_budget.expect("adaptive requires young budget"),
                )
            }),
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

    fn charge_payload(&self, bytes: usize) {
        if self
            .config
            .adaptive
            .and_then(|a| a.large_payload_threshold)
            .is_some_and(|limit| bytes >= limit)
        {
            // Large backing allocations spend the full allowance directly;
            // their small object headers still spend the nursery reservation.
            // Storage remains in Gen0; this is a trigger experiment, not a LOH.
            self.adaptive.as_ref().unwrap().charge_growth(bytes);
            saturating_add(&self.full_spent, bytes);
            saturating_add(&self.total_charged, bytes);
            if self.due().is_some() {
                self.pressure.store(true, Ordering::Relaxed);
            }
        } else {
            self.charge(bytes);
        }
    }

    pub fn snapshot(&self) -> GcExperimentSnapshot {
        GcExperimentSnapshot {
            adaptive: self.adaptive.as_ref().map(AdaptivePolicy::snapshot),
            young_spent: self.young_spent.load(Ordering::Relaxed),
            full_spent: self.full_spent.load(Ordering::Relaxed),
            full_budget: self.full_budget.load(Ordering::Relaxed),
            total_charged: self.total_charged.load(Ordering::Relaxed),
            last_live_bytes: self.last_live_bytes.load(Ordering::Relaxed),
            collections: self.collections.load(Ordering::Relaxed),
        }
    }

    pub fn due(&self) -> Option<CollectionLevel> {
        if let Some(adaptive) = &self.adaptive {
            if adaptive.full_only() {
                // Young GC did not reset this counter. Switching modes must
                // not grant another full allowance on top of prior spending.
                return (self.full_spent.load(Ordering::Relaxed)
                    >= self.full_budget.load(Ordering::Relaxed))
                .then_some(CollectionLevel::Major);
            }
            return adaptive.due(
                self.young_spent.load(Ordering::Relaxed),
                self.full_budget.load(Ordering::Relaxed),
            );
        }
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
        promoted: [usize; 2],
        elapsed: Duration,
        reclaimed_slots: usize,
    ) {
        self.collections.fetch_add(1, Ordering::Relaxed);
        let young_spent = self.young_spent.swap(0, Ordering::Relaxed);
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
        if let Some(adaptive) = &self.adaptive {
            adaptive.after_gc(
                level,
                young_spent,
                promoted,
                self.full_budget.load(Ordering::Relaxed),
                elapsed,
                reclaimed_slots,
            );
        }
        // An explicit minor must not satisfy a due full collection.
        self.pressure.store(self.due().is_some(), Ordering::Relaxed);
    }
}

pub(crate) fn saturating_add(counter: &AtomicUsize, amount: usize) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
        Some(old.saturating_add(amount))
    });
}

impl AllocationAccount for GcExperiment {
    fn charge_growth(&self, bytes: usize) {
        if bytes == 0 {
            return;
        }
        if let Some(adaptive) = &self.adaptive {
            adaptive.charge_growth(bytes);
        }
        self.charge(bytes);
    }

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

    pub(crate) fn charge_gc_payload_bytes(&self, bytes: usize) {
        if let Some(policy) = self.gc_experiment() {
            policy.charge_payload(bytes);
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
    pub(crate) unsafe fn finish_gc_experiment(
        &self,
        level: CollectionLevel,
        promoted_counts: [usize; 2],
        started: Option<Instant>,
        reclaimed_slots: usize,
        full_young_survived: Option<usize>,
    ) {
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
            let mut promoted = [0usize; 2];
            // On Major, source-Gen0 survival is used only to reconsider backoff;
            // these bytes went directly to Gen2, not through Gen1.
            promoted[0] = full_young_survived.unwrap_or(0);
            if policy.adaptive.is_some() && level != CollectionLevel::Major {
                // Only scan the newly copied tails, never the entire old heap.
                for (index, space) in [unsafe { self.gen1_ref() }, unsafe { self.gen2_ref() }]
                    .into_iter()
                    .enumerate()
                {
                    for i in space.len().saturating_sub(promoted_counts[index])..space.len() {
                        promoted[index] = promoted[index]
                            .saturating_add(size_of::<Object>())
                            .saturating_add(payload_bytes(space.get(i)));
                    }
                }
            }
            policy.after_gc(
                level,
                live_bytes,
                live_slot_bytes,
                promoted,
                started.map_or(Duration::ZERO, |t| t.elapsed()),
                reclaimed_slots,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tlab;

    #[test]
    fn survivor_backoff_preserves_spending_and_eventually_probes_young_again() {
        let p = GcExperiment::new(GcExperimentConfig {
            adaptive: Some(AdaptiveConfig {
                gen1_budget_floor: 200,
                growth_percent: 25,
                gen1_mode: crate::gc_adaptive::Gen1Mode::Minor,
                large_payload_threshold: None,
                survival_backoff: 2,
                survival_early_probe: false,
                gc_time_percent: 5,
            }),
            young_budget: Some(100),
            full_budget_floor: 1000,
            live_multiplier: 1,
            live_budget_uses_slots: true,
            first_chunk: 32,
            max_chunk: 1024,
            poll_interval: 64,
        });
        for _ in 0..2 {
            p.charge(150);
            // Synthetic long pause makes the feedback deterministic without sleeping.
            p.after_gc(
                CollectionLevel::Nursery,
                0,
                0,
                [140, 0],
                Duration::from_secs(60),
                10,
            );
        }
        assert!(p.adaptive.as_ref().unwrap().full_only());
        assert_eq!(p.snapshot().full_spent, 300);
        assert_eq!(p.due(), None); // Gen1 debt must not immediately force collection.
        p.charge(700);
        assert_eq!(p.due(), Some(CollectionLevel::Major));
        p.after_gc(CollectionLevel::Minor, 0, 0, [0, 0], Duration::ZERO, 0);
        assert_eq!(p.due(), Some(CollectionLevel::Major));
        p.after_gc(CollectionLevel::Major, 0, 0, [0, 0], Duration::ZERO, 0);
        assert!(p.adaptive.as_ref().unwrap().full_only());
        p.charge(1000);
        assert_eq!(p.due(), Some(CollectionLevel::Major));
        p.after_gc(CollectionLevel::Major, 0, 0, [0, 0], Duration::ZERO, 0);
        assert!(!p.adaptive.as_ref().unwrap().full_only());
        p.charge(500);
        assert_eq!(p.due(), Some(CollectionLevel::Nursery));
        // A costly collection with few survivors must not re-enter backoff.
        p.after_gc(
            CollectionLevel::Nursery,
            0,
            0,
            [0, 0],
            Duration::from_secs(60),
            500,
        );
        assert!(!p.adaptive.as_ref().unwrap().full_only());
    }

    #[test]
    fn full_feedback_distinguishes_old_cache_from_young_survivors() {
        for keep_young in [false, true] {
            let heap = BexHeap::new(vec![]);
            heap.configure_gc_experiment(GcExperimentConfig {
                adaptive: Some(AdaptiveConfig {
                    gen1_budget_floor: 200,
                    growth_percent: 25,
                    gen1_mode: crate::gc_adaptive::Gen1Mode::Minor,
                    large_payload_threshold: None,
                    survival_backoff: 2,
                    survival_early_probe: true,
                    gc_time_percent: 5,
                }),
                young_budget: Some(100),
                full_budget_floor: 1000,
                live_multiplier: 1,
                live_budget_uses_slots: true,
                first_chunk: 32,
                max_chunk: 1024,
                poll_interval: 64,
            });
            let mut tlab = Tlab::new_empty(heap.clone());
            let old = tlab.alloc_string("old cache".repeat(2000));
            drop(tlab);
            // SAFETY: this thread owns all roots and no mutators are running.
            let (_, mut roots, _) = unsafe { heap.collect_garbage(&[old]) };
            let policy = heap.gc_experiment().unwrap();
            // Seed the controller with two costly young-survivor observations.
            for _ in 0..2 {
                policy.charge(150);
                policy.after_gc(
                    CollectionLevel::Nursery,
                    0,
                    0,
                    [140, 0],
                    Duration::from_secs(60),
                    10,
                );
            }
            assert!(policy.adaptive.as_ref().unwrap().full_only());
            let mut tlab = Tlab::new_empty(heap.clone());
            let young = tlab.alloc_string("new allocation".repeat(1000));
            drop(tlab);
            if keep_young {
                roots.push(young);
            }
            // SAFETY: root vector is complete; all allocation scopes have ended.
            let (stats, _, _) = unsafe { heap.collect_garbage(&roots) };
            assert_eq!(stats.live_count, 1 + usize::from(keep_young));
            assert_eq!(policy.adaptive.as_ref().unwrap().full_only(), keep_young);
        }
    }

    #[test]
    fn large_payload_debt_bypasses_nursery_and_survives_explicit_minor() {
        let p = GcExperiment::new(GcExperimentConfig {
            adaptive: Some(AdaptiveConfig {
                gen1_budget_floor: 1024,
                growth_percent: 25,
                gen1_mode: crate::gc_adaptive::Gen1Mode::Minor,
                large_payload_threshold: Some(512),
                survival_backoff: 0,
                survival_early_probe: false,
                gc_time_percent: 5,
            }),
            young_budget: Some(512),
            full_budget_floor: 2048,
            live_multiplier: 1,
            live_budget_uses_slots: true,
            first_chunk: 32,
            max_chunk: 1024,
            poll_interval: 64,
        });
        p.charge_payload(1024);
        assert_eq!(p.snapshot().young_spent, 0);
        assert_eq!(p.due(), None);
        p.charge_payload(1024);
        assert_eq!(p.due(), Some(CollectionLevel::Major));
        p.after_gc(CollectionLevel::Minor, 0, 0, [0, 0], Duration::ZERO, 0);
        assert_eq!(p.due(), Some(CollectionLevel::Major));
        assert_eq!(p.snapshot().total_charged, 2048);
    }

    #[test]
    fn minor_collection_does_not_erase_full_gc_spending() {
        let heap = BexHeap::new(vec![]);
        heap.configure_gc_experiment(GcExperimentConfig {
            adaptive: None,
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
            adaptive: None,
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
