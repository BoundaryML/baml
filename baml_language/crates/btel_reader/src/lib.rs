//! Reading Btel recordings without a query backend.
//!
//! Recording files and CAS blobs stay authoritative on disk. This crate finds
//! completed files, validates one file at a time, interprets clock and call
//! evidence, and decodes captured values lazily. It never writes, never keeps
//! state between calls, and has no SQL dependency: a local index and a later
//! cloud reader share it.
#![cfg(not(target_arch = "wasm32"))]

pub mod cas;
pub mod discovery;
pub mod evidence;
pub mod file;
pub mod layout;
pub mod source_map;
pub mod timing;
pub mod value;

pub use btel_recorder::{RecordingId, proto};
pub use btel_snapshot::SnapshotId;
