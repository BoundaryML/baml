//! The soft ceiling on producer-side memory.
//!
//! A producer that finds its pool exhausted probes for a buffer to come back,
//! and when none does it allocates beyond the pool's size: nothing is dropped
//! for being slow, and nothing waits forever. How long it probes first grows
//! with how far the pool already is over its size, measured in live demand
//! (buffers in use beyond the size), so growth is taxed ever more steeply
//! while steady use at any size costs nothing: a buffer that is free is
//! taken at once. Buffers beyond the size are freed as they return idle.
//!
//! The probes a producer has spent are held against the threshold for the
//! overage as it is *now*, not as it was when it began waiting. The threshold
//! is the same for every waiter, so the one that has waited longest is always
//! the first past it: a fall in overage releases the oldest waiters first,
//! and no newcomer allocates beyond the size ahead of them.
//!
//! Each pool names its own base and unit: [`crate::transport::overage_probes`]
//! for chunk lanes, [`crate::snapshot::overage_probes`] for capture storage.

/// Probes before allocating beyond the size, at `units` of overage:
/// `base × (1 + units)²`. Saturates.
pub const fn probes(base: usize, units: usize) -> usize {
    let scale = units.saturating_add(1);
    base.saturating_mul(scale.saturating_mul(scale))
}

/// **Test-only.** One probe lets Loom explore both the recycled and the
/// overage path without exhausting its budget; it does not set production
/// behavior.
pub const MODEL_PROBES: usize = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probes_grow_quadratically_from_the_base_and_saturate() {
        assert_eq!(probes(256, 0), 256);
        assert_eq!(probes(256, 1), 1024);
        assert_eq!(probes(256, 9), 25_600);
        assert_eq!(probes(256, usize::MAX), usize::MAX);
        assert_eq!(probes(usize::MAX, 1), usize::MAX);
    }
}
