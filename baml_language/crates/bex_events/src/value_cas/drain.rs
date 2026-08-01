//! Native continuous drain for owned captured values.
//!
//! Producers hand this service fully-owned semantic values. The worker never
//! receives a VM heap handle and is therefore free to canonicalize, hash, and
//! perform durability work without parking or consulting the runtime heap.

#![allow(unsafe_code)]

use std::{
    collections::{HashMap, VecDeque},
    fs::{self, OpenOptions},
    io::{self, Write as _},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::{
    ids::{BoundaryId, ProcessEuid},
    run::TraceCallKey,
    value::{
        CaptureLossKind, CaptureLossReason, CaptureLossRecord, CapturePolicyChangedRecord, DagRef,
        LogEventRecord, LogRecord, PromotionOccurredRecord, RunStartedRecord, ValueAuditRecord,
        ValueAvailability, ValueCapture, ValueCaptureKind, ValueCodec, ValueRecord, ValueRef,
        encode::{
            encode_audit, encode_capture_loss, encode_header, encode_log_event, encode_record,
            encode_run_started,
        },
    },
};

use super::{
    CallPath, CanonicalField, CanonicalValue, CidManifestWriter, FieldPresence, MediaContent,
    PackWriter, RootCommitBatch, RootCommitter, TriggerId, encode_value_dag,
};

const DEFAULT_PENDING_BYTES: usize = 32 * 1024 * 1024;
const DEFAULT_QUEUE_CAPACITY: usize = 1024;
const DEFAULT_STAGING_TOMBSTONES: usize = 4096;

#[derive(Clone, Debug)]
pub struct ValueDrainConfig {
    pub project_baml_dir: PathBuf,
    pub process_euid: ProcessEuid,
    pub pending_byte_budget: usize,
    pub queue_capacity: usize,
    pub high_water_bytes: usize,
    pub staging_byte_budget: usize,
    pub staging_tombstone_capacity: usize,
    /// Optional value-plane stats destination. When omitted,
    /// `BAML_OBS_STATS` is interpreted as a stem and `.values.json` is used.
    pub stats_path: Option<PathBuf>,
}

impl ValueDrainConfig {
    #[must_use]
    pub fn new(project_baml_dir: impl Into<PathBuf>, process_euid: ProcessEuid) -> Self {
        let pending_byte_budget = DEFAULT_PENDING_BYTES;
        Self {
            project_baml_dir: project_baml_dir.into(),
            process_euid,
            pending_byte_budget,
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
            high_water_bytes: pending_byte_budget / 2,
            staging_byte_budget: super::DEFAULT_NATIVE_STAGING_BYTES,
            staging_tombstone_capacity: DEFAULT_STAGING_TOMBSTONES,
            stats_path: value_stats_path_from_env(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ValueBoundaryRegistration {
    pub boundary_id: BoundaryId,
    pub boundary_dir: PathBuf,
    pub created_ms: u64,
    /// Written lazily immediately before the boundary's first drain record.
    pub run_started: Option<RunStartedRecord>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DurableValueCapture {
    pub boundary_id: BoundaryId,
    pub call: TraceCallKey,
    pub kind: ValueCaptureKind,
    pub log_event: Option<LogEventRecord>,
    pub value: CanonicalValue,
    pub promoted_by: Option<TriggerId>,
}

impl DurableValueCapture {
    #[must_use]
    pub fn estimated_retained_bytes(&self) -> usize {
        estimate_canonical_bytes(&self.value).saturating_add(128)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueEnqueueOutcome {
    Enqueued,
    DroppedPendingBudget,
    DroppedQueueFull,
    ServiceClosed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ValueStageOutcome {
    pub retained: bool,
    pub evicted_records: usize,
    pub evicted_bytes: usize,
    pub value_too_large: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ValuePromotionOutcome {
    pub promoted_records: usize,
    pub queued_records: usize,
    pub staged_evicted: u64,
    pub losses_recorded: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ValueDrainStatsSnapshot {
    pub enqueued: u64,
    pub dropped_pending_budget: u64,
    pub dropped_queue_full: u64,
    pub captures_committed: u64,
    pub roots_committed: u64,
    pub chunks_considered: u64,
    pub chunks_appended: u64,
    pub value_record_bytes: u64,
    pub capture_losses: u64,
    pub promotions: u64,
    pub staging_evictions: u64,
    pub flushes: u64,
    pub fsync_barriers: u64,
    pub worker_failures: u64,
    pub high_water_wakes: u64,
    pub pending_bytes: usize,
    pub staging_bytes: usize,
}

#[derive(Debug, Default)]
struct ValueDrainStats {
    enqueued: AtomicU64,
    dropped_pending_budget: AtomicU64,
    dropped_queue_full: AtomicU64,
    captures_committed: AtomicU64,
    roots_committed: AtomicU64,
    chunks_considered: AtomicU64,
    chunks_appended: AtomicU64,
    value_record_bytes: AtomicU64,
    capture_losses: AtomicU64,
    promotions: AtomicU64,
    staging_evictions: AtomicU64,
    flushes: AtomicU64,
    fsync_barriers: AtomicU64,
    worker_failures: AtomicU64,
    high_water_wakes: AtomicU64,
}

#[derive(Debug)]
struct Shared {
    sender: SyncSender<Command>,
    pending_bytes: AtomicUsize,
    pending_byte_budget: usize,
    high_water_bytes: usize,
    overflow_losses: Mutex<HashMap<BoundaryId, PendingLoss>>,
    staging: Mutex<ValueStaging>,
    stats: ValueDrainStats,
    closed: AtomicBool,
}

#[derive(Clone, Debug)]
pub struct ValueDrainHandle {
    shared: Arc<Shared>,
}

#[derive(Debug)]
pub struct ValueDrainService {
    handle: ValueDrainHandle,
    worker: Option<JoinHandle<io::Result<()>>>,
}

#[derive(Debug)]
enum Command {
    Register {
        registration: ValueBoundaryRegistration,
        reply: mpsc::Sender<io::Result<()>>,
    },
    Capture {
        capture: DurableValueCapture,
        reserved_bytes: usize,
    },
    Loss {
        boundary_id: BoundaryId,
        record: CaptureLossRecord,
    },
    Audit {
        boundary_id: BoundaryId,
        record: ValueAuditRecord,
    },
    Flush {
        reply: mpsc::Sender<io::Result<()>>,
    },
    Finish {
        boundary_id: BoundaryId,
        reply: mpsc::Sender<io::Result<()>>,
    },
    Shutdown {
        reply: mpsc::Sender<io::Result<()>>,
    },
}

#[derive(Clone, Debug)]
struct PendingLoss {
    skipped_count: u64,
    skipped_bytes: usize,
}

#[derive(Debug)]
struct StagedCapture {
    call: CallPath,
    retained_bytes: usize,
    capture: DurableValueCapture,
}

#[derive(Clone, Debug)]
struct EvictedCapture {
    call: CallPath,
    retained_bytes: usize,
    reason: CaptureLossReason,
}

#[derive(Debug)]
struct ValueStaging {
    max_bytes: usize,
    max_tombstones: usize,
    current_bytes: usize,
    captures: VecDeque<StagedCapture>,
    evicted: VecDeque<EvictedCapture>,
    unattributed_evictions: u64,
}

impl ValueDrainService {
    pub fn start(config: ValueDrainConfig) -> io::Result<Self> {
        if config.queue_capacity == 0 || config.pending_byte_budget == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "value drain queue and pending-byte budgets must be nonzero",
            ));
        }
        fs::create_dir_all(config.project_baml_dir.join("store"))?;
        let (sender, receiver) = mpsc::sync_channel(config.queue_capacity);
        let shared = Arc::new(Shared {
            sender,
            pending_bytes: AtomicUsize::new(0),
            pending_byte_budget: config.pending_byte_budget,
            high_water_bytes: config
                .high_water_bytes
                .min(config.pending_byte_budget)
                .max(1),
            overflow_losses: Mutex::new(HashMap::new()),
            staging: Mutex::new(ValueStaging {
                max_bytes: config.staging_byte_budget,
                max_tombstones: config.staging_tombstone_capacity,
                current_bytes: 0,
                captures: VecDeque::new(),
                evicted: VecDeque::new(),
                unattributed_evictions: 0,
            }),
            stats: ValueDrainStats::default(),
            closed: AtomicBool::new(false),
        });
        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("baml-value-drain".to_string())
            .spawn(move || Worker::new(config, worker_shared).run(receiver))?;
        Ok(Self {
            handle: ValueDrainHandle { shared },
            worker: Some(worker),
        })
    }

    #[must_use]
    pub fn handle(&self) -> ValueDrainHandle {
        self.handle.clone()
    }

    /// Final process barrier. Every command sent before this call has either
    /// reached its D1 root commit or produced an explicit persisted loss.
    pub fn shutdown(mut self) -> io::Result<()> {
        self.shutdown_inner()
    }

    fn shutdown_inner(&mut self) -> io::Result<()> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        self.handle.shared.closed.store(true, Ordering::Release);
        let (reply, receive) = mpsc::channel();
        let send_result = self.handle.shared.sender.send(Command::Shutdown { reply });
        let command_result = match send_result {
            Ok(()) => receive.recv().map_err(io::Error::other)?,
            Err(_) => Ok(()),
        };
        let worker_result = worker
            .join()
            .map_err(|_| io::Error::other("value drain worker panicked"))?;
        command_result.and(worker_result)
    }
}

impl Drop for ValueDrainService {
    fn drop(&mut self) {
        let _ = self.shutdown_inner();
    }
}

impl ValueDrainHandle {
    pub fn register_boundary(&self, registration: ValueBoundaryRegistration) -> io::Result<()> {
        self.request(|reply| Command::Register {
            registration,
            reply,
        })
    }

    #[must_use]
    pub fn try_enqueue(&self, capture: DurableValueCapture) -> ValueEnqueueOutcome {
        if self.shared.closed.load(Ordering::Acquire) {
            return ValueEnqueueOutcome::ServiceClosed;
        }
        let retained_bytes = capture.estimated_retained_bytes();
        if !reserve_pending_bytes(
            &self.shared.pending_bytes,
            self.shared.pending_byte_budget,
            retained_bytes,
        ) {
            self.shared
                .stats
                .dropped_pending_budget
                .fetch_add(1, Ordering::Relaxed);
            self.record_overflow_loss(&capture, retained_bytes);
            return ValueEnqueueOutcome::DroppedPendingBudget;
        }
        let pending = self.shared.pending_bytes.load(Ordering::Relaxed);
        if pending >= self.shared.high_water_bytes {
            self.shared
                .stats
                .high_water_wakes
                .fetch_add(1, Ordering::Relaxed);
        }
        match self.shared.sender.try_send(Command::Capture {
            capture,
            reserved_bytes: retained_bytes,
        }) {
            Ok(()) => {
                self.shared.stats.enqueued.fetch_add(1, Ordering::Relaxed);
                ValueEnqueueOutcome::Enqueued
            }
            Err(TrySendError::Full(Command::Capture {
                capture,
                reserved_bytes,
            })) => {
                self.shared
                    .pending_bytes
                    .fetch_sub(reserved_bytes, Ordering::AcqRel);
                self.shared
                    .stats
                    .dropped_queue_full
                    .fetch_add(1, Ordering::Relaxed);
                self.record_overflow_loss(&capture, reserved_bytes);
                ValueEnqueueOutcome::DroppedQueueFull
            }
            Err(TrySendError::Disconnected(Command::Capture { reserved_bytes, .. })) => {
                self.shared
                    .pending_bytes
                    .fetch_sub(reserved_bytes, Ordering::AcqRel);
                ValueEnqueueOutcome::ServiceClosed
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                unreachable!("try_enqueue only sends Capture")
            }
        }
    }

    /// Reserve staging capacity before invoking `copy`. Normal staging does no
    /// canonical encoding, hashing, or I/O.
    pub fn stage_with(
        &self,
        call: CallPath,
        retained_bytes: usize,
        copy: impl FnOnce() -> DurableValueCapture,
    ) -> ValueStageOutcome {
        let mut staging = self
            .shared
            .staging
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if retained_bytes > staging.max_bytes {
            staging.record_tombstone(EvictedCapture {
                call,
                retained_bytes,
                reason: CaptureLossReason::StagingValueTooLarge,
            });
            return ValueStageOutcome {
                value_too_large: true,
                ..ValueStageOutcome::default()
            };
        }
        let mut evicted_records = 0;
        let mut evicted_bytes = 0_usize;
        while staging.current_bytes.saturating_add(retained_bytes) > staging.max_bytes {
            let Some(evicted) = staging.captures.pop_front() else {
                break;
            };
            staging.current_bytes = staging.current_bytes.saturating_sub(evicted.retained_bytes);
            evicted_records += 1;
            evicted_bytes = evicted_bytes.saturating_add(evicted.retained_bytes);
            staging.evicted.push_back(EvictedCapture {
                call: evicted.call,
                retained_bytes: evicted.retained_bytes,
                reason: CaptureLossReason::StagingEvicted,
            });
        }
        while staging.evicted.len() > staging.max_tombstones {
            staging.evicted.pop_front();
            staging.unattributed_evictions = staging.unattributed_evictions.saturating_add(1);
        }
        let capture = copy();
        if capture.boundary_id != call.boundary_id {
            staging.record_tombstone(EvictedCapture {
                call,
                retained_bytes,
                reason: CaptureLossReason::StagingValueTooLarge,
            });
            return ValueStageOutcome {
                evicted_records,
                evicted_bytes,
                value_too_large: true,
                ..ValueStageOutcome::default()
            };
        }
        staging.current_bytes = staging.current_bytes.saturating_add(retained_bytes);
        staging.captures.push_back(StagedCapture {
            call,
            retained_bytes,
            capture,
        });
        self.shared
            .stats
            .staging_evictions
            .fetch_add(evicted_records as u64, Ordering::Relaxed);
        ValueStageOutcome {
            retained: true,
            evicted_records,
            evicted_bytes,
            value_too_large: false,
        }
    }

    pub fn release_staged(&self, scope: &CallPath) -> usize {
        let mut staging = self
            .shared
            .staging
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut released = 0;
        let mut retained = VecDeque::with_capacity(staging.captures.len());
        while let Some(capture) = staging.captures.pop_front() {
            if scope.contains(&capture.call) {
                staging.current_bytes =
                    staging.current_bytes.saturating_sub(capture.retained_bytes);
                released += 1;
            } else {
                retained.push_back(capture);
            }
        }
        staging.captures = retained;
        staging
            .evicted
            .retain(|capture| !scope.contains(&capture.call));
        released
    }

    pub fn promote_staged(
        &self,
        scope: &CallPath,
        trigger: TriggerId,
        timestamp_ms: u64,
    ) -> io::Result<ValuePromotionOutcome> {
        let (mut promoted, losses, unattributed) = {
            let mut staging = self
                .shared
                .staging
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut promoted = Vec::new();
            let mut retained = VecDeque::with_capacity(staging.captures.len());
            while let Some(capture) = staging.captures.pop_front() {
                if scope.contains(&capture.call) {
                    staging.current_bytes =
                        staging.current_bytes.saturating_sub(capture.retained_bytes);
                    promoted.push(capture.capture);
                } else {
                    retained.push_back(capture);
                }
            }
            staging.captures = retained;
            let mut losses = Vec::new();
            let mut unrelated = VecDeque::with_capacity(staging.evicted.len());
            while let Some(evicted) = staging.evicted.pop_front() {
                if scope.contains(&evicted.call) {
                    losses.push(evicted);
                } else {
                    unrelated.push_back(evicted);
                }
            }
            staging.evicted = unrelated;
            let unattributed = std::mem::take(&mut staging.unattributed_evictions);
            (promoted, losses, unattributed)
        };

        let promoted_records = promoted.len();
        let mut queued_records = 0;
        for capture in &mut promoted {
            capture.promoted_by = Some(trigger.clone());
        }
        for capture in promoted {
            queued_records +=
                usize::from(self.try_enqueue(capture) == ValueEnqueueOutcome::Enqueued);
        }

        for loss in &losses {
            self.send_loss(
                scope.boundary_id,
                CaptureLossRecord {
                    kind: CaptureLossKind::Value,
                    reason: loss.reason,
                    skipped_count: 1,
                    call: None,
                    message: Some(format!(
                        "trigger {trigger} requested a {}-byte staged value unavailable under {}",
                        loss.retained_bytes,
                        display_call_path(&loss.call),
                    )),
                    timestamp_ms,
                },
            )?;
        }
        if unattributed > 0 {
            self.send_loss(
                scope.boundary_id,
                CaptureLossRecord {
                    kind: CaptureLossKind::Value,
                    reason: CaptureLossReason::EvictionHistoryOverflow,
                    skipped_count: unattributed,
                    call: None,
                    message: Some(format!(
                        "trigger {trigger} crossed bounded staging eviction history"
                    )),
                    timestamp_ms,
                },
            )?;
        }
        let staged_evicted = (losses.len() as u64).saturating_add(unattributed);
        self.send_audit(
            scope.boundary_id,
            ValueAuditRecord::PromotionOccurred(PromotionOccurredRecord {
                trigger: trigger.0,
                scope: display_call_path(scope),
                records: promoted_records as u64,
                staged_evicted,
                timestamp_ms,
            }),
        )?;
        self.shared.stats.promotions.fetch_add(1, Ordering::Relaxed);
        Ok(ValuePromotionOutcome {
            promoted_records,
            queued_records,
            staged_evicted,
            losses_recorded: losses.len() + usize::from(unattributed > 0),
        })
    }

    pub fn capture_policy_changed(
        &self,
        boundary_id: BoundaryId,
        record: CapturePolicyChangedRecord,
    ) -> io::Result<()> {
        self.send_audit(boundary_id, ValueAuditRecord::CapturePolicyChanged(record))
    }

    /// Persist an explicit producer/adapter loss through the same ordered
    /// value stream. This is intentionally cold-path and may wait for queue
    /// capacity so loss reporting itself is not silently dropped.
    pub fn record_capture_loss(
        &self,
        boundary_id: BoundaryId,
        record: CaptureLossRecord,
    ) -> io::Result<()> {
        self.send_loss(boundary_id, record)
    }

    pub fn flush(&self) -> io::Result<()> {
        self.request(|reply| Command::Flush { reply })
    }

    /// Boundary completion barrier. Call only after its producers have
    /// stopped; this seals the root manifest and active pack index.
    pub fn finish_boundary(&self, boundary_id: BoundaryId) -> io::Result<()> {
        self.request(|reply| Command::Finish { boundary_id, reply })
    }

    #[must_use]
    pub fn stats(&self) -> ValueDrainStatsSnapshot {
        self.shared.stats.snapshot(
            self.shared.pending_bytes.load(Ordering::Relaxed),
            self.shared
                .staging
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .current_bytes,
        )
    }

    fn request(
        &self,
        command: impl FnOnce(mpsc::Sender<io::Result<()>>) -> Command,
    ) -> io::Result<()> {
        if self.shared.closed.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "value drain service is closed",
            ));
        }
        let (reply, receive) = mpsc::channel();
        self.shared
            .sender
            .send(command(reply))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "value drain worker stopped"))?;
        receive
            .recv()
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "value drain reply dropped"))?
    }

    fn send_loss(&self, boundary_id: BoundaryId, record: CaptureLossRecord) -> io::Result<()> {
        self.shared
            .sender
            .send(Command::Loss {
                boundary_id,
                record,
            })
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "value drain worker stopped"))
    }

    fn send_audit(&self, boundary_id: BoundaryId, record: ValueAuditRecord) -> io::Result<()> {
        self.shared
            .sender
            .send(Command::Audit {
                boundary_id,
                record,
            })
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "value drain worker stopped"))
    }

    fn record_overflow_loss(&self, capture: &DurableValueCapture, skipped_bytes: usize) {
        let mut losses = self
            .shared
            .overflow_losses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let loss = losses.entry(capture.boundary_id).or_insert(PendingLoss {
            skipped_count: 0,
            skipped_bytes: 0,
        });
        loss.skipped_count = loss.skipped_count.saturating_add(1);
        loss.skipped_bytes = loss.skipped_bytes.saturating_add(skipped_bytes);
    }
}

