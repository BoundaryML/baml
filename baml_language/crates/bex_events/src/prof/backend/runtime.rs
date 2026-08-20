//! Process engine-to-session registry used by the native ring consumer.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, Weak},
};

use super::{
    BoundaryHandle, ErrorCaptureAttempt, ErrorCaptureId, ProfilerSession, Reservation,
    TerminalErrorTarget, ValueLossReason, ValueState,
};
use crate::ids::{CallRef, EngineId, ProcessEuid};

fn engines() -> &'static Mutex<HashMap<u64, Weak<ProfilerSession>>> {
    static ENGINES: OnceLock<Mutex<HashMap<u64, Weak<ProfilerSession>>>> = OnceLock::new();
    ENGINES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn register_engine_session(engine_id: EngineId, session: &Arc<ProfilerSession>) {
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
    let session = engines()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&engine_id.0)
        .and_then(Weak::upgrade);
    let Some(session) = session else { return };
    session.consume_raw_bytes(process_euid, engine_id, bytes);
}

pub fn record_engine_transport_loss(engine_id: EngineId, handle: BoundaryHandle) {
    let session = engines()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&engine_id.0)
        .and_then(Weak::upgrade);
    if let Some(session) = session {
        session.record_structural_transport_loss(handle);
    }
}

pub fn reserve_engine_error_attempt(
    engine_id: EngineId,
    handle: BoundaryHandle,
    manual_eligible: bool,
) -> Option<Reservation> {
    engines()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&engine_id.0)
        .and_then(Weak::upgrade)
        .and_then(|session| session.reserve_error_attempt(handle, manual_eligible))
}

pub fn reserve_engine_error_value(
    engine_id: EngineId,
    handle: BoundaryHandle,
    manual_eligible: bool,
) -> Result<Reservation, ValueLossReason> {
    let session = engines()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&engine_id.0)
        .and_then(Weak::upgrade)
        .ok_or(ValueLossReason::StoreUnavailable)?;
    session
        .boundary_publisher(handle)
        .ok_or(ValueLossReason::StoreUnavailable)?;
    session.reserve_value_work(manual_eligible)
}

pub fn submit_engine_error_attempt(
    engine_id: EngineId,
    handle: BoundaryHandle,
    attempt: ErrorCaptureAttempt,
    reservation: Reservation,
) {
    if let Some(session) = engines()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&engine_id.0)
        .and_then(Weak::upgrade)
    {
        session.submit_error_attempt(handle, attempt, reservation);
    }
}

pub fn complete_engine_error_value(engine_id: EngineId, id: ErrorCaptureId, value: ValueState) {
    if let Some(session) = engines()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&engine_id.0)
        .and_then(Weak::upgrade)
    {
        session.complete_error_value(id, value);
    }
}

pub fn submit_engine_terminal_error(
    engine_id: EngineId,
    handle: BoundaryHandle,
    call_ref: CallRef,
    target: TerminalErrorTarget,
    reservation: Reservation,
) {
    if let Some(session) = engines()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&engine_id.0)
        .and_then(Weak::upgrade)
    {
        session.submit_terminal_error(handle, call_ref, target, reservation);
    }
}

pub fn record_engine_error_attempt_loss(engine_id: EngineId, handle: BoundaryHandle) {
    if let Some(session) = engines()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&engine_id.0)
        .and_then(Weak::upgrade)
    {
        session.record_error_attempt_transport_loss(handle);
    }
}

pub fn record_engine_terminal_error_loss(engine_id: EngineId, handle: BoundaryHandle) {
    if let Some(session) = engines()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&engine_id.0)
        .and_then(Weak::upgrade)
    {
        session.record_terminal_error_transport_loss(handle);
    }
}

pub fn maintain_sessions() {
    let mut engines = engines()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    engines.retain(|_, session| session.strong_count() != 0);
    for session in engines.values().filter_map(Weak::upgrade) {
        session.maintain_ready_boundaries();
    }
}
