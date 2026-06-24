//! Producer-side trace value capture queue.

use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Mutex},
};

use bex_events::{
    ids::BoundaryId,
    run::TraceCallKey,
    value::{ValueArtifactSink, ValueCodec, ValueRef, ValueWriter},
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
}

#[derive(Debug, PartialEq, Eq)]
pub struct TraceValueDraft {
    pub boundary_id: BoundaryId,
    pub call: TraceCallKey,
    pub kind: CaptureKind,
    pub snapshot: TraceSnapshotHandle,
}

#[derive(Debug, PartialEq, Eq)]
pub struct EncodedTraceValue {
    pub boundary_id: BoundaryId,
    pub call: TraceCallKey,
    pub kind: CaptureKind,
    pub value_ref: ValueRef,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceCaptureConfig {
    pub enabled: bool,
    pub max_pending_drafts: usize,
}

impl TraceCaptureConfig {
    #[must_use]
    pub fn enabled(max_pending_drafts: usize) -> Self {
        Self {
            enabled: true,
            max_pending_drafts,
        }
    }

    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            max_pending_drafts: 0,
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
    reserved_slots: usize,
    pending: VecDeque<TraceValueDraft>,
    stats: TraceCaptureStats,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TraceCaptureStats {
    pub published: u64,
    pub skipped_disabled: u64,
    pub skipped_queue_full: u64,
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
                reserved_slots: 0,
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
        let occupied = inner.pending.len().saturating_add(inner.reserved_slots);
        if occupied >= inner.config.max_pending_drafts {
            inner.stats.skipped_queue_full = inner.stats.skipped_queue_full.saturating_add(1);
            return Err(CaptureSkipReason::QueueFull);
        }
        inner.reserved_slots = inner.reserved_slots.saturating_add(1);
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
            let outcome = writer.append_body(ValueCodec::BamlOutboundValue, body.clone());
            let _ = self.trace_heap.release(draft.snapshot);
            let outcome = outcome?;
            encoded.push(EncodedTraceValue {
                boundary_id: draft.boundary_id,
                call: draft.call,
                kind: draft.kind,
                value_ref: outcome.value_ref,
                body,
            });
        }
        Ok(encoded)
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
        let mut inner = self
            .producer
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.reserved_slots = inner.reserved_slots.saturating_sub(1);
        inner.pending.push_back(TraceValueDraft {
            boundary_id: self.boundary_id,
            call: self.call,
            kind: self.kind,
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
        inner.reserved_slots = inner.reserved_slots.saturating_sub(1);
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
        value_capture::{CaptureKind, CaptureSkipReason, TraceCaptureConfig, TraceCaptureProducer},
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

    #[test]
    fn zero_capacity_capture_does_not_run_copy_closure() {
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_copy = Arc::clone(&calls);

        let result = producer.capture_with(
            boundary_id(),
            trace_key(),
            CaptureKind::RootOutput,
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
