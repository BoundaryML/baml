//! Domain contracts for the segmented local profiling backend.
//!
//! Producers, stores, and readers share these domain types directly.

mod boundary;
mod cct;
#[cfg(not(target_arch = "wasm32"))]
mod cct_codec;
#[cfg(not(target_arch = "wasm32"))]
mod decoder;
mod domain;
mod evidence;
#[cfg(not(target_arch = "wasm32"))]
mod evidence_codec;
mod memory;
#[cfg(not(target_arch = "wasm32"))]
mod reader;
#[cfg(not(target_arch = "wasm32"))]
mod runtime;
mod session;
mod sizing;
#[cfg(not(target_arch = "wasm32"))]
mod store;

pub use boundary::{
    BoundaryEndStatus, BoundaryHandle, BoundaryMetadata, BoundaryPhase,
    BoundaryProducerHealthSnapshot, BoundaryRegistry, BoundarySlotUnavailable, BoundaryThreadLease,
    LeaseUnavailable, RootBoundaryCompletionGuard,
};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use cct::{ActiveCctEpoch, ContextAdmission, ParentContextRef};
pub use cct::{
    BoundaryRef, CctCounters, ContextDelta, ContextRef, CounterHealth, DerivedTiming,
    OverflowDelta, OverflowReason, SealedCctEpoch,
};
#[cfg(not(target_arch = "wasm32"))]
pub use cct_codec::{CctCodecError, CctSegmentData};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use cct_codec::{decode_cct_payload, encode_cct_epoch};
#[cfg(not(target_arch = "wasm32"))]
pub use decoder::{BoundaryHealthSnapshot, ProfilerCheckpoint, QueueHealthSnapshot};
pub use domain::{
    CapturePlan, CapturePlanDecodeError, CodecVersion, ContextKey, ContextTuple, EdgeKind,
    FunctionCaptureClass, LocalIdOverrides, RoleMask, SelectionReasons, ValueCid,
    resolve_capture_plan,
};
pub use evidence::{
    ErrorCapture, ErrorCaptureAttempt, ErrorCaptureId, ErrorCaptureLossReason, ErrorSource,
    ErrorUnwindKind, RuntimeIdAnnotation, SpanEnd, SpanRuntimeId, SpanStart, TerminalErrorRef,
    TerminalErrorTarget, ThrowSite, ValueLossReason, ValueOccurrence, ValueRole, ValueState,
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
pub use memory::{MemoryDenied, Owner, ProfilerMemoryGovernor, Reservation, ReservationClass};
#[cfg(not(target_arch = "wasm32"))]
pub use reader::{
    DurableRunReader, ErrorStack, MergedContext, ProfileRun, RunReadError, RunReaderCursor,
    SpanEvidence,
};
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub use runtime::{
    complete_session_error_value, consume_engine_bytes, drain_session_commands, maintain_sessions,
    record_session_error_attempt_loss, record_session_terminal_error_loss,
    record_session_transport_loss, register_engine_session, reserve_session_error_attempt,
    reserve_session_error_value, resolve_session_thread_ends, submit_session_error_attempt,
    submit_session_terminal_error, unregister_engine_session,
};
#[cfg(not(target_arch = "wasm32"))]
pub use session::ActiveRootAdmission;
pub use session::{
    ActiveRootProfiler, AwaitClockInvalid, InactiveReason, ProfilerSession, RootAdmission,
    RootProfileIntent, RootProfiler, SetupDiagnostic,
};
pub use sizing::{
    DerivedSizing, DiskBudget, InvalidMemoryBudget, MeasuredLayouts, ProfilerConfig,
    ProfilerSizingPolicy,
};
#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
pub use wasm_runtime_stubs::{
    complete_session_error_value, record_session_error_attempt_loss,
    record_session_terminal_error_loss, record_session_transport_loss, register_engine_session,
    reserve_session_error_attempt, reserve_session_error_value, submit_session_error_attempt,
    submit_session_terminal_error, unregister_engine_session,
};

#[cfg(target_arch = "wasm32")]
mod wasm_runtime_stubs {
    use std::sync::Arc;

    use super::{
        BoundaryHandle, ErrorCaptureAttempt, ErrorCaptureId, ProfilerSession, Reservation,
        TerminalErrorTarget, ValueLossReason, ValueState,
    };
    use crate::ids::{CallRef, EngineId};

    pub fn register_engine_session(_engine_id: EngineId, _session: &Arc<ProfilerSession>) {}

    pub fn unregister_engine_session(_engine_id: EngineId) {}

    pub fn reserve_session_error_attempt(
        _session: &ProfilerSession,
        _handle: BoundaryHandle,
        _manual_eligible: bool,
    ) -> Option<Reservation> {
        None
    }

    pub fn reserve_session_error_value(
        _session: &ProfilerSession,
        _handle: BoundaryHandle,
        _manual_eligible: bool,
    ) -> Result<Reservation, ValueLossReason> {
        Err(ValueLossReason::StoreUnavailable)
    }

    pub fn submit_session_error_attempt(
        _session: &ProfilerSession,
        _handle: BoundaryHandle,
        _attempt: ErrorCaptureAttempt,
        _reservation: Reservation,
    ) {
    }

    pub fn complete_session_error_value(
        _session: &ProfilerSession,
        _handle: BoundaryHandle,
        _id: ErrorCaptureId,
        _value: ValueState,
    ) {
    }

    pub fn submit_session_terminal_error(
        _session: &ProfilerSession,
        _handle: BoundaryHandle,
        _call_ref: CallRef,
        _target: TerminalErrorTarget,
        _reservation: Reservation,
    ) {
    }

    pub fn record_session_error_attempt_loss(_session: &ProfilerSession, _handle: BoundaryHandle) {}

    pub fn record_session_terminal_error_loss(_session: &ProfilerSession, _handle: BoundaryHandle) {
    }

    pub fn record_session_transport_loss(_session: &ProfilerSession, _handle: BoundaryHandle) {}
}
#[cfg(not(target_arch = "wasm32"))]
pub use store::{
    AdmittedBoundary, BeginBoundaryResult, BoundaryRunMeta, DecodedCasObject, DecodedCctSegment,
    DecodedEvidenceSegment, DecodedRunEnd, FinishBoundaryResult, IndeterminateToken, ProfilerStore,
    PublishBatchResult, PublishCasResult, ResolveIndeterminateResult, RunEnd, RunEndSegmentFence,
    SegmentHighWater, SegmentReadError, StoreFailureReason, StoreFileKind, StoreOpenError,
    StorePlatform,
};
#[cfg(not(target_arch = "wasm32"))]
pub use store::{CleanProfilesError, SegmentKind, clean_profiles_v1, decode_evidence_segment};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use store::{decode_cas_object, decode_cct_segment, decode_run_end, decode_run_meta};
