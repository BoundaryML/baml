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
    trace_value_encode::encode_trace_snapshot_body,
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

    pub fn try_reserve(
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
        let mut encoded = Vec::new();
        for draft in self.drain() {
            let snapshot = self.trace_heap.get(draft.snapshot).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "trace snapshot {} was already released",
                        draft.snapshot.raw()
                    ),
                )
            })?;
            let body = match encode_trace_snapshot_body(&snapshot) {
                Ok(body) => body,
                Err(err) => {
                    let _ = self.trace_heap.release(draft.snapshot);
                    return Err(io::Error::new(io::ErrorKind::InvalidData, err));
                }
            };
            let outcome = record_value(&draft, body.clone());
            let _ = self.trace_heap.release(draft.snapshot);
            let outcome = outcome?;
            encoded.push(EncodedTraceValue {
                boundary_id: draft.boundary_id,
                call: draft.call,
                kind: draft.kind,
                log: draft.log,
                value_ref: outcome.value_ref,
                body,
            });
        }
        Ok(encoded)
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
pub struct CaptureReservation {
    producer: TraceCaptureProducer,
    boundary_id: BoundaryId,
    call: TraceCallKey,
    kind: CaptureKind,
    committed: bool,
}

impl CaptureReservation {
    pub fn commit(mut self, snapshot: TraceSnapshotHandle) {
        self.commit_with_log(snapshot, None);
    }

    pub fn commit_log(mut self, snapshot: TraceSnapshotHandle, log: TraceLogMetadata) {
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
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use bex_events::{
        ids::{BexCallId, BexThreadId, BoundaryId, EngineId, ProcessEuid},
        run::TraceCallKey,
    };

    use crate::{
        trace_heap::{TraceHeap, TraceSnapshotHandle},
        value_capture::{
            CaptureKind, CaptureSkipReason, TraceCaptureConfig, TraceCaptureProducer,
            TraceLogMetadata,
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
}
