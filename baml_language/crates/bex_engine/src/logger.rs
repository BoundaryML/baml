//! Independent, bounded structured-log capture.
//!
//! Logging has its own queue and heap lifetime. Enabling it neither enables
//! profiling nor creates a profiler session, ring, store, or capture policy.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use bex_events::{
    ids::BoundaryId,
    run::{SourceLocation, TraceCallKey},
};

use crate::{
    trace_heap::{TraceHeap, TraceSnapshotHandle},
    trace_value_encode::{encode_trace_snapshot_body, render_encoded_trace_value},
};

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
    heap: TraceHeap,
    inner: Mutex<TraceLoggerInner>,
}

#[derive(Debug)]
struct TraceLoggerInner {
    max_pending: usize,
    reserved: usize,
    pending: VecDeque<TraceLogDraft>,
    stats: TraceLoggerStats,
}

#[derive(Debug)]
struct TraceLogDraft {
    boundary_id: BoundaryId,
    call: TraceCallKey,
    metadata: TraceLogMetadata,
    snapshot: TraceSnapshotHandle,
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
                heap: TraceHeap::new(),
                inner: Mutex::new(TraceLoggerInner {
                    max_pending: max_pending_logs,
                    reserved: 0,
                    pending: VecDeque::new(),
                    stats: TraceLoggerStats::default(),
                }),
            })),
        }
    }

    #[must_use]
    pub const fn disabled() -> Self {
        Self { enabled: None }
    }

    pub fn capture_with(
        &self,
        boundary_id: BoundaryId,
        call: TraceCallKey,
        copy_snapshot: impl FnOnce(&TraceHeap) -> (TraceLogMetadata, TraceSnapshotHandle),
    ) {
        let Some(mut reservation) = self.try_reserve(boundary_id, call) else {
            return;
        };
        let (metadata, snapshot) = copy_snapshot(&reservation.enabled.heap);
        reservation.commit(metadata, snapshot);
    }

    #[must_use]
    pub fn drain_encoded_logs(&self) -> EncodedTraceLogDrainReport {
        let mut report = EncodedTraceLogDrainReport::default();
        while let Some(draft) = self.pop_pending() {
            let Some(enabled) = &self.enabled else {
                unreachable!("an enabled logger owns every queued draft");
            };
            let Some(snapshot) = enabled.heap.get(draft.snapshot) else {
                report.failures.push(Self::failure(
                    &draft,
                    TraceLogFailureReason::SnapshotMissing,
                    format!("log snapshot {} was already released", draft.snapshot.raw()),
                ));
                continue;
            };
            let encoded = encode_trace_snapshot_body(&snapshot);
            let _ = enabled.heap.release(draft.snapshot);
            match encoded {
                Ok(body) => report.logs.push(EncodedTraceLog {
                    boundary_id: draft.boundary_id,
                    call: draft.call,
                    metadata: draft.metadata,
                    body,
                }),
                Err(diagnostic) => report.failures.push(Self::failure(
                    &draft,
                    TraceLogFailureReason::EncodeFailed,
                    diagnostic,
                )),
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
                }),
            }
        }
        report
    }

    #[must_use]
    pub fn stats(&self) -> TraceLoggerStats {
        self.enabled
            .as_ref()
            .map_or_else(TraceLoggerStats::default, |enabled| {
                enabled
                    .inner
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .stats
            })
    }

    fn try_reserve(
        &self,
        boundary_id: BoundaryId,
        call: TraceCallKey,
    ) -> Option<TraceLogReservation> {
        let enabled = self.enabled.as_ref()?;
        let mut inner = enabled
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if inner.pending.len().saturating_add(inner.reserved) >= inner.max_pending {
            inner.stats.skipped_log_queue_full =
                inner.stats.skipped_log_queue_full.saturating_add(1);
            return None;
        }
        inner.reserved = inner.reserved.saturating_add(1);
        drop(inner);
        Some(TraceLogReservation {
            enabled: Arc::clone(enabled),
            boundary_id,
            call,
            committed: false,
        })
    }

    fn pop_pending(&self) -> Option<TraceLogDraft> {
        self.enabled
            .as_ref()?
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pending
            .pop_front()
    }

    fn failure(
        draft: &TraceLogDraft,
        reason: TraceLogFailureReason,
        diagnostic: String,
    ) -> TraceLogFailure {
        TraceLogFailure {
            boundary_id: draft.boundary_id,
            call: draft.call,
            metadata: draft.metadata.clone(),
            reason,
            diagnostic,
        }
    }
}

#[must_use = "a log reservation must be committed or dropped"]
struct TraceLogReservation {
    enabled: Arc<TraceLoggerEnabled>,
    boundary_id: BoundaryId,
    call: TraceCallKey,
    committed: bool,
}

impl TraceLogReservation {
    fn commit(&mut self, metadata: TraceLogMetadata, snapshot: TraceSnapshotHandle) {
        let mut inner = self
            .enabled
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.reserved = inner.reserved.saturating_sub(1);
        inner.pending.push_back(TraceLogDraft {
            boundary_id: self.boundary_id,
            call: self.call,
            metadata,
            snapshot,
        });
        inner.stats.published = inner.stats.published.saturating_add(1);
        self.committed = true;
    }
}

impl Drop for TraceLogReservation {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let mut inner = self
            .enabled
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.reserved = inner.reserved.saturating_sub(1);
        inner.stats.abandoned_reservations = inner.stats.abandoned_reservations.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use bex_events::{
        ids::{BexCallId, BexThreadId, BoundaryId, EngineId, ProcessEuid},
        run::TraceCallKey,
    };

    use crate::{
        logger::{TraceLogMetadata, TraceLogger, TraceLoggerStats},
        trace_heap::{TraceSnapshot, TraceValue},
    };

    fn call() -> TraceCallKey {
        TraceCallKey {
            process_euid: ProcessEuid([1; 16]),
            engine_id: EngineId(2),
            thread_id: BexThreadId(3),
            call_id: BexCallId(4),
        }
    }

    #[test]
    fn bounded_queue_is_logging_only_and_releases_snapshots() {
        let logger = TraceLogger::bounded(1);
        for body in ["first", "dropped"] {
            logger.capture_with(BoundaryId::from_bytes([7; 16]), call(), |heap| {
                let handle = heap.insert_for_test(TraceSnapshot::for_test(
                    crate::trace_heap::TraceValueRef::for_test(0),
                    vec![TraceValue::String(body.to_string())],
                ));
                (
                    TraceLogMetadata {
                        level: Some("info".to_string()),
                        source: None,
                        timestamp_ms: 1,
                        message_preview: Some(body.to_string()),
                    },
                    handle,
                )
            });
        }

        let report = logger.drain_rendered_logs();
        assert_eq!(report.logs.len(), 1);
        assert!(report.logs[0].body.contains("first"));
        assert!(report.failures.is_empty());
        assert_eq!(logger.stats().skipped_log_queue_full, 1);
    }

    #[test]
    fn disabled_logger_never_invokes_capture() {
        let logger = TraceLogger::disabled();
        logger.capture_with(BoundaryId::from_bytes([7; 16]), call(), |_| {
            panic!("disabled logger must not allocate or copy")
        });
        assert_eq!(logger.stats(), TraceLoggerStats::default());
    }
}
