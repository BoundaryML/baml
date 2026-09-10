//! Bounded output subscription for logs consumed from the shared event transport.

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

use bex_events::{
    ids::BoundaryId,
    prof::backend::{EncodedLog, LogDelivery, LogEvent, Reservation},
    run::{SourceLocation, TraceCallKey},
};

use crate::trace_value_encode::render_encoded_trace_value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceLogMetadata {
    pub level: Option<String>,
    pub source: Option<SourceLocation>,
    pub timestamp_ms: u64,
    pub message_preview: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedTraceLog {
    pub boundary_id: BoundaryId,
    pub call: TraceCallKey,
    pub metadata: TraceLogMetadata,
    pub body: Vec<u8>,
    pub event: LogEvent,
    pub context: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedTraceLog {
    pub metadata: TraceLogMetadata,
    pub body: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceLogFailureReason {
    SnapshotMissing,
    EncodeFailed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceLogFailure {
    pub boundary_id: BoundaryId,
    pub call: TraceCallKey,
    pub metadata: TraceLogMetadata,
    pub reason: TraceLogFailureReason,
    pub diagnostic: String,
    pub event: Option<LogEvent>,
    pub context: Option<Vec<u8>>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct EncodedTraceLogDrainReport {
    pub logs: Vec<EncodedTraceLog>,
    pub failures: Vec<TraceLogFailure>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct TraceLogDrainReport {
    pub logs: Vec<RenderedTraceLog>,
    pub failures: Vec<TraceLogFailure>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TraceLoggerStats {
    pub published: u64,
    pub skipped_log_queue_full: u64,
    pub abandoned_reservations: u64,
}

#[derive(Clone, Debug)]
pub struct TraceLogger {
    enabled: Option<Arc<TraceLoggerEnabled>>,
}

#[derive(Debug)]
struct TraceLoggerEnabled {
    max_pending: usize,
    occupied: AtomicUsize,
    pending: Mutex<VecDeque<InboxEntry>>,
    published: AtomicU64,
    skipped: AtomicU64,
    abandoned: AtomicU64,
}

#[derive(Debug)]
struct InboxEntry {
    log: Result<EncodedTraceLog, TraceLogFailure>,
    _reservation: Reservation,
}

impl TraceLogger {
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled.is_some()
    }

    #[must_use]
    pub fn bounded(max_pending_logs: usize) -> Self {
        Self {
            enabled: Some(Arc::new(TraceLoggerEnabled {
                max_pending: max_pending_logs,
                occupied: AtomicUsize::new(0),
                pending: Mutex::new(VecDeque::new()),
                published: AtomicU64::new(0),
                skipped: AtomicU64::new(0),
                abandoned: AtomicU64::new(0),
            })),
        }
    }

    #[must_use]
    pub const fn disabled() -> Self {
        Self { enabled: None }
    }

    #[must_use]
    pub fn drain_encoded_logs(&self) -> EncodedTraceLogDrainReport {
        if self.is_enabled() {
            let _ = bex_events::prof::drain_logs(std::time::Duration::from_secs(5));
        }
        let mut report = EncodedTraceLogDrainReport::default();
        while let Some(log) = self.pop_pending() {
            match log {
                Ok(log) => report.logs.push(log),
                Err(failure) => report.failures.push(failure),
            }
        }
        report
    }

    #[must_use]
    pub fn drain_rendered_logs(&self) -> TraceLogDrainReport {
        let encoded = self.drain_encoded_logs();
        let mut report = TraceLogDrainReport {
            logs: Vec::with_capacity(encoded.logs.len()),
            failures: encoded.failures,
        };
        for log in encoded.logs {
            match render_encoded_trace_value(&log.body) {
                Ok(body) => report.logs.push(RenderedTraceLog {
                    metadata: log.metadata,
                    body,
                }),
                Err(diagnostic) => report.failures.push(TraceLogFailure {
                    boundary_id: log.boundary_id,
                    call: log.call,
                    metadata: log.metadata,
                    reason: TraceLogFailureReason::EncodeFailed,
                    diagnostic,
                    event: Some(log.event),
                    context: log.context,
                }),
            }
        }
        report
    }

    #[must_use]
    pub fn stats(&self) -> TraceLoggerStats {
        self.enabled
            .as_ref()
            .map_or_else(TraceLoggerStats::default, |enabled| TraceLoggerStats {
                published: enabled.published.load(Ordering::Relaxed),
                skipped_log_queue_full: enabled.skipped.load(Ordering::Relaxed),
                abandoned_reservations: enabled.abandoned.load(Ordering::Relaxed),
            })
    }

    pub(crate) fn try_reserve(
        &self,
        boundary_id: BoundaryId,
        call: TraceCallKey,
    ) -> Option<TraceLogReservation> {
        let enabled = self.enabled.as_ref()?;
        if enabled
            .occupied
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |occupied| {
                (occupied < enabled.max_pending).then(|| occupied + 1)
            })
            .is_err()
        {
            enabled.skipped.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        Some(TraceLogReservation {
            enabled: Arc::clone(enabled),
            boundary_id,
            call,
            committed: false,
        })
    }

    fn pop_pending(&self) -> Option<Result<EncodedTraceLog, TraceLogFailure>> {
        let enabled = self.enabled.as_ref()?;
        let entry = enabled
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop_front()?;
        enabled.occupied.fetch_sub(1, Ordering::Relaxed);
        Some(entry.log)
    }
}

#[must_use = "a log reservation must be committed or dropped"]
#[derive(Debug)]
pub(crate) struct TraceLogReservation {
    enabled: Arc<TraceLoggerEnabled>,
    boundary_id: BoundaryId,
    call: TraceCallKey,
    committed: bool,
}

impl LogDelivery for TraceLogReservation {
    fn deliver(mut self: Box<Self>, log: EncodedLog, mut reservation: Reservation) {
        let metadata_bytes = [&log.event.level, &log.event.message_preview]
            .into_iter()
            .flatten()
            .fold(std::mem::size_of::<InboxEntry>() as u64, |bytes, text| {
                bytes.saturating_add(text.len() as u64)
            });
        if reservation.try_grow(metadata_bytes).is_err() {
            return;
        }
        let metadata = TraceLogMetadata {
            level: log.event.level.clone(),
            source: log.event.source.map(|source| SourceLocation {
                file_path: None,
                file_id: Some(u64::from(source.file_id)),
                line: source.line,
                column: log.event.source_column.unwrap_or(0),
                end_line: None,
                end_column: None,
                start_offset: Some(source.start_offset),
                end_offset: Some(source.end_offset),
            }),
            timestamp_ms: log.event.timestamp_ms,
            message_preview: log.event.message_preview.clone(),
        };
        let result = if let Some(body) = log.data {
            Ok(EncodedTraceLog {
                boundary_id: self.boundary_id,
                call: self.call,
                metadata,
                body,
                event: log.event,
                context: log.context,
            })
        } else {
            Err(TraceLogFailure {
                boundary_id: self.boundary_id,
                call: self.call,
                metadata,
                reason: TraceLogFailureReason::EncodeFailed,
                diagnostic: format!("log data unavailable: {:?}", log.event.data),
                event: Some(log.event),
                context: log.context,
            })
        };
        self.enabled
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_back(InboxEntry {
                log: result,
                _reservation: reservation,
            });
        self.enabled.published.fetch_add(1, Ordering::Relaxed);
        self.committed = true;
    }
}

impl Drop for TraceLogReservation {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        self.enabled.occupied.fetch_sub(1, Ordering::Relaxed);
        self.enabled.abandoned.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use bex_events::{
        ids::{BexCallId, BexThreadId, BoundaryId, EngineId, ProcessEuid},
        prof::backend::{
            EncodedLog, LogDelivery, LogEvent, MeasuredLayouts, Owner, ProfilerMemoryGovernor,
            ProfilerSizingPolicy, ReservationClass, ValueLossReason, ValueState,
        },
        run::TraceCallKey,
    };

    use crate::{
        logger::{TraceLogger, TraceLoggerStats},
        trace_heap::{TraceSnapshot, TraceValue},
        trace_value_encode::encode_trace_snapshot_body,
    };

    fn call() -> TraceCallKey {
        TraceCallKey {
            process_euid: ProcessEuid([1; 16]),
            engine_id: EngineId(2),
            thread_id: BexThreadId(3),
            call_id: BexCallId(4),
        }
    }

    fn encoded(body: &str) -> EncodedLog {
        let body = encode_trace_snapshot_body(&TraceSnapshot::for_test(
            crate::trace_heap::TraceValueRef::for_test(0),
            vec![TraceValue::String(body.to_string())],
        ))
        .unwrap();
        EncodedLog {
            event: LogEvent {
                call_ref: None,
                timestamp_ms: 1,
                level: Some("info".to_string()),
                message_preview: None,
                source_column: None,
                source: None,
                event_name: None,
                distinct_id: None,
                context: ValueState::Lost(ValueLossReason::StoreUnavailable),
                data: ValueState::Lost(ValueLossReason::StoreUnavailable),
            },
            data: Some(body),
            context: None,
        }
    }

    fn memory() -> ProfilerMemoryGovernor {
        let layouts = MeasuredLayouts::V1;
        ProfilerMemoryGovernor::new(
            ProfilerSizingPolicy::derive(32 * 1024 * 1024, layouts).unwrap(),
            layouts,
        )
    }

    #[test]
    fn bounded_inbox_retains_budget_until_drain() {
        let logger = TraceLogger::bounded(1);
        let memory = memory();
        let delivery = logger
            .try_reserve(BoundaryId::from_bytes([7; 16]), call())
            .unwrap();
        let reservation = memory
            .try_reserve(ReservationClass::General, Owner::Transport, 128)
            .unwrap();
        Box::new(delivery).deliver(encoded("first"), reservation);
        assert!(memory.used_bytes(ReservationClass::General) >= 128);
        assert!(
            logger
                .try_reserve(BoundaryId::from_bytes([7; 16]), call())
                .is_none()
        );

        let report = logger.drain_rendered_logs();
        assert_eq!(report.logs.len(), 1);
        assert!(report.logs[0].body.contains("first"));
        assert!(report.failures.is_empty());
        assert_eq!(logger.stats().skipped_log_queue_full, 1);
        assert_eq!(memory.used_bytes(ReservationClass::General), 0);
        assert!(logger.drain_encoded_logs().logs.is_empty());
    }

    #[test]
    fn abandoned_reservation_releases_capacity_without_locking_inbox() {
        let logger = TraceLogger::bounded(1);
        let delivery = logger
            .try_reserve(BoundaryId::from_bytes([7; 16]), call())
            .unwrap();
        let _guard = logger.enabled.as_ref().unwrap().pending.lock().unwrap();
        drop(delivery);
        assert_eq!(logger.stats().abandoned_reservations, 1);
        assert!(
            logger
                .try_reserve(BoundaryId::from_bytes([7; 16]), call())
                .is_some()
        );
    }

    #[test]
    fn missing_data_and_render_failures_retain_loss_evidence() {
        for missing_data in [true, false] {
            let logger = TraceLogger::bounded(1);
            let memory = memory();
            let mut log = encoded("body");
            log.context = Some(vec![9, 8, 7]);
            log.data = if missing_data { None } else { Some(vec![0xff]) };
            let expected_event = log.event.clone();
            let expected_context = log.context.clone();
            let delivery = logger
                .try_reserve(BoundaryId::from_bytes([7; 16]), call())
                .unwrap();
            Box::new(delivery).deliver(
                log,
                memory
                    .try_reserve(ReservationClass::General, Owner::Transport, 128)
                    .unwrap(),
            );
            let report = logger.drain_rendered_logs();
            assert!(report.logs.is_empty());
            assert_eq!(report.failures.len(), 1);
            assert_eq!(report.failures[0].event.as_ref(), Some(&expected_event));
            assert_eq!(report.failures[0].context, expected_context);
            assert_eq!(report.failures[0].metadata.level.as_deref(), Some("info"));
            assert_eq!(logger.stats().published, 1);
            assert_eq!(logger.stats().abandoned_reservations, 0);
            assert_eq!(memory.used_bytes(ReservationClass::General), 0);
        }
    }

    #[test]
    fn rejected_transport_abandons_subscription_once() {
        let (session, _) = bex_events::prof::backend::ProfilerSession::from_config(
            bex_events::prof::backend::ProfilerConfig {
                enabled: false,
                ..Default::default()
            },
        );
        assert!(session.enable_log_collection());
        let logger = TraceLogger::bounded(1);
        let delivery = logger
            .try_reserve(BoundaryId::from_bytes([7; 16]), call())
            .unwrap();
        assert!(!session.publish_log_with_delivery(
            None,
            encoded("rejected"),
            session.reserve_log_work().unwrap(),
            Some(Box::new(delivery)),
            |_| false,
        ));
        assert_eq!(logger.stats().abandoned_reservations, 1);
        assert_eq!(logger.stats().published, 0);
        assert!(logger.drain_encoded_logs().logs.is_empty());
        assert!(
            logger
                .try_reserve(BoundaryId::from_bytes([7; 16]), call())
                .is_some()
        );
    }

    #[test]
    fn denied_delivery_budget_abandons_subscription_once() {
        let logger = TraceLogger::bounded(1);
        let memory = memory();
        let mut reservation = memory
            .try_reserve(ReservationClass::General, Owner::Transport, 128)
            .unwrap();
        let mut remaining = Vec::new();
        for bytes in [1024 * 1024, 1024, 1] {
            while let Ok(held) =
                memory.try_reserve(ReservationClass::General, Owner::Transport, bytes)
            {
                remaining.push(held);
            }
        }
        assert!(reservation.try_grow(1).is_err());
        let delivery = logger
            .try_reserve(BoundaryId::from_bytes([7; 16]), call())
            .unwrap();
        Box::new(delivery).deliver(encoded("denied"), reservation);
        assert_eq!(logger.stats().abandoned_reservations, 1);
        assert_eq!(logger.stats().published, 0);
        assert!(logger.drain_encoded_logs().logs.is_empty());
        drop(remaining);
        assert_eq!(memory.used_bytes(ReservationClass::General), 0);
    }

    #[test]
    fn teardown_releases_retained_budget() {
        let logger = TraceLogger::bounded(1);
        let memory = memory();
        let delivery = logger
            .try_reserve(BoundaryId::from_bytes([7; 16]), call())
            .unwrap();
        Box::new(delivery).deliver(
            encoded("retained"),
            memory
                .try_reserve(ReservationClass::General, Owner::Transport, 128)
                .unwrap(),
        );
        drop(logger);
        assert_eq!(memory.used_bytes(ReservationClass::General), 0);
    }

    #[test]
    fn disabled_logger_never_reserves() {
        let logger = TraceLogger::disabled();
        assert!(
            logger
                .try_reserve(BoundaryId::from_bytes([7; 16]), call())
                .is_none()
        );
        assert_eq!(logger.stats(), TraceLoggerStats::default());
    }
}
