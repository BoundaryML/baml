//! Process engine-to-session registry used by the native ring consumer.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

#[cfg(not(target_arch = "wasm32"))]
use smallvec::SmallVec;

use super::ProfilerSession;
#[cfg(not(target_arch = "wasm32"))]
use super::{
    ErrorCaptureAttempt, ErrorCaptureId, ExecutionHandle, Reservation, TerminalErrorTarget,
    ValueLossReason, ValueState,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::ids::CallRef;
use crate::ids::{EngineId, ProcessEuid};

// Retain queued payloads until the consumer drains and acknowledges engine close.
fn engines() -> &'static Mutex<HashMap<u64, Arc<ProfilerSession>>> {
    static ENGINES: OnceLock<Mutex<HashMap<u64, Arc<ProfilerSession>>>> = OnceLock::new();
    ENGINES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn engine_session(engine_id: EngineId) -> Option<Arc<ProfilerSession>> {
    engines()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&engine_id.0)
        .cloned()
}

#[cfg(not(target_arch = "wasm32"))]
fn live_sessions() -> SmallVec<[Arc<ProfilerSession>; 4]> {
    let engines = engines()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut sessions = SmallVec::new();
    for session in engines.values() {
        if !sessions
            .iter()
            .any(|existing| Arc::ptr_eq(existing, session))
        {
            sessions.push(Arc::clone(session));
        }
    }
    sessions
}

pub fn register_engine_session(engine_id: EngineId, session: &Arc<ProfilerSession>) {
    if !session.is_on() && !session.is_collecting_logs() {
        return;
    }
    #[cfg(not(baml_loom))]
    if let (Some(sizing), Some(memory)) = (session.sizing(), session.memory()) {
        super::hooks::configure_transport(
            memory.clone(),
            sizing.transport_segment_bytes,
            sizing.transport_freelist_segments,
        );
    }
    engines()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(engine_id.0, Arc::clone(session));
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
    #[cfg(not(target_arch = "wasm32"))]
    session.drain_producer_commands();
    session.consume_raw_bytes(process_euid, engine_id, bytes);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn record_session_transport_loss(session: &ProfilerSession, handle: ExecutionHandle) {
    session.record_structural_transport_loss(handle);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn reserve_session_error_attempt(
    session: &ProfilerSession,
    handle: ExecutionHandle,
    manual_eligible: bool,
) -> Option<Reservation> {
    session.reserve_error_attempt(handle, manual_eligible)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn reserve_session_error_value(
    session: &ProfilerSession,
    handle: ExecutionHandle,
    manual_eligible: bool,
) -> Result<Reservation, ValueLossReason> {
    if !session.boundary_accepts_producer(handle) {
        return Err(ValueLossReason::StoreUnavailable);
    }
    session.reserve_value_work(manual_eligible)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn submit_session_error_attempt(
    session: &ProfilerSession,
    handle: ExecutionHandle,
    attempt: ErrorCaptureAttempt,
    reservation: Reservation,
) {
    session.submit_error_attempt(handle, attempt, reservation);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn complete_session_error_value(
    session: &ProfilerSession,
    handle: ExecutionHandle,
    id: ErrorCaptureId,
    value: ValueState,
) {
    session.complete_error_value(handle, id, value);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn submit_session_terminal_error(
    session: &ProfilerSession,
    handle: ExecutionHandle,
    call_ref: CallRef,
    target: TerminalErrorTarget,
    reservation: Reservation,
) {
    session.submit_terminal_error(handle, call_ref, target, reservation);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn record_session_error_attempt_loss(session: &ProfilerSession, handle: ExecutionHandle) {
    session.record_error_attempt_transport_loss(handle);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn record_session_terminal_error_loss(session: &ProfilerSession, handle: ExecutionHandle) {
    session.record_terminal_error_transport_loss(handle);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn drain_session_commands() -> bool {
    let mut progress = false;
    for session in live_sessions() {
        progress |= session.drain_producer_commands();
    }
    progress
}

#[cfg(not(target_arch = "wasm32"))]
pub fn resolve_session_thread_ends() -> bool {
    let mut progress = false;
    for session in live_sessions() {
        progress |= session.resolve_thread_ends_after_sweep();
    }
    progress
}

#[cfg(not(target_arch = "wasm32"))]
pub fn maintain_sessions() -> bool {
    let mut progress = false;
    for session in live_sessions() {
        progress |= session.maintain_ready_executions();
    }
    progress
}

/// Flush path: publish everything publishable in every live session
/// (`flush_and_join` / process exit).
#[cfg(not(target_arch = "wasm32"))]
pub fn flush_sessions() {
    for session in live_sessions() {
        session.force_publish();
    }
}

/// The smallest publication age trigger across live sessions, for the
/// consumer's park timeout (streams spec §5.3: `min(WAKE_INTERVAL,
/// publish_interval)` when the interval is shorter than the park).
#[must_use]
#[cfg(not(target_arch = "wasm32"))]
pub fn min_publish_interval() -> Option<std::time::Duration> {
    live_sessions()
        .iter()
        .filter_map(|session| session.publish_interval())
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prof::backend::{
        EncodedLog, LogDelivery, LogEvent, ProfilerConfig, Reservation, ValueLossReason, ValueState,
    };

    #[derive(Debug)]
    struct CountDelivery(Arc<std::sync::atomic::AtomicUsize>);

    impl LogDelivery for CountDelivery {
        fn deliver(self: Box<Self>, log: EncodedLog, _reservation: Reservation) {
            assert_eq!(log.data, Some(vec![1, 2, 3]));
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    #[test]
    fn registration_owns_active_session_until_close_but_not_off_session() {
        let (session, _) = ProfilerSession::from_config(ProfilerConfig {
            enabled: false,
            ..Default::default()
        });
        let engine_id = EngineId(u64::MAX - 91);
        register_engine_session(engine_id, &session);
        assert!(engine_session(engine_id).is_none());
        assert!(session.enable_log_collection());
        register_engine_session(engine_id, &session);
        let delivered = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut bytes = Vec::new();
        assert!(session.publish_log_with_delivery(
            None,
            EncodedLog {
                event: LogEvent {
                    call_ref: None,
                    timestamp_ms: 1,
                    level: None,
                    message_preview: None,
                    source_column: None,
                    source: None,
                    event_name: None,
                    distinct_id: None,
                    context: ValueState::Lost(ValueLossReason::StoreUnavailable),
                    data: ValueState::Lost(ValueLossReason::StoreUnavailable),
                },
                data: Some(vec![1, 2, 3]),
                context: None,
            },
            session.reserve_log_work().unwrap(),
            Some(Box::new(CountDelivery(delivered.clone()))),
            |payload_id| {
                let record = crate::prof::record::RawRecord::Log { payload_id };
                bytes.resize(record.encoded_len(), 0);
                record.encode_to(&mut bytes);
                true
            },
        ));
        let weak = Arc::downgrade(&session);
        drop(session);
        assert!(weak.upgrade().is_some());
        assert_eq!(delivered.load(std::sync::atomic::Ordering::Relaxed), 0);
        for _ in 0..2 {
            consume_engine_bytes(ProcessEuid([0; 16]), engine_id, &bytes);
        }
        assert_eq!(delivered.load(std::sync::atomic::Ordering::Relaxed), 1);
        unregister_engine_session(engine_id);
        assert!(weak.upgrade().is_none());
    }
}
