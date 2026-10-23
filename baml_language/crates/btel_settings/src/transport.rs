//! Transport capacities and worker admission. Capacities count records/chunks.
use std::{num::NonZeroUsize, time::Duration};
/// **Tune first.** Larger chunks amortize handoff only when they fill. Measure actual
/// records/chunk, sparse polls and visibility latency; check publisher batch size overshoot.
pub const CHUNK_CAPACITY: NonZeroUsize = NonZeroUsize::new(256).unwrap();
/// **Tune with processing limits.** Scratch capacity for removing/recycling ready chunks;
/// raising it cannot override the processor/publisher batch cap.
pub const DRAIN_BATCH_CHUNKS: usize = 8;
/// **Measure first under backpressure.** Extra chunks can reduce producer probing at the
/// cost of resident memory. Each lane covers the configured producers with this many
/// chunks each; threads beyond the table share the lanes. This sizes the lane, not
/// its ceiling: see [`overage_wait`].
pub const CHUNKS_PER_PRODUCER_PER_LANE: usize = 2;
/// **Backstop; tune first under sustained backpressure.** How long a producer
/// waits for a recycled chunk, when its lane has none free and is less than
/// one [`OVERAGE_UNIT_CHUNKS`] over its size, before allocating beyond it;
/// the default of [`ChunkConfig::overage_wait_base`].
pub const OVERAGE_WAIT_BASE_DURATION: Duration = Duration::from_micros(10);
/// **Backstop.** Chunks of overage per step of the wait curve; see
/// [`crate::overage`].
pub const OVERAGE_UNIT_CHUNKS: usize = 8;

/// How long a producer waits before allocating beyond its lane's size, with
/// the lane `overage_chunks` in use beyond it already, from a pool's `base`.
pub fn overage_wait(base: Duration, overage_chunks: usize) -> Duration {
    crate::overage::wait(base, overage_chunks / OVERAGE_UNIT_CHUNKS)
}
/// **Measure first.** Trades startup work/resident memory for fewer allocation misses during
/// recording. Does not change warmed append cost.
pub const PREALLOCATE: bool = false;
/// **Measure first.** More probes trade idle CPU for wake latency. This controls idle
/// consumers; producers out of storage wait by [`overage_wait`].
pub const IDLE_PROBES: usize = 64;
/// **Capacity policy.** The producer table's initial size and the sizing basis of
/// the lanes and the capture budget, not threads to spawn and not a limit: a thread
/// beyond it gets a slot of its own and shares the lanes.
pub const MIN_PRODUCER_SLOTS: usize = 64;
/// **Test-only.** Reduced probing lets Loom explore interleavings without exhausting its
/// search budget; it does not set production behavior.
pub const MODEL_IDLE_PROBES: usize = 1;

#[derive(Clone, Copy, Debug)]
pub struct ChunkConfig {
    /// Records per chunk; see the tuning tradeoffs on [`CHUNK_CAPACITY`].
    pub chunk_capacity: NonZeroUsize,
    /// The timing lane's size: chunks kept for reuse, counting producer- and
    /// processor-owned ones. Demand beyond it is met by allocating beyond it,
    /// after the wait of [`overage_wait`].
    pub timing_chunks: NonZeroUsize,
    /// The span lane's size, likewise; tune from backpressure and memory
    /// measurements.
    pub span_chunks: NonZeroUsize,
    /// The producer table's initial size: registered OS-worker slots, which
    /// survive polls. A thread beyond it gets a slot of its own, but the
    /// lanes stay sized for this many: each needs at least as many chunks, so
    /// it is not over its size with one chunk per configured producer.
    pub max_producers: NonZeroUsize,
    /// Allocate the configured pool at startup; see [`PREALLOCATE`].
    pub preallocate: bool,
    /// The base of this pool's overage wait curve; see
    /// [`OVERAGE_WAIT_BASE_DURATION`]. Tests set it to make a wait certain.
    pub overage_wait_base: Duration,
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
            overage_wait_base: OVERAGE_WAIT_BASE_DURATION,
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
const _: () = assert!(!OVERAGE_WAIT_BASE_DURATION.is_zero() && OVERAGE_UNIT_CHUNKS > 0);
const _: () = assert!(DRAIN_BATCH_CHUNKS > 0);
