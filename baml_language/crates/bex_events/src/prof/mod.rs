//! BEX profiling event stream (`.bamlprof`) — the lock-free tracing pipeline.
//!
//! This module implements the M2–M4 milestones of the BEX event-stream design
//! (`bex-event-stream-design-v2.md`): a segmented SPSC ring per
//! `(engine, os-thread)` pair, a raw fixed-layout record format, and the
//! clock/knob scaffolding they share. It exists to replace the global
//! `Mutex<CollectorStore>` path in [`crate::event_store`] for per-call
//! profiling: producers (VM threads) write records with one `memcpy` plus one
//! `Release` store per event and never block; a background consumer drains
//! rings and transcodes to per-engine `.bamlprof` files.
//!
//! The ring is *lossless by growth*: when a segment fills, the producer links
//! a fresh (or recycled) segment instead of dropping or blocking. Memory is
//! bounded by [`config::ProfConfig::max_overflow_bytes`]; exceeding that cap
//! is a hard process error, never a silent drop.
//!
//! Capacity model (design D6): the documented 100M events/s figure is a
//! *burst* write budget. The sustainable rate is bounded by the consumer's
//! per-core transcode rate (measured in the consumer milestone), and burst
//! tolerance is `max_overflow_bytes / (produce_rate − drain_rate)` seconds of
//! backlog growth.
//!
//! Naming note: `sys_types::CallId` identifies one *engine root invocation*
//! and is unrelated to the per-function-call ids that flow through these
//! records as plain `u64`s. Final id newtypes land with the `ids.rs`
//! milestone (M0); nothing here should reuse `sys_types::CallId`.

pub mod clock;
pub mod config;
#[cfg(all(not(target_arch = "wasm32"), not(baml_loom)))]
pub(crate) mod consumer;
#[cfg(not(target_arch = "wasm32"))]
pub mod file;
pub mod record;
pub(crate) mod registry;
pub(crate) mod ring;
pub(crate) mod sync;
pub(crate) mod wake;

/// The `.bamlprof` wire types, generated from `prof/proto/bamlprof.proto`.
#[cfg(not(target_arch = "wasm32"))]
#[allow(
    clippy::pedantic,
    clippy::doc_markdown,
    unreachable_pub,
    reason = "prost-generated code"
)]
pub mod pb {
    include!(concat!(env!("OUT_DIR"), "/baml.prof.v1.rs"));
}

#[cfg(test)]
mod concurrency_tests;

pub use config::ProfConfig;
#[cfg(all(not(target_arch = "wasm32"), not(baml_loom)))]
pub use consumer::{
    EngineProfileMetadata, FunctionMetaEntry, flush_and_join, register_engine_metadata,
};
#[cfg(not(baml_loom))]
pub use registry::ring_for_engine;
pub use ring::{Ring, RingHandle};

// wasm32: profiling is forced off (no consumer thread, no TSC clock); the
// producer-facing surface compiles to no-ops so dual-target callers don't
// need their own cfgs.
#[cfg(target_arch = "wasm32")]
pub fn flush_and_join(_timeout: std::time::Duration) -> bool {
    true
}
