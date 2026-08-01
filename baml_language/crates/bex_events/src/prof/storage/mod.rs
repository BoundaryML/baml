//! Append-only BCCT session segments and boundary snapshots.
//!
//! This module deliberately has no dependency on the profile consumer.  The
//! consumer-facing wiring can therefore be added independently while the wire
//! codec remains usable by native files and in-memory/WASM sinks.

mod format;
mod layout;
mod meta;
mod rows;
mod session;
mod writer;

pub use format::{
    BCCT_FORMAT_VERSION, BCCT_HEADER_LEN, BCCT_TRAILER_LEN, BLOCK_HEADER_LEN, BLOCK_TRAILER_LEN,
    BcctHeader, BlockHeader, BlockKind, ClockDescriptor, FooterTrailer, crc32c,
};
pub use layout::{
    BoundarySnapshot, SessionLayout, anchored_rename, create_dir_all_anchored,
    sync_parent_directory,
};
pub use meta::{
    BoundaryBeginMeta, BoundaryBoundMeta, BoundaryCompleteMeta, BoundaryCounts, BoundaryLossMeta,
    BoundaryMetaKind, BoundaryTriggerMeta, META_MAX_RECORD_LEN, MetaRecord, MetaScan, MetaWriter,
    SessionBeginMeta, SessionEndMeta, SessionEpochCloseMeta, SessionHeartbeatMeta, SessionMetaKind,
    TypedBoundaryMeta, TypedSessionMeta, append_meta_d0, append_meta_d2,
    decode_typed_boundary_meta, decode_typed_session_meta, encode_typed_boundary_meta,
    encode_typed_session_meta, scan_meta_bytes, scan_meta_reader,
};
pub use rows::{
    BlockRows, CctDeltaRow, CctHistogramRow, FooterIndexRow, InstanceRow, LlmDeltaRow, MarkerKind,
    MarkerRow, ModelBirthRow, NodeBirthRow, PartitionBindRow, SpawnEdgeRow, WatermarkRow,
};
pub use session::{
    IDLE_WATERMARK_NS, SEGMENT_ROTATE_BYTES, SEGMENT_ROTATE_NS, SessionStreamWriter,
};
pub use writer::{
    AppendOutcome, AsyncFileSync, BcctWriter, BlockIndexEntry, CheckpointCadence,
    FileSyncCompletion, RawBlock, ScanResult, ScannedBlock, SealedIndex, SegmentState,
    probe_sealed_index, scan_bcct_bytes, scan_bcct_reader,
};
