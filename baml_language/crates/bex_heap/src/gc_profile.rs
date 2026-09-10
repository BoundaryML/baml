//! Opt-in collection diagnostics. No timers or snapshots on allocation paths.
use std::time::Duration;

use web_time::Instant;

use crate::BexHeap;

/// Process CPU (user + kernel), and lifetime peak resident bytes. CPU deltas
/// inside the collector are attributable only in isolated single-process tests;
/// other host threads may consume CPU during the same interval.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn process_usage() -> Option<(Duration, u64)> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: getrusage initializes this correctly sized output on success.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: the successful call initialized the structure.
    let usage = unsafe { usage.assume_init() };
    let duration = |t: libc::timeval| -> Option<Duration> {
        Some(
            Duration::from_secs(u64::try_from(t.tv_sec).ok()?)
                + Duration::from_micros(u64::try_from(t.tv_usec).ok()?),
        )
    };
    let peak = u64::try_from(usage.ru_maxrss).ok()?;
    // Darwin reports bytes; Linux reports KiB. Other targets return unavailable.
    #[cfg(target_os = "linux")]
    let peak = peak.saturating_mul(1024);
    Some((duration(usage.ru_utime)? + duration(usage.ru_stime)?, peak))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn process_usage() -> Option<(Duration, u64)> {
    None
}

/// Storage counts, not payload bytes or a measurement of reachability.
#[derive(Clone, Debug, Default)]
pub struct GcHeapSnapshot {
    /// Reserved/occupied slots in Gen0, Gen1, Gen2 respectively.
    pub generation_slots: [usize; 3],
    /// Backing capacity in Gen0, Gen1, Gen2, scratch respectively.
    pub capacity_slots: [usize; 4],
    /// Actual TLAB allocations since the previous collection (excludes holes).
    pub new_objects: usize,
}

impl GcHeapSnapshot {
    /// Caller must hold exclusive GC access; generations can move otherwise.
    pub(crate) unsafe fn capture(heap: &BexHeap) -> Self {
        // SAFETY: caller holds exclusive GC access.
        let spaces = unsafe {
            [
                heap.gen0_ref(),
                heap.gen1_ref(),
                heap.gen2_ref(),
                heap.inactive_ref(),
            ]
        };
        Self {
            generation_slots: [spaces[0].len(), spaces[1].len(), spaces[2].len()],
            capacity_slots: spaces.map(|space| space.capacity()),
            new_objects: heap.allocs_since_gc(),
        }
    }
}

/// Per-cycle wall times. Heap phases are disjoint; `pause` includes them.
/// `park_wait` precedes exclusive access and is not a uniform application pause.
/// Post-GC callbacks/finalizers run after permits are released.
#[derive(Clone, Debug, Default)]
pub struct GcProfile {
    pub before: GcHeapSnapshot,
    pub after: GcHeapSnapshot,
    pub roots: usize,
    pub prepare: Duration,
    pub trace: Duration,
    pub keepalive: Duration,
    pub fixup: Duration,
    pub reclaim: Duration,
    pub bookkeeping: Duration,
    pub heap_total: Duration,
    /// Process CPU consumed over heap collection, excluding park wait and callbacks.
    pub heap_cpu: Option<Duration>,
    pub park_wait: Duration,
    pub root_scan: Duration,
    pub holder_fixup: Duration,
    pub pause: Duration,
    pub post_gc: Duration,
    pub total: Duration,
}

pub(crate) struct GcClock {
    start: Instant,
    last: Instant,
    cpu_start: Option<Duration>,
}

impl GcClock {
    pub(crate) fn new() -> Self {
        let start = Instant::now();
        Self {
            start,
            last: start,
            cpu_start: process_usage().map(|u| u.0),
        }
    }

    pub(crate) fn lap(&mut self) -> Duration {
        let now = Instant::now();
        let elapsed = now - self.last;
        self.last = now;
        elapsed
    }

    pub(crate) fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    pub(crate) fn cpu_elapsed(&self) -> Option<Duration> {
        Some(process_usage()?.0.saturating_sub(self.cpu_start?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CollectionLevel, Tlab};

    #[test]
    fn reports_allocations_separately_from_reserved_slots() {
        let heap = BexHeap::with_tlab_size(vec![], 16);
        let mut tlab = Tlab::new(heap.clone());
        let live = tlab.alloc_string("live");
        tlab.alloc_string("dead");
        // SAFETY: this standalone heap has no concurrent holders.
        let (minor, roots, _) = unsafe { heap.collect_garbage_minor(&[live]) };
        assert_eq!(minor.profile.before.generation_slots, [16, 0, 0]);
        assert_eq!(minor.profile.before.new_objects, 2);
        assert_eq!(minor.profile.after.generation_slots, [0, 1, 0]);
        assert_eq!(minor.profile.after.new_objects, 0);
        assert_eq!(minor.collected_count, 15); // 1 dead object + 14 holes.
        assert_eq!(minor.profile.roots, 1);
        let p = &minor.profile;
        assert!(
            p.heap_total >= p.prepare + p.trace + p.keepalive + p.fixup + p.reclaim + p.bookkeeping
        );
        // SAFETY: roots were forwarded by the preceding collection; no mutator.
        let (major, _, _) = unsafe { heap.collect_garbage(&roots) };
        assert_eq!(major.level, CollectionLevel::Major);
        assert_eq!(major.profile.before.generation_slots, [0, 1, 0]);
        assert_eq!(major.profile.after.generation_slots, [0, 0, 1]);
    }
}
