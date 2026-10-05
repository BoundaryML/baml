//! Reading Btel recordings without a query backend.
//!
//! Recording files and CAS blobs stay authoritative on disk. This crate finds
//! completed files, validates one file at a time, interprets clock and call
//! evidence, and decodes captured values lazily. It never writes or maintains a
//! persistent index, and has no SQL dependency. Query backends and telemetry
//! projectors share captured-value navigation, usage extraction and estimated
//! model prices; callers own aggregation state.
#![cfg(not(target_arch = "wasm32"))]

pub mod cas;
pub mod context;
pub mod discovery;
pub mod evidence;
pub mod file;
pub mod layout;
pub mod pricing;
pub mod source_map;
pub mod timing;
pub mod usage;
pub mod value;

pub use btel_recorder::{RecordingId, proto};
pub use btel_snapshot::CasId;
