//! One source for telemetry configuration, defaults, tuning constants and limits.
//! Compile-time choices stay constants; runtime settings are instance-owned.
//! The mode environment variable is read once per engine, never per record.
//! No mutable global configuration or per-record settings lookup.
//!
//! # Start here
//!
//! Settings are annotated at their definitions so callers need not infer whether
//! a number is a tuning knob or a correctness requirement:
//!
//! - **Tune first:** chunk/batch sizes and file size/flush duration. These are
//!   practical experiments; measure throughput, memory and visibility latency.
//! - **Measure first:** cache sizes, growth/shrink policy, spin counts, shards and
//!   ID ranges. Changes can regress other workloads; use realistic traces.
//! - **Sensitive:** clock tolerances and bounded transport capacity.
//!   Preserve their stated contracts when tuning.
//! - **Fixed/derived:** encoding bounds, layouts, supported thread/cache topology,
//!   and values calculated from other settings. Change the implementation or
//!   source setting first; do not tune these independently.
//!
//! "Sensitive" does not mean every current value is experimentally optimal.
//! It means a change needs evidence appropriate to its correctness contract.
//! Time settings use `_DURATION` for `Duration` values and explicit units such
//! as `_NS` for scalars. Rate errors use `_PPB`; an error rate is not a duration.
//!
//! # Suggested investigation order
//!
//! These are investigation priorities, not a measured ranking of speedups.
//! Profile the producer, processor and publisher separately; changing downstream
//! settings cannot remove VM call-path lookup or raw-clock-read costs.
//!
//! 1. **Chunk size and processing batches:** [`transport::CHUNK_CAPACITY`],
//!    [`processor::REQUESTED_BATCH_CHUNKS`], [`publisher::MAX_BATCH_CHUNKS`],
//!    and [`transport::DRAIN_BATCH_CHUNKS`]. These amortize publication, locking,
//!    admission and per-batch bookkeeping. The effective processing limit is
//!    the smaller processor/publisher limit; the transport scratch batch is a
//!    separate constraint. Larger batches cost memory and latency. Measure actual
//!    records per sealed chunk, sparse traffic and backpressure, not just fullness.
//! 2. **Lookup/aggregation locality:** [`processor::COMBINING_SLOTS`] trades
//!    fixed cache footprint against downstream aggregate updates. Measure both
//!    ns/completion and updates/completion under realistic call-path sequences.
//!    [`policy::CALL_PATH_CACHE_SLOTS`] describes a separate VM cache: increasing
//!    it requires a resolver change, not just editing a number.
//! 3. **Publisher sealing and retained allocation:** [`publisher::TARGET_BYTES`],
//!    [`publisher::FILE_FLUSH_INTERVAL_DURATION`], [`processor::CACHE_FLUSH_INTERVAL_DURATION`], and publisher
//!    growth/shrink settings. These affect file/sealing frequency, aggregation
//!    windows, allocation churn, visibility latency and resident memory. Cache
//!    flushing and file sealing have separate deadlines despite equal defaults.
//! 4. **Contention and stalls:** pool quotas, [`transport::IDLE_PROBES`], and
//!    [`identity::SHARD_COUNT`]. Quotas matter when producers run out of storage;
//!    idle probes trade wake latency for idle CPU. Function shards only affect
//!    registration/lookup/GC, not existing call-path hits. Profile those paths
//!    before increasing memory or spinning longer.
//!
//! # Important settings that are not general speed knobs
//!
//! - [`transport::MIN_PRODUCER_SLOTS`] is admission capacity, not threads to spawn.
//!   [`processor::THREADS_PER_RUNTIME`] describes the supported single-consumer
//!   architecture; adding processors requires an ownership/routing design.
//! - [`clock::DEFAULT_MODE`] selects a backend and can change read cost. Calibration
//!   intervals/error thresholds govern startup and clock validity; relaxing them
//!   does not accelerate each raw read. Validation cadence runs at boundaries.
//! - Capture/visibility policies change which observations are collected. Less
//!   data can cost less, but that is a semantic change, not an equivalent speedup.
//! - Encoding bounds, format version, checked layouts and alignment carry safety
//!   or representation constraints. Bounds do not determine encoded record size;
//!   shrink the representation first and then update its proven bounds. Changing
//!   padding requires contention measurements; hash changes require workload data.
//! - [`identity::ID_RANGE_SIZE`] amortizes rare global refills. A larger range
//!   cannot remove per-allocation TLS access; tune only if refills are material.
pub mod clock;
pub mod encoding;
pub mod identity;
pub mod layout;
pub mod local_files;
pub mod mode;
pub mod policy;
pub mod processor;
pub mod publisher;
pub mod transport;
