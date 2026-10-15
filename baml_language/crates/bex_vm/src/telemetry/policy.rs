//! Engine-owned immutable policies. Publication is cold; reads never lock.
use std::sync::{Arc, Mutex, OnceLock, Weak};

use btel_clock::{ClockDomainId, ClockThreshold};
use btel_types::{ClockDuration, InvocationOutcome, TelemetryPolicyId};
use rustc_hash::FxHashMap;

pub type TelemetryPolicy = btel_settings::policy::TelemetryPolicy<ClockThreshold>;
use btel_settings::policy::{PAGE_COUNT, PAGE_SIZE};

#[inline(always)]
pub(super) fn promotes(
    policy: TelemetryPolicy,
    elapsed: ClockDuration,
    outcome: InvocationOutcome,
    domain: ClockDomainId,
) -> bool {
    policy.span_from_entry
        || policy.promote_errors && outcome.is_error()
        || policy
            .promotion_duration_threshold
            .is_some_and(|threshold| threshold.reached(elapsed, domain))
}

type Page = [OnceLock<TelemetryPolicy>; PAGE_SIZE];

/// Shared by every VM over the same engine's function objects. IDs are scoped
/// to this table. Published slots never move, change, or get reused, so readers
/// can safely finish using an old ID while a function's policy is updated.
pub struct TelemetryPolicies {
    auto_level: btel_settings::mode::AutoTelemetryLevel,
    http_bodies: bool,
    pages: [OnceLock<Box<Page>>; PAGE_COUNT],
    interned: Mutex<FxHashMap<TelemetryPolicy, u16>>,
    // Only owned spans subscribe. Ordinary calls keep their existing atomic read.
    owned_watchers: Mutex<FxHashMap<btel_types::FunctionId, Weak<TelemetryPolicyId>>>,
}

impl Default for TelemetryPolicies {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryPolicies {
    pub fn new() -> Self {
        Self::with_auto_level(btel_settings::mode::AutoTelemetryLevel::default())
    }

    pub fn with_auto_level(auto_level: btel_settings::mode::AutoTelemetryLevel) -> Self {
        Self {
            auto_level,
            http_bodies: true,
            pages: [const { OnceLock::new() }; PAGE_COUNT],
            interned: Mutex::new(FxHashMap::default()),
            owned_watchers: Mutex::new(FxHashMap::default()),
        }
    }

    /// Whether network spans record request and response bodies. On by default.
    #[must_use]
    pub fn with_http_bodies(mut self, http_bodies: bool) -> Self {
        self.http_bodies = http_bodies;
        self
    }

    pub fn auto_level(&self) -> btel_settings::mode::AutoTelemetryLevel {
        self.auto_level
    }

    pub fn http_bodies(&self) -> bool {
        self.http_bodies
    }

    #[inline(always)]
    pub(super) fn get(&self, id: u16) -> TelemetryPolicy {
        if id == TelemetryPolicyId::NONE {
            return TelemetryPolicy::NONE;
        }
        let id = usize::from(id);
        *self.pages[id / PAGE_SIZE]
            .get()
            .and_then(|page| page[id % PAGE_SIZE].get())
            .expect("function policy ID must refer to a published policy in its engine")
    }

    pub(super) fn watch_owned(
        self: &Arc<Self>,
        function: btel_types::FunctionId,
        current: &TelemetryPolicyId,
    ) -> OwnedPolicy {
        let mut watchers = self
            .owned_watchers
            .lock()
            .expect("policy watchers poisoned");
        let slot = watchers
            .get(&function)
            .and_then(Weak::upgrade)
            .unwrap_or_else(|| {
                let slot = Arc::new(TelemetryPolicyId::none());
                slot.store(current.load());
                watchers.insert(function, Arc::downgrade(&slot));
                slot
            });
        OwnedPolicy {
            policies: self.clone(),
            slot,
        }
    }

    pub(super) fn publish_for_function(
        &self,
        function: btel_types::FunctionId,
        target: &TelemetryPolicyId,
        policy: TelemetryPolicy,
    ) -> Result<(), &'static str> {
        // Serialize subscription with publication so no new watcher can miss an update.
        let mut watchers = self
            .owned_watchers
            .lock()
            .expect("policy watchers poisoned");
        self.publish(target, policy)?;
        if let Some(slot) = watchers.get(&function).and_then(Weak::upgrade) {
            slot.store(target.load());
        } else {
            watchers.remove(&function);
        }
        Ok(())
    }

