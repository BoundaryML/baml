//! The soft ceiling on producer-side memory.
//!
//! A producer that finds its pool exhausted waits for a buffer to come back,
//! and when none does it allocates beyond the pool's size: nothing is dropped
//! for being slow, and nothing waits forever. How long it waits first grows
//! with how far the pool already is over its size, measured in live demand
//! (buffers in use beyond the size), so growth is taxed ever more steeply
//! while steady use at any size costs nothing: a buffer that is free is
//! taken at once. Buffers beyond the size are freed as they return idle.
//!
//! The time a producer has waited is held against the threshold for the
//! overage as it is *now*, not as it was when it began waiting. The threshold
//! is the same for every waiter, so the one that has waited longest is always
//! the first eligible: a fall in overage releases the oldest waiters first.
//! Which eligible waiter takes a chunk that comes back is decided by the
//! lock, not by age, and a waiter asleep between probes notices later than
//! one still spinning.
//!
//! Waiting costs no CPU to speak of: a producer spins only for the first
//! probes of a wait, when a recycle is moments away, then yields, then
//! sleeps ([`SPIN_PROBES`], [`YIELD_PROBES`], [`SLEEP_PROBE_DURATION`]). The
//! soft ceiling is a tax in time on the application's progress, never in
//! cores. The tax scales with concurrency: waiters past the same threshold
//! together each allocate.
//!
//! Each pool names its own base and unit: [`crate::transport::overage_wait`]
//! for chunk lanes, [`crate::snapshot::overage_wait`] for capture storage.
use std::time::Duration;

/// How long to wait before allocating beyond the size, at `units` of
/// overage: `base × (1 + units)²`. Saturates.
pub fn wait(base: Duration, units: usize) -> Duration {
    let scale = units.saturating_add(1) as u128;
    let nanos = base.as_nanos().saturating_mul(scale.saturating_mul(scale));
    Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
}

/// **Measure first.** Probes of a wait spent spinning, before yielding.
pub const SPIN_PROBES: usize = 64;
/// **Measure first.** Probes of a wait spent spinning or yielding, before sleeping.
pub const YIELD_PROBES: usize = 256;
/// **Measure first.** How long each probe sleeps after that; the OS may
/// round it up, by a millisecond or more on some platforms.
pub const SLEEP_PROBE_DURATION: Duration = Duration::from_micros(50);

/// **Test-only.** Loom has no clock: under the model a producer makes one
/// probe and then allocates beyond the size, so both paths are explored
/// without exhausting the model's budget. This does not set production
/// behavior.
pub const MODEL_PROBES: usize = 1;

const _: () = assert!(SPIN_PROBES < YIELD_PROBES);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wait_grows_quadratically_from_the_base_and_saturates() {
        let base = Duration::from_micros(10);
        assert_eq!(wait(base, 0), Duration::from_micros(10));
        assert_eq!(wait(base, 1), Duration::from_micros(40));
        assert_eq!(wait(base, 9), Duration::from_millis(1));
        assert_eq!(wait(base, usize::MAX), Duration::from_nanos(u64::MAX));
        assert_eq!(wait(Duration::MAX, 1), Duration::from_nanos(u64::MAX));
    }
}
