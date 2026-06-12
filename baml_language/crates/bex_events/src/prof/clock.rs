//! The profiling clock: nanoseconds since a process-global zero point.
//!
//! Record timestamps (`ts_ns`) are `u64` nanoseconds relative to the zero
//! point; the file header carries [`started_at_epoch_ns`] so readers rebase to
//! wall time as `wall(event) = started_at_epoch_ns + ts_ns`.
//!
//! Backed by `minstant` (calibrated TSC where available, OS monotonic clock
//! otherwise). The design's ~25 ns per call-pair budget assumes ≤10 ns per
//! read — `benches/prof_clock.rs` measures it (`quanta` is the designated
//! fallback if `minstant` misbehaves on some target).
//!
//! On wasm32 this module is a stub: profiling is forced off there (no
//! consumer thread, no TSC), and a js-backed clock is part of the deferred
//! cooperative-drain design.

pub use imp::{init, now_ns, started_at_epoch_ns};

#[cfg(not(any(target_arch = "wasm32", miri)))]
mod imp {
    use std::sync::OnceLock;

    struct Zero {
        instant: minstant::Instant,
        epoch_ns: u128,
    }

    fn zero() -> &'static Zero {
        static ZERO: OnceLock<Zero> = OnceLock::new();
        ZERO.get_or_init(|| {
            let anchor = minstant::Anchor::new();
            let instant = minstant::Instant::now();
            Zero {
                instant,
                epoch_ns: u128::from(instant.as_unix_nanos(&anchor)),
            }
        })
    }

    /// Pins the zero point (and pays minstant's one-time TSC calibration).
    ///
    /// Call once at engine startup; later `now_ns` reads are then just a
    /// clock read plus a subtraction. Lazily initialized on first use either
    /// way.
    pub fn init() {
        let _ = zero();
    }

    /// Nanoseconds since the process zero point.
    ///
    /// `u64` overflows after ~584 years of uptime; truncation is fine.
    #[inline]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "u128 nanos since process start; wraps after ~584 years"
    )]
    pub fn now_ns() -> u64 {
        zero().instant.elapsed().as_nanos() as u64
    }

    /// Wall-clock time of the zero point, in nanoseconds since the Unix
    /// epoch. Written into the `.bamlprof` file header.
    pub fn started_at_epoch_ns() -> u128 {
        zero().epoch_ns
    }
}

// wasm32: profiling is forced off, nothing reads the clock. miri: minstant
// cannot run there (its ctor probes the TSC), and the concurrency tests miri
// executes stamp their own ts values.
#[cfg(any(target_arch = "wasm32", miri))]
mod imp {
    /// No-op in stub builds (profiling is forced off).
    pub fn init() {}

    /// Always `0` in stub builds (nothing reads this).
    #[inline]
    pub fn now_ns() -> u64 {
        0
    }

    /// Always `0` in stub builds (nothing reads this).
    pub fn started_at_epoch_ns() -> u128 {
        0
    }
}

// minstant's TSC/cpuid probing uses intrinsics miri cannot execute, so the
// clock is exercised under real builds only.
#[cfg(all(test, not(miri), not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn monotonic_nondecreasing() {
        init();
        let mut prev = now_ns();
        for _ in 0..10_000 {
            let next = now_ns();
            assert!(next >= prev, "clock went backwards: {prev} -> {next}");
            prev = next;
        }
    }

    #[test]
    fn zero_point_matches_wall_clock() {
        init();
        let wall_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let prof_now = started_at_epoch_ns() + u128::from(now_ns());
        let drift = wall_now.abs_diff(prof_now);
        // Generous tolerance: calibration error + scheduling between the two
        // reads. Catches "anchor is garbage", not clock-quality regressions.
        assert!(
            drift < 5_000_000_000,
            "prof clock and wall clock disagree by {drift} ns"
        );
    }
}
