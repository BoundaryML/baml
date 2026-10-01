//! Independently owned capture graphs. No VM pointers, user-code execution or filesystem dependencies.
//!
//! Values are inline. Only identity-bearing objects require graph IDs. Containers
//! are sampled independently; this is not an atomic snapshot of the entire graph.
//!
//! A capture is plain data (`graph`) in storage reused through a pool
//! (`pool`). A [`Builder`] fills it; finishing it shapes the data into
//! content-addressed blobs (`shape`) and yields the immutable [`Snapshot`].
//! Everything about a capture beyond its data, its size included, is found by
//! walking it (`walk`) when it is shaped, never kept up to date while it is
//! built.

/// The string type the builder takes, so callers need not depend on `bex_str`.
pub use bex_str::BexStr;

mod arena;
mod build;
pub mod context;
mod decode;
mod encoding;
mod graph;
mod hash;
mod memory;
mod pool;
mod shape;
mod snapshot;
mod tags;
mod walk;

pub use build::{Builder, string_map};
pub use decode::{
    BlobError, ChildIndex, DecodeLimits, DecodedMedia, DecodedMediaSource, DecodedName,
    DecodedObject, DecodedRoot, DecodedSnapshot, DecodedValue, Entries, MediaPayload, NodeId,
    SHALLOW_TYPE_BYTES, SharedSnapshot, TypeDescription, decode_blob,
};
pub use encoding::{BLOB_MAGIC, BLOB_VERSION, BlobScratch};
pub use graph::{
    BigintId, Description, FunctionArgs, Limit, MapEntry, MediaSource, NameId, ObjectId, OwnedType,
    Range, SnapshotObject, SnapshotRoot, SnapshotValue, StringId, TypeId, TypeIdentity,
    Uint8ArrayData,
};
pub use hash::CasId;
pub use pool::{Limits, PoolConfig, PoolStats, SnapshotPool};
pub use shape::{BlobIndex, REFERENCE_WEIGHT, ShapePolicy, Shaper};
#[cfg(any(test, feature = "stats"))]
pub use snapshot::CaptureStats;
pub use snapshot::{Blob, Snapshot};

#[cfg(test)]
mod tests;