impl ValueStaging {
    fn record_tombstone(&mut self, tombstone: EvictedCapture) {
        self.evicted.push_back(tombstone);
        while self.evicted.len() > self.max_tombstones {
            self.evicted.pop_front();
            self.unattributed_evictions = self.unattributed_evictions.saturating_add(1);
        }
    }
}

impl ValueDrainStats {
    fn snapshot(&self, pending_bytes: usize, staging_bytes: usize) -> ValueDrainStatsSnapshot {
        ValueDrainStatsSnapshot {
            enqueued: self.enqueued.load(Ordering::Relaxed),
            dropped_pending_budget: self.dropped_pending_budget.load(Ordering::Relaxed),
            dropped_queue_full: self.dropped_queue_full.load(Ordering::Relaxed),
            captures_committed: self.captures_committed.load(Ordering::Relaxed),
            roots_committed: self.roots_committed.load(Ordering::Relaxed),
            chunks_considered: self.chunks_considered.load(Ordering::Relaxed),
            chunks_appended: self.chunks_appended.load(Ordering::Relaxed),
            value_record_bytes: self.value_record_bytes.load(Ordering::Relaxed),
            capture_losses: self.capture_losses.load(Ordering::Relaxed),
            promotions: self.promotions.load(Ordering::Relaxed),
            staging_evictions: self.staging_evictions.load(Ordering::Relaxed),
            flushes: self.flushes.load(Ordering::Relaxed),
            fsync_barriers: self.fsync_barriers.load(Ordering::Relaxed),
            worker_failures: self.worker_failures.load(Ordering::Relaxed),
            high_water_wakes: self.high_water_wakes.load(Ordering::Relaxed),
            pending_bytes,
            staging_bytes,
        }
    }
}

