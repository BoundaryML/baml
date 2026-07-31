//! Producer-side trace value capture queue.

#[cfg(not(target_arch = "wasm32"))]
use std::{collections::HashMap, path::Path, sync::OnceLock};
use std::{
    collections::VecDeque,
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
#[cfg(not(target_arch = "wasm32"))]
use std::{
    sync::{Condvar, atomic::AtomicBool},
    time::Duration,
};

#[cfg(not(target_arch = "wasm32"))]
use bex_events::ids::ProcessEuid;
#[cfg(not(target_arch = "wasm32"))]
use bex_events::value::{CaptureLossKind, CaptureLossReason, CaptureLossRecord};
#[cfg(not(target_arch = "wasm32"))]
use bex_events::value_cas::{
    CallPath, CanonicalField, CanonicalValue, DurableValueCapture, FieldPresence, MediaContent,
    MediaValue, OmissionValue, ValueDrainConfig, ValueDrainHandle, ValueDrainService,
    ValueEnqueueOutcome,
};
use bex_events::{
    ids::BoundaryId,
    run::{SourceLocation, TraceCallKey},
    value::{
        LogEventRecord, ValueArtifactSink, ValueCapture, ValueCaptureKind, ValueCodec, ValueRef,
        ValueWriteOutcome, ValueWriter,
    },
};

#[cfg(not(target_arch = "wasm32"))]
struct ProjectValueDrain {
    _service: ValueDrainService,
    handle: ValueDrainHandle,
    process_euid: ProcessEuid,
}

/// One long-lived value worker per project in this process. Boundary-level
/// completion still performs an explicit flush/finish; keeping the worker
/// alive only amortizes pack/index ownership and avoids a thread per call.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn project_value_drain(
    project_root: &Path,
    process_euid: ProcessEuid,
) -> io::Result<ValueDrainHandle> {
    static SERVICES: OnceLock<Mutex<HashMap<std::path::PathBuf, ProjectValueDrain>>> =
        OnceLock::new();
    let project_baml_dir = project_root.join(".baml");
    let mut services = SERVICES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(existing) = services.get(&project_baml_dir) {
        if existing.process_euid != process_euid {
            return Err(io::Error::other(
                "project value drain was initialized for another process identity",
            ));
        }
        return Ok(existing.handle.clone());
    }
    let service = ValueDrainService::start(ValueDrainConfig::new(
        project_baml_dir.clone(),
        process_euid,
    ))?;
    let handle = service.handle();
    services.insert(
        project_baml_dir,
        ProjectValueDrain {
            _service: service,
            handle: handle.clone(),
            process_euid,
        },
    );
    Ok(handle)
}

#[cfg(not(target_arch = "wasm32"))]
use crate::trace_heap::{TraceMediaContent, TraceOmissionReason, TraceValue, TraceValueRef};
use crate::{
    trace_heap::{TraceHeap, TraceSnapshot, TraceSnapshotHandle},
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
    #[cfg(not(target_arch = "wasm32"))]
    pub stage_path: Option<CallPath>,
}

/// Heap-independent handoff object. Once this exists, neither the adapter nor
/// the value drain needs to consult `TraceHeap`.
#[derive(Debug, PartialEq)]
pub struct OwnedTraceValueDraft {
    pub boundary_id: BoundaryId,
    pub call: TraceCallKey,
    pub kind: CaptureKind,
    pub log: Option<TraceLogMetadata>,
    pub snapshot: TraceSnapshot,
    #[cfg(not(target_arch = "wasm32"))]
    pub stage_path: Option<CallPath>,
}

