//! Producer-side trace value capture queue.

use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Mutex},
};

use bex_events::{
    ids::BoundaryId,
    run::{SourceLocation, TraceCallKey},
    value::{
        LogEventRecord, ValueArtifactSink, ValueCapture, ValueCaptureKind, ValueCodec, ValueRef,
        ValueWriteOutcome, ValueWriter,
    },
};

use crate::{
    trace_heap::{TraceHeap, TraceSnapshotHandle},
    trace_value_encode::{encode_trace_snapshot_body, render_encoded_trace_value},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureKind {
    RootInput,
    RootOutput,
    RootError,
    LogBody,
    CallOutput,
    CallError,
    CallInput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceLogMetadata {
    pub level: Option<String>,
    pub source: Option<SourceLocation>,
    pub timestamp_ms: u64,
    pub message_preview: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TraceValueDraft {
    pub boundary_id: BoundaryId,
    pub call: TraceCallKey,
    pub kind: CaptureKind,
    pub log: Option<TraceLogMetadata>,
    pub snapshot: TraceSnapshotHandle,
}

#[derive(Debug, PartialEq, Eq)]
pub struct EncodedTraceValue {
    pub boundary_id: BoundaryId,
    pub call: TraceCallKey,
    pub kind: CaptureKind,
    pub log: Option<TraceLogMetadata>,
    pub value_ref: ValueRef,
    pub body: Vec<u8>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct TraceDrainReport {
    pub encoded: Vec<EncodedTraceValue>,
    pub failures: Vec<TraceDrainFailure>,
}

/// A captured BAML log whose value body has already been rendered by the
/// engine. Consumers do not need to know how trace snapshots are encoded or
/// persisted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedTraceLog {
    pub metadata: TraceLogMetadata,
    pub body: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct TraceLogDrainReport {
    pub logs: Vec<RenderedTraceLog>,
    pub failures: Vec<TraceDrainFailure>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceDrainFailure {
    pub boundary_id: BoundaryId,
    pub call: TraceCallKey,
    pub kind: CaptureKind,
    pub log: Option<TraceLogMetadata>,
    pub snapshot: TraceSnapshotHandle,
    pub reason: TraceDrainFailureReason,
    pub diagnostic: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceDrainFailureReason {
    SnapshotMissing,
    EncodeFailed,
    RecordFailed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceCaptureConfig {
    pub enabled: bool,
    pub max_pending_value_drafts: usize,
    pub max_pending_log_drafts: usize,
    pub max_pending_root_result_drafts: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceLogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl TraceLogLevel {
    /// Parse the process-wide `BAML_LOG` setting used by native SDK bridges.
    /// An unset or empty setting preserves the capture-disabled default.
    #[must_use]
    pub fn from_baml_log(raw: Option<&str>) -> Self {
        match raw.unwrap_or_default().trim().to_ascii_lowercase().as_str() {
            "off" => Self::Off,
            "error" => Self::Error,
            "warn" | "warning" => Self::Warn,
            "debug" => Self::Debug,
            "trace" => Self::Trace,
            "" => Self::Off,
            "info" => Self::Info,
            _ => Self::Info,
        }
    }

    #[must_use]
    pub fn allows(self, raw_event_level: Option<&str>) -> bool {
        if self == Self::Off {
            return false;
        }
        Self::parse_event(raw_event_level).severity() >= self.severity()
    }

    fn parse_event(raw: Option<&str>) -> Self {
        match raw.unwrap_or("info").to_ascii_lowercase().as_str() {
            "error" => Self::Error,
            "warn" | "warning" => Self::Warn,
            "debug" => Self::Debug,
            "trace" => Self::Trace,
            _ => Self::Info,
        }
    }

    const fn severity(self) -> u8 {
        match self {
            Self::Off => u8::MAX,
            Self::Error => 4,
            Self::Warn => 3,
            Self::Info => 2,
            Self::Debug => 1,
            Self::Trace => 0,
        }
    }
}

impl TraceCaptureConfig {
    const ROOT_RESULT_RESERVED_DRAFTS: usize = 2;

    #[must_use]
    pub fn enabled(max_pending_drafts_per_kind: usize) -> Self {
        Self {
            enabled: true,
            max_pending_value_drafts: max_pending_drafts_per_kind,
            max_pending_log_drafts: max_pending_drafts_per_kind,
            max_pending_root_result_drafts: Self::ROOT_RESULT_RESERVED_DRAFTS,
        }
    }

    #[must_use]
    pub fn enabled_with_budgets(
        max_pending_value_drafts: usize,
        max_pending_log_drafts: usize,
    ) -> Self {
        Self {
            enabled: true,
            max_pending_value_drafts,
            max_pending_log_drafts,
            max_pending_root_result_drafts: Self::ROOT_RESULT_RESERVED_DRAFTS,
        }
    }

    /// Capture only structured BAML log bodies, up to a bounded number of
    /// pending events.
    #[must_use]
    pub fn logs_only(max_pending_log_drafts: usize) -> Self {
        Self::enabled_with_budgets(0, max_pending_log_drafts)
    }

    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            max_pending_value_drafts: 0,
            max_pending_log_drafts: 0,
            max_pending_root_result_drafts: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TraceCaptureProducer {
    trace_heap: TraceHeap,
    inner: Arc<Mutex<TraceCaptureInner>>,
}

#[derive(Debug)]
struct TraceCaptureInner {
    config: TraceCaptureConfig,
    log_level: Option<TraceLogLevel>,
    reserved_value_slots: usize,
    reserved_log_slots: usize,
    reserved_root_result_slots: usize,
    pending: VecDeque<TraceValueDraft>,
    stats: TraceCaptureStats,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TraceCaptureStats {
    pub published: u64,
    pub skipped_disabled: u64,
    pub skipped_queue_full: u64,
    pub skipped_value_queue_full: u64,
    pub skipped_log_queue_full: u64,
    pub skipped_root_result_queue_full: u64,
    pub abandoned_reservations: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureSkipReason {
    Disabled,
    QueueFull,
}

impl TraceCaptureProducer {
    #[must_use]
    pub fn new(config: TraceCaptureConfig) -> Self {
        Self {
            trace_heap: TraceHeap::new(),
            inner: Arc::new(Mutex::new(TraceCaptureInner {
                config,
                log_level: None,
                reserved_value_slots: 0,
                reserved_log_slots: 0,
                reserved_root_result_slots: 0,
                pending: VecDeque::new(),
                stats: TraceCaptureStats::default(),
            })),
        }
    }

    #[must_use]
    pub fn disabled() -> Self {
        Self::new(TraceCaptureConfig::disabled())
    }

    /// Create a producer that rejects suppressed log levels before snapshot
    /// copying or bounded-queue reservation.
    #[must_use]
    pub fn new_with_log_level(config: TraceCaptureConfig, log_level: TraceLogLevel) -> Self {
        let producer = Self::new(config);
        producer
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .log_level = Some(log_level);
        producer
    }

    /// Returns whether another producer handle can still publish captures.
    ///
    /// Once this returns `false`, no new producer can be cloned from another
    /// handle, so a consumer can perform one final drain and stop polling.
    #[must_use]
    pub fn has_other_handles(&self) -> bool {
        Arc::strong_count(&self.inner) > 1
    }

    /// Returns whether a log at `level` should be copied into this producer.
    #[must_use]
    pub fn captures_log_level(&self, level: Option<&str>) -> bool {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.config.enabled
            && inner
                .log_level
                .is_none_or(|configured| configured.allows(level))
    }

    #[must_use]
    pub fn trace_heap(&self) -> &TraceHeap {
        &self.trace_heap
    }

    fn try_reserve(
        &self,
        boundary_id: BoundaryId,
        call: TraceCallKey,
        kind: CaptureKind,
    ) -> Result<CaptureReservation, CaptureSkipReason> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !inner.config.enabled {
            inner.stats.skipped_disabled = inner.stats.skipped_disabled.saturating_add(1);
            return Err(CaptureSkipReason::Disabled);
        }
        let occupied = inner
            .pending
            .iter()
            .filter(|draft| draft.kind.queue_class() == kind.queue_class())
            .count()
            .saturating_add(match kind.queue_class() {
                CaptureQueueClass::Value => inner.reserved_value_slots,
                CaptureQueueClass::Log => inner.reserved_log_slots,
                CaptureQueueClass::RootResult => inner.reserved_root_result_slots,
            });
        let max_pending = match kind.queue_class() {
            CaptureQueueClass::Value => inner.config.max_pending_value_drafts,
            CaptureQueueClass::Log => inner.config.max_pending_log_drafts,
            CaptureQueueClass::RootResult => inner.config.max_pending_root_result_drafts,
        };
        if occupied >= max_pending {
            inner.stats.skipped_queue_full = inner.stats.skipped_queue_full.saturating_add(1);
            match kind.queue_class() {
                CaptureQueueClass::Value => {
                    inner.stats.skipped_value_queue_full =
                        inner.stats.skipped_value_queue_full.saturating_add(1);
                }
                CaptureQueueClass::Log => {
                    inner.stats.skipped_log_queue_full =
                        inner.stats.skipped_log_queue_full.saturating_add(1);
                }
                CaptureQueueClass::RootResult => {
                    inner.stats.skipped_root_result_queue_full =
                        inner.stats.skipped_root_result_queue_full.saturating_add(1);
                }
            }
            return Err(CaptureSkipReason::QueueFull);
        }
        match kind.queue_class() {
            CaptureQueueClass::Value => {
                inner.reserved_value_slots = inner.reserved_value_slots.saturating_add(1);
            }
            CaptureQueueClass::Log => {
                inner.reserved_log_slots = inner.reserved_log_slots.saturating_add(1);
            }
            CaptureQueueClass::RootResult => {
                inner.reserved_root_result_slots =
                    inner.reserved_root_result_slots.saturating_add(1);
            }
        }
        Ok(CaptureReservation {
            producer: self.clone(),
            boundary_id,
            call,
            kind,
            committed: false,
        })
    }

    pub fn capture_with(
        &self,
        boundary_id: BoundaryId,
        call: TraceCallKey,
        kind: CaptureKind,
        copy_snapshot: impl FnOnce(&TraceHeap) -> TraceSnapshotHandle,
    ) -> Result<(), CaptureSkipReason> {
        assert_ne!(
            kind,
            CaptureKind::LogBody,
            "log captures must use capture_log_with"
        );
        let reservation = self.try_reserve(boundary_id, call, kind)?;
        let snapshot = copy_snapshot(self.trace_heap());
        reservation.commit(snapshot);
        Ok(())
    }

    pub fn capture_log_with(
        &self,
        boundary_id: BoundaryId,
        call: TraceCallKey,
        copy_snapshot: impl FnOnce(&TraceHeap) -> (TraceLogMetadata, TraceSnapshotHandle),
    ) -> Result<(), CaptureSkipReason> {
        let reservation = self.try_reserve(boundary_id, call, CaptureKind::LogBody)?;
        let (log, snapshot) = copy_snapshot(self.trace_heap());
        reservation.commit_log(snapshot, log);
        Ok(())
    }

    #[must_use]
    pub fn drain(&self) -> Vec<TraceValueDraft> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.pending.drain(..).collect()
    }

    fn pop_pending(&self) -> Option<TraceValueDraft> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pending
            .pop_front()
    }

    #[must_use]
    pub fn stats(&self) -> TraceCaptureStats {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stats
    }

    pub fn drain_to_value_writer<S: ValueArtifactSink>(
        &self,
        writer: &mut ValueWriter<S>,
    ) -> io::Result<Vec<EncodedTraceValue>> {
        self.drain_to_value_recorder(|draft, body| {
            if let Some(log) = &draft.log {
                writer.append_log_body(
                    ValueCodec::BamlOutboundValue,
                    body,
                    LogEventRecord {
                        call: draft.call,
                        level: log.level.clone(),
                        source: log.source.clone(),
                        timestamp_ms: log.timestamp_ms,
                        message_preview: log.message_preview.clone(),
                    },
                )
            } else {
                writer.append_body_with_capture(
                    ValueCodec::BamlOutboundValue,
                    body,
                    Some(ValueCapture {
                        kind: value_capture_kind(draft.kind),
                        call: draft.call,
                    }),
                )
            }
        })
    }

    pub fn drain_to_value_recorder(
        &self,
        mut record_value: impl FnMut(&TraceValueDraft, Vec<u8>) -> io::Result<ValueWriteOutcome>,
    ) -> io::Result<Vec<EncodedTraceValue>> {
        let report = self.drain_to_value_recorder_report(|draft, body| record_value(draft, body));
        if let Some(failure) = report.failures.first() {
            return Err(io::Error::new(
                failure.reason.io_error_kind(),
                failure.diagnostic.clone(),
            ));
        }
        Ok(report.encoded)
    }

    pub fn drain_to_value_recorder_report(
        &self,
        mut record_value: impl FnMut(&TraceValueDraft, Vec<u8>) -> io::Result<ValueWriteOutcome>,
    ) -> TraceDrainReport {
        let mut report = TraceDrainReport::default();
        while let Some(draft) = self.pop_pending() {
            let body = match self.encode_snapshot(&draft) {
                Ok(body) => body,
                Err((reason, diagnostic)) => {
                    let _ = self.trace_heap.release(draft.snapshot);
                    report
                        .failures
                        .push(Self::drain_failure(&draft, reason, diagnostic));
                    continue;
                }
            };
            let outcome = record_value(&draft, body.clone());
            let _ = self.trace_heap.release(draft.snapshot);
            match outcome {
                Ok(outcome) => report.encoded.push(EncodedTraceValue {
                    boundary_id: draft.boundary_id,
                    call: draft.call,
                    kind: draft.kind,
                    log: draft.log,
                    value_ref: outcome.value_ref,
                    body,
                }),
                Err(err) => report.failures.push(TraceDrainFailure {
                    boundary_id: draft.boundary_id,
                    call: draft.call,
                    kind: draft.kind,
                    log: draft.log,
                    snapshot: draft.snapshot,
                    reason: TraceDrainFailureReason::RecordFailed,
                    diagnostic: err.to_string(),
                }),
            }
        }
        report
    }

    /// Drain captured logs into display-ready text while retaining their
    /// structured metadata. Snapshot lookup, wire encoding, decoding, and
    /// release all remain owned by the engine.
    #[must_use]
    pub fn drain_rendered_logs(&self) -> TraceLogDrainReport {
        let mut report = TraceLogDrainReport::default();
        while let Some(draft) = self.pop_pending() {
            let Some(metadata) = draft.log.clone() else {
                let _ = self.trace_heap.release(draft.snapshot);
                report.failures.push(Self::drain_failure(
                    &draft,
                    TraceDrainFailureReason::RecordFailed,
                    "rendered log drain encountered a non-log capture".to_string(),
                ));
                continue;
            };
            let body = match self.encode_snapshot(&draft) {
                Ok(body) => body,
                Err((reason, diagnostic)) => {
                    let _ = self.trace_heap.release(draft.snapshot);
                    report
                        .failures
                        .push(Self::drain_failure(&draft, reason, diagnostic));
                    continue;
                }
            };
            let _ = self.trace_heap.release(draft.snapshot);
            match render_encoded_trace_value(&body) {
                Ok(body) => report.logs.push(RenderedTraceLog { metadata, body }),
                Err(diagnostic) => report.failures.push(Self::drain_failure(
                    &draft,
                    TraceDrainFailureReason::EncodeFailed,
                    diagnostic,
                )),
            }
        }
        report
    }

    fn encode_snapshot(
        &self,
        draft: &TraceValueDraft,
    ) -> Result<Vec<u8>, (TraceDrainFailureReason, String)> {
        let Some(snapshot) = self.trace_heap.get(draft.snapshot) else {
            return Err((
                TraceDrainFailureReason::SnapshotMissing,
                format!(
                    "trace snapshot {} was already released",
                    draft.snapshot.raw()
                ),
            ));
        };
        encode_trace_snapshot_body(&snapshot)
            .map_err(|diagnostic| (TraceDrainFailureReason::EncodeFailed, diagnostic))
    }

    fn drain_failure(
        draft: &TraceValueDraft,
        reason: TraceDrainFailureReason,
        diagnostic: String,
    ) -> TraceDrainFailure {
        TraceDrainFailure {
            boundary_id: draft.boundary_id,
            call: draft.call,
            kind: draft.kind,
            log: draft.log.clone(),
            snapshot: draft.snapshot,
            reason,
            diagnostic,
        }
    }
}

impl TraceDrainFailureReason {
    fn io_error_kind(self) -> io::ErrorKind {
        match self {
            Self::SnapshotMissing => io::ErrorKind::NotFound,
            Self::EncodeFailed => io::ErrorKind::InvalidData,
            Self::RecordFailed => io::ErrorKind::Other,
        }
    }
}

fn value_capture_kind(kind: CaptureKind) -> ValueCaptureKind {
    match kind {
        CaptureKind::RootInput => ValueCaptureKind::RootInput,
        CaptureKind::RootOutput => ValueCaptureKind::RootOutput,
        CaptureKind::RootError => ValueCaptureKind::RootError,
        CaptureKind::LogBody => ValueCaptureKind::LogBody,
        CaptureKind::CallOutput => ValueCaptureKind::CallOutput,
        CaptureKind::CallError => ValueCaptureKind::CallError,
        CaptureKind::CallInput => ValueCaptureKind::CallInput,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CaptureQueueClass {
    Value,
    Log,
    RootResult,
}

impl CaptureKind {
    fn queue_class(self) -> CaptureQueueClass {
        match self {
            CaptureKind::RootInput
            | CaptureKind::CallOutput
            | CaptureKind::CallError
            | CaptureKind::CallInput => CaptureQueueClass::Value,
            CaptureKind::LogBody => CaptureQueueClass::Log,
            CaptureKind::RootOutput | CaptureKind::RootError => CaptureQueueClass::RootResult,
        }
    }
}

#[must_use = "a capture reservation must be committed or explicitly abandoned"]
struct CaptureReservation {
    producer: TraceCaptureProducer,
    boundary_id: BoundaryId,
    call: TraceCallKey,
    kind: CaptureKind,
    committed: bool,
}

impl CaptureReservation {
    fn commit(mut self, snapshot: TraceSnapshotHandle) {
        self.commit_with_log(snapshot, None);
    }

    fn commit_log(mut self, snapshot: TraceSnapshotHandle, log: TraceLogMetadata) {
        debug_assert_eq!(
            self.kind,
            CaptureKind::LogBody,
            "log metadata can only be committed for LogBody reservations"
        );
        self.commit_with_log(snapshot, Some(log));
    }

    fn commit_with_log(&mut self, snapshot: TraceSnapshotHandle, log: Option<TraceLogMetadata>) {
        let mut inner = self
            .producer
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match self.kind.queue_class() {
            CaptureQueueClass::Value => {
                inner.reserved_value_slots = inner.reserved_value_slots.saturating_sub(1);
            }
            CaptureQueueClass::Log => {
                inner.reserved_log_slots = inner.reserved_log_slots.saturating_sub(1);
            }
            CaptureQueueClass::RootResult => {
                inner.reserved_root_result_slots =
                    inner.reserved_root_result_slots.saturating_sub(1);
            }
        }
        inner.pending.push_back(TraceValueDraft {
            boundary_id: self.boundary_id,
            call: self.call,
            kind: self.kind,
            log,
            snapshot,
        });
        inner.stats.published = inner.stats.published.saturating_add(1);
        self.committed = true;
    }
}

impl Drop for CaptureReservation {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let mut inner = self
            .producer
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match self.kind.queue_class() {
            CaptureQueueClass::Value => {
                inner.reserved_value_slots = inner.reserved_value_slots.saturating_sub(1);
            }
            CaptureQueueClass::Log => {
                inner.reserved_log_slots = inner.reserved_log_slots.saturating_sub(1);
            }
            CaptureQueueClass::RootResult => {
                inner.reserved_root_result_slots =
                    inner.reserved_root_result_slots.saturating_sub(1);
            }
        }
        inner.stats.abandoned_reservations = inner.stats.abandoned_reservations.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use bex_events::{
        ids::{BexCallId, BexThreadId, BoundaryId, EngineId, ProcessEuid},
        run::TraceCallKey,
        value::{ValueCodec, ValueRef, ValueWriteOutcome},
    };

    use crate::{
        trace_heap::{TraceHeap, TraceSnapshot, TraceSnapshotHandle, TraceValue, TraceValueRef},
        value_capture::{
            CaptureKind, CaptureSkipReason, TraceCaptureConfig, TraceCaptureProducer,
            TraceDrainFailureReason, TraceLogLevel, TraceLogMetadata,
        },
    };

    fn boundary_id() -> BoundaryId {
        BoundaryId::from_bytes([7; 16])
    }

    fn trace_key() -> TraceCallKey {
        TraceCallKey {
            process_euid: ProcessEuid([1; 16]),
            engine_id: EngineId(2),
            thread_id: BexThreadId(3),
            call_id: BexCallId(4),
        }
    }

    fn fake_snapshot(_: &TraceHeap) -> TraceSnapshotHandle {
        TraceSnapshotHandle::for_test(99)
    }

    fn fake_log_metadata() -> TraceLogMetadata {
        TraceLogMetadata {
            level: Some("info".to_string()),
            source: None,
            timestamp_ms: 123,
            message_preview: Some("hello".to_string()),
        }
    }

    fn test_snapshot(trace_heap: &TraceHeap, value: i64) -> TraceSnapshotHandle {
        trace_heap.insert_for_test(TraceSnapshot::for_test(
            TraceValueRef::for_test(0),
            vec![TraceValue::Int(value)],
        ))
    }

    fn structured_test_snapshot(trace_heap: &TraceHeap) -> TraceSnapshotHandle {
        trace_heap.insert_for_test(TraceSnapshot::for_test(
            TraceValueRef::for_test(2),
            vec![
                TraceValue::String("ada".to_string()),
                TraceValue::Int(42),
                TraceValue::Map(vec![
                    ("user".to_string(), TraceValueRef::for_test(0)),
                    ("count".to_string(), TraceValueRef::for_test(1)),
                ]),
            ],
        ))
    }

    #[test]
    fn zero_capacity_capture_does_not_run_copy_closure() {
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_copy = Arc::clone(&calls);

        let result = producer.capture_with(
            boundary_id(),
            trace_key(),
            CaptureKind::RootInput,
            move |_| {
                calls_for_copy.fetch_add(1, Ordering::Relaxed);
                TraceSnapshotHandle::for_test(1)
            },
        );

        assert_eq!(result, Err(CaptureSkipReason::QueueFull));
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        assert_eq!(producer.stats().skipped_queue_full, 1);
        assert!(producer.drain().is_empty());
    }

    #[test]
    fn root_result_capture_uses_reserved_capacity() {
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled(0));
        producer
            .capture_with(
                boundary_id(),
                trace_key(),
                CaptureKind::RootOutput,
                fake_snapshot,
            )
            .unwrap();
        producer
            .capture_with(boundary_id(), trace_key(), CaptureKind::RootError, |_| {
                TraceSnapshotHandle::for_test(100)
            })
            .unwrap();
        assert_eq!(
            producer.capture_with(
                boundary_id(),
                trace_key(),
                CaptureKind::RootInput,
                fake_snapshot,
            ),
            Err(CaptureSkipReason::QueueFull),
        );

        let drafts = producer.drain();
        assert_eq!(drafts.len(), 2);
        assert_eq!(drafts[0].kind, CaptureKind::RootOutput);
        assert_eq!(drafts[1].kind, CaptureKind::RootError);
        assert_eq!(producer.stats().skipped_value_queue_full, 1);
        assert_eq!(producer.stats().skipped_root_result_queue_full, 0);
    }

    #[test]
    fn zero_capacity_log_capture_does_not_run_copy_closure() {
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled_with_budgets(1, 0));
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_copy = Arc::clone(&calls);

        let result = producer.capture_log_with(boundary_id(), trace_key(), move |_| {
            calls_for_copy.fetch_add(1, Ordering::Relaxed);
            (fake_log_metadata(), TraceSnapshotHandle::for_test(1))
        });

        assert_eq!(result, Err(CaptureSkipReason::QueueFull));
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        assert_eq!(producer.stats().skipped_queue_full, 1);
        assert_eq!(producer.stats().skipped_log_queue_full, 1);
        assert!(producer.drain().is_empty());
    }

    #[test]
    fn disabled_capture_does_not_run_copy_closure() {
        let producer = TraceCaptureProducer::disabled();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_copy = Arc::clone(&calls);

        let result = producer.capture_with(
            boundary_id(),
            trace_key(),
            CaptureKind::RootInput,
            move |_| {
                calls_for_copy.fetch_add(1, Ordering::Relaxed);
                TraceSnapshotHandle::for_test(1)
            },
        );

        assert_eq!(result, Err(CaptureSkipReason::Disabled));
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        assert_eq!(producer.stats().skipped_disabled, 1);
        assert!(producer.drain().is_empty());
    }

    #[test]
    fn committed_reservations_drain_in_fifo_order() {
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled(2));
        producer
            .capture_with(
                boundary_id(),
                trace_key(),
                CaptureKind::RootInput,
                fake_snapshot,
            )
            .unwrap();
        producer
            .capture_with(boundary_id(), trace_key(), CaptureKind::RootOutput, |_| {
                TraceSnapshotHandle::for_test(100)
            })
            .unwrap();

        let drafts = producer.drain();
        assert_eq!(drafts.len(), 2);
        assert_eq!(drafts[0].kind, CaptureKind::RootInput);
        assert_eq!(drafts[0].snapshot, TraceSnapshotHandle::for_test(99));
        assert_eq!(drafts[1].kind, CaptureKind::RootOutput);
        assert_eq!(drafts[1].snapshot, TraceSnapshotHandle::for_test(100));
        assert_eq!(producer.stats().published, 2);
    }

    #[test]
    fn log_capture_uses_log_budget_and_keeps_metadata() {
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled_with_budgets(1, 1));
        producer
            .capture_with(
                boundary_id(),
                trace_key(),
                CaptureKind::RootInput,
                fake_snapshot,
            )
            .unwrap();
        producer
            .capture_log_with(boundary_id(), trace_key(), |_| {
                (fake_log_metadata(), TraceSnapshotHandle::for_test(100))
            })
            .unwrap();

        assert_eq!(
            producer.capture_with(
                boundary_id(),
                trace_key(),
                CaptureKind::CallOutput,
                fake_snapshot,
            ),
            Err(CaptureSkipReason::QueueFull),
        );
        assert_eq!(
            producer.capture_log_with(boundary_id(), trace_key(), |_| {
                (fake_log_metadata(), TraceSnapshotHandle::for_test(101))
            }),
            Err(CaptureSkipReason::QueueFull),
        );

        let drafts = producer.drain();
        assert_eq!(drafts.len(), 2);
        assert_eq!(drafts[0].kind, CaptureKind::RootInput);
        assert_eq!(drafts[0].log, None);
        assert_eq!(drafts[1].kind, CaptureKind::LogBody);
        assert_eq!(drafts[1].snapshot, TraceSnapshotHandle::for_test(100));
        assert_eq!(drafts[1].log, Some(fake_log_metadata()));
        assert_eq!(producer.stats().published, 2);
        assert_eq!(producer.stats().skipped_value_queue_full, 1);
        assert_eq!(producer.stats().skipped_log_queue_full, 1);
    }

    #[test]
    fn rendered_log_drain_preserves_metadata_and_releases_structured_snapshots() {
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::logs_only(2));
        producer
            .capture_log_with(boundary_id(), trace_key(), |trace_heap| {
                (fake_log_metadata(), test_snapshot(trace_heap, 42))
            })
            .unwrap();
        producer
            .capture_log_with(boundary_id(), trace_key(), |trace_heap| {
                (fake_log_metadata(), structured_test_snapshot(trace_heap))
            })
            .unwrap();

        let report = producer.drain_rendered_logs();

        assert!(report.failures.is_empty());
        assert_eq!(report.logs.len(), 2);
        assert_eq!(report.logs[0].metadata, fake_log_metadata());
        assert_eq!(report.logs[0].body, "42");
        assert_eq!(report.logs[1].body, r#"{"user": "ada", "count": 42}"#);
        assert_eq!(producer.trace_heap().retained_snapshot_count(), 0);
        assert!(producer.drain().is_empty());
    }

    #[test]
    fn log_level_filter_rejects_suppressed_events_before_queueing() {
        let producer = TraceCaptureProducer::new_with_log_level(
            TraceCaptureConfig::logs_only(1),
            TraceLogLevel::Error,
        );

        for _ in 0..10 {
            assert!(!producer.captures_log_level(Some("debug")));
        }
        assert!(producer.captures_log_level(Some("error")));
        producer
            .capture_log_with(boundary_id(), trace_key(), |trace_heap| {
                (fake_log_metadata(), test_snapshot(trace_heap, 42))
            })
            .unwrap();

        assert_eq!(producer.drain_rendered_logs().logs.len(), 1);
        assert_eq!(producer.stats().skipped_log_queue_full, 0);
    }

    #[test]
    fn abandoned_reservation_frees_capacity() {
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled(1));
        drop(
            producer
                .try_reserve(boundary_id(), trace_key(), CaptureKind::RootError)
                .unwrap(),
        );

        assert_eq!(producer.stats().abandoned_reservations, 1);
        producer
            .capture_with(
                boundary_id(),
                trace_key(),
                CaptureKind::RootError,
                fake_snapshot,
            )
            .unwrap();
        assert_eq!(producer.drain().len(), 1);
    }

    #[test]
    fn drain_report_continues_after_record_failure_and_releases_snapshots() {
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled(4));
        producer
            .capture_with(
                boundary_id(),
                trace_key(),
                CaptureKind::RootInput,
                |trace_heap| test_snapshot(trace_heap, 1),
            )
            .unwrap();
        producer
            .capture_with(
                boundary_id(),
                trace_key(),
                CaptureKind::CallOutput,
                |trace_heap| test_snapshot(trace_heap, 2),
            )
            .unwrap();
        producer
            .capture_with(
                boundary_id(),
                trace_key(),
                CaptureKind::CallError,
                |trace_heap| test_snapshot(trace_heap, 3),
            )
            .unwrap();

        let mut next_id = 1_u64;
        let report = producer.drain_to_value_recorder_report(|draft, body| {
            assert!(!body.is_empty());
            if draft.kind == CaptureKind::CallOutput {
                return Err(io::Error::other("writer unavailable"));
            }
            let id = format!("value_{next_id}");
            next_id = next_id.saturating_add(1);
            Ok(ValueWriteOutcome {
                value_ref: ValueRef::available(
                    id,
                    ValueCodec::BamlOutboundValue,
                    body.len(),
                    body.len(),
                ),
            })
        });

        assert_eq!(report.encoded.len(), 2);
        assert_eq!(report.encoded[0].kind, CaptureKind::RootInput);
        assert_eq!(report.encoded[1].kind, CaptureKind::CallError);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].kind, CaptureKind::CallOutput);
        assert_eq!(
            report.failures[0].reason,
            TraceDrainFailureReason::RecordFailed
        );
        assert_eq!(producer.trace_heap().retained_snapshot_count(), 0);
        assert!(producer.drain().is_empty());
    }

    #[test]
    #[should_panic(expected = "log captures must use capture_log_with")]
    fn capture_with_rejects_log_body_kind() {
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled(1));
        let _ = producer.capture_with(boundary_id(), trace_key(), CaptureKind::LogBody, |_| {
            TraceSnapshotHandle::for_test(1)
        });
    }
}
