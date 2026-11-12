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
pub use enabled::{ConservativeScan, GcCycleProfiler, GcHeapSnapshot, GcProfile};

#[cfg(feature = "gc_profiling")]
mod enabled {
    use std::time::Duration;

    use web_time::Instant;

    use super::HeapPhase;
    use crate::{BexHeap, GcStats};

    /// Storage counts, not payload bytes or a measurement of reachability.
    #[derive(Clone, Debug, Default)]
    pub struct GcHeapSnapshot {
        /// Runtime objects allocated and not yet swept, dead ones included.
        pub objects: usize,
        /// Blocks carved, and how many of them are free.
        pub blocks: [usize; 2],
        /// Objects above the large-object threshold.
        pub large_objects: usize,
        /// Actual TLAB allocations since the previous collection.
        pub new_objects: usize,
    }

    impl GcHeapSnapshot {
        /// Caller must hold exclusive GC access.
        pub(crate) unsafe fn capture(heap: &BexHeap) -> Self {
            let stats = heap.stats();
            Self {
                objects: stats.runtime_objects,
                blocks: [stats.blocks, stats.free_blocks],
                large_objects: stats.large_objects,
                new_objects: heap.allocs_since_gc(),
            }
        }
    }

    /// What the conservative scan of the parked stack ranges found, summed
    /// over the cycle; each range is also traced on `bex_gc` as it is
    /// scanned, so pins can be attributed to the fiber that holds them.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct ConservativeScan {
        pub ranges: usize,
        pub words: usize,
        /// Words that resolved to at least one cell.
        pub resolved: usize,
        /// Cells this scan pinned; a cell several words name counts once.
        pub pinned: usize,
    }

    impl ConservativeScan {
        pub(crate) fn add(&mut self, range: Self) {
            self.ranges += range.ranges;
            self.words += range.words;
            self.resolved += range.resolved;
            self.pinned += range.pinned;
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
        pub conservative: ConservativeScan,
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
        pub(crate) unsafe fn start(heap: &BexHeap) -> Self {
            let start = Instant::now();
            Self {
                profile: GcProfile {
                    // SAFETY: caller holds exclusive heap access.
                    before: unsafe { GcHeapSnapshot::capture(heap) },
                    ..Default::default()
                },
                start,
                last: start,
            }
        }

        pub(crate) fn set_roots(&mut self, roots: usize) {
            self.profile.roots = roots;
        }

        pub(crate) fn set_conservative(&mut self, scan: ConservativeScan) {
            self.profile.conservative = scan;
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

        pub fn finish(self, stats: &mut GcStats, reason: &'static str, heap: &BexHeap) {
            let finished_at = Instant::now();
            let profile = &mut stats.profile;
            profile.park_wait = self.parked_at - self.start;
            profile.root_scan = self.roots_scanned_at - self.parked_at;
            profile.holder_fixup = self.released_at - self.heap_done_at;
            profile.pause = self.released_at - self.parked_at;
            profile.post_gc = finished_at - self.released_at;
            profile.total = finished_at - self.start;
            tracing::debug!(target: "bex_gc", reason = reason,
            budget = ?heap.gc_budget(), profile = ?profile, "GC cycle");
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::Tlab;

        #[test]
        fn reports_allocations_and_objects() {
            let heap = BexHeap::new(vec![]);
            let mut tlab = Tlab::new(heap.clone());
            let live = tlab.alloc_string("live");
            tlab.alloc_string("dead");
            // SAFETY: this standalone heap has no concurrent holders.
            let (first, roots, _) = unsafe { heap.collect_garbage(&[live]) };
            assert_eq!(first.profile.before.objects, 2);
            assert_eq!(first.profile.before.new_objects, 2);
            assert_eq!(first.profile.after.objects, 1);
            assert_eq!(first.profile.after.new_objects, 0);
            assert_eq!(first.collected_count, 1);
            assert_eq!(first.profile.roots, 1);
            let p = &first.profile;
            assert!(
                p.heap_total
                    >= p.prepare + p.trace + p.keepalive + p.fixup + p.reclaim + p.bookkeeping
            );
            // SAFETY: no mutator; `roots` survived the preceding collection.
            let (second, _, _) = unsafe { heap.collect_garbage(&roots) };
            assert_eq!(second.profile.before.objects, 1);
            assert_eq!(second.profile.after.objects, 1);
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
        pub(crate) unsafe fn start(_: &BexHeap) -> Self {
            Self
        }
        #[inline]
        pub(crate) fn set_roots(&mut self, _: usize) {}

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
        pub fn finish(self, _: &mut GcStats, _: &'static str, _: &BexHeap) {}
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