#[derive(Debug)]
struct BoundaryState {
    committer: Option<RootCommitter>,
    run_started: Option<Vec<u8>>,
    started: bool,
    next_value_id: u64,
}

impl BoundaryState {
    fn ensure_started(&mut self) -> io::Result<()> {
        if self.started {
            return Ok(());
        }
        if let Some(record) = self.run_started.take() {
            self.committer_mut()?.append_audit_record(&record)?;
        }
        self.started = true;
        Ok(())
    }

    fn committer_mut(&mut self) -> io::Result<&mut RootCommitter> {
        self.committer
            .as_mut()
            .ok_or_else(|| io::Error::other("boundary value committer was already sealed"))
    }

    fn allocate_value_id(&mut self) -> String {
        let value_id = format!("bamlv_1_{}", self.next_value_id);
        self.next_value_id = self.next_value_id.saturating_add(1);
        value_id
    }
}

#[derive(Debug)]
struct Worker {
    config: ValueDrainConfig,
    shared: Arc<Shared>,
    boundaries: HashMap<BoundaryId, BoundaryState>,
    next_pack_seq: u32,
    started: Instant,
    started_cpu_ns: Option<u64>,
    last_error: Option<String>,
}

impl Worker {
    fn new(config: ValueDrainConfig, shared: Arc<Shared>) -> Self {
        Self {
            config,
            shared,
            boundaries: HashMap::new(),
            next_pack_seq: 1,
            started: Instant::now(),
            started_cpu_ns: thread_cpu_time_ns(),
            last_error: None,
        }
    }

