//! `bex_query` — the sans-io observability query engine (design §9.2).
//!
//! One engine, three hosts: native (mmap), wasm (linear-memory cache), and
//! HTTP Range. Queries either complete from resident bytes or return the
//! exact byte ranges they need ([`source::Poll::NeedData`]); the host
//! fetches and retries. No async runtime in core.
//!
//! - [`source`] — the byte plane: `SegmentSource`, `MmapSource`, `SliceSource`
//! - [`runs`] — §9.6 runs list (bamlmeta scans, no segment reads)
//! - [`cct`] — the shared CCT fold + Left Heavy / top-functions / bands
//! - [`bqf1`] — §9.3 columnar wire frames
//! - [`engine`] — `ObserveEngine`, the host-facing fast-path API
//! - [`bql`] — §8 BQL v1: parser + typed planner + executor

pub mod bqf1;
pub mod bql;
pub mod cct;
pub mod engine;
pub mod runs;
pub mod source;
pub mod values;

pub use bql::{BqlError, BqlTable, ColData, Completeness};
pub use engine::ObserveEngine;
pub use source::{ByteRange, FileId, Poll, SegmentSource};
pub use values::{FnJoin, Hydrated, RunValues, ValueRow};
