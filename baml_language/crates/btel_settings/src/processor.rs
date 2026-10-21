//! First-level aggregation: batching first, cache locality next.
use std::{num::NonZeroUsize, time::Duration};

/// **Tune first, together with the publisher limit.** The smaller limit wins;
/// raising this alone has no effect while the publisher caps batches at one.
/// Larger batches amortize setup but increase time between deadline checks.
pub const REQUESTED_BATCH_CHUNKS: NonZeroUsize = NonZeroUsize::new(8).unwrap();
/// **Measure first.** More slots may reduce downstream updates but increase the
/// cache footprint. Compare realistic call paths and high-cardinality input;
/// measure both time/completion and updates/completion. Must be a power of two.
pub const COMBINING_SLOTS: usize = 16;
/// **Tune first for aggregation latency.** Longer intervals may combine more
/// updates; shorter intervals expose partial totals sooner. Collisions still
/// emit immediately. This is separate from the publisher's file-sealing timer.
pub const CACHE_FLUSH_INTERVAL_DURATION: Duration = Duration::from_secs(1);
/// **Measure first; diagnostic publisher only.** Bounds its aggregation window.
/// The engine's recording publisher instead seals by encoded size and elapsed time.
pub const NO_SINK_MAX_NODES: NonZeroUsize = NonZeroUsize::new(4096).unwrap();

/// **Fixed architecture.** One consumer owns each pool. Increasing this requires
/// routing/ownership changes; it cannot be used to request more parallelism.
pub const THREADS_PER_RUNTIME: usize = 1;
/// **Fixed protocol.** One worker sends one startup acknowledgement.
pub const STARTUP_CHANNEL_CAPACITY: usize = 1;
/// **Memory bound, exception path only.** Failed futures whose escaping raise an
/// engine remembers for later awaits. Older entries are evicted first; an await
/// of an evicted future reports its origin as unavailable, never guessed.
pub const FUTURE_ERROR_LINKS: usize = 4096;
/// **Sentinel.** A publisher may impose no further processing-batch restriction.
pub const UNLIMITED_PUBLISHER_BATCH: usize = usize::MAX;
const _: () = assert!(COMBINING_SLOTS > 1 && COMBINING_SLOTS.is_power_of_two());
