//! Leaf crate for the BEX profiling store.
//!
//! Holds the id newtypes and wire encodings (`ids`), the profiling clock and
//! producer record formats (`prof::{clock, record}`), and the segmented
//! local backend (`prof::backend`): durable formats, publication store,
//! reader, CCT/evidence codecs, memory governor, and session state.
//!
//! This crate is deliberately a dependency leaf: it must never depend on the
//! VM, the engine, tokio, or protobuf. `bex_events` re-exports these modules
//! so existing callers are unchanged; `baml_query_profiles` depends on this
//! crate directly.
//!
//! The transport substrate (rings, registry, consumer thread) stays in
//! `bex_events`; the backend reaches it through the function-pointer hooks in
//! [`prof::backend::hooks`].

pub mod ids;
pub mod prof;
