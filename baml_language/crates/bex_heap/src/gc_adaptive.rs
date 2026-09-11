//! Experimental, stop-the-world generational trigger controller.
//! The constants are candidates for measurement, not .NET's tuning constants.
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use parking_lot::Mutex;
use web_time::{Duration, Instant};

use crate::CollectionLevel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gen1Mode {
    Minor,
    Full,
    CompareReclamation,
}

#[derive(Clone, Copy, Debug)]
pub struct AdaptiveConfig {
    pub gen1_budget_floor: usize,
    pub growth_percent: usize,
    pub gen1_mode: Gen1Mode,
    pub large_payload_threshold: Option<usize>,
    /// Number of full cycles before probing young collection again after two
    /// costly, high-survival young collections. Zero disables this experiment.
    pub survival_backoff: usize,
    /// End backoff early if full GC finds at most 20% of the newly allocated
    /// bytes still alive. This distinguishes cache construction from reuse.
    pub survival_early_probe: bool,
    /// Heap collection wall time as a percentage of elapsed time. This is a
    /// tuning signal, not a pause SLA or process CPU measurement.
    pub gc_time_percent: usize,
}

pub(crate) struct AdaptivePolicy {
    config: AdaptiveConfig,
    gen0_floor: usize,
    gen0_budget: AtomicUsize,
    gen1_budget: AtomicUsize,
    gen1_spent: AtomicUsize,
    old_spent: AtomicUsize,
    history: Mutex<History>,
    prefer_full: AtomicBool,
    full_only_remaining: AtomicUsize,
    counts: [AtomicUsize; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct AdaptiveSnapshot {
    pub gen0_budget: usize,
    pub gen1_budget: usize,
    pub gen1_spent: usize,
    pub old_spent: usize,
    /// Gen0-only, Gen0+Gen1, full.
    pub collections: [usize; 3],
    pub prefer_full: bool,
    pub full_only_remaining: usize,
}

struct History {
    last_end: Instant,
    // Reclaimed object-storage slots and collection duration. This measures
    // storage reclamation, not payload bytes or a count of dead objects.
    minor: Option<(usize, Duration)>,
    full: Option<(usize, Duration)>,
    full_since_probe: usize,
    costly_survivor_streak: usize,
}

fn prefer_full(minor: Option<(usize, Duration)>, full: Option<(usize, Duration)>) -> bool {
    match (minor, full) {
        (Some((minor_reclaimed, minor_time)), Some((full_reclaimed, full_time)))
            if full_reclaimed > 0 =>
        {
            // Require a 25% efficiency difference, not a timing tie.
            minor_reclaimed == 0
                || minor_time.as_secs_f64() * full_reclaimed as f64
                    > full_time.as_secs_f64() * minor_reclaimed as f64 * 1.25
        }
        _ => false,
    }
}

impl AdaptivePolicy {
    pub(crate) fn new(config: AdaptiveConfig, gen0_floor: usize) -> Self {
        assert!((1..=100).contains(&config.growth_percent));
        assert!(config.large_payload_threshold.is_none_or(|n| n > 0));
        assert!(gen0_floor > 0 && config.gen1_budget_floor >= gen0_floor);
        assert!((1..100).contains(&config.gc_time_percent));
        Self {
            config,
            gen0_floor,
            gen0_budget: AtomicUsize::new(gen0_floor),
            gen1_budget: AtomicUsize::new(config.gen1_budget_floor),
            gen1_spent: AtomicUsize::new(0),
            old_spent: AtomicUsize::new(0),
            history: Mutex::new(History {
                last_end: Instant::now(),
                minor: None,
                full: None,
                full_since_probe: 0,
                costly_survivor_streak: 0,
            }),
            prefer_full: AtomicBool::new(false),
            full_only_remaining: AtomicUsize::new(0),
            counts: std::array::from_fn(|_| AtomicUsize::new(0)),
        }
    }

    pub(crate) fn snapshot(&self) -> AdaptiveSnapshot {
        AdaptiveSnapshot {
            prefer_full: self.prefer_full.load(Ordering::Relaxed),
            full_only_remaining: self.full_only_remaining.load(Ordering::Relaxed),
            gen0_budget: self.gen0_budget.load(Ordering::Relaxed),
            gen1_budget: self.gen1_budget.load(Ordering::Relaxed),
            gen1_spent: self.gen1_spent.load(Ordering::Relaxed),
            old_spent: self.old_spent.load(Ordering::Relaxed),
            collections: self.counts.each_ref().map(|n| n.load(Ordering::Relaxed)),
        }
    }

    pub(crate) fn full_only(&self) -> bool {
        self.full_only_remaining.load(Ordering::Relaxed) > 0
    }

    pub(crate) fn due(&self, young_spent: usize, full_budget: usize) -> Option<CollectionLevel> {
        // Large allocation bursts can overshoot the nursery in one VM slice.
        // Use a full collection when they exhaust the whole growth allowance.
        if self.old_spent.load(Ordering::Relaxed) >= full_budget || young_spent >= full_budget {
            Some(CollectionLevel::Major)
        } else if self.gen1_spent.load(Ordering::Relaxed)
            >= self.gen1_budget.load(Ordering::Relaxed)
        {
            Some(
                if self.config.gen1_mode == Gen1Mode::Full
                    || (self.config.gen1_mode == Gen1Mode::CompareReclamation
                        && self.prefer_full.load(Ordering::Relaxed))
                {
                    CollectionLevel::Major
                } else {
                    CollectionLevel::Minor
                },
            )
        } else if young_spent >= self.gen0_budget.load(Ordering::Relaxed) {
            Some(CollectionLevel::Nursery)
        } else {
            None
        }
    }

    pub(crate) fn charge_growth(&self, bytes: usize) {
        // Mutation accounting currently doesn't carry the owner's generation.
        // Conservatively count all capacity growth as old-generation debt too,
        // so growing an already-old array cannot evade full collection.
        super::gc_experiment::saturating_add(&self.old_spent, bytes);
    }

    pub(crate) fn after_gc(
        &self,
        level: CollectionLevel,
        young_spent: usize,
        promoted: [usize; 2],
        full_budget: usize,
        elapsed: Duration,
        reclaimed_slots: usize,
    ) {
        let now = Instant::now();
        let mut history = self.history.lock();
        let interval = now.duration_since(history.last_end).max(elapsed);
        history.last_end = now;
        match level {
            CollectionLevel::Minor => {
                history.minor = Some((reclaimed_slots, elapsed));
                history.full_since_probe = 0;
            }
            CollectionLevel::Major => {
                history.full = Some((reclaimed_slots, elapsed));
                history.full_since_probe = history.full_since_probe.saturating_add(1);
            }
            CollectionLevel::Nursery => {}
        }
        // Periodically try Gen1 again so an earlier workload cannot permanently
        // lock the engine into full collection at every Gen1 threshold.
        let favor_full = prefer_full(history.minor, history.full) && history.full_since_probe < 8;
        self.prefer_full.store(favor_full, Ordering::Relaxed);
        let expensive = elapsed.as_secs_f64() * 100.0
            > interval.as_secs_f64() * self.config.gc_time_percent as f64;
        let cheap = elapsed.as_secs_f64() * 200.0
            < interval.as_secs_f64() * self.config.gc_time_percent as f64;
        if level == CollectionLevel::Major {
            history.costly_survivor_streak = 0;
            self.full_only_remaining
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                    Some(n.saturating_sub(1))
                })
                .unwrap();
            if self.config.survival_early_probe && young_spent > 0 && promoted[0] <= young_spent / 5
            {
                self.full_only_remaining.store(0, Ordering::Relaxed);
            }
        } else if self.config.survival_backoff > 0 && young_spent > 0 {
            history.costly_survivor_streak = if expensive && promoted[0] >= young_spent / 2 {
                history.costly_survivor_streak.saturating_add(1)
            } else {
                0
            };
            if history.costly_survivor_streak >= 2 {
                self.full_only_remaining
                    .store(self.config.survival_backoff, Ordering::Relaxed);
                history.costly_survivor_streak = 0;
            }
        }
        let gen0_cap = (full_budget / 2).max(self.gen0_floor);
        let gen1_cap = full_budget.max(self.config.gen1_budget_floor);
        // Preserve learned young budgets on full GC, clamping them to the new
        // heap-size limits. Optional source-Gen0 feedback only ends backoff.
        if level != CollectionLevel::Major {
            self.gen0_budget.store(
                choose_budget(
                    self.gen0_budget.load(Ordering::Relaxed),
                    self.gen0_floor,
                    gen0_cap,
                    young_spent,
                    promoted[0],
                    (expensive, cheap),
                    self.config.growth_percent,
                ),
                Ordering::Relaxed,
            );
        }
        match level {
            CollectionLevel::Nursery => {
                self.counts[0].fetch_add(1, Ordering::Relaxed);
                super::gc_experiment::saturating_add(&self.gen1_spent, promoted[0]);
            }
            CollectionLevel::Minor => {
                self.counts[1].fetch_add(1, Ordering::Relaxed);
                self.gen1_budget.store(
                    choose_budget(
                        self.gen1_budget.load(Ordering::Relaxed),
                        self.config.gen1_budget_floor,
                        gen1_cap,
                        self.gen1_spent.load(Ordering::Relaxed),
                        promoted[1],
                        (expensive, cheap),
                        self.config.growth_percent,
                    ),
                    Ordering::Relaxed,
                );
                self.gen1_spent.store(promoted[0], Ordering::Relaxed);
                super::gc_experiment::saturating_add(&self.old_spent, promoted[1]);
            }
            CollectionLevel::Major => {
                self.counts[2].fetch_add(1, Ordering::Relaxed);
                self.gen1_spent.store(0, Ordering::Relaxed);
                self.old_spent.store(0, Ordering::Relaxed);
            }
        }
        self.gen0_budget.fetch_min(gen0_cap, Ordering::Relaxed);
        self.gen1_budget.fetch_min(gen1_cap, Ordering::Relaxed);
    }
}