    // No external setter yet. Publish the immutable contents before releasing
    // the function's ID; its existing acquire load observes the matching slot.
    pub(super) fn publish(
        &self,
        target: &TelemetryPolicyId,
        policy: TelemetryPolicy,
    ) -> Result<(), &'static str> {
        if policy == TelemetryPolicy::NONE {
            target.store(TelemetryPolicyId::NONE);
            return Ok(());
        }
        let mut interned = self
            .interned
            .lock()
            .expect("telemetry policy writer poisoned");
        let id = if let Some(id) = interned.get(&policy) {
            *id
        } else {
            let id = u16::try_from(interned.len() + 1)
                .map_err(|_| "telemetry policy table exhausted")?;
            let index = usize::from(id);
            let page = self.pages[index / PAGE_SIZE]
                .get_or_init(|| Box::new([const { OnceLock::new() }; PAGE_SIZE]));
            page[index % PAGE_SIZE]
                .set(policy)
                .expect("fresh policy slot");
            interned.insert(policy, id);
            id
        };
        target.store(id);
        Ok(())
    }
}

/// A GC-independent policy subscription that may outlive the creating VM frame.
#[derive(Clone)]
pub(super) struct OwnedPolicy {
    policies: Arc<TelemetryPolicies>,
    slot: Arc<TelemetryPolicyId>,
}

impl OwnedPolicy {
    pub(super) fn get(&self) -> TelemetryPolicy {
        self.policies.get(self.slot.load())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_policy_observes_updates_after_its_creating_frame_returns() {
        let policies = Arc::new(TelemetryPolicies::new());
        let function = btel_types::FunctionIdAllocator::default()
            .allocate()
            .unwrap();
        let target = TelemetryPolicyId::none();
        let first = policies.watch_owned(function, &target);
        let second = policies.watch_owned(function, &target);
        assert_eq!(first.get(), TelemetryPolicy::NONE);
        let promoted = TelemetryPolicy {
            promote_errors: true,
            capture_error: true,
            ..TelemetryPolicy::NONE
        };
        policies
            .publish_for_function(function, &target, promoted)
            .unwrap();
        assert_eq!(first.get(), promoted);
        assert_eq!(second.get(), promoted);
        policies
            .publish_for_function(function, &target, TelemetryPolicy::NONE)
            .unwrap();
        assert_eq!(first.get(), TelemetryPolicy::NONE);
        drop(first);
        drop(second);
        policies
            .publish_for_function(function, &target, promoted)
            .unwrap();
        assert_eq!(policies.watch_owned(function, &target).get(), promoted);
    }

    #[test]
    fn readers_observe_published_policies_during_growth_and_exhaustion() {
        let policies = TelemetryPolicies::new();
        let target = TelemetryPolicyId::none();
        let clock = btel_clock::ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run();
        let policy = |ticks| TelemetryPolicy {
            promotion_duration_threshold: Some(
                clock.threshold(std::time::Duration::from_secs(ticks)),
            ),
            ..TelemetryPolicy::NONE
        };
        policies.publish(&target, policy(1)).unwrap();
        let retained_id = target.load();
        std::thread::scope(|scope| {
            let reader = scope.spawn(|| {
                for _ in 0..100_000 {
                    let id = target.load();
                    assert_eq!(policies.get(id), policy(u64::from(id)));
                    assert_eq!(policies.get(retained_id), policy(1));
                }
            });
            for ticks in 2..=u16::MAX {
                policies.publish(&target, policy(u64::from(ticks))).unwrap();
            }
            reader.join().unwrap();
        });
        assert!(policies.publish(&target, policy(65_536)).is_err());
        assert_eq!(
            target.load(),
            u16::MAX,
            "failure must not change the function"
        );
        // Reuse and reset still work when the ID namespace is full.
        policies.publish(&target, policy(1)).unwrap();
        assert_eq!(target.load(), retained_id);
        policies.publish(&target, TelemetryPolicy::NONE).unwrap();
        assert_eq!(target.load(), TelemetryPolicyId::NONE);
        assert_eq!(policies.get(retained_id), policy(1));
    }
}
