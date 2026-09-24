//! Transport capacities and worker admission. Capacities count records/chunks.
use std::num::NonZeroUsize;
/// **Tune first.** Larger chunks amortize handoff only when they fill. Measure actual
/// records/chunk, sparse polls and visibility latency; check publisher batch size overshoot.
pub const CHUNK_CAPACITY: NonZeroUsize = NonZeroUsize::new(256).unwrap();
/// **Tune with processing limits.** Scratch capacity for removing/recycling ready chunks;
/// raising it cannot override the processor/publisher batch cap.
pub const DRAIN_BATCH_CHUNKS: usize = 8;
/// **Measure first under backpressure.** Extra chunks can reduce producer spinning at the
/// cost of resident memory. Each lane must cover every admitted producer.
pub const CHUNKS_PER_PRODUCER_PER_LANE: usize = 2;
/// **Measure first.** Trades startup work/resident memory for fewer allocation misses during
/// recording. Does not change warmed append cost.
pub const PREALLOCATE: bool = false;
/// **Measure first.** More probes trade idle CPU for wake latency. This controls idle
/// consumers; full-buffer producers always spin until storage is available.
pub const IDLE_PROBES: usize = 64;
/// **Capacity policy.** Minimum/fallback registered OS-worker slots, not threads to spawn.
/// Increase for host pools that exceed admission capacity, not as a throughput shortcut.
pub const MIN_PRODUCER_SLOTS: usize = 64;
/// **Test-only.** Reduced probing lets Loom explore interleavings without exhausting its
/// search budget; it does not set production behavior.
pub const MODEL_IDLE_PROBES: usize = 1;

#[derive(Clone, Copy, Debug)]
pub struct ChunkConfig {
    /// Records per chunk; see the tuning tradeoffs on [`CHUNK_CAPACITY`].
    pub chunk_capacity: NonZeroUsize,
    /// Total timing-lane chunks, including producer/processor-owned allocations.
    pub timing_chunks: NonZeroUsize,
    /// Total span-lane chunks; tune from backpressure and memory measurements.
    pub span_chunks: NonZeroUsize,
    /// Registered OS-worker slots survive polls. Each lane needs at least as
    /// many chunks, so private chunks cannot exhaust every available allocation.
    pub max_producers: NonZeroUsize,
    /// Allocate the configured pool at startup; see [`PREALLOCATE`].
    pub preallocate: bool,
}
impl Default for ChunkConfig {
    fn default() -> Self {
        let producers = std::thread::available_parallelism()
            .map_or(MIN_PRODUCER_SLOTS, |n| n.get().max(MIN_PRODUCER_SLOTS));
        Self::for_producers(NonZeroUsize::new(producers).unwrap())
    }
}
impl ChunkConfig {
    pub fn for_producers(producers: NonZeroUsize) -> Self {
        let chunks = producers
            .get()
            .checked_mul(CHUNKS_PER_PRODUCER_PER_LANE)
            .and_then(NonZeroUsize::new)
            .expect("telemetry chunk count overflow");
        Self {
            chunk_capacity: CHUNK_CAPACITY,
            timing_chunks: chunks,
            span_chunks: chunks,
            max_producers: producers,
            preallocate: PREALLOCATE,
        }
    }
}
/// Alternative SPSC transport. Callers select explicit capacities.
#[derive(Clone, Copy, Debug)]
pub struct RingConfig {
    /// Timing record slots per ring; must be a power of two.
    pub timing_capacity: NonZeroUsize,
    /// Span record slots per ring; must be a power of two.
    pub span_capacity: NonZeroUsize,
    /// Consumer routing partitions; callers supply the workers that drive them.
    pub processors: NonZeroUsize,
    /// Admission/memory limit for active and retained reusable ring pairs.
    pub max_pairs: NonZeroUsize,
}
const _: () = assert!(MIN_PRODUCER_SLOTS > 0 && CHUNKS_PER_PRODUCER_PER_LANE > 0);
const _: () = assert!(DRAIN_BATCH_CHUNKS > 0);