#[derive(Debug, Default, PartialEq)]
pub struct OwnedTraceDrainReport {
    pub drafts: Vec<OwnedTraceValueDraft>,
    pub failures: Vec<TraceDrainFailure>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CasTraceDrainReport {
    pub enqueued: usize,
    pub staged: usize,
    pub dropped: usize,
    pub staging_evictions: usize,
    pub adapter_failures: usize,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub struct ContinuousValueDrain {
    stop: Arc<AtomicBool>,
    wake: Arc<CaptureWake>,
    worker: Option<std::thread::JoinHandle<io::Result<()>>>,
    drain: ValueDrainHandle,
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
    wake: Arc<CaptureWake>,
}

#[derive(Debug, Default)]
struct CaptureWake {
    generation: AtomicU64,
    #[cfg(not(target_arch = "wasm32"))]
    mutex: Mutex<()>,
    #[cfg(not(target_arch = "wasm32"))]
    condvar: Condvar,
}

#[derive(Debug)]
struct TraceCaptureInner {
    config: TraceCaptureConfig,
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
                reserved_value_slots: 0,
                reserved_log_slots: 0,
                reserved_root_result_slots: 0,
                pending: VecDeque::new(),
                stats: TraceCaptureStats::default(),
            })),
            wake: Arc::new(CaptureWake::default()),
        }
    }

    #[must_use]
    pub fn disabled() -> Self {
        Self::new(TraceCaptureConfig::disabled())
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
        #[cfg(not(target_arch = "wasm32"))] stage_path: Option<CallPath>,
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
            #[cfg(not(target_arch = "wasm32"))]
            stage_path,
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
        let reservation = self.try_reserve(
            boundary_id,
            call,
            kind,
            #[cfg(not(target_arch = "wasm32"))]
            None,
        )?;
        let snapshot = copy_snapshot(self.trace_heap());
        reservation.commit(snapshot);
        Ok(())
    }

    /// Capture an input speculatively. The snapshot is deep-copied now, but
    /// the continuous handoff stages it under a byte cap instead of making it
    /// durable until an error trigger promotes its call subtree.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn capture_staged_with(
        &self,
        boundary_id: BoundaryId,
        call: TraceCallKey,
        call_path: CallPath,
        kind: CaptureKind,
        copy_snapshot: impl FnOnce(&TraceHeap) -> TraceSnapshotHandle,
    ) -> Result<(), CaptureSkipReason> {
        assert_eq!(
            kind,
            CaptureKind::CallInput,
            "only call inputs are staged speculatively"
        );
        let reservation = self.try_reserve(boundary_id, call, kind, Some(call_path))?;
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
        let reservation = self.try_reserve(
            boundary_id,
            call,
            CaptureKind::LogBody,
            #[cfg(not(target_arch = "wasm32"))]
            None,
        )?;
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

    /// Move pending snapshots out of `TraceHeap` into owned handoff objects.
    /// This is the only heap-facing step in the CAS path.
    #[must_use]
    pub fn drain_owned_snapshots(&self) -> OwnedTraceDrainReport {
        let mut report = OwnedTraceDrainReport::default();
        while let Some(draft) = self.pop_pending() {
            match self.trace_heap.release(draft.snapshot) {
                Some(snapshot) => report.drafts.push(OwnedTraceValueDraft {
                    boundary_id: draft.boundary_id,
                    call: draft.call,
                    kind: draft.kind,
                    log: draft.log,
                    snapshot,
                    #[cfg(not(target_arch = "wasm32"))]
                    stage_path: draft.stage_path,
                }),
                None => report.failures.push(Self::drain_failure(
                    &draft,
                    TraceDrainFailureReason::SnapshotMissing,
                    format!(
                        "trace snapshot {} was already released",
                        draft.snapshot.raw()
                    ),
                )),
            }
        }
        report
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn drain_owned_to_value_service(
        &self,
        drain: &ValueDrainHandle,
    ) -> io::Result<CasTraceDrainReport> {
        let owned = self.drain_owned_snapshots();
        let mut report = CasTraceDrainReport {
            adapter_failures: owned.failures.len(),
            ..CasTraceDrainReport::default()
        };
        for failure in owned.failures {
            drain.record_capture_loss(
                failure.boundary_id,
                CaptureLossRecord {
                    kind: if failure.kind == CaptureKind::LogBody {
                        CaptureLossKind::Log
                    } else {
                        CaptureLossKind::Value
                    },
                    reason: CaptureLossReason::SnapshotMissing,
                    skipped_count: 1,
                    call: Some(failure.call),
                    message: Some(failure.diagnostic),
                    timestamp_ms: wall_clock_ms(),
                },
            )?;
        }
        for draft in owned.drafts {
            let boundary_id = draft.boundary_id;
            let call = draft.call;
            let kind = draft.kind;
            let retained_bytes = draft.snapshot.estimated_retained_bytes();
            let stage_path = draft.stage_path.clone();
            match durable_capture_from_owned(draft) {
                Ok(capture) => {
                    if let Some(stage_path) = stage_path {
                        let outcome = drain.stage_with(stage_path, retained_bytes, || capture);
                        report.staging_evictions = report
                            .staging_evictions
                            .saturating_add(outcome.evicted_records);
                        if outcome.retained {
                            report.staged += 1;
                        } else {
                            report.dropped += 1;
                        }
                    } else {
                        match drain.try_enqueue(capture) {
                            ValueEnqueueOutcome::Enqueued => report.enqueued += 1,
                            ValueEnqueueOutcome::DroppedPendingBudget
                            | ValueEnqueueOutcome::DroppedQueueFull
                            | ValueEnqueueOutcome::ServiceClosed => report.dropped += 1,
                        }
                    }
                }
                Err(diagnostic) => {
                    report.adapter_failures += 1;
                    drain.record_capture_loss(
                        boundary_id,
                        CaptureLossRecord {
                            kind: if kind == CaptureKind::LogBody {
                                CaptureLossKind::Log
                            } else {
                                CaptureLossKind::Value
                            },
                            reason: CaptureLossReason::EncodeFailed,
                            skipped_count: 1,
                            call: Some(call),
                            message: Some(diagnostic),
                            timestamp_ms: wall_clock_ms(),
                        },
                    )?;
                }
            }
        }
        Ok(report)
    }

    /// Start a coarse-interval plus producer-wake pump. The worker only moves
    /// already-owned trace snapshots and feeds the dedicated CAS drain; CAS
    /// encoding, hashing, pack I/O, and fsync remain on the bex_events worker.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_continuous_value_drain(
        &self,
        drain: ValueDrainHandle,
        interval: Duration,
    ) -> io::Result<ContinuousValueDrain> {
        let producer = self.clone();
        let wake = Arc::clone(&self.wake);
        let worker_wake = Arc::clone(&wake);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker_drain = drain.clone();
        let worker = std::thread::Builder::new()
            .name("baml-trace-value-handoff".to_string())
            .spawn(move || {
                let mut generation = worker_wake.generation.load(Ordering::Acquire);
                while !worker_stop.load(Ordering::Acquire) {
                    generation = worker_wake.wait(generation, interval);
                    producer.drain_owned_to_value_service(&worker_drain)?;
                }
                producer.drain_owned_to_value_service(&worker_drain)?;
                Ok(())
            })?;
        Ok(ContinuousValueDrain {
            stop,
            wake,
            worker: Some(worker),
            drain,
        })
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
                        promotion_trigger: None,
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

impl CaptureWake {
    fn notify(&self) {
        self.generation.fetch_add(1, Ordering::Release);
        #[cfg(not(target_arch = "wasm32"))]
        self.condvar.notify_one();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn wait(&self, observed_generation: u64, timeout: Duration) -> u64 {
        if self.generation.load(Ordering::Acquire) != observed_generation {
            return self.generation.load(Ordering::Acquire);
        }
        let guard = self
            .mutex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = self
            .condvar
            .wait_timeout_while(guard, timeout, |_| {
                self.generation.load(Ordering::Acquire) == observed_generation
            })
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.generation.load(Ordering::Acquire)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ContinuousValueDrain {
    /// Stop the handoff pump, perform its final owned-snapshot drain, and wait
    /// for the CAS worker's ordered completion barrier.
    pub fn flush_and_join(mut self) -> io::Result<()> {
        self.stop_and_join()
    }

    fn stop_and_join(&mut self) -> io::Result<()> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        self.stop.store(true, Ordering::Release);
        self.wake.notify();
        worker
            .join()
            .map_err(|_| io::Error::other("trace value handoff worker panicked"))??;
        self.drain.flush()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for ContinuousValueDrain {
    fn drop(&mut self) {
        let _ = self.stop_and_join();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn stage_owned_trace_value(
    drain: &ValueDrainHandle,
    call_path: CallPath,
    draft: OwnedTraceValueDraft,
) -> Result<bex_events::value_cas::ValueStageOutcome, String> {
    let retained_bytes = draft.snapshot.estimated_retained_bytes();
    let capture = durable_capture_from_owned(draft)?;
    Ok(drain.stage_with(call_path, retained_bytes, || capture))
}

#[cfg(not(target_arch = "wasm32"))]
fn durable_capture_from_owned(draft: OwnedTraceValueDraft) -> Result<DurableValueCapture, String> {
    let value = canonical_value_from_snapshot(&draft.snapshot)?;
    let log_event = draft.log.map(|log| LogEventRecord {
        call: draft.call,
        level: log.level,
        source: log.source,
        timestamp_ms: log.timestamp_ms,
        message_preview: log.message_preview,
    });
    Ok(DurableValueCapture {
        boundary_id: draft.boundary_id,
        call: draft.call,
        kind: value_capture_kind(draft.kind),
        log_event,
        value,
        promoted_by: None,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn canonical_value_from_snapshot(snapshot: &TraceSnapshot) -> Result<CanonicalValue, String> {
    let mut stack = Vec::new();
    canonical_trace_value(snapshot, snapshot.root(), &mut stack)
}

#[cfg(not(target_arch = "wasm32"))]
fn canonical_trace_value(
    snapshot: &TraceSnapshot,
    value_ref: TraceValueRef,
    stack: &mut Vec<usize>,
) -> Result<CanonicalValue, String> {
    if stack.contains(&value_ref.raw()) {
        return Ok(CanonicalValue::Omitted(OmissionValue {
            reason: "cyclicReference".to_string(),
            message: "cycle detected while adapting owned trace snapshot".to_string(),
        }));
    }
    let value = snapshot.value(value_ref).ok_or_else(|| {
        format!(
            "owned trace snapshot references missing value {}",
            value_ref.raw()
        )
    })?;
    stack.push(value_ref.raw());
    let converted = match value {
        TraceValue::Null => CanonicalValue::Null,
        TraceValue::Bool(value) => CanonicalValue::Bool(*value),
        TraceValue::Int(value) => CanonicalValue::Int(*value),
        TraceValue::Float(value) => CanonicalValue::Float(*value),
        TraceValue::Bigint(value) => CanonicalValue::BigInt(value.clone()),
        TraceValue::String(value) => CanonicalValue::String(value.clone()),
        TraceValue::Bytes(value) => CanonicalValue::Bytes(value.clone()),
        TraceValue::Array(values) => CanonicalValue::List(
            values
                .iter()
                .map(|value| canonical_trace_value(snapshot, *value, stack))
                .collect::<Result<_, _>>()?,
        ),
        TraceValue::Map(values) => CanonicalValue::Map(
            values
                .iter()
                .map(|(key, value)| {
                    Ok((key.clone(), canonical_trace_value(snapshot, *value, stack)?))
                })
                .collect::<Result<_, String>>()?,
        ),
        TraceValue::Media(value) => CanonicalValue::Media(MediaValue {
            kind: value.kind.tag_str().to_string(),
            mime_type: value.mime_type.clone(),
            content: match &value.content {
                TraceMediaContent::Url(value) => MediaContent::Url(value.clone()),
                TraceMediaContent::Base64(value) => MediaContent::Base64(value.clone()),
                TraceMediaContent::File(value) => MediaContent::File(value.clone()),
            },
        }),
        TraceValue::Instance {
            type_name, fields, ..
        } => CanonicalValue::Class {
            definition_key: type_name.clone(),
            fields: fields
                .iter()
                .map(|(name, value)| {
                    Ok(CanonicalField {
                        name: name.clone(),
                        presence: FieldPresence::Present(canonical_trace_value(
                            snapshot, *value, stack,
                        )?),
                    })
                })
                .collect::<Result<_, String>>()?,
        },
        TraceValue::Enum { type_name, variant } => CanonicalValue::Enum {
            definition_key: type_name.clone(),
            variant: variant.clone(),
        },
        TraceValue::Omitted(value) => CanonicalValue::Omitted(OmissionValue {
            reason: omission_reason_name(value.reason).to_string(),
            message: value.message.clone(),
        }),
    };
    let popped = stack.pop();
    debug_assert_eq!(popped, Some(value_ref.raw()));
    Ok(converted)
}

#[cfg(not(target_arch = "wasm32"))]
fn omission_reason_name(reason: TraceOmissionReason) -> &'static str {
    match reason {
        TraceOmissionReason::OmittedArgument => "omittedArgument",
        TraceOmissionReason::UnsupportedValue => "unsupportedValue",
        TraceOmissionReason::HostOwnedValue => "hostOwnedValue",
        TraceOmissionReason::InvalidRuntimeValue => "invalidRuntimeValue",
        TraceOmissionReason::CyclicReference => "cyclicReference",
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn wall_clock_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
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
    #[cfg(not(target_arch = "wasm32"))]
    stage_path: Option<CallPath>,
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
            #[cfg(not(target_arch = "wasm32"))]
            stage_path: self.stage_path.take(),
        });
        inner.stats.published = inner.stats.published.saturating_add(1);
        self.committed = true;
        drop(inner);
        self.producer.wake.notify();
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
        time::Duration,
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
            TraceDrainFailureReason, TraceLogMetadata,
        },
    };
    #[cfg(not(target_arch = "wasm32"))]
    use bex_events::{
        value::{ValueCaptureKind, ValueFileRecord, read_bamlvalue_from_bytes},
        value_cas::{
            CallPath, TriggerId, ValueBoundaryRegistration, ValueDrainConfig, ValueDrainService,
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
        assert!(report.logs[1].body.contains("MapValue"));
        assert!(report.logs[1].body.contains("user"));
        assert!(report.logs[1].body.contains("ada"));
        assert_eq!(producer.trace_heap().retained_snapshot_count(), 0);
        assert!(producer.drain().is_empty());
    }

    #[test]
    fn abandoned_reservation_frees_capacity() {
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled(1));
        drop(
            producer
                .try_reserve(
                    boundary_id(),
                    trace_key(),
                    CaptureKind::RootError,
                    #[cfg(not(target_arch = "wasm32"))]
                    None,
                )
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

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn continuous_owned_handoff_commits_without_retaining_trace_heap_snapshots() {
        let root = std::env::temp_dir().join(format!(
            "baml-owned-value-handoff-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let boundary_dir = root.join("history/run");
        let process = ProcessEuid([1; 16]);
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled(4));
        let service = ValueDrainService::start(ValueDrainConfig::new(&root, process)).unwrap();
        let drain = service.handle();
        drain
            .register_boundary(ValueBoundaryRegistration {
                boundary_id: boundary_id(),
                boundary_dir: boundary_dir.clone(),
                created_ms: 10,
                run_started: None,
            })
            .unwrap();
        let pump = producer
            .start_continuous_value_drain(drain.clone(), Duration::from_secs(60))
            .unwrap();
        producer
            .capture_with(
                boundary_id(),
                trace_key(),
                CaptureKind::CallInput,
                |trace_heap| structured_test_snapshot(trace_heap),
            )
            .unwrap();
        pump.flush_and_join().unwrap();
        assert_eq!(producer.trace_heap().retained_snapshot_count(), 0);
        drain.finish_boundary(boundary_id()).unwrap();
        service.shutdown().unwrap();

        let contents = read_bamlvalue_from_bytes(
            &std::fs::read(boundary_dir.join("values.bamlvalue")).unwrap(),
        )
        .unwrap();
        let [ValueFileRecord::CapturedValue(record)] = contents.records.as_slice() else {
            panic!("expected one CAS-backed value root");
        };
        assert!(record.body.is_empty());
        assert!(record.dag_ref.is_some());
        assert_eq!(
            record.capture.as_ref().map(|capture| capture.kind),
            Some(ValueCaptureKind::CallInput)
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn speculative_owned_handoff_is_durable_only_after_trigger_promotion() {
        let root = std::env::temp_dir().join(format!(
            "baml-staged-value-handoff-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let boundary_dir = root.join("history/run");
        let process = ProcessEuid([1; 16]);
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled(4));
        let service = ValueDrainService::start(ValueDrainConfig::new(&root, process)).unwrap();
        let drain = service.handle();
        drain
            .register_boundary(ValueBoundaryRegistration {
                boundary_id: boundary_id(),
                boundary_dir: boundary_dir.clone(),
                created_ms: 10,
                run_started: None,
            })
            .unwrap();
        let path = CallPath {
            boundary_id: boundary_id(),
            process_euid: process,
            engine_id: trace_key().engine_id,
            logical_thread_id: trace_key().thread_id.0,
            call_ids: vec![1, trace_key().call_id.0],
        };
        let pump = producer
            .start_continuous_value_drain(drain.clone(), Duration::from_secs(60))
            .unwrap();
        producer
            .capture_staged_with(
                boundary_id(),
                trace_key(),
                path,
                CaptureKind::CallInput,
                structured_test_snapshot,
            )
            .unwrap();
        pump.flush_and_join().unwrap();
        assert!(drain.stats().staging_bytes > 0);
        drain
            .promote_staged(
                &CallPath::boundary(boundary_id(), process, trace_key().engine_id),
                TriggerId("error".to_owned()),
                99,
            )
            .unwrap();
        drain.finish_boundary(boundary_id()).unwrap();
        service.shutdown().unwrap();

        let contents = read_bamlvalue_from_bytes(
            &std::fs::read(boundary_dir.join("values.bamlvalue")).unwrap(),
        )
        .unwrap();
        assert!(contents.records.iter().any(|record| matches!(
            record,
            ValueFileRecord::CapturedValue(record)
                if record.capture.as_ref().and_then(|capture| capture.promotion_trigger.as_deref())
                    == Some("error")
        )));
        std::fs::remove_dir_all(root).unwrap();
    }
}
