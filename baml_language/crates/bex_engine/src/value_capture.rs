//! Producer-side trace value capture queue.

use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Mutex},
};

use bex_events::store::ValueStoreSink;
#[cfg(not(target_arch = "wasm32"))]
use bex_events::store::{Store, drain::ValueDrainService};
use bex_events::{
    ids::{BexCallId, BexThreadId, BoundaryId, EngineId, ProcessEuid},
    run::{SourceLocation, TraceCallKey, TraceThreadKey},
    store::canon,
    value::{
        CaptureLossKind, CaptureLossReason, CaptureLossRecord, DagRef, LogEventRecord,
        ValueArtifactSink, ValueCapture, ValueCaptureKind, ValueCodec, ValueRef, ValueWriteOutcome,
        ValueWriter,
    },
};

use crate::{
    trace_heap::{TraceHeap, TraceSnapshot, TraceSnapshotHandle},
    trace_value_encode::{
        canonical_from_snapshot, encode_trace_snapshot_body, render_encoded_trace_value,
    },
};

/// §7.2 staging-ring byte budget defaults ("short-lived value buffer").
pub const DEFAULT_NATIVE_STAGING_RING_BYTES: usize = 32 * 1024 * 1024;
pub const DEFAULT_WASM_STAGING_RING_BYTES: usize = 8 * 1024 * 1024;

#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_STAGING_RING_BYTES: usize = DEFAULT_NATIVE_STAGING_RING_BYTES;
#[cfg(target_arch = "wasm32")]
const DEFAULT_STAGING_RING_BYTES: usize = DEFAULT_WASM_STAGING_RING_BYTES;

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
    /// §7.2 `role: promoted`: the trigger id that moved this draft from the
    /// speculative staging ring into the durable drain queue. `None` for
    /// directly captured drafts.
    pub promoted_by: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct EncodedTraceValue {
    pub boundary_id: BoundaryId,
    pub call: TraceCallKey,
    pub kind: CaptureKind,
    pub log: Option<TraceLogMetadata>,
    pub value_ref: ValueRef,
    pub body: Vec<u8>,
    /// §7.2 `role: promoted` marking carried through the drain.
    pub promoted_by: Option<String>,
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
    /// §7.2 staging-ring byte budget for SPECULATIVE drafts (0 disables
    /// staging). Defaults: 32 MiB native / 8 MiB wasm.
    pub staging_ring_bytes: usize,
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
            staging_ring_bytes: DEFAULT_STAGING_RING_BYTES,
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
            staging_ring_bytes: DEFAULT_STAGING_RING_BYTES,
        }
    }

    /// Override the §7.2 staging-ring byte budget (0 disables staging).
    #[must_use]
    pub fn with_staging_ring_bytes(mut self, staging_ring_bytes: usize) -> Self {
        self.staging_ring_bytes = staging_ring_bytes;
        self
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
            staging_ring_bytes: 0,
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
    /// §7.2 staging ring: SPECULATIVE drafts, oldest at the front (staging
    /// order doubles as LRU order — staged drafts are never re-touched).
    staged: VecDeque<StagedDraft>,
    /// Bytes currently held by `staged` (approx snapshot bytes).
    staged_bytes: usize,
    /// `stats.staged_evicted` watermark already reported as a
    /// `CaptureLoss{StagingEvicted}` record by a writer drain.
    staged_evicted_reported: u64,
    stats: TraceCaptureStats,
}

/// One §7.2 staged (speculative) draft plus its byte accounting.
#[derive(Debug)]
struct StagedDraft {
    draft: TraceValueDraft,
    bytes: usize,
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
    /// §7.2 staging: drafts accepted into the staging ring.
    pub staged: u64,
    /// §7.2 staging: drafts dropped under byte pressure (LRU eviction).
    pub staged_evicted: u64,
    /// §7.2 staging: drafts released at frame close (normal completion).
    pub staged_released: u64,
    /// §7.2 staging: drafts promoted into the durable drain queue.
    pub staged_promoted: u64,
}

