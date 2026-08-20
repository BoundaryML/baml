//! Process engine-to-session registry used by the native ring consumer.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, Weak},
};

use smallvec::SmallVec;

use super::{
    BoundaryHandle, ErrorCaptureAttempt, ErrorCaptureId, ProfilerSession, Reservation,
    TerminalErrorTarget, ValueLossReason, ValueState,
};
use crate::ids::{CallRef, EngineId, ProcessEuid};

fn engines() -> &'static Mutex<HashMap<u64, Weak<ProfilerSession>>> {
    static ENGINES: OnceLock<Mutex<HashMap<u64, Weak<ProfilerSession>>>> = OnceLock::new();
    ENGINES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn engine_session(engine_id: EngineId) -> Option<Arc<ProfilerSession>> {
    engines()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&engine_id.0)
        .and_then(Weak::upgrade)
}

fn live_sessions() -> SmallVec<[Arc<ProfilerSession>; 4]> {
    let mut engines = engines()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    engines.retain(|_, session| session.strong_count() != 0);
    let mut sessions = SmallVec::new();
    for session in engines.values().filter_map(Weak::upgrade) {
        if !sessions
            .iter()
            .any(|existing| Arc::ptr_eq(existing, &session))
        {
            sessions.push(session);
        }
    }
    sessions
}

pub fn register_engine_session(engine_id: EngineId, session: &Arc<ProfilerSession>) {
    #[cfg(not(baml_loom))]
    if let (Some(sizing), Some(memory)) = (session.sizing(), session.memory()) {
        crate::prof::registry::configure_global_transport(
            memory.clone(),
            sizing.transport_segment_bytes,
            sizing.transport_freelist_segments,
        );
    }
    engines()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(engine_id.0, Arc::downgrade(session));
}

pub fn unregister_engine_session(engine_id: EngineId) {
    engines()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&engine_id.0);
}

pub fn consume_engine_bytes(process_euid: ProcessEuid, engine_id: EngineId, bytes: &[u8]) {
    let session = engine_session(engine_id);
    let Some(session) = session else { return };
    // A producer command committed before a later structural end must be
    // folded first. Resolve the engine once here instead of rebuilding and
    // deduplicating the process-wide session list for every drained slice.
    session.drain_producer_commands();
    session.consume_raw_bytes(process_euid, engine_id, bytes);
}

pub fn record_session_transport_loss(session: &ProfilerSession, handle: BoundaryHandle) {
    session.record_structural_transport_loss(handle);
}

pub fn reserve_session_error_attempt(
    session: &ProfilerSession,
    handle: BoundaryHandle,
    manual_eligible: bool,
) -> Option<Reservation> {
    session.reserve_error_attempt(handle, manual_eligible)
}

pub fn reserve_session_error_value(
    session: &ProfilerSession,
    handle: BoundaryHandle,
    manual_eligible: bool,
) -> Result<Reservation, ValueLossReason> {
    if !session.boundary_accepts_producer(handle) {
        return Err(ValueLossReason::StoreUnavailable);
    }
    session.reserve_value_work(manual_eligible)
}

pub fn submit_session_error_attempt(
    session: &ProfilerSession,
    handle: BoundaryHandle,
    attempt: ErrorCaptureAttempt,
    reservation: Reservation,
) {
    session.submit_error_attempt(handle, attempt, reservation);
}

pub fn complete_session_error_value(
    session: &ProfilerSession,
    handle: BoundaryHandle,
    id: ErrorCaptureId,
    value: ValueState,
) {
    session.complete_error_value(handle, id, value);
}

pub fn submit_session_terminal_error(
    session: &ProfilerSession,
    handle: BoundaryHandle,
    call_ref: CallRef,
    target: TerminalErrorTarget,
    reservation: Reservation,
) {
    session.submit_terminal_error(handle, call_ref, target, reservation);
}

pub fn record_session_error_attempt_loss(session: &ProfilerSession, handle: BoundaryHandle) {
    session.record_error_attempt_transport_loss(handle);
}

pub fn record_session_terminal_error_loss(session: &ProfilerSession, handle: BoundaryHandle) {
    session.record_terminal_error_transport_loss(handle);
}

pub fn drain_session_commands() -> bool {
    let mut progress = false;
    for session in live_sessions() {
        progress |= session.drain_producer_commands();
    }
    progress
}

pub fn resolve_session_thread_ends() -> bool {
    let mut progress = false;
    for session in live_sessions() {
        progress |= session.resolve_thread_ends_after_sweep();
    }
    progress
}

pub fn maintain_sessions() -> bool {
    let mut progress = false;
    for session in live_sessions() {
        progress |= session.maintain_ready_boundaries();
    }
    progress
}
