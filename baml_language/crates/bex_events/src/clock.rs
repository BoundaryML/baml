//! Process-wide monotonic event clock.
//!
//! BEX event timestamps (`timestamp_ns` on every [`crate::DiskEventV1`]) are
//! **monotonic nanoseconds since process start**, never wall-clock — wall-clock
//! can step backward (NTP) and corrupt durations and per-thread ordering.
//! Consumers rebase to wall time once per file via the header:
//!
//! ```text
//! wall(event) = started_at_epoch_ns + event.timestamp_ns
//! ```
//!
//! where `started_at_epoch_ns` is [`process_started_at_epoch_ns`] — the
//! wall-clock anchor captured atomically alongside the monotonic anchor the
//! first time either is read.

use std::sync::OnceLock;

use web_time::{Instant, SystemTime, UNIX_EPOCH};

struct ProcessClock {
    started_at_epoch_ns: u128,
    anchor: Instant,
}

fn process_clock() -> &'static ProcessClock {
    static CLOCK: OnceLock<ProcessClock> = OnceLock::new();
    CLOCK.get_or_init(|| ProcessClock {
        started_at_epoch_ns: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
        anchor: Instant::now(),
    })
}

/// Wall-clock epoch nanoseconds captured when the process clock was first
/// used. This is the rebase anchor written into every
/// [`crate::EventFileHeaderV1::started_at_epoch_ns`]:
/// `wall(event) = started_at_epoch_ns + event.timestamp_ns`.
#[must_use]
pub fn process_started_at_epoch_ns() -> u128 {
    process_clock().started_at_epoch_ns
}

/// Monotonic nanoseconds since the process clock anchor.
///
/// This is the only valid source for `timestamp_ns` on disk events: values
/// are small (process uptime), strictly non-decreasing per thread, and safe
/// for JS consumers (a process would need ~104 days of uptime to exceed
/// 2^53 ns).
#[must_use]
pub fn now_ns() -> u64 {
    u64::try_from(process_clock().anchor.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_ns_is_monotonic_and_relative_to_process_start() {
        let a = now_ns();
        let b = now_ns();
        assert!(b >= a);
        // Relative-to-process-start values are far below epoch nanos
        // (~1.78e18 in 2026). 10^15 ns is ~11 days of uptime.
        assert!(
            a < 1_000_000_000_000_000,
            "timestamp looks like epoch nanos: {a}"
        );
    }

    #[test]
    fn wall_anchor_composes_with_now_ns() {
        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let wall = process_started_at_epoch_ns() + u128::from(now_ns());
        let after = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // Allow slack for Instant/SystemTime drift; the composed value must
        // land in the test's own wall-clock window, give or take 100ms.
        let slack = 100_000_000u128;
        assert!(wall + slack >= before, "wall={wall} before={before}");
        assert!(wall <= after + slack, "wall={wall} after={after}");
    }
}