/// §7.2 promotion outcome: how many staged drafts moved to the durable
/// queue, plus the cumulative staging-ring evictions so far — "we would
/// have had it but the buffer was too small", visible and tunable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PromotionReport {
    pub promoted: usize,
    /// Cumulative `staged_evicted` counter at promotion time (not scoped
    /// to the promoted prefix — eviction drops the call key's draft, so
    /// per-prefix attribution is no longer possible by then).
    pub staged_evicted: u64,
}

/// A leading-components match over [`TraceCallKey`] (§7.2 release /
/// promotion scope).
///
/// `TraceCallKey` nests `process_euid → engine_id → thread_id → call_id`,
/// so a prefix can scope a whole engine, one thread, or one exact call.
/// Call *ancestry* (the failing subtree) is not encoded in the key itself
/// — parent links live in the prof stream — so subtree promotion is
/// expressed as one exact-call prefix per subtree call (or a whole-thread
/// prefix); there is deliberately no "descendants of call X" form here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceCallKeyPrefix {
    process_euid: ProcessEuid,
    engine_id: EngineId,
    thread_id: Option<BexThreadId>,
    call_id: Option<BexCallId>,
}

impl TraceCallKeyPrefix {
    /// Every call of one engine in one process.
    #[must_use]
    pub fn engine(process_euid: ProcessEuid, engine_id: EngineId) -> Self {
        Self {
            process_euid,
            engine_id,
            thread_id: None,
            call_id: None,
        }
    }

    /// Every call of one BEX thread.
    #[must_use]
    pub fn thread(key: TraceThreadKey) -> Self {
        Self {
            process_euid: key.process_euid,
            engine_id: key.engine_id,
            thread_id: Some(key.thread_id),
            call_id: None,
        }
    }

    /// Exactly one call.
    #[must_use]
    pub fn exact(key: TraceCallKey) -> Self {
        Self {
            process_euid: key.process_euid,
            engine_id: key.engine_id,
            thread_id: Some(key.thread_id),
            call_id: Some(key.call_id),
        }
    }

    #[must_use]
    pub fn matches(&self, key: TraceCallKey) -> bool {
        self.process_euid == key.process_euid
            && self.engine_id == key.engine_id
            && self
                .thread_id
                .is_none_or(|thread_id| thread_id == key.thread_id)
            && self.call_id.is_none_or(|call_id| call_id == key.call_id)
    }
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
                staged: VecDeque::new(),
                staged_bytes: 0,
                staged_evicted_reported: 0,
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

    /// Stage a SPECULATIVE capture into the §7.2 staging ring instead of
    /// the durable drain queue: no serialization, no hashing, no I/O — the
    /// draft only becomes durable if a trigger later promotes it
    /// ([`Self::promote_staged`]); frame close releases it
    /// ([`Self::release_staged`]) and byte pressure evicts oldest-first.
    ///
    /// Reserve-before-copy holds: with capture disabled or a zero staging
    /// budget the copy closure never runs.
    pub fn stage_with(
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
        {
            let mut inner = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !inner.config.enabled {
                inner.stats.skipped_disabled = inner.stats.skipped_disabled.saturating_add(1);
                return Err(CaptureSkipReason::Disabled);
            }
            if inner.config.staging_ring_bytes == 0 {
                inner.stats.skipped_queue_full = inner.stats.skipped_queue_full.saturating_add(1);
                return Err(CaptureSkipReason::QueueFull);
            }
        }
        // §7.2 cost model: staging pays the deep copy on every staged call.
        let snapshot = copy_snapshot(self.trace_heap());
        let bytes = self.trace_heap.snapshot_bytes(snapshot).unwrap_or(0);
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.staged.push_back(StagedDraft {
            draft: TraceValueDraft {
                boundary_id,
                call,
                kind,
                log: None,
                snapshot,
                promoted_by: None,
            },
            bytes,
        });
        inner.staged_bytes = inner.staged_bytes.saturating_add(bytes);
        inner.stats.staged = inner.stats.staged.saturating_add(1);
        // LRU/byte-pressure eviction, oldest first (the newest draft is
        // itself evicted when it alone exceeds the whole budget).
        while inner.staged_bytes > inner.config.staging_ring_bytes {
            let Some(evicted) = inner.staged.pop_front() else {
                break;
            };
            inner.staged_bytes = inner.staged_bytes.saturating_sub(evicted.bytes);
            inner.stats.staged_evicted = inner.stats.staged_evicted.saturating_add(1);
            let _ = self.trace_heap.release(evicted.draft.snapshot);
        }
        Ok(())
    }

