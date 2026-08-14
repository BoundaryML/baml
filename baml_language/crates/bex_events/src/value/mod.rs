//! Target-neutral `.bamlvalue` metadata and framing.
//!
//! `RunStore` carries [`ValueRef`] metadata only. The bytes live in a value
//! artifact and are hydrated by `readValue(boundaryId, valueRef)`.

pub mod artifact;
pub mod encode;
pub mod live_cache;
pub mod read;
pub mod record;
pub mod writer;

/// The `.bamlvalue` wire types, generated from `value/proto/bamlvalue.proto`.
#[allow(
    clippy::pedantic,
    clippy::doc_markdown,
    unreachable_pub,
    reason = "prost-generated code"
)]
pub mod pb {
    include!(concat!(env!("OUT_DIR"), "/baml.value.v1.rs"));
}

pub use artifact::{
    BlobRef, BlobStore, ByteValueArtifactSink, FileValueArtifactSink, ValueArtifactRef,
    ValueArtifactSink,
};
pub use live_cache::{
    DEFAULT_NATIVE_LIVE_VALUE_CACHE_BYTES, DEFAULT_WASM_LIVE_VALUE_CACHE_BYTES, LiveValueBody,
    LiveValueCache, LiveValueEviction, LiveValueInsertResult, LiveValueKey, LiveValueLookup,
};
pub use read::{BamlvalueContents, read_bamlvalue_from_bytes};
pub use record::{
    CaptureLossKind, CaptureLossReason, CaptureLossRecord, LogEventRecord, LogRecord,
    RunCompletedRecord, RunStartedRecord, ValueAvailability, ValueCapture, ValueCaptureKind,
    ValueCodec, ValueFileRecord, ValueRecord, ValueRef,
};
pub use writer::{ValueIdAllocator, ValueWriteOutcome, ValueWriter};