    fn run(mut self, receiver: Receiver<Command>) -> io::Result<()> {
        while let Ok(command) = receiver.recv() {
            match command {
                Command::Register {
                    registration,
                    reply,
                } => {
                    let result = self.register(registration);
                    let _ = reply.send(clone_io_result(&result));
                    self.note_result(result);
                }
                Command::Capture {
                    capture,
                    reserved_bytes,
                } => {
                    self.shared
                        .pending_bytes
                        .fetch_sub(reserved_bytes, Ordering::AcqRel);
                    self.flush_overflow_for(capture.boundary_id);
                    let result = self.capture(capture);
                    self.note_result(result);
                }
                Command::Loss {
                    boundary_id,
                    record,
                } => {
                    self.flush_overflow_for(boundary_id);
                    let result = self.write_loss(boundary_id, record);
                    self.note_result(result);
                }
                Command::Audit {
                    boundary_id,
                    record,
                } => {
                    self.flush_overflow_for(boundary_id);
                    let result = self.write_audit(boundary_id, record);
                    self.note_result(result);
                }
                Command::Flush { reply } => {
                    self.flush_all_overflow();
                    self.shared.stats.flushes.fetch_add(1, Ordering::Relaxed);
                    self.shared
                        .stats
                        .fsync_barriers
                        .fetch_add(1, Ordering::Relaxed);
                    let result = self.last_error_result();
                    let _ = reply.send(clone_io_result(&result));
                }
                Command::Finish { boundary_id, reply } => {
                    self.flush_overflow_for(boundary_id);
                    let result = self.finish(boundary_id);
                    let _ = reply.send(clone_io_result(&result));
                    self.note_result(result);
                }
                Command::Shutdown { reply } => {
                    self.flush_all_overflow();
                    let result = self.finish_all();
                    let combined = result.and_then(|()| self.last_error_result());
                    let _ = self.write_stats("shutdown");
                    let _ = reply.send(clone_io_result(&combined));
                    return combined;
                }
            }
        }
        self.flush_all_overflow();
        let result = self.finish_all().and_then(|()| self.last_error_result());
        let _ = self.write_stats("channel_disconnected");
        result
    }

