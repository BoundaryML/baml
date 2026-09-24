//! Engine-owned immutable policies. Publication is cold; reads never lock.
use std::sync::{Mutex, OnceLock};

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
        || policy.promote_errors && outcome == InvocationOutcome::Errored
        || policy
            .promotion_duration_threshold
            .is_some_and(|threshold| threshold.reached(elapsed, domain))
}

type Page = [OnceLock<TelemetryPolicy>; PAGE_SIZE];

/// Shared by every VM over the same engine's function objects. IDs are scoped
/// to this table. Published slots never move, change, or get reused, so readers
/// can safely finish using an old ID while a function's policy is updated.
pub struct TelemetryPolicies {
    mode: btel_settings::mode::TelemetryMode,
    pages: [OnceLock<Box<Page>>; PAGE_COUNT],
    interned: Mutex<FxHashMap<TelemetryPolicy, u16>>,
}

impl Default for TelemetryPolicies {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryPolicies {
    pub fn new() -> Self {
        Self::with_mode(btel_settings::mode::DEFAULT_MODE)
    }

    pub fn with_mode(mode: btel_settings::mode::TelemetryMode) -> Self {
        Self {
            mode,
            pages: [const { OnceLock::new() }; PAGE_COUNT],
            interned: Mutex::new(FxHashMap::default()),
        }
    }

    pub fn mode(&self) -> btel_settings::mode::TelemetryMode {
        self.mode
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

#[cfg(test)]
mod tests {
    use super::*;

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