    /// §7.2 frame-close release: drop every staged draft matching `prefix`
    /// — cheap (ring removal + snapshot release, no serialization).
    /// Returns the number of drafts released.
    pub fn release_staged(&self, prefix: TraceCallKeyPrefix) -> usize {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut kept = VecDeque::with_capacity(inner.staged.len());
        let mut released = 0usize;
        while let Some(staged) = inner.staged.pop_front() {
            if prefix.matches(staged.draft.call) {
                inner.staged_bytes = inner.staged_bytes.saturating_sub(staged.bytes);
                inner.stats.staged_released = inner.stats.staged_released.saturating_add(1);
                released += 1;
                let _ = self.trace_heap.release(staged.draft.snapshot);
            } else {
                kept.push_back(staged);
            }
        }
        inner.staged = kept;
        released
    }

    /// §7.2 trigger promotion: move every staged draft matching `prefix`
    /// into the durable drain queue (preserving capture order), marked
    /// `role: promoted` with `trigger_id`; the NEXT drain writes them like
    /// normal captures.
    pub fn promote_staged(&self, prefix: TraceCallKeyPrefix, trigger_id: &str) -> PromotionReport {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut kept = VecDeque::with_capacity(inner.staged.len());
        let mut promoted = 0usize;
        while let Some(mut staged) = inner.staged.pop_front() {
            if prefix.matches(staged.draft.call) {
                inner.staged_bytes = inner.staged_bytes.saturating_sub(staged.bytes);
                inner.stats.staged_promoted = inner.stats.staged_promoted.saturating_add(1);
                staged.draft.promoted_by = Some(trigger_id.to_string());
                inner.pending.push_back(staged.draft);
                promoted += 1;
            } else {
                kept.push_back(staged);
            }
        }
        inner.staged = kept;
        PromotionReport {
            promoted,
            staged_evicted: inner.stats.staged_evicted,
        }
    }

    /// Bytes currently held by the §7.2 staging ring.
    #[must_use]
    pub fn staged_bytes(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .staged_bytes
    }

