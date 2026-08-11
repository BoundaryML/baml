//! The CCT aggregation engine (observability design §5): the consumer-side
//! replacement for "one disk event per call". Aggregates raw ring records
//! into calling-context-tree counters whose cost grows with *unique
//! behavior* (contexts × windows), never call rate.
//!
//! Everything here is target-neutral (no fs, no threads): the native
//! consumer and the wasm cooperative drain both embed [`engine::CctEngine`];
//! the P3 segment writer drains [`engine::WindowSnapshot`]s to disk.

pub mod blocks;
pub mod crc32c;
pub mod engine;
pub mod flight;
pub mod fold;
pub mod meta;
pub mod nodes;
pub mod raw;
pub mod recent;
pub mod segment;
#[cfg(not(target_arch = "wasm32"))]
pub mod session;
pub mod spawn;

pub use engine::{
    CctDiagnostics, CctEngine, DEFER_MAX_PENDING, DEFER_MAX_SWEEPS, LlmCounters, WindowSnapshot,
};
pub use nodes::{HIST_BUCKETS, Nodes, RECURSION_FOLD_DEPTH};
pub use recent::{RECENT_RING_SLOTS, RecentCall, RecentRing};
pub use spawn::{INSTANCES_EXCEPTIONAL, INSTANCES_FIRST, SpawnEdges};
