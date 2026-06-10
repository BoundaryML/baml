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
pub mod record;

pub use config::ProfConfig;