/// Grow at the configured rate; shrink gradually after cheap, low-survival collections.
fn choose_budget(
    current: usize,
    floor: usize,
    cap: usize,
    input: usize,
    survived: usize,
    timing: (bool, bool),
    growth_percent: usize,
) -> usize {
    let (expensive, cheap) = timing;
    let high_survival = input > 0 && survived >= input / 2;
    let low_survival = input > 0 && survived <= input / 5;
    let step = (current / 4).max(1);
    let desired = if high_survival || expensive {
        current.saturating_add(current.saturating_mul(growth_percent).div_ceil(100))
    } else if low_survival && cheap {
        current.saturating_sub(step)
    } else {
        current
    };
    desired.clamp(floor, cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expensive_gen1_can_escalate_then_probe_again() {
        let p = AdaptivePolicy::new(
            AdaptiveConfig {
                gen1_budget_floor: 100,
                growth_percent: 25,
                gen1_mode: Gen1Mode::CompareReclamation,
                large_payload_threshold: None,
                survival_backoff: 0,
                survival_early_probe: false,
                gc_time_percent: 5,
            },
            50,
        );
        p.after_gc(
            CollectionLevel::Major,
            0,
            [0, 0],
            1000,
            Duration::from_millis(1),
            100,
        );
        p.after_gc(
            CollectionLevel::Minor,
            100,
            [150, 0],
            1000,
            Duration::from_millis(10),
            10,
        );
        assert_eq!(p.due(0, 1000), Some(CollectionLevel::Major));
        for _ in 0..8 {
            p.after_gc(
                CollectionLevel::Major,
                0,
                [0, 0],
                1000,
                Duration::from_millis(1),
                100,
            );
        }
        p.after_gc(
            CollectionLevel::Nursery,
            150,
            [150, 0],
            1000,
            Duration::ZERO,
            0,
        );
        assert_eq!(p.due(0, 1000), Some(CollectionLevel::Minor));
    }

    #[test]
    fn scope_preference_requires_observed_reclamation_and_can_recover() {
        let fast = Some((100, Duration::from_millis(1)));
        let slow = Some((100, Duration::from_millis(10)));
        let empty = Some((0, Duration::from_millis(1)));
        let near = Some((100, Duration::from_millis(9)));
        assert!(!prefer_full(None, fast));
        assert!(!prefer_full(fast, empty));
        assert!(prefer_full(empty, fast));
        assert!(prefer_full(slow, fast));
        assert!(!prefer_full(fast, slow));
        assert!(!prefer_full(slow, near));
    }

    #[test]
    fn collection_feedback_backs_off_but_obeys_bounds() {
        assert_eq!(
            choose_budget(100, 50, 200, 100, 90, (false, false), 25),
            125
        );
        assert_eq!(choose_budget(100, 50, 200, 100, 0, (true, false), 25), 125);
        assert_eq!(choose_budget(100, 50, 200, 100, 0, (false, true), 25), 75);
        assert_eq!(choose_budget(190, 50, 200, 100, 90, (true, false), 25), 200);
        assert_eq!(choose_budget(50, 50, 200, 100, 0, (false, true), 25), 50);
    }

    #[test]
    fn promotions_escalate_without_charging_dead_temporary_allocations_to_old() {
        let p = AdaptivePolicy::new(
            AdaptiveConfig {
                gen1_budget_floor: 200,
                growth_percent: 25,
                gen1_mode: Gen1Mode::Minor,
                large_payload_threshold: None,
                survival_backoff: 0,
                survival_early_probe: false,
                gc_time_percent: 5,
            },
            100,
        );
        let elapsed = Duration::ZERO;
        for _ in 0..100 {
            assert_eq!(p.due(100, 1000), Some(CollectionLevel::Nursery));
            p.after_gc(CollectionLevel::Nursery, 100, [0, 0], 1000, elapsed, 0);
            assert_eq!(p.due(0, 1000), None);
        }
        p.after_gc(CollectionLevel::Nursery, 200, [200, 0], 1000, elapsed, 0);
        assert_eq!(p.due(0, 1000), Some(CollectionLevel::Minor));
        p.after_gc(CollectionLevel::Minor, 0, [0, 1000], 1000, elapsed, 0);
        assert_eq!(p.due(0, 1000), Some(CollectionLevel::Major));
        // Explicit younger collections cannot erase older-generation debt.
        p.after_gc(CollectionLevel::Nursery, 0, [0, 0], 1000, elapsed, 0);
        assert_eq!(p.due(0, 1000), Some(CollectionLevel::Major));
        p.after_gc(CollectionLevel::Major, 0, [0, 0], 1000, elapsed, 0);
        assert_eq!(p.due(0, 1000), None);
        p.charge_growth(1000);
        assert_eq!(p.due(0, 1000), Some(CollectionLevel::Major));
    }
}
