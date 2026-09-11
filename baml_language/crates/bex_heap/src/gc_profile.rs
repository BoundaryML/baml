//! Compile-time GC instrumentation. Disabled builds never read clocks or snapshots.

/// Disjoint phases of a heap collection, ending at each instrumentation call.
#[derive(Clone, Copy)]
pub(crate) enum HeapPhase {
    Prepare,
    Trace,
    Keepalive,
    Fixup,
    Reclaim,
    Bookkeeping,
}

#[cfg(not(feature = "gc_profiling"))]
pub(crate) use disabled::NoopGcProfiler as GcProfiler;
#[cfg(not(feature = "gc_profiling"))]
pub use disabled::{NoopGcCycleProfiler as GcCycleProfiler, NoopGcProfile as GcProfile};
#[cfg(feature = "gc_profiling")]
pub(crate) use enabled::GcProfiler;
#[cfg(feature = "gc_profiling")]
pub use enabled::{GcCycleProfiler, GcHeapSnapshot, GcProfile};

#[cfg(feature = "gc_profiling")]
mod enabled {
    use std::time::Duration;

    use web_time::Instant;

    use super::HeapPhase;
    use crate::{BexHeap, GcStats};

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
        pub park_wait: Duration,
        pub root_scan: Duration,
        pub holder_fixup: Duration,
        pub pause: Duration,
        pub post_gc: Duration,
        pub total: Duration,
    }

    /// Captures heap-local phases while the caller holds exclusive heap access.
    pub(crate) struct GcProfiler {
        profile: GcProfile,
        start: Instant,
        last: Instant,
    }

    impl GcProfiler {
        /// Caller must hold exclusive heap access through `finish`.
        pub(crate) unsafe fn start(heap: &BexHeap, roots: usize) -> Self {
            let start = Instant::now();
            Self {
                profile: GcProfile {
                    // SAFETY: caller holds exclusive heap access.
                    before: unsafe { GcHeapSnapshot::capture(heap) },
                    roots,
                    ..Default::default()
                },
                start,
                last: start,
            }
        }

        pub(crate) fn finish_phase(&mut self, phase: HeapPhase) {
            let now = Instant::now();
            let elapsed = now - self.last;
            self.last = now;
            match phase {
                HeapPhase::Prepare => self.profile.prepare = elapsed,
                HeapPhase::Trace => self.profile.trace = elapsed,
                HeapPhase::Keepalive => self.profile.keepalive = elapsed,
                HeapPhase::Fixup => self.profile.fixup = elapsed,
                HeapPhase::Reclaim => self.profile.reclaim = elapsed,
                HeapPhase::Bookkeeping => self.profile.bookkeeping = elapsed,
            }
        }

        /// Caller must still hold exclusive heap access.
        pub(crate) unsafe fn finish(mut self, heap: &BexHeap) -> GcProfile {
            // SAFETY: caller holds exclusive heap access.
            self.profile.after = unsafe { GcHeapSnapshot::capture(heap) };
            self.profile.heap_total = self.start.elapsed();
            self.profile
        }
    }

    /// Engine-level timing across permit acquisition, heap collection and callbacks.
    /// Start before requesting exclusive access; record boundaries in lifecycle order.
    pub struct GcCycleProfiler {
        start: Instant,
        parked_at: Instant,
        roots_scanned_at: Instant,
        heap_done_at: Instant,
        released_at: Instant,
    }

    impl GcCycleProfiler {
        pub fn start() -> Self {
            let start = Instant::now();
            Self {
                start,
                parked_at: start,
                roots_scanned_at: start,
                heap_done_at: start,
                released_at: start,
            }
        }

        pub fn parked(&mut self) {
            self.parked_at = Instant::now();
        }
        pub fn roots_scanned(&mut self) {
            self.roots_scanned_at = Instant::now();
        }
        pub fn heap_done(&mut self) {
            self.heap_done_at = Instant::now();
        }
        pub fn released(&mut self) {
            self.released_at = Instant::now();
        }

        pub fn finish(self, stats: &mut GcStats, reason: &'static str) {
            let finished_at = Instant::now();
            let profile = &mut stats.profile;
            profile.park_wait = self.parked_at - self.start;
            profile.root_scan = self.roots_scanned_at - self.parked_at;
            profile.holder_fixup = self.released_at - self.heap_done_at;
            profile.pause = self.released_at - self.parked_at;
            profile.post_gc = finished_at - self.released_at;
            profile.total = finished_at - self.start;
            tracing::debug!(target: "bex_gc", reason = reason, level = ?stats.level,
            profile = ?profile, "GC cycle");
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
                p.heap_total
                    >= p.prepare + p.trace + p.keepalive + p.fixup + p.reclaim + p.bookkeeping
            );
            // SAFETY: roots were forwarded by the preceding collection; no mutator.
            let (major, _, _) = unsafe { heap.collect_garbage(&roots) };
            assert_eq!(major.level, CollectionLevel::Major);
            assert_eq!(major.profile.before.generation_slots, [0, 1, 0]);
            assert_eq!(major.profile.after.generation_slots, [0, 0, 1]);
        }
    }
}

#[cfg(not(feature = "gc_profiling"))]
mod disabled {
    use super::HeapPhase;
    use crate::{BexHeap, GcStats};

    /// Profiling is unavailable in this build; occupies no space in `GcStats`.
    #[derive(Clone, Debug, Default)]
    pub struct NoopGcProfile;

    pub(crate) struct NoopGcProfiler;

    impl NoopGcProfiler {
        /// Same exclusive-access contract as the enabled implementation.
        #[inline]
        pub(crate) unsafe fn start(_: &BexHeap, _: usize) -> Self {
            Self
        }
        #[inline]
        pub(crate) fn finish_phase(&mut self, _: HeapPhase) {}
        /// Same exclusive-access contract as the enabled implementation.
        #[inline]
        pub(crate) unsafe fn finish(self, _: &BexHeap) -> NoopGcProfile {
            NoopGcProfile
        }
    }

    /// Disabled engine instrumentation: no clock reads, logging or stored state.
    pub struct NoopGcCycleProfiler;

    impl NoopGcCycleProfiler {
        #[inline]
        pub fn start() -> Self {
            Self
        }
        #[inline]
        pub fn parked(&mut self) {}
        #[inline]
        pub fn roots_scanned(&mut self) {}
        #[inline]
        pub fn heap_done(&mut self) {}
        #[inline]
        pub fn released(&mut self) {}
        #[inline]
        pub fn finish(self, _: &mut GcStats, _: &'static str) {}
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn disabled_instrumentation_has_no_stored_state() {
            assert_eq!(size_of::<NoopGcProfile>(), 0);
            assert_eq!(size_of::<NoopGcProfiler>(), 0);
            assert_eq!(size_of::<NoopGcCycleProfiler>(), 0);
        }
    }
}