    fn register(&mut self, registration: ValueBoundaryRegistration) -> io::Result<()> {
        if self.boundaries.contains_key(&registration.boundary_id) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "value boundary is already registered",
            ));
        }
        fs::create_dir_all(&registration.boundary_dir)?;
        let value_path = registration.boundary_dir.join("values.bamlvalue");
        let mut header = Vec::new();
        encode_header(&mut header, registration.boundary_id)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let mut value_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&value_path)?;
        value_file.write_all(&header)?;
        value_file.sync_data()?;
        drop(value_file);

        let manifest = CidManifestWriter::create(
            registration.boundary_dir.join("manifest.bamlcids"),
            registration.boundary_id,
        )?;
        let pack = PackWriter::create(
            self.config.project_baml_dir.join("store"),
            self.config.process_euid.0,
            self.next_pack_seq,
            registration.created_ms,
        )?;
        self.next_pack_seq = self.next_pack_seq.saturating_add(1);
        let committer = RootCommitter::new(pack, manifest, &value_path)?;
        let run_started = registration
            .run_started
            .as_ref()
            .map(|record| {
                let mut bytes = Vec::new();
                encode_run_started(&mut bytes, record)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                Ok::<Vec<u8>, io::Error>(bytes)
            })
            .transpose()?;
        self.boundaries.insert(
            registration.boundary_id,
            BoundaryState {
                committer: Some(committer),
                run_started,
                started: false,
                next_value_id: 1,
            },
        );
        Ok(())
    }

    fn capture(&mut self, capture: DurableValueCapture) -> io::Result<()> {
        let Some(state) = self.boundaries.get_mut(&capture.boundary_id) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "capture references an unregistered value boundary",
            ));
        };
        state.ensure_started()?;
        let dag = match encode_value_dag(&capture.value) {
            Ok(dag) => dag,
            Err(error) => {
                return self.write_loss(
                    capture.boundary_id,
                    CaptureLossRecord {
                        kind: if capture.kind == ValueCaptureKind::LogBody {
                            CaptureLossKind::Log
                        } else {
                            CaptureLossKind::Value
                        },
                        reason: CaptureLossReason::EncodeFailed,
                        skipped_count: 1,
                        call: Some(capture.call),
                        message: Some(error.to_string()),
                        timestamp_ms: now_ms(),
                    },
                );
            }
        };
        let value_id = state.allocate_value_id();
        let retained_size = dag
            .chunks
            .iter()
            .map(|chunk| chunk.canonical_bytes.len())
            .fold(0_usize, usize::saturating_add);
        let logical_size = usize::try_from(dag.logical_len).unwrap_or(usize::MAX);
        let value_ref = ValueRef {
            id: value_id,
            codec: ValueCodec::BamlOutboundValue,
            availability: ValueAvailability::Available,
            original_size_bytes: Some(logical_size),
            retained_size_bytes: Some(retained_size),
            diagnostic: None,
        };
        let dag_ref = DagRef {
            root_cid: *dag.root.as_bytes(),
            node_codec_version: dag.node_codec_version,
            logical_len: dag.logical_len,
        };
        let mut framed_record = Vec::new();
        if let Some(log_event) = capture.log_event {
            encode_log_event(
                &mut framed_record,
                &LogRecord {
                    value_ref,
                    body: Vec::new(),
                    blob_ref: None,
                    dag_ref: Some(dag_ref),
                    event: log_event,
                },
            )
        } else {
            encode_record(
                &mut framed_record,
                &ValueRecord {
                    value_ref,
                    body: Vec::new(),
                    blob_ref: None,
                    dag_ref: Some(dag_ref),
                    capture: Some(ValueCapture {
                        kind: capture.kind,
                        call: capture.call,
                        promotion_trigger: capture.promoted_by.map(|trigger| trigger.0),
                    }),
                },
            )
        }
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

        match state.committer_mut()?.commit(RootCommitBatch {
            dags: vec![dag],
            value_records: vec![framed_record],
        }) {
            Ok(outcome) => {
                self.shared
                    .stats
                    .captures_committed
                    .fetch_add(1, Ordering::Relaxed);
                self.shared
                    .stats
                    .roots_committed
                    .fetch_add(outcome.roots_committed as u64, Ordering::Relaxed);
                self.shared
                    .stats
                    .chunks_considered
                    .fetch_add(outcome.chunks_considered as u64, Ordering::Relaxed);
                self.shared
                    .stats
                    .chunks_appended
                    .fetch_add(outcome.chunks_appended as u64, Ordering::Relaxed);
                self.shared
                    .stats
                    .value_record_bytes
                    .fetch_add(outcome.value_record_bytes as u64, Ordering::Relaxed);
                self.shared
                    .stats
                    .fsync_barriers
                    .fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(error) => {
                let loss = CaptureLossRecord {
                    kind: CaptureLossKind::Value,
                    reason: CaptureLossReason::CommitFailed,
                    skipped_count: 1,
                    call: Some(capture.call),
                    message: Some(error.to_string()),
                    timestamp_ms: now_ms(),
                };
                let _ = append_loss_to_state(state, &loss);
                Err(error)
            }
        }
    }

    fn write_loss(&mut self, boundary_id: BoundaryId, record: CaptureLossRecord) -> io::Result<()> {
        let state = self.boundaries.get_mut(&boundary_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "capture loss references an unregistered value boundary",
            )
        })?;
        state.ensure_started()?;
        append_loss_to_state(state, &record)?;
        self.shared
            .stats
            .capture_losses
            .fetch_add(record.skipped_count, Ordering::Relaxed);
        Ok(())
    }

    fn write_audit(&mut self, boundary_id: BoundaryId, record: ValueAuditRecord) -> io::Result<()> {
        let state = self.boundaries.get_mut(&boundary_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "audit record references an unregistered value boundary",
            )
        })?;
        state.ensure_started()?;
        let mut framed = Vec::new();
        encode_audit(&mut framed, &record)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        state.committer_mut()?.append_audit_record(&framed)
    }

    fn flush_overflow_for(&mut self, boundary_id: BoundaryId) {
        let loss = self
            .shared
            .overflow_losses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&boundary_id);
        if let Some(loss) = loss {
            let result = self.write_loss(
                boundary_id,
                CaptureLossRecord {
                    kind: CaptureLossKind::Value,
                    reason: CaptureLossReason::DrainQueueFull,
                    skipped_count: loss.skipped_count,
                    call: None,
                    message: Some(format!(
                        "{} bytes were rejected by the bounded value drain flow-control window",
                        loss.skipped_bytes
                    )),
                    timestamp_ms: now_ms(),
                },
            );
            self.note_result(result);
        }
    }

    fn flush_all_overflow(&mut self) {
        let boundaries = self
            .shared
            .overflow_losses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for boundary_id in boundaries {
            self.flush_overflow_for(boundary_id);
        }
    }

    fn finish(&mut self, boundary_id: BoundaryId) -> io::Result<()> {
        let mut state = self.boundaries.remove(&boundary_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "cannot finish an unregistered value boundary",
            )
        })?;
        state.ensure_started()?;
        let committer = state
            .committer
            .take()
            .ok_or_else(|| io::Error::other("boundary value committer was already sealed"))?;
        let (pack, manifest, value_file) = committer.into_parts();
        value_file.sync_data()?;
        manifest.seal()?;
        pack.seal()?;
        self.shared.stats.flushes.fetch_add(1, Ordering::Relaxed);
        self.shared
            .stats
            .fsync_barriers
            .fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn finish_all(&mut self) -> io::Result<()> {
        let boundaries = self.boundaries.keys().copied().collect::<Vec<_>>();
        let mut first_error = None;
        for boundary_id in boundaries {
            if let Err(error) = self.finish(boundary_id)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn note_result(&mut self, result: io::Result<()>) {
        if let Err(error) = result {
            self.shared
                .stats
                .worker_failures
                .fetch_add(1, Ordering::Relaxed);
            if self.last_error.is_none() {
                self.last_error = Some(error.to_string());
            }
        }
    }

    fn last_error_result(&self) -> io::Result<()> {
        self.last_error.as_ref().map_or(Ok(()), |error| {
            Err(io::Error::other(format!(
                "value drain previously failed: {error}"
            )))
        })
    }

    fn write_stats(&self, reason: &str) -> io::Result<()> {
        let Some(path) = &self.config.stats_path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let snapshot = self.shared.stats.snapshot(
            self.shared.pending_bytes.load(Ordering::Relaxed),
            self.shared
                .staging
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .current_bytes,
        );
        let cpu_ns = thread_cpu_time_ns().and_then(|now| {
            self.started_cpu_ns
                .map(|started| now.saturating_sub(started))
        });
        let value = serde_json::json!({
            "schema_version": 1,
            "kind": "baml_observability_value_drain_stats",
            "pid": std::process::id(),
            "snapshot_reason": reason,
            "wall_time_ns": u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX),
            "consumer_cpu_ns": cpu_ns,
            "enqueued": snapshot.enqueued,
            "dropped_pending_budget": snapshot.dropped_pending_budget,
            "dropped_queue_full": snapshot.dropped_queue_full,
            "captures_committed": snapshot.captures_committed,
            "roots_committed": snapshot.roots_committed,
            "chunks_considered": snapshot.chunks_considered,
            "chunks_appended": snapshot.chunks_appended,
            "value_record_bytes": snapshot.value_record_bytes,
            "capture_losses": snapshot.capture_losses,
            "promotions": snapshot.promotions,
            "staging_evictions": snapshot.staging_evictions,
            "flushes": snapshot.flushes,
            "fsync_barriers": snapshot.fsync_barriers,
            "worker_failures": snapshot.worker_failures,
            "high_water_wakes": snapshot.high_water_wakes,
            "pending_bytes": snapshot.pending_bytes,
            "staging_bytes": snapshot.staging_bytes,
        });
        let mut bytes = serde_json::to_vec(&value).map_err(io::Error::other)?;
        bytes.push(b'\n');
        fs::write(path, bytes)
    }
}

