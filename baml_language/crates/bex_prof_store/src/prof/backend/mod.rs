//! Domain contracts for the segmented local profiling backend.
//!
//! Producers, stores, and readers share these domain types directly.

mod cct;
#[cfg(not(target_arch = "wasm32"))]
mod cct_codec;
mod domain;
mod evidence;
#[cfg(not(target_arch = "wasm32"))]
mod evidence_codec;
mod execution;
#[cfg(not(target_arch = "wasm32"))]
mod function_table;
mod memory;
#[cfg(not(target_arch = "wasm32"))]
mod reader;
mod sizing;
#[cfg(not(target_arch = "wasm32"))]
mod store;
#[cfg(not(target_arch = "wasm32"))]
mod writer;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use cct::{ActiveCctEpoch, ContextAdmission, ParentContextRef};
pub use cct::{
    CctCounters, ContextDelta, ContextRef, CounterHealth, DerivedTiming, OverflowDelta,
    OverflowReason, SealedCctEpoch,
};
#[cfg(not(target_arch = "wasm32"))]
pub use cct_codec::{CctCodecError, CctSegmentData};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use cct_codec::{decode_cct_payload, encode_cct_epoch};
pub use domain::{
    CapturePlan, CapturePlanDecodeError, CodecVersion, ContextKey, ContextTuple, EdgeKind,
    FunctionCaptureClass, LocalIdOverrides, RoleMask, SelectionReasons, ValueCid,
    resolve_capture_plan,
};
pub use evidence::{
    ErrorCapture, ErrorCaptureAttempt, ErrorCaptureId, ErrorCaptureLossReason, ErrorSource,
    ErrorUnwindKind, RuntimeIdAnnotation, SpanEnd, SpanRuntimeId, SpanStart, TerminalErrorRef,
    TerminalErrorTarget, ThreadEnd, ThreadStart, ThreadStartKind, ThrowSite, ValueLossReason,
    ValueOccurrence, ValueRole, ValueState,
};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use evidence::{
    ErrorCodecError, decode_error_capture, decode_terminal_error_ref, encode_error_capture,
    encode_terminal_error_ref,
};
#[cfg(not(target_arch = "wasm32"))]
pub use evidence_codec::{EvidenceCodecError, EvidenceFact};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use evidence_codec::{decode_evidence_payload, encode_evidence_facts};
pub use execution::{
    ExecutionEndStatus, ExecutionHandle, ExecutionMetadata, ExecutionPhase,
    ExecutionProducerHealthSnapshot, ExecutionRegistry, ExecutionSlotUnavailable,
    ExecutionThreadLease, LeaseUnavailable, RootExecutionCompletionGuard,
};
#[cfg(not(target_arch = "wasm32"))]
pub use function_table::{
    FunctionKindCode, FunctionOriginCode, FunctionSourceSpan, FunctionTable, FunctionTableEntry,
    FunctionTableError, FunctionTableFile, decode_function_table, encode_function_table,
};
pub use memory::{MemoryDenied, Owner, ProfilerMemoryGovernor, Reservation, ReservationClass};
#[cfg(not(target_arch = "wasm32"))]
pub use reader::{
    DataIssue, DataState, EngineStarted, ErrorStack, ExecutionProfile, ExecutionReader,
    ExecutionStatus, ExecutionSummary, IndexState, MergedContext, ReadError, RootEnded,
    RootIndexEntry, RootStarted, SpanEvidence, StreamReader, StreamStarted, ThreadEvidence,
    ThreadIssue, ThreadIssueKind, UnresolvedDependency, list_executions, list_streams,
};
pub use sizing::{
    DerivedSizing, DiskBudget, InvalidMemoryBudget, MeasuredLayouts, ProfilerConfig,
    ProfilerSizingPolicy,
};
#[cfg(not(target_arch = "wasm32"))]
pub use writer::{ExecutionCheckpoint, StreamCheckpoint, counters};


#[cfg(not(target_arch = "wasm32"))]
pub use store::{
    CAS_FORMAT_VERSION, CleanProfilesError, DataGroup, DecodedCasObject, DecodedDataSegment,
    DecodedMetaSegment, IndeterminateToken, MetaRecord, Plane, ProfilerStore, PublishBatchResult,
    PublishCasResult, ROOT_ENDED_FLAG_ROOT_STARTED_LOST, RawDataGroup, ResolveIndeterminateResult,
    SCHEMA_VERSION, SegmentReadError, StoreFailureReason, StoreFileKind, StoreOpenError,
    StorePlatform, StreamHighWater, StreamId, clean_profiles_v1, decode_cas_object,
    decode_data_segment, decode_meta_segment, segment_path, stream_directory,
    stream_open_in_process,
};