    /// Drafts currently in the §7.2 staging ring.
    #[must_use]
    pub fn staged_len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .staged
            .len()
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
        self.drain_to_value_writer_with_sink(writer, None, 0)
    }

    /// Drain like [`Self::drain_to_value_writer`], additionally persisting
    /// each captured value's canonical DAG to `store` when one is available
    /// (§7.4 dual-write): on a successful store write the `.bamlvalue`
    /// record carries a `DagRef`. With `store: None` (playground live /
    /// wasm paths) behavior is unchanged — no store write, no `DagRef`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn drain_to_value_writer_with_store<S: ValueArtifactSink>(
        &self,
        writer: &mut ValueWriter<S>,
        store: Option<&mut Store>,
        created_ms: u64,
    ) -> io::Result<Vec<EncodedTraceValue>> {
        match store {
            Some(store) => self.drain_to_value_writer_with_sink(writer, Some(store), created_ms),
            None => self.drain_to_value_writer_with_sink(writer, None, created_ms),
        }
    }

    /// Sibling of [`Self::drain_to_value_writer_with_store`] routing store
    /// puts through the per-process §7.3 [`ValueDrainService`] instead of a
    /// borrowed [`Store`]: canonical DAGs are appended on the service
    /// thread (sync round-trips, so `.bamlvalue` record order still matches
    /// pack append order).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn drain_to_value_writer_with_service<S: ValueArtifactSink>(
        &self,
        writer: &mut ValueWriter<S>,
        service: &ValueDrainService,
        created_ms: u64,
    ) -> io::Result<Vec<EncodedTraceValue>> {
        let mut sink = service;
        self.drain_to_value_writer_with_sink(writer, Some(&mut sink), created_ms)
    }

    fn drain_to_value_writer_with_sink<S: ValueArtifactSink>(
        &self,
        writer: &mut ValueWriter<S>,
        mut sink: Option<&mut dyn ValueStoreSink>,
        created_ms: u64,
    ) -> io::Result<Vec<EncodedTraceValue>> {
        let encoded = self.drain_to_value_recorder(|draft, body, canonical| {
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
                // A failed store write degrades to the legacy record shape
                // (no DagRef); the inline body stays authoritative until P9.
                let dag_ref = sink.as_deref_mut().and_then(|sink| {
                    sink.put_encoded(canonical, created_ms)
                        .ok()
                        .map(|_| DagRef {
                            root_cid: canonical.root_cid,
                            node_codec_version: canon::NODE_CODEC_VERSION,
                            logical_len: canonical.logical_len,
                        })
                });
                writer.append_body_with_capture_dag_and_promotion(
                    ValueCodec::BamlOutboundValue,
                    body,
                    Some(ValueCapture {
                        kind: value_capture_kind(draft.kind),
                        call: draft.call,
                    }),
                    dag_ref,
                    draft.promoted_by.clone(),
                )
            }
        })?;
        self.append_staging_eviction_loss(writer, created_ms)?;
        Ok(encoded)
    }

    /// §7.2 capture-loss visibility: staging-ring evictions not yet
    /// reported become one `CaptureLoss{StagingEvicted}` record. No-op
    /// (and no bytes) when staging saw no new evictions.
    fn append_staging_eviction_loss<S: ValueArtifactSink>(
        &self,
        writer: &mut ValueWriter<S>,
        timestamp_ms: u64,
    ) -> io::Result<()> {
        let evicted = {
            let mut inner = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let evicted = inner
                .stats
                .staged_evicted
                .saturating_sub(inner.staged_evicted_reported);
            inner.staged_evicted_reported = inner.stats.staged_evicted;
            evicted
        };
        if evicted == 0 {
            return Ok(());
        }
        writer.append_capture_loss(&CaptureLossRecord {
            kind: CaptureLossKind::Value,
            reason: CaptureLossReason::StagingEvicted,
            skipped_count: evicted,
            call: None,
            message: Some(format!(
                "Evicted {evicted} staged speculative capture(s) under staging-ring byte pressure"
            )),
            timestamp_ms,
        })
    }

    pub fn drain_to_value_recorder(
        &self,
        mut record_value: impl FnMut(
            &TraceValueDraft,
            Vec<u8>,
            &canon::CanonEncoded,
        ) -> io::Result<ValueWriteOutcome>,
    ) -> io::Result<Vec<EncodedTraceValue>> {
        let report = self.drain_to_value_recorder_report(|draft, body, canonical| {
            record_value(draft, body, canonical)
        });
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
        mut record_value: impl FnMut(
            &TraceValueDraft,
            Vec<u8>,
            &canon::CanonEncoded,
        ) -> io::Result<ValueWriteOutcome>,
    ) -> TraceDrainReport {
        let mut report = TraceDrainReport::default();
        while let Some(draft) = self.pop_pending() {
            let (body, canonical) = match self.encode_snapshot(&draft) {
                Ok(encoded) => encoded,
                Err((reason, diagnostic)) => {
                    let _ = self.trace_heap.release(draft.snapshot);
                    report
                        .failures
                        .push(Self::drain_failure(&draft, reason, diagnostic));
                    continue;
                }
            };
            let outcome = record_value(&draft, body.clone(), &canonical);
            let _ = self.trace_heap.release(draft.snapshot);
            match outcome {
                Ok(outcome) => report.encoded.push(EncodedTraceValue {
                    boundary_id: draft.boundary_id,
                    call: draft.call,
                    kind: draft.kind,
                    log: draft.log,
                    value_ref: outcome.value_ref,
                    body,
                    promoted_by: draft.promoted_by,
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
            let body = match self.encode_snapshot_body(&draft) {
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

    /// Encode a draft's snapshot both ways (§7.4 dual-write): the legacy
    /// proto body plus the canonical DAG encoding.
    fn encode_snapshot(
        &self,
        draft: &TraceValueDraft,
    ) -> Result<(Vec<u8>, canon::CanonEncoded), (TraceDrainFailureReason, String)> {
        let snapshot = self.snapshot(draft)?;
        let body = encode_trace_snapshot_body(&snapshot)
            .map_err(|diagnostic| (TraceDrainFailureReason::EncodeFailed, diagnostic))?;
        let canonical = canon::encode(&canonical_from_snapshot(&snapshot));
        Ok((body, canonical))
    }

    /// Legacy proto body only — for render paths that never persist.
    fn encode_snapshot_body(
        &self,
        draft: &TraceValueDraft,
    ) -> Result<Vec<u8>, (TraceDrainFailureReason, String)> {
        let snapshot = self.snapshot(draft)?;
        encode_trace_snapshot_body(&snapshot)
            .map_err(|diagnostic| (TraceDrainFailureReason::EncodeFailed, diagnostic))
    }

    fn snapshot(
        &self,
        draft: &TraceValueDraft,
    ) -> Result<TraceSnapshot, (TraceDrainFailureReason, String)> {
        self.trace_heap.get(draft.snapshot).ok_or_else(|| {
            (
                TraceDrainFailureReason::SnapshotMissing,
                format!(
                    "trace snapshot {} was already released",
                    draft.snapshot.raw()
                ),
            )
        })
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
            promoted_by: None,
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
        store::{Store, canon, drain::ValueDrainService},
        value::{
            ByteValueArtifactSink, ValueCodec, ValueFileRecord, ValueRef, ValueWriteOutcome,
            ValueWriter, read_bamlvalue_from_bytes,
        },
    };

    use crate::{
        trace_heap::{TraceHeap, TraceSnapshot, TraceSnapshotHandle, TraceValue, TraceValueRef},
        value_capture::{
            CaptureKind, CaptureSkipReason, TraceCallKeyPrefix, TraceCaptureConfig,
            TraceCaptureProducer, TraceDrainFailureReason, TraceLogMetadata,
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
        let report = producer.drain_to_value_recorder_report(|draft, body, canonical| {
            assert!(!body.is_empty());
            assert_ne!(canonical.root_cid, [0u8; 32]);
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
    fn drain_with_store_attaches_dag_refs_and_persists_the_canonical_dag() {
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled(2));
        producer
            .capture_with(
                boundary_id(),
                trace_key(),
                CaptureKind::RootOutput,
                structured_test_snapshot,
            )
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path(), [3; 16]).unwrap();
        let mut writer = ValueWriter::new(ByteValueArtifactSink::new(), boundary_id()).unwrap();
        let encoded = producer
            .drain_to_value_writer_with_store(&mut writer, Some(&mut store), 7)
            .unwrap();
        assert_eq!(encoded.len(), 1);

        let parsed = read_bamlvalue_from_bytes(writer.sink().bytes()).unwrap();
        let ValueFileRecord::CapturedValue(record) = &parsed.records[0] else {
            panic!("expected value record");
        };
        assert!(!record.body.is_empty(), "legacy body stays (dual-write)");
        let dag_ref = record.dag_ref.expect("dag ref attached");
        assert_eq!(dag_ref.node_codec_version, canon::NODE_CODEC_VERSION);
        assert!(
            store.get(&dag_ref.root_cid).unwrap().is_some(),
            "store serves the DAG root",
        );

        // The stored root is exactly the canonical encoding of the snapshot.
        let expected = canon::encode(&canon::CanonValue::Map(vec![
            ("user".to_string(), canon::CanonValue::String("ada".into())),
            ("count".to_string(), canon::CanonValue::Int(42)),
        ]));
        assert_eq!(dag_ref.root_cid, expected.root_cid);
        assert_eq!(dag_ref.logical_len, expected.logical_len);
        assert_eq!(producer.trace_heap().retained_snapshot_count(), 0);
    }

    #[test]
    fn drain_without_store_writes_no_dag_ref() {
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled(2));
        producer
            .capture_with(
                boundary_id(),
                trace_key(),
                CaptureKind::RootOutput,
                structured_test_snapshot,
            )
            .unwrap();

        let mut writer = ValueWriter::new(ByteValueArtifactSink::new(), boundary_id()).unwrap();
        let encoded = producer.drain_to_value_writer(&mut writer).unwrap();
        assert_eq!(encoded.len(), 1);

        let parsed = read_bamlvalue_from_bytes(writer.sink().bytes()).unwrap();
        let ValueFileRecord::CapturedValue(record) = &parsed.records[0] else {
            panic!("expected value record");
        };
        assert_eq!(record.dag_ref, None, "no store, no DagRef — unchanged");
        assert!(!record.body.is_empty());
    }

    #[test]
    #[should_panic(expected = "log captures must use capture_log_with")]
    fn capture_with_rejects_log_body_kind() {
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled(1));
        let _ = producer.capture_with(boundary_id(), trace_key(), CaptureKind::LogBody, |_| {
            TraceSnapshotHandle::for_test(1)
        });
    }

    // -----------------------------------------------------------------
    // §7.2 trigger-promoted staging ring
    // -----------------------------------------------------------------

    fn trace_key_with(thread_id: u64, call_id: u64) -> TraceCallKey {
        TraceCallKey {
            process_euid: ProcessEuid([1; 16]),
            engine_id: EngineId(2),
            thread_id: BexThreadId(thread_id),
            call_id: BexCallId(call_id),
        }
    }

    fn string_snapshot(trace_heap: &TraceHeap, text: &str) -> TraceSnapshotHandle {
        trace_heap.insert_for_test(TraceSnapshot::for_test(
            TraceValueRef::for_test(0),
            vec![TraceValue::String(text.to_string())],
        ))
    }

    /// Bytes one single-string staged snapshot occupies in the ring.
    fn string_snapshot_bytes(len: usize) -> usize {
        size_of::<TraceValue>() + len
    }

    #[test]
    fn staged_drafts_do_not_reach_a_drain_until_promoted() {
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled(4));
        producer
            .stage_with(
                boundary_id(),
                trace_key(),
                CaptureKind::CallInput,
                |trace_heap| test_snapshot(trace_heap, 5),
            )
            .unwrap();

        assert!(producer.drain().is_empty(), "staged is not pending");
        let report = producer.drain_to_value_recorder_report(|_, _, _| {
            panic!("speculative drafts must not be encoded by a drain")
        });
        assert!(report.encoded.is_empty());
        assert!(report.failures.is_empty());
        assert_eq!(producer.staged_len(), 1, "draft still staged");
        assert_eq!(producer.stats().staged, 1);
        assert_eq!(
            producer.trace_heap().retained_snapshot_count(),
            1,
            "staged snapshot stays retained until release/promotion"
        );

        let released = producer.release_staged(TraceCallKeyPrefix::exact(trace_key()));
        assert_eq!(released, 1);
        assert_eq!(producer.trace_heap().retained_snapshot_count(), 0);
    }

    #[test]
    fn zero_staging_budget_rejects_without_running_copy_closure() {
        let producer =
            TraceCaptureProducer::new(TraceCaptureConfig::enabled(4).with_staging_ring_bytes(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_copy = Arc::clone(&calls);
        let result = producer.stage_with(
            boundary_id(),
            trace_key(),
            CaptureKind::CallInput,
            move |_| {
                calls_for_copy.fetch_add(1, Ordering::Relaxed);
                TraceSnapshotHandle::for_test(1)
            },
        );
        assert_eq!(result, Err(CaptureSkipReason::QueueFull));
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        assert_eq!(producer.stats().skipped_queue_full, 1);
    }

    #[test]
    fn promotion_delivers_staged_drafts_with_promoted_marking() {
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled(4));
        producer
            .stage_with(
                boundary_id(),
                trace_key(),
                CaptureKind::CallInput,
                structured_test_snapshot,
            )
            .unwrap();

        let report =
            producer.promote_staged(TraceCallKeyPrefix::exact(trace_key()), "trigger:on_error");
        assert_eq!(report.promoted, 1);
        assert_eq!(report.staged_evicted, 0);
        assert_eq!(producer.staged_len(), 0);
        assert_eq!(producer.staged_bytes(), 0);

        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path(), [3; 16]).unwrap();
        let mut writer = ValueWriter::new(ByteValueArtifactSink::new(), boundary_id()).unwrap();
        let encoded = producer
            .drain_to_value_writer_with_store(&mut writer, Some(&mut store), 7)
            .unwrap();
        assert_eq!(encoded.len(), 1);
        assert_eq!(encoded[0].promoted_by.as_deref(), Some("trigger:on_error"));

        let parsed = read_bamlvalue_from_bytes(writer.sink().bytes()).unwrap();
        let ValueFileRecord::CapturedValue(record) = &parsed.records[0] else {
            panic!("expected value record");
        };
        assert_eq!(
            record.promoted_by.as_deref(),
            Some("trigger:on_error"),
            "promoted role + trigger id ride the .bamlvalue record"
        );
        let capture = record.capture.as_ref().expect("capture metadata kept");
        assert_eq!(capture.kind, bex_events::value::ValueCaptureKind::CallInput);
        assert!(record.dag_ref.is_some(), "promoted drains persist normally");
        assert_eq!(producer.trace_heap().retained_snapshot_count(), 0);
    }

    #[test]
    fn staging_ring_evicts_oldest_under_byte_pressure_and_reports_loss() {
        let unit = string_snapshot_bytes(64);
        let producer = TraceCaptureProducer::new(
            TraceCaptureConfig::enabled(8).with_staging_ring_bytes(2 * unit),
        );
        for call_id in 1..=3u64 {
            producer
                .stage_with(
                    boundary_id(),
                    trace_key_with(3, call_id),
                    CaptureKind::CallInput,
                    |trace_heap| string_snapshot(trace_heap, &"x".repeat(64)),
                )
                .unwrap();
        }

        // Third stage pushed the ring over budget: the OLDEST draft went.
        assert_eq!(producer.stats().staged, 3);
        assert_eq!(producer.stats().staged_evicted, 1);
        assert_eq!(producer.staged_len(), 2);
        assert_eq!(producer.staged_bytes(), 2 * unit);
        assert_eq!(
            producer.trace_heap().retained_snapshot_count(),
            2,
            "evicted snapshot is released immediately"
        );

        let report = producer.promote_staged(
            TraceCallKeyPrefix::thread(trace_key_with(3, 1).thread_key()),
            "trigger:t1",
        );
        assert_eq!(report.promoted, 2, "call 1 was evicted before the trigger");
        assert_eq!(report.staged_evicted, 1, "the miss is visible and tunable");

        let mut writer = ValueWriter::new(ByteValueArtifactSink::new(), boundary_id()).unwrap();
        let encoded = producer.drain_to_value_writer(&mut writer).unwrap();
        assert_eq!(encoded.len(), 2);
        assert_eq!(
            (encoded[0].call.call_id, encoded[1].call.call_id),
            (BexCallId(2), BexCallId(3)),
            "promotion preserves capture order; oldest was evicted"
        );

        let parsed = read_bamlvalue_from_bytes(writer.sink().bytes()).unwrap();
        assert_eq!(parsed.records.len(), 3);
        let ValueFileRecord::CaptureLoss(loss) = &parsed.records[2] else {
            panic!("expected trailing staging-eviction capture loss");
        };
        assert_eq!(
            loss.reason,
            bex_events::value::CaptureLossReason::StagingEvicted
        );
        assert_eq!(loss.skipped_count, 1);

        // Already-reported evictions do not repeat on the next drain.
        let mut writer = ValueWriter::new(ByteValueArtifactSink::new(), boundary_id()).unwrap();
        assert!(
            producer
                .drain_to_value_writer(&mut writer)
                .unwrap()
                .is_empty()
        );
        let parsed = read_bamlvalue_from_bytes(writer.sink().bytes()).unwrap();
        assert!(parsed.records.is_empty());
    }

    #[test]
    fn release_staged_frees_budget_by_prefix() {
        let unit = string_snapshot_bytes(32);
        let producer = TraceCaptureProducer::new(
            TraceCaptureConfig::enabled(8).with_staging_ring_bytes(8 * unit),
        );
        for (thread_id, call_id) in [(3, 1u64), (3, 2), (4, 3)] {
            producer
                .stage_with(
                    boundary_id(),
                    trace_key_with(thread_id, call_id),
                    CaptureKind::CallInput,
                    |trace_heap| string_snapshot(trace_heap, &"y".repeat(32)),
                )
                .unwrap();
        }
        assert_eq!(producer.staged_bytes(), 3 * unit);

        // Frame close on thread 3: its drafts drop, thread 4's stays.
        let released = producer.release_staged(TraceCallKeyPrefix::thread(
            trace_key_with(3, 1).thread_key(),
        ));
        assert_eq!(released, 2);
        assert_eq!(producer.staged_len(), 1);
        assert_eq!(producer.staged_bytes(), unit);
        assert_eq!(producer.stats().staged_released, 2);
        assert_eq!(producer.stats().staged_evicted, 0, "release is not loss");
        assert_eq!(producer.trace_heap().retained_snapshot_count(), 1);

        // The survivor still promotes.
        let report = producer.promote_staged(
            TraceCallKeyPrefix::engine(ProcessEuid([1; 16]), EngineId(2)),
            "trigger:t2",
        );
        assert_eq!(report.promoted, 1);
        let drafts = producer.drain();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].call, trace_key_with(4, 3));
        assert_eq!(drafts[0].promoted_by.as_deref(), Some("trigger:t2"));
        producer.trace_heap().release(drafts[0].snapshot);
    }

    #[test]
    fn drain_with_service_attaches_dag_refs_and_persists_via_service_thread() {
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled(2));
        producer
            .capture_with(
                boundary_id(),
                trace_key(),
                CaptureKind::RootOutput,
                structured_test_snapshot,
            )
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let service = ValueDrainService::open(dir.path(), [3; 16]).unwrap();
        let mut writer = ValueWriter::new(ByteValueArtifactSink::new(), boundary_id()).unwrap();
        let encoded = producer
            .drain_to_value_writer_with_service(&mut writer, &service, 7)
            .unwrap();
        assert_eq!(encoded.len(), 1);
        if cfg!(unix) {
            assert!(service.cpu_ns() > 0, "the service thread did the writes");
        }
        service.seal_and_stop().unwrap();

        let parsed = read_bamlvalue_from_bytes(writer.sink().bytes()).unwrap();
        let ValueFileRecord::CapturedValue(record) = &parsed.records[0] else {
            panic!("expected value record");
        };
        let dag_ref = record.dag_ref.expect("dag ref attached via the service");
        let expected = canon::encode(&canon::CanonValue::Map(vec![
            ("user".to_string(), canon::CanonValue::String("ada".into())),
            ("count".to_string(), canon::CanonValue::Int(42)),
        ]));
        assert_eq!(dag_ref.root_cid, expected.root_cid);
        assert_eq!(dag_ref.logical_len, expected.logical_len);

        // The store the service owned serves the DAG after shutdown.
        let store = Store::open(dir.path(), [3; 16]).unwrap();
        assert!(store.get(&dag_ref.root_cid).unwrap().is_some());
        assert_eq!(producer.trace_heap().retained_snapshot_count(), 0);
    }
}