fn append_loss_to_state(state: &mut BoundaryState, record: &CaptureLossRecord) -> io::Result<()> {
    let mut framed = Vec::new();
    encode_capture_loss(&mut framed, record)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    state.committer_mut()?.append_audit_record(&framed)
}

fn reserve_pending_bytes(pending: &AtomicUsize, budget: usize, requested: usize) -> bool {
    let mut current = pending.load(Ordering::Relaxed);
    loop {
        let Some(next) = current.checked_add(requested) else {
            return false;
        };
        if next > budget {
            return false;
        }
        match pending.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Relaxed) {
            Ok(_) => return true,
            Err(actual) => current = actual,
        }
    }
}

fn estimate_canonical_bytes(value: &CanonicalValue) -> usize {
    match value {
        CanonicalValue::Null | CanonicalValue::Bool(_) | CanonicalValue::Int(_) => 16,
        CanonicalValue::Float(_) => 16,
        CanonicalValue::BigInt(value) | CanonicalValue::String(value) => {
            value.len().saturating_add(24)
        }
        CanonicalValue::Bytes(value) => value.len().saturating_add(24),
        CanonicalValue::List(values) => values.iter().fold(24, |total, value| {
            total.saturating_add(estimate_canonical_bytes(value))
        }),
        CanonicalValue::Map(entries) => entries.iter().fold(24, |total, (key, value)| {
            total
                .saturating_add(key.len())
                .saturating_add(estimate_canonical_bytes(value))
        }),
        CanonicalValue::Class {
            definition_key,
            fields,
        } => fields.iter().fold(
            definition_key.len().saturating_add(24),
            |total, CanonicalField { name, presence }| {
                total
                    .saturating_add(name.len())
                    .saturating_add(match presence {
                        FieldPresence::Absent => 1,
                        FieldPresence::Present(value) | FieldPresence::DefaultFilled(value) => {
                            estimate_canonical_bytes(value)
                        }
                    })
            },
        ),
        CanonicalValue::Enum {
            definition_key,
            variant,
        } => definition_key
            .len()
            .saturating_add(variant.len())
            .saturating_add(24),
        CanonicalValue::Media(media) => {
            let content = match &media.content {
                MediaContent::Url(value)
                | MediaContent::Base64(value)
                | MediaContent::File(value) => value.len(),
                MediaContent::Bytes(value) => value.len(),
            };
            media
                .kind
                .len()
                .saturating_add(media.mime_type.as_ref().map_or(0, String::len))
                .saturating_add(content)
                .saturating_add(32)
        }
        CanonicalValue::Omitted(value) => value
            .reason
            .len()
            .saturating_add(value.message.len())
            .saturating_add(24),
    }
}

