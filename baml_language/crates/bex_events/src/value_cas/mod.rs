//! Project-scoped content-addressed storage for captured values.
//!
//! The logical encoder is target-neutral. Native builds additionally expose
//! append-only packs, root manifests, writer/GC locking, and retention
//! primitives. Engine snapshot adaptation and the continuous drain thread are
//! deliberately outside this module: callers enqueue owned
//! [`CanonicalValue`]s or already-built [`ValueDag`]s.

mod audit;
mod canonical;
mod cid;
mod staging;

#[cfg(not(target_arch = "wasm32"))]
mod drain;
#[cfg(not(target_arch = "wasm32"))]
mod gc;
#[cfg(not(target_arch = "wasm32"))]
mod lock;
#[cfg(not(target_arch = "wasm32"))]
mod manifest;
#[cfg(not(target_arch = "wasm32"))]
mod pack;
#[cfg(not(target_arch = "wasm32"))]
mod retention;
#[cfg(not(target_arch = "wasm32"))]
mod root_commit;

pub use audit::{CapturePolicyChangedAudit, ValueAuditRecord};
pub use canonical::{
    CanonicalEncodeError, CanonicalField, CanonicalValue, DagChunk, FieldPresence, MediaContent,
    MediaValue, OmissionValue, ValueDag, encode_value_dag, referenced_cids,
};
pub use cid::{Cid, CidParseError, NODE_CODEC_VERSION};
pub use staging::{
    CallPath, CaptureLoss, CaptureLossReason, DEFAULT_NATIVE_STAGING_BYTES,
    DEFAULT_WASM_STAGING_BYTES, PromotionAudit, PromotionReport, ReleaseReport, StageReport,
    StagedDraft, StagingRing, TriggerId,
};

#[cfg(not(target_arch = "wasm32"))]
pub use drain::{
    DurableValueCapture, ValueBoundaryRegistration, ValueDrainConfig, ValueDrainHandle,
    ValueDrainService, ValueDrainStatsSnapshot, ValueEnqueueOutcome, ValuePromotionOutcome,
    ValueStageOutcome,
};
#[cfg(not(target_arch = "wasm32"))]
pub use gc::{
    GcPackDisposition, GcPackPlan, MarkSet, PackInventory, ProjectGcOptions, ProjectGcOutcome,
    RootMarkReport, build_pack_inventory, collect_project_roots, derive_unsealed_bamlvalue_roots,
    execute_project_gc, expand_mark_closure, plan_sweep,
};
#[cfg(not(target_arch = "wasm32"))]
pub use lock::{GcGuard, WritersLockGuard};
#[cfg(not(target_arch = "wasm32"))]
pub use manifest::{CidManifest, CidManifestReader, CidManifestWriter, ManifestReadOutcome};
#[cfg(not(target_arch = "wasm32"))]
pub use pack::{
    PACK_HEADER_LEN, PackAppendOutcome, PackIndex, PackIndexEntry, PackPaths, PackScan, PackWriter,
    read_pack_chunk, rebuild_pack_index, scan_pack,
};
#[cfg(not(target_arch = "wasm32"))]
pub use retention::{
    RetentionCandidate, RetentionExecution, RetentionLog, RetentionPolicy, Tombstone,
    execute_retention_plan, plan_retention,
};
#[cfg(not(target_arch = "wasm32"))]
pub use root_commit::{RootCommitBatch, RootCommitOutcome, RootCommitter};

/// DAG nodes at or below this size are embedded in their parent.
pub const NODE_INLINE_THRESHOLD: usize = 2 * 1024;
/// Strings and byte strings are split on stable fixed boundaries.
pub const BYTE_CHUNK_LEN: usize = 128 * 1024;
/// Collection leaves and internal nodes have stable fixed fanout.
pub const COLLECTION_CHUNK_LEN: usize = 128;
