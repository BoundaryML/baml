//! Profiling clock, producer record formats, and the segmented backend.
//!
//! The ring transport and consumer thread live in `bex_events::prof`; this
//! module holds everything the durable store needs.

pub mod backend;
pub mod clock;