fn display_call_path(path: &CallPath) -> String {
    let calls = path
        .call_ids
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join("/");
    format!(
        "{}:{}/{}/{}",
        path.boundary_id.encode(),
        path.engine_id.0,
        path.logical_thread_id,
        calls
    )
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn clone_io_result(result: &io::Result<()>) -> io::Result<()> {
    result
        .as_ref()
        .map(|()| ())
        .map_err(|error| io::Error::new(error.kind(), error.to_string()))
}

fn value_stats_path_from_env() -> Option<PathBuf> {
    let path = std::env::var_os("BAML_OBS_STATS").map(PathBuf::from)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("stats");
    Some(path.with_file_name(format!("{file_name}.values.json")))
}

#[cfg(unix)]
fn thread_cpu_time_ns() -> Option<u64> {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid out-pointer for `clock_gettime`.
    if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &raw mut ts) } != 0 {
        return None;
    }
    let seconds = u64::try_from(ts.tv_sec).ok()?;
    let nanos = u64::try_from(ts.tv_nsec).ok()?;
    seconds.checked_mul(1_000_000_000)?.checked_add(nanos)
}

#[cfg(not(unix))]
fn thread_cpu_time_ns() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::{
        ids::{BexCallId, BexThreadId, EngineId},
        value::{ValueFileRecord, read_bamlvalue_from_bytes},
        value_cas::{
            CallPath, CidManifestReader, PackIndex, TriggerId, ValueBoundaryRegistration,
            ValueDrainConfig, ValueDrainService,
        },
    };

    use super::*;

    fn temp_project(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "baml-value-drain-{name}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ))
    }

    fn trace(process: ProcessEuid, call_id: u64) -> TraceCallKey {
        TraceCallKey {
            process_euid: process,
            engine_id: EngineId(3),
            thread_id: BexThreadId(4),
            call_id: BexCallId(call_id),
        }
    }

    fn capture(boundary_id: BoundaryId, process: ProcessEuid, call_id: u64) -> DurableValueCapture {
        DurableValueCapture {
            boundary_id,
            call: trace(process, call_id),
            kind: ValueCaptureKind::CallInput,
            log_event: None,
            value: CanonicalValue::Map(vec![(
                "prompt".to_string(),
                CanonicalValue::String("hello".repeat(1000)),
            )]),
            promoted_by: None,
        }
    }

    #[test]
    fn continuous_drain_commits_pack_before_manifest_and_root_record() {
        let project = temp_project("commit");
        let boundary_id = BoundaryId::from_bytes([7; 16]);
        let boundary_dir = project.join("history").join(boundary_id.encode());
        let process = ProcessEuid([8; 16]);
        let mut config = ValueDrainConfig::new(&project, process);
        config.stats_path = Some(project.join("value-stats.json"));
        let service = ValueDrainService::start(config).unwrap();
        let handle = service.handle();
        handle
            .register_boundary(ValueBoundaryRegistration {
                boundary_id,
                boundary_dir: boundary_dir.clone(),
                created_ms: 10,
                run_started: None,
            })
            .unwrap();
        assert_eq!(
            handle.try_enqueue(capture(boundary_id, process, 1)),
            ValueEnqueueOutcome::Enqueued
        );
        handle.finish_boundary(boundary_id).unwrap();
        let stats = handle.stats();
        assert_eq!(stats.captures_committed, 1);
        assert_eq!(stats.roots_committed, 1);
        service.shutdown().unwrap();

        let values =
            read_bamlvalue_from_bytes(&fs::read(boundary_dir.join("values.bamlvalue")).unwrap())
                .unwrap();
        let [ValueFileRecord::CapturedValue(record)] = values.records.as_slice() else {
            panic!("expected one captured value");
        };
        let dag_ref = record.dag_ref.as_ref().expect("capture uses DAG ref");
        assert!(record.body.is_empty());
        let manifest = CidManifestReader::read(boundary_dir.join("manifest.bamlcids")).unwrap();
        assert!(manifest.manifest.sealed);
        assert_eq!(manifest.manifest.cids[0].as_bytes(), &dag_ref.root_cid);
        let packs = fs::read_dir(project.join("store/packs"))
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        let index_path = packs
            .iter()
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".bamlpack.idx"))
            })
            .unwrap();
        let mut pack_path = index_path.clone();
        pack_path.set_extension("");
        let index = PackIndex::read(index_path, pack_path).unwrap();
        assert!(
            index
                .entries
                .iter()
                .any(|entry| entry.cid.as_bytes() == &dag_ref.root_cid)
        );
        assert!(project.join("value-stats.json").exists());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn staging_promotes_subtree_and_persists_loss_and_audit() {
        let project = temp_project("promotion");
        let boundary_id = BoundaryId::from_bytes([9; 16]);
        let boundary_dir = project.join("history").join(boundary_id.encode());
        let process = ProcessEuid([10; 16]);
        let mut config = ValueDrainConfig::new(&project, process);
        config.staging_byte_budget = 256;
        config.staging_tombstone_capacity = 8;
        let service = ValueDrainService::start(config).unwrap();
        let handle = service.handle();
        handle
            .register_boundary(ValueBoundaryRegistration {
                boundary_id,
                boundary_dir: boundary_dir.clone(),
                created_ms: 10,
                run_started: None,
            })
            .unwrap();
        let root = CallPath {
            boundary_id,
            process_euid: process,
            engine_id: EngineId(3),
            logical_thread_id: 4,
            call_ids: vec![1],
        };
        for call_id in 1..=3 {
            let path = CallPath {
                call_ids: vec![1, call_id],
                ..root.clone()
            };
            let outcome = handle.stage_with(path, 128, || capture(boundary_id, process, call_id));
            assert!(outcome.retained);
        }
        let report = handle
            .promote_staged(&root, TriggerId("error-1".to_string()), 99)
            .unwrap();
        assert_eq!(report.promoted_records, 2);
        assert_eq!(report.staged_evicted, 1);
        handle.finish_boundary(boundary_id).unwrap();
        service.shutdown().unwrap();

        let values =
            read_bamlvalue_from_bytes(&fs::read(boundary_dir.join("values.bamlvalue")).unwrap())
                .unwrap();
        assert_eq!(
            values
                .records
                .iter()
                .filter(|record| matches!(record, ValueFileRecord::CapturedValue(_)))
                .count(),
            2
        );
        assert!(values.records.iter().any(|record| matches!(
            record,
            ValueFileRecord::CaptureLoss(loss)
                if loss.reason == CaptureLossReason::StagingEvicted
        )));
        assert!(values.records.iter().any(|record| matches!(
            record,
            ValueFileRecord::Audit(ValueAuditRecord::PromotionOccurred(audit))
                if audit.trigger == "error-1" && audit.staged_evicted == 1
        )));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn pending_budget_degrades_to_persisted_capture_loss() {
        let project = temp_project("loss");
        let boundary_id = BoundaryId::from_bytes([11; 16]);
        let boundary_dir = project.join("history").join(boundary_id.encode());
        let process = ProcessEuid([12; 16]);
        let mut config = ValueDrainConfig::new(&project, process);
        config.pending_byte_budget = 1;
        config.high_water_bytes = 1;
        let service = ValueDrainService::start(config).unwrap();
        let handle = service.handle();
        handle
            .register_boundary(ValueBoundaryRegistration {
                boundary_id,
                boundary_dir: boundary_dir.clone(),
                created_ms: 10,
                run_started: None,
            })
            .unwrap();
        assert_eq!(
            handle.try_enqueue(capture(boundary_id, process, 1)),
            ValueEnqueueOutcome::DroppedPendingBudget
        );
        handle.finish_boundary(boundary_id).unwrap();
        service.shutdown().unwrap();
        let values =
            read_bamlvalue_from_bytes(&fs::read(boundary_dir.join("values.bamlvalue")).unwrap())
                .unwrap();
        assert!(matches!(
            values.records.as_slice(),
            [ValueFileRecord::CaptureLoss(CaptureLossRecord {
                reason: CaptureLossReason::DrainQueueFull,
                skipped_count: 1,
                ..
            })]
        ));
        fs::remove_dir_all(project).unwrap();
    }
}
