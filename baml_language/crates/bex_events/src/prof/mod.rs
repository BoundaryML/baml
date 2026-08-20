//! BEX profiling transport and segmented local backend.
//!
//! Producers write compact fixed-layout records to bounded per-thread rings;
//! the consumer decodes them directly into the CCT/evidence backend.
//!
//! When a segment fills, the producer links a fresh or recycled segment
//! without blocking. Memory is bounded by
//! the derived transport reserve; a record that cannot obtain
//! growth capacity is rejected and reported through boundary health without
//! aborting BAML execution.
//!
//! Capacity model (design D6): the documented 100M events/s figure is a
//! *burst* write budget. The sustainable rate is bounded by the consumer's
//! per-core transcode rate (measured in the consumer milestone), and burst
//! tolerance is `max_overflow_bytes / (produce_rate − drain_rate)` seconds of
//! backlog growth.
//!
//! Naming note: `sys_types::CallId` identifies one *engine root invocation*
//! and is unrelated to the per-function-call ids that flow through these
//! records as plain `u64`s. The id newtypes ([`crate::ids::BexCallId`],
//! [`crate::ids::BexThreadId`], [`crate::ids::FunctionId`]) landed with the
//! M0 `ids.rs` milestone; adopting them in [`record::RawRecord`]'s fields
//! is the remaining follow-up. Nothing here should reuse
//! `sys_types::CallId`.

pub mod backend;
pub mod clock;
pub mod config;
#[cfg(all(not(target_arch = "wasm32"), not(baml_loom)))]
pub(crate) mod consumer;
pub mod record;
pub(crate) mod registry;
pub(crate) mod ring;
pub(crate) mod sync;
pub(crate) mod wake;

#[cfg(test)]
mod concurrency_tests;

pub use config::ProfConfig;
#[cfg(all(not(target_arch = "wasm32"), not(baml_loom)))]
pub use consumer::{engine_closed, flush_and_join};
#[cfg(not(baml_loom))]
pub use registry::ring_for_engine;
pub use ring::{Ring, RingHandle};

// wasm32 has no native background consumer. Generic embedders keep profiling
// off through config, while adapters such as bridge_wasm may opt into a
// cooperative drain; this function remains a no-op because there is no
// background thread to flush.
#[cfg(target_arch = "wasm32")]
pub fn flush_and_join(_timeout: std::time::Duration) -> bool {
    true
}

/// WASM has no native consumer, so engine close has nothing to release.
#[cfg(target_arch = "wasm32")]
pub fn engine_closed(_engine_id: u64) {}
