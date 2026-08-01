//! The background profile consumer (plan §5 PR3): a single `std::thread`
//! named `bex-prof-consumer` that sweeps every ring (§3.4), transcodes raw
//! records to protobuf [`pb::DiskEventV1`]s, and appends per-engine
//! `.bamlprof` files.
//!
//! Invariants (plan §6): this thread never touches the GC heap or heap
//! permits — it reads rings, the registered (immutable) engine metadata, and
//! its own scratch. That is what keeps lossless-by-growth deadlock-free.
//! It also owns no ring: the heartbeat goes straight to the file writers.
//!
//! Engine lifecycle: `BexEngine::drop` sends [`ControlMsg::EngineClosed`],
//! which drains the engine's remaining events, syncs + closes its file, and
//! frees its metadata — long-lived engine-churning hosts (LSP recompiles)
//! don't accumulate fds or heartbeat work. Residual growth per closed
//! engine: one tombstoned id (8 bytes). Rings claimed by still-live threads
//! stay registered (idle) until those threads die and the rings pool —
//! bounded by peak concurrency, by design (plan invariant 7).
//!
//! Capacity model (D6): measured drain+transcode+write throughput is
//! **7.5M events/s on one core** (`prof_drain_throughput`, release, Linux
//! dev workstation, 2026-06; ~285 MB/s of `.bamlprof` output). The
//! 100M ev/s figure in the design is the *burst* producer write budget;
//! sustainable rate ≈ consumers × per-core transcode rate, and burst
//! tolerance ≈ `BAML_RING_MAX_OVERFLOW_BYTES / ((produce − drain) × ~30 B)`
//! seconds of backlog growth — ≈0.4 s at the defaults under a worst-case
//! 100M ev/s burst. Consumer sharding is a tuning knob with a concrete
//! trigger (a bench showing one consumer saturated), not MVP scope.

#![allow(unsafe_code)]

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{OnceLock, mpsc},
    time::{Duration, Instant},
};

use crate::{
    ids::{BoundaryId, EngineId, ProcessEuid, ThreadRef},
    prof::{
        boundary::{self, BoundaryBinding, BoundaryCompletion},
        cct::{EngineCct, RecentCall},
        clock::{self, TickConverter},
        config::{ObsLayout, ProfConfig, ProfilePipeline},
        encode::build_header,
        file::ProfileWriter,
        metadata, pb, record,
        recorder::{
            FlightRecorder, FullTraceBudget, FullTraceRecorder, TriggerFired, TriggerObservation,
            TriggerSet, write_exact_artifact,
        },
        registry::Registry,
        ring::{Ring, RingCtx},
        stats::{ConsumerStats, Snapshot as StatsSnapshot},
        storage::{
            BcctHeader, BlockRows, BoundaryBoundMeta, BoundaryCompleteMeta, BoundaryCounts,
            BoundaryLossMeta, BoundarySnapshot, BoundaryTriggerMeta, CctDeltaRow, CctHistogramRow,
            ClockDescriptor, LlmDeltaRow, ModelBirthRow, NodeBirthRow, PartitionBindRow,
            SegmentState, SessionStreamWriter, SpawnEdgeRow, TypedBoundaryMeta, append_meta_d2,
            encode_typed_boundary_meta, scan_bcct_bytes,
        },
        transcode::to_disk_event,
    },
    run::{ProfileEventSource, RuntimeTarget, profile_event_envelope_from_disk_event},
    value_cas::{Cid, CidManifestReader, derive_unsealed_bamlvalue_roots},
};

/// How often the consumer stamps a process-liveness heartbeat into every
/// open file (MVP: consumer-stamped, v2 §4).
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
const LATENCY_DUMP_MIN_INTERVAL_NS: u64 = 5_000_000_000;
// Reserve the final slot for the boundary's existing completion trigger
// (error/manual/root latency), keeping the hard per-boundary maximum at 16.
const MAX_CALL_CLOSE_LATENCY_DUMPS: u8 = 15;
const MAX_BOUNDARY_DUMPS: u8 = 16;

pub(crate) enum ControlMsg {
    /// Drain everything currently committed, sync files durably, then ack.
    Flush(mpsc::SyncSender<()>),
    /// An engine was dropped: drain its remaining events, sync + close its
    /// file (freeing the fd and stopping its heartbeats), drop its metadata,
    /// and tombstone the id. Sent non-blocking from `BexEngine::drop`.
    EngineClosed(u64),
    /// Bind a host boundary to the partition created by its root thread. The
    /// consumer owns this handshake because only it can name session segments.
    BindBoundary {
        boundary_id: BoundaryId,
        root_thread: ThreadRef,
        ack: mpsc::SyncSender<io::Result<BoundaryBinding>>,
    },
    /// Final drain, snapshot seal, complete milestone, then acknowledge.
    CompleteBoundary {
        boundary_id: BoundaryId,
        completion: BoundaryCompletion,
        ack: mpsc::SyncSender<io::Result<()>>,
    },
}

/// Everything the consumer loop needs; owned so tests can run private
/// consumers against private registries and directories.
pub(crate) struct ConsumerEnv {
    pub(crate) registry: &'static Registry,
    pub(crate) ctx: &'static RingCtx,
    pub(crate) dir: PathBuf,
    pub(crate) wake_interval: Duration,
    pub(crate) clock: ClockMode,
    pub(crate) pipeline: ProfilePipeline,
    pub(crate) obs_layout: ObsLayout,
    pub(crate) obs_stats_path: Option<PathBuf>,
}

/// How the consumer obtains its tick→ns converter.
pub(crate) enum ClockMode {
    /// Build from the process clock (production). For the x86 TSC this
    /// takes the ~2 ms coarse two-point estimate — on the consumer thread,
    /// never on a producer.
    Detect,
    /// Use the given converter as-is (tests: identity keeps synthetic tick
    /// values byte-stable through the roundtrip).
    #[cfg_attr(not(test), allow(dead_code))]
    Fixed(TickConverter),
}

impl ClockMode {
    fn build(&self) -> TickConverter {
        match self {
            ClockMode::Detect => TickConverter::from_clock(),
            ClockMode::Fixed(conv) => conv.clone(),
        }
    }
}

static CONTROL_TX: OnceLock<mpsc::Sender<ControlMsg>> = OnceLock::new();

/// Spawns the process-wide consumer on first call (cheap afterwards).
/// Called from the ring-acquisition path when profiling is enabled.
pub(crate) fn ensure_started() {
    CONTROL_TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        let cfg = ProfConfig::global();
        let env = ConsumerEnv {
            registry: crate::prof::registry::global_registry(),
            ctx: crate::prof::registry::global_ctx(),
            dir: cfg.profile_dir.clone(),
            wake_interval: cfg.wake_interval,
            clock: ClockMode::Detect,
            pipeline: cfg.pipeline,
            obs_layout: cfg.obs_layout,
            obs_stats_path: cfg.obs_stats_path.clone(),
        };
        std::thread::Builder::new()
            .name("bex-prof-consumer".into())
            .spawn(move || consumer_main(&rx, &env))
            .expect("failed to spawn bex-prof-consumer thread");
        tx
    });
}

/// Drains everything committed so far and flushes the writers, waiting up
/// to `timeout` for the consumer's ack. Returns whether the ack arrived. A
/// no-op `true` when profiling never started.
///
/// Durability: the ack means `fsync`ed (survives power loss). The
/// consumer's idle-cadence flushes in between are OS-buffer only.
///
/// Call sites should have joined/stopped their VMs first: thread join is a
/// full sync, so the final commits are visible to the drain (plan §1).
/// The consumer thread keeps running afterwards (it is a daemon; repeated
/// flushes are fine) — "join" refers to joining the flush, not the thread.
pub fn flush_and_join(timeout: Duration) -> bool {
    let Some(tx) = CONTROL_TX.get() else {
        return true;
    };
    let (ack_tx, ack_rx) = mpsc::sync_channel(1);
    if tx.send(ControlMsg::Flush(ack_tx)).is_err() {
        return false;
    }
    crate::prof::registry::global_ctx().wake().force_wake();
    ack_rx.recv_timeout(timeout).is_ok()
}

/// Notifies the consumer that an engine was dropped (called from
/// `BexEngine::drop`): its remaining events are drained, its `.bamlprof` is
/// synced and closed (freeing the fd and stopping its heartbeats), and its
/// metadata entry is freed. Non-blocking — safe from `Drop`. If profiling
/// never started, only the shared metadata entry is removed.
pub fn engine_closed(engine_id: u64) {
    let Some(tx) = CONTROL_TX.get() else {
        let _ = metadata::remove_engine_metadata(engine_id);
        return;
    };
    if tx.send(ControlMsg::EngineClosed(engine_id)).is_ok() {
        crate::prof::registry::global_ctx().wake().force_wake();
    } else {
        let _ = metadata::remove_engine_metadata(engine_id);
    }
}

pub(crate) fn bind_boundary(
    boundary_id: BoundaryId,
    root_thread: ThreadRef,
    timeout: Duration,
) -> io::Result<BoundaryBinding> {
    ensure_started();
    let tx = CONTROL_TX
        .get()
        .ok_or_else(|| io::Error::other("profile consumer did not start"))?;
    let (ack_tx, ack_rx) = mpsc::sync_channel(1);
    tx.send(ControlMsg::BindBoundary {
        boundary_id,
        root_thread,
        ack: ack_tx,
    })
    .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "profile consumer stopped"))?;
    crate::prof::registry::global_ctx().wake().force_wake();
    ack_rx
        .recv_timeout(timeout)
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "boundary bind timed out"))?
}

pub(crate) fn complete_boundary(
    boundary_id: BoundaryId,
    completion: BoundaryCompletion,
    timeout: Duration,
) -> io::Result<()> {
    let Some(tx) = CONTROL_TX.get() else {
        return Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "profile consumer was not started",
        ));
    };
    let (ack_tx, ack_rx) = mpsc::sync_channel(1);
    tx.send(ControlMsg::CompleteBoundary {
        boundary_id,
        completion,
        ack: ack_tx,
    })
    .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "profile consumer stopped"))?;
    crate::prof::registry::global_ctx().wake().force_wake();
    ack_rx
        .recv_timeout(timeout)
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "boundary completion timed out"))?
}

/// The consumer loop: §3.4 sweep + §3.6 wake protocol + control messages.
pub(crate) fn consumer_main(control: &mpsc::Receiver<ControlMsg>, env: &ConsumerEnv) {
    env.ctx.wake().register_consumer();
    let mut state = ConsumerState::new_configured(
        env.dir.clone(),
        env.clock.build(),
        env.pipeline,
        env.obs_layout,
        env.obs_stats_path.clone(),
    );
    loop {
        while let Ok(msg) = control.try_recv() {
            match msg {
                ControlMsg::Flush(ack) => {
                    // Drain until a sweep makes no progress — everything
                    // committed before the request is then on disk — but
                    // bounded: with a producer outrunning the drain, an
                    // unbounded loop would starve the control channel (and
                    // the ack) forever. The bound only cuts work the flush
                    // contract never promised (post-request events).
                    for _ in 0..1024 {
                        if !state.sweep_once(env) {
                            break;
                        }
                    }
                    state.sync_files();
                    state.write_stats("flush", env.ctx);
                    let _ = ack.send(());
                }
                ControlMsg::EngineClosed(engine_id) => {
                    // Every commit for the engine happened-before its last
                    // Arc release (and so before this message): a bounded
                    // drain-to-idle collects them all before the close.
                    for _ in 0..1024 {
                        if !state.sweep_once(env) {
                            break;
                        }
                    }
                    state.close_engine(engine_id);
                    state.write_stats("engine_closed", env.ctx);
                    // All of the engine's events have been delivered; let
                    // observers (run store, history store) release whatever
                    // they buffered for it.
                    crate::run::publish_engine_closed(crate::ids::EngineId(engine_id));
                    crate::history::publish_history_engine_closed(crate::ids::EngineId(engine_id));
                }
                ControlMsg::BindBoundary {
                    boundary_id,
                    root_thread,
                    ack,
                } => {
                    for _ in 0..1024 {
                        if !state.sweep_once(env) {
                            break;
                        }
                    }
                    let result = state.bind_boundary(boundary_id, root_thread);
                    let _ = ack.send(result);
                }
                ControlMsg::CompleteBoundary {
                    boundary_id,
                    completion,
                    ack,
                } => {
                    for _ in 0..1024 {
                        if !state.sweep_once(env) {
                            break;
                        }
                    }
                    let result = state.complete_boundary(boundary_id, completion);
                    let _ = ack.send(result);
                }
            }
        }
        let progress = state.sweep_once(env);
        state.maybe_heartbeat();
        if !progress {
            state.flush_files();
            let wake = env.ctx.wake();
            wake.pre_park();
            // Recheck after raising the flag (shrinks the benign D4 race);
            // the timeout bounds whatever remains of it.
            if !state.sweep_once(env) {
                wake.park(env.wake_interval);
            }
            wake.post_park();
        }
    }
}

struct ConsumerState {
    dir: PathBuf,
    /// False only when `.baml` artifact hygiene setup failed. In that case we
    /// keep draining safe by dropping profile records instead of creating
    /// unignored files in a user's repo.
    profile_writes_enabled: bool,
    /// Tick→ns conversion for every disk timestamp (events + heartbeats).
    conv: TickConverter,
    /// `None` = opening or writing failed permanently (already reported).
    writers: HashMap<u64, Option<ProfileWriter>>,
    /// Engines whose files were closed (8 bytes per engine, forever): a
    /// record arriving after the close is a logic error — reported once and
    /// dropped, never silently resurrecting the file.
    closed_engines: std::collections::HashSet<u64>,
    closed_reported: std::collections::HashSet<u64>,
    process_id: [u8; 16],
    started_at_epoch_ns: u128,
    last_heartbeat: Instant,
    corrupt_reported: bool,
    pipeline: ProfilePipeline,
    stats: ConsumerStats,
    closed_profile_bytes: u64,
    stats_write_failure_reported: bool,
    cct_engines: HashMap<u64, EngineCct>,
    cct_events: HashMap<u64, u64>,
    session_writers: HashMap<u64, Option<SessionStreamWriter>>,
    closed_session_bytes: u64,
    obs_layout: ObsLayout,
    boundaries: HashMap<BoundaryId, BoundBoundary>,
    next_boundary_local_id: HashMap<u64, u32>,
    flight_recorder: FlightRecorder,
    full_trace_budget: Option<FullTraceBudget>,
    full_traces: HashMap<u64, FullTraceRecorder>,
    latency_trigger_ns: Option<u64>,
    pending_latency_calls: HashMap<u64, Vec<PendingLatencyCall>>,
}

#[derive(Clone, Copy, Debug)]
struct PendingLatencyCall {
    partition_id: Option<u32>,
    thread_id: u64,
    call_id: u64,
}

#[derive(Clone, Debug)]
struct BoundBoundary {
    engine_id: u64,
    partition_id: u32,
    boundary_local_id: u32,
    boundary_dir: PathBuf,
    created_ms: u64,
    first_seg_seq: u32,
    root_thread_id: u64,
    latency_triggers: TriggerSet,
    latency_dump_count: u8,
    last_latency_dump_ns: Option<u64>,
    dropped_dumps: u64,
    last_drop_detail: Option<String>,
    root_latency_handled: bool,
    root_latency_dumped: bool,
    trigger_dump_refs: Vec<String>,
}

#[derive(Debug)]
enum LatencyDumpDecision {
    Ignore,
    Fire(TriggerFired),
    Drop(String),
}

impl BoundBoundary {
    fn observe_latency(&mut self, call: &RecentCall) -> LatencyDumpDecision {
        let Some(fired) = self.latency_triggers.observe(TriggerObservation {
            thread_id: call.thread_id,
            call_id: call.call_id,
            node_id: call.node_id,
            start_ns: call.start_ns,
            end_ns: call.end_ns,
            status: call.status,
        }) else {
            return LatencyDumpDecision::Ignore;
        };
        self.root_latency_handled |=
            call.thread_id == self.root_thread_id && call.parent_call_id == 0;
        if self.latency_dump_count >= MAX_CALL_CLOSE_LATENCY_DUMPS {
            return LatencyDumpDecision::Drop(format!(
                "call-close latency dump cap reached ({MAX_CALL_CLOSE_LATENCY_DUMPS}); \
                 reserved final slot for completion trigger"
            ));
        }
        if let Some(last) = self.last_latency_dump_ns {
            let since_last = fired.timestamp_ns.saturating_sub(last);
            if since_last < LATENCY_DUMP_MIN_INTERVAL_NS {
                return LatencyDumpDecision::Drop(format!(
                    "call-close latency dump rate-limited: {since_last}ns since prior dump, \
                     minimum {LATENCY_DUMP_MIN_INTERVAL_NS}ns"
                ));
            }
        }
        LatencyDumpDecision::Fire(fired)
    }

    fn record_latency_dump(&mut self, fired: &TriggerFired, is_root_call: bool, dump_ref: String) {
        self.latency_dump_count = self.latency_dump_count.saturating_add(1);
        self.last_latency_dump_ns = Some(fired.timestamp_ns);
        self.root_latency_dumped |= is_root_call;
        self.trigger_dump_refs.push(dump_ref);
    }

    fn record_dropped_dump(&mut self, detail: String) {
        self.dropped_dumps = self.dropped_dumps.saturating_add(1);
        self.last_drop_detail = Some(detail);
    }
}

impl ConsumerState {
    #[cfg(test)]
    fn new(dir: PathBuf, conv: TickConverter) -> ConsumerState {
        Self::new_configured(dir, conv, ProfilePipeline::Legacy, ObsLayout::V1, None)
    }

    fn new_configured(
        dir: PathBuf,
        conv: TickConverter,
        pipeline: ProfilePipeline,
        obs_layout: ObsLayout,
        obs_stats_path: Option<PathBuf>,
    ) -> ConsumerState {
        let profile_writes_enabled = match ensure_profile_dir_ignored(&dir) {
            Ok(_) => true,
            Err(err) => {
                report(format_args!(
                    "cannot prepare .baml/.gitignore for profile dir {}; disabling .bamlprof persistence: {err}",
                    dir.display()
                ));
                false
            }
        };
        let config = ProfConfig::global();
        let latency_trigger_ns = config
            .latency_trigger_ms
            .map(|millis| millis.saturating_mul(1_000_000));
        ConsumerState {
            dir,
            profile_writes_enabled,
            conv,
            writers: HashMap::new(),
            closed_engines: std::collections::HashSet::new(),
            closed_reported: std::collections::HashSet::new(),
            process_id: process_id(),
            started_at_epoch_ns: clock::started_at_epoch_ns(),
            last_heartbeat: Instant::now(),
            corrupt_reported: false,
            pipeline,
            stats: ConsumerStats::new(obs_stats_path, pipeline),
            closed_profile_bytes: 0,
            stats_write_failure_reported: false,
            cct_engines: HashMap::new(),
            cct_events: HashMap::new(),
            session_writers: HashMap::new(),
            closed_session_bytes: 0,
            obs_layout,
            boundaries: HashMap::new(),
            next_boundary_local_id: HashMap::new(),
            flight_recorder: FlightRecorder::new(config.flight_recorder_bytes),
            full_trace_budget: config.full_trace.then_some(FullTraceBudget {
                max_bytes: config.full_trace_max_bytes,
                max_duration_ns: u64::try_from(config.full_trace_max_duration.as_nanos())
                    .unwrap_or(u64::MAX),
            }),
            full_traces: HashMap::new(),
            latency_trigger_ns,
            pending_latency_calls: HashMap::new(),
        }
    }

    fn sweep_once(&mut self, env: &ConsumerEnv) -> bool {
        // SAFETY: consumer_main is the process's (or this test registry's)
        // single consumer thread.
        let progress = unsafe {
            env.registry
                .sweep(&mut |ring, bytes| self.transcode(ring, bytes))
        };
        let engines = self.cct_engines.keys().copied().collect::<Vec<_>>();
        let mut latency_calls = Vec::new();
        for engine_id in engines {
            let mut pending = self
                .pending_latency_calls
                .remove(&engine_id)
                .unwrap_or_default();
            let cct = self
                .cct_engines
                .get_mut(&engine_id)
                .expect("engine id collected above");
            cct.finish_sweep();
            let mut closed = Vec::new();
            Self::resolve_pending_latency_calls(cct, &mut pending, &mut closed);
            latency_calls.extend(
                closed
                    .into_iter()
                    .map(|(partition, call)| (engine_id, partition, call)),
            );
            if !pending.is_empty() {
                self.pending_latency_calls.insert(engine_id, pending);
            }
        }
        for (engine_id, partition, call) in latency_calls {
            self.observe_call_close_latency(engine_id, partition, call);
        }
        let mut shed = Vec::new();
        let shed_progress = env
            .registry
            .take_shed_events(|engine_id, count| shed.push((engine_id, count)));
        for (engine_id, count) in shed {
            self.stats.counters.shed_structural_ranges = self
                .stats
                .counters
                .shed_structural_ranges
                .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
            self.cct_engines
                .entry(engine_id)
                .or_default()
                .mark_shed_range(u64::try_from(count).unwrap_or(u64::MAX));
        }
        progress || shed_progress
    }

    fn transcode(&mut self, ring: &'static Ring, bytes: &[u8]) {
        self.stats.counters.drained_ranges = self.stats.counters.drained_ranges.saturating_add(1);
        self.stats.counters.raw_bytes_drained = self
            .stats
            .counters
            .raw_bytes_drained
            .saturating_add(bytes.len() as u64);
        if !matches!(self.pipeline, ProfilePipeline::Legacy) {
            // SAFETY: `Registry::sweep` handed us a committed range, which is
            // the same proof required by `Ring::engine_id`.
            let engine_id = unsafe { ring.engine_id() };
            self.flight_recorder.retain(engine_id, bytes);
            if let Some(budget) = self.full_trace_budget {
                let trace = self
                    .full_traces
                    .entry(engine_id)
                    .or_insert_with(|| FullTraceRecorder::new(budget));
                let was_exhausted = trace.exhausted().is_some();
                trace.retain(bytes);
                if !was_exhausted && trace.exhausted().is_some() {
                    self.stats.counters.shed_full_trace =
                        self.stats.counters.shed_full_trace.saturating_add(1);
                }
            }
        }
        match self.pipeline {
            ProfilePipeline::Legacy => self.transcode_legacy(ring, bytes),
            ProfilePipeline::Dual => {
                self.transcode_cct(ring, bytes, false);
                self.transcode_legacy(ring, bytes);
            }
            ProfilePipeline::Cct => {
                self.transcode_cct(ring, bytes, true);
            }
        }
    }

    fn transcode_cct(&mut self, ring: &'static Ring, bytes: &[u8], count_stats: bool) {
        // SAFETY: bytes in hand are drain progress (Ring::engine_id contract).
        let engine_id = unsafe { ring.engine_id() };
        if self.closed_engines.contains(&engine_id) {
            if count_stats {
                self.stats.counters.closed_record_ranges =
                    self.stats.counters.closed_record_ranges.saturating_add(1);
            }
            return;
        }
        let mut events = 0u64;
        let mut corrupt = false;
        let mut latency_calls = Vec::new();
        let mut pending = self
            .pending_latency_calls
            .remove(&engine_id)
            .unwrap_or_default();
        for rec in record::iter(bytes) {
            match rec {
                Ok(raw) => {
                    events = events.saturating_add(1);
                    let timestamp_ns = self.conv.to_ns(match raw {
                        record::RawRecord::CallFunction { ts_ticks, .. }
                        | record::RawRecord::EndFunction { ts_ticks, .. }
                        | record::RawRecord::StartThread { ts_ticks, .. }
                        | record::RawRecord::EndThread { ts_ticks, .. }
                        | record::RawRecord::SetFunctionId { ts_ticks, .. }
                        | record::RawRecord::SuspendThread { ts_ticks, .. }
                        | record::RawRecord::ResumeThread { ts_ticks, .. }
                        | record::RawRecord::LlmCallMeta { ts_ticks, .. } => ts_ticks,
                    });
                    if let Err(error) = self.rotate_session_epoch_if_due(engine_id, timestamp_ns) {
                        self.fail_session_writer(engine_id, &error);
                    }
                    let cct = self.cct_engines.entry(engine_id).or_default();
                    if self.latency_trigger_ns.is_some()
                        && let record::RawRecord::EndFunction {
                            thread_id, call_id, ..
                        } = raw
                    {
                        let partition_id = cct.partition_for_thread(thread_id.0);
                        let already_closed = partition_id.is_some_and(|partition| {
                            cct.recent_call_in_partition(partition, thread_id.0, call_id.0)
                                .is_some()
                        });
                        let already_pending = pending.iter().any(|candidate| {
                            candidate.thread_id == thread_id.0 && candidate.call_id == call_id.0
                        });
                        if !already_closed && !already_pending {
                            pending.push(PendingLatencyCall {
                                partition_id,
                                thread_id: thread_id.0,
                                call_id: call_id.0,
                            });
                        }
                    }
                    cct.ingest_raw(&raw, &self.conv);
                    Self::resolve_pending_latency_calls(cct, &mut pending, &mut latency_calls);
                }
                Err(_) => {
                    corrupt = true;
                    self.cct_engines
                        .entry(engine_id)
                        .or_default()
                        .mark_corrupt_range();
                    break;
                }
            }
        }
        if !pending.is_empty() {
            self.pending_latency_calls.insert(engine_id, pending);
        }
        for (partition, call) in latency_calls {
            self.observe_call_close_latency(engine_id, partition, call);
        }
        let total = self.cct_events.entry(engine_id).or_default();
        *total = total.saturating_add(events);
        if count_stats {
            self.stats.counters.events = self.stats.counters.events.saturating_add(events);
            if corrupt {
                self.stats.counters.corrupt_ranges =
                    self.stats.counters.corrupt_ranges.saturating_add(1);
            }
        }
    }

    fn resolve_pending_latency_calls(
        cct: &EngineCct,
        pending: &mut Vec<PendingLatencyCall>,
        closed: &mut Vec<(u32, RecentCall)>,
    ) {
        pending.retain_mut(|candidate| {
            if candidate.partition_id.is_none() {
                candidate.partition_id = cct.partition_for_thread(candidate.thread_id);
            }
            let Some(partition) = candidate.partition_id else {
                return true;
            };
            let Some(call) =
                cct.recent_call_in_partition(partition, candidate.thread_id, candidate.call_id)
            else {
                return true;
            };
            closed.push((partition, call));
            false
        });
    }

    fn transcode_legacy(&mut self, ring: &'static Ring, bytes: &[u8]) {
        // SAFETY: bytes in hand are drain progress (Ring::engine_id contract).
        let engine_id = unsafe { ring.engine_id() };
        if self.closed_engines.contains(&engine_id) {
            self.stats.counters.closed_record_ranges =
                self.stats.counters.closed_record_ranges.saturating_add(1);
            if self.closed_reported.insert(engine_id) {
                report(format_args!(
                    "dropping records for closed engine {engine_id} (post-Drop emission?)"
                ));
            }
            return;
        }
        // Cloned out of `self` so the converter can be read while the
        // writer (a `&mut self` borrow) is held; one clone per drained
        // range, not per record. `already_reported` is likewise snapshotted
        // so the writer borrow below covers no other `self` access.
        let conv = self.conv.clone();
        let already_reported = self.corrupt_reported;
        let process_id = self.process_id;
        let Some(writer) = self.writer_for(engine_id) else {
            return;
        };
        // Accumulate the whole drained range into the writer's buffer, then
        // issue ONE write for the range (vs one per event).
        let mut corrupt = None;
        let mut events = 0u64;
        for rec in record::iter(bytes) {
            match rec {
                Ok(raw) => {
                    events = events.saturating_add(1);
                    let event = to_disk_event(&raw, &conv);
                    if let Some(envelope) = profile_event_envelope_from_disk_event(
                        ProfileEventSource::Live {
                            target: RuntimeTarget::Native,
                            source_id: "bex-prof-consumer".to_string(),
                        },
                        ProcessEuid(process_id),
                        EngineId(engine_id),
                        &event,
                    ) {
                        crate::run::publish_profile_event(&envelope);
                        crate::history::publish_history_profile_event(&envelope, &event);
                    }
                    writer.encode_event(&event);
                }
                // A committed range that fails to decode is a producer bug:
                // the framing is unrecoverable past this point, so drop the
                // rest of the range. The already-encoded prefix is still
                // flushed below.
                Err(err) => {
                    corrupt = Some(err);
                    break;
                }
            }
        }
        let flush = writer.flush_buffered();
        self.stats.counters.events = self.stats.counters.events.saturating_add(events);
        if corrupt.is_some() {
            self.stats.counters.corrupt_ranges =
                self.stats.counters.corrupt_ranges.saturating_add(1);
        }
        // The writer borrow ends above; `self` field access is safe again.
        if let Some(err) = corrupt
            && !already_reported
        {
            self.corrupt_reported = true;
            report(format_args!(
                "corrupt profiling record in committed range (engine {engine_id}): {err:?}"
            ));
        }
        if let Err(err) = flush {
            self.fail_writer(engine_id, &err);
        }
    }

    fn writer_for(&mut self, engine_id: u64) -> Option<&mut ProfileWriter> {
        if !self.profile_writes_enabled || !self.obs_layout.writes_v1() {
            return None;
        }
        self.writers
            .entry(engine_id)
            .or_insert_with(|| {
                let mut meta = metadata::get_engine_metadata(engine_id);
                if let Some(meta) = meta.as_mut()
                    && let Some(dictionary) = meta.revision_dictionary.as_ref()
                {
                    if let Some(project_root) = nearest_baml_dir(&self.dir)
                        .and_then(|path| path.parent().map(Path::to_path_buf))
                    {
                        let store = crate::revision_dictionary::file::RevisionDictionaryStore::new(
                            project_root,
                        );
                        match store.ensure_written(dictionary) {
                            Ok(_) => {
                                // The revision reference is now durable and
                                // discoverable beside `.baml/profiles`.
                                meta.functions.clear();
                            }
                            Err(err) => {
                                // Degrade explicitly but remain complete:
                                // retain the embedded table in this header.
                                report(format_args!(
                                    "cannot persist revision dictionary {} before profile header; \
                                     embedding fallback function table: {err}",
                                    dictionary.identity.revision_id
                                ));
                            }
                        }
                    }
                }
                let header = build_header(
                    self.process_id,
                    engine_id,
                    self.started_at_epoch_ns,
                    meta.as_ref(),
                    &self.conv,
                );
                match ProfileWriter::create(
                    &self.dir,
                    self.process_id,
                    self.started_at_epoch_ns,
                    engine_id,
                    &header,
                ) {
                    Ok(writer) => Some(writer),
                    Err(err) => {
                        report(format_args!(
                            "cannot create .bamlprof for engine {engine_id} under {}: {err}",
                            self.dir.display()
                        ));
                        None
                    }
                }
            })
            .as_mut()
    }

    fn fail_writer(&mut self, engine_id: u64, err: &io::Error) {
        self.stats.counters.writer_failures = self.stats.counters.writer_failures.saturating_add(1);
        if let Some(slot) = self.writers.get_mut(&engine_id) {
            if let Some(writer) = slot {
                report(format_args!(
                    "write to {} failed; disabling this engine's profile: {err}",
                    writer.path().display()
                ));
            }
            *slot = None;
        }
    }

    fn session_writer_for(&mut self, engine_id: u64) -> Option<&mut SessionStreamWriter> {
        if !self.profile_writes_enabled || !self.obs_layout.writes_v2() {
            return None;
        }
        if !self.session_writers.contains_key(&engine_id) {
            let meta = metadata::get_engine_metadata(engine_id);
            let revision_id = meta.as_ref().map_or([0; 32], |meta| meta.revision_id_bytes);
            let (numer, denom) = self.conv.rate();
            let header = BcctHeader {
                process_euid: self.process_id,
                engine_id,
                session_seg_seq: 1,
                started_epoch_ns: u64::try_from(self.started_at_epoch_ns).unwrap_or(u64::MAX),
                clock: ClockDescriptor {
                    kind: match self.conv.kind() {
                        clock::ClockKind::Tsc => 1,
                        clock::ClockKind::Cntvct => 2,
                        clock::ClockKind::Instant => 3,
                        clock::ClockKind::Stub => 4,
                    },
                    quality: match self.conv.quality() {
                        clock::ClockQuality::Exact => 1,
                        clock::ClockQuality::Calibrated => 2,
                        clock::ClockQuality::Coarse => 3,
                    },
                    tick_ns_numer: u32::try_from(numer).unwrap_or(u32::MAX),
                    tick_ns_denom: u32::try_from(denom).unwrap_or(u32::MAX),
                },
                revision_id,
            };
            let project_root = nearest_baml_dir(&self.dir)
                .and_then(|path| path.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            if let Some(dictionary) = meta
                .as_ref()
                .and_then(|metadata| metadata.revision_dictionary.as_ref())
            {
                let store =
                    crate::revision_dictionary::file::RevisionDictionaryStore::new(&project_root);
                if let Err(error) = store.ensure_written(dictionary) {
                    self.stats.counters.writer_failures =
                        self.stats.counters.writer_failures.saturating_add(1);
                    report(format_args!(
                        "cannot persist revision dictionary {} before v2 session creation: {error}",
                        dictionary.identity.revision_id
                    ));
                }
            }
            let slot = match SessionStreamWriter::create(&project_root, header) {
                Ok(writer) => Some(writer),
                Err(err) => {
                    self.stats.counters.writer_failures =
                        self.stats.counters.writer_failures.saturating_add(1);
                    report(format_args!(
                        "cannot create v2 CCT session for engine {engine_id} under {}: {err}",
                        project_root.display()
                    ));
                    None
                }
            };
            self.session_writers.insert(engine_id, slot);
        }
        self.session_writers.get_mut(&engine_id)?.as_mut()
    }

    fn rotate_session_epoch_if_due(&mut self, engine_id: u64, timestamp_ns: u64) -> io::Result<()> {
        if !self.obs_layout.writes_v2()
            || self
                .boundaries
                .values()
                .any(|boundary| boundary.engine_id == engine_id)
            || self
                .pending_latency_calls
                .get(&engine_id)
                .is_some_and(|pending| !pending.is_empty())
            || !self
                .cct_engines
                .get(&engine_id)
                .is_some_and(EngineCct::can_rotate_epoch)
            || !self
                .session_writers
                .get(&engine_id)
                .and_then(Option::as_ref)
                .is_some_and(|writer| writer.epoch_rotation_due(timestamp_ns))
        {
            return Ok(());
        }

        let started_epoch_ns = u64::try_from(
            self.started_at_epoch_ns
                .saturating_add(u128::from(timestamp_ns)),
        )
        .unwrap_or(u64::MAX);
        let writer = self
            .session_writers
            .get_mut(&engine_id)
            .and_then(Option::take)
            .ok_or_else(|| io::Error::other("session epoch writer disappeared"))?;
        let rotated_bytes = writer.bytes_written();
        match writer.rotate_epoch(started_epoch_ns, timestamp_ns) {
            Ok(next) => {
                self.closed_session_bytes = self.closed_session_bytes.saturating_add(rotated_bytes);
                self.session_writers.insert(engine_id, Some(next));
                self.cct_engines.insert(engine_id, EngineCct::default());
                self.cct_events.insert(engine_id, 0);
                self.next_boundary_local_id.insert(engine_id, 1);
                Ok(())
            }
            Err(error) => {
                self.session_writers.insert(engine_id, None);
                Err(error)
            }
        }
    }

    fn persist_cct_windows(
        &mut self,
        engine_id: u64,
        windows: &[crate::prof::cct::WindowDelta],
        snapshot: &crate::prof::cct::CctSnapshot,
    ) {
        self.stats.counters.cct_blocks = self
            .stats
            .counters
            .cct_blocks
            .saturating_add(windows.len() as u64);
        if windows.is_empty() || !self.obs_layout.writes_v2() {
            return;
        }

        let models = metadata::get_engine_metadata(engine_id)
            .map(|meta| meta.models.entries_after(0))
            .unwrap_or_default();
        let Some(writer) = self.session_writer_for(engine_id) else {
            return;
        };
        let timestamp_ns = windows.first().map_or(0, |window| window.start_ns);
        let result = models
            .iter()
            .try_for_each(|model| writer.register_model(model.model_id, &model.name, timestamp_ns))
            .and_then(|()| {
                windows
                    .iter()
                    .try_for_each(|window| writer.write_window(window, snapshot))
            });
        if let Err(err) = result {
            self.fail_session_writer(engine_id, &err);
        }
    }

    fn fail_session_writer(&mut self, engine_id: u64, err: &io::Error) {
        self.stats.counters.writer_failures = self.stats.counters.writer_failures.saturating_add(1);
        if let Some(slot) = self.session_writers.get_mut(&engine_id) {
            report(format_args!(
                "write to v2 CCT session for engine {engine_id} failed; disabling it: {err}"
            ));
            *slot = None;
        }
    }

    fn bind_boundary(
        &mut self,
        boundary_id: BoundaryId,
        root_thread: ThreadRef,
    ) -> io::Result<BoundaryBinding> {
        if root_thread.process_euid.0 != self.process_id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "boundary root thread belongs to another process",
            ));
        }
        if let Some(existing) = self.boundaries.get(&boundary_id) {
            if existing.engine_id != root_thread.engine_id.0 {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "boundary is already bound to another engine",
                ));
            }
            let writer = self
                .session_writers
                .get(&existing.engine_id)
                .and_then(Option::as_ref)
                .ok_or_else(|| io::Error::other("bound boundary lost its session writer"))?;
            return Ok(BoundaryBinding {
                session_dir: writer.layout().session_dir.clone(),
                first_seg_seq: existing.first_seg_seq,
                partition_id: existing.partition_id,
                boundary_local_id: existing.boundary_local_id,
            });
        }
        let registration = boundary::registration(boundary_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "boundary begin milestone was not registered",
            )
        })?;
        let engine_id = root_thread.engine_id.0;
        let partition_id = self
            .cct_engines
            .get(&engine_id)
            .and_then(|cct| cct.partition_for_thread(root_thread.thread_id.0))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "root StartThread was not observed by the CCT consumer",
                )
            })?;
        let boundary_local_id = {
            let next = self.next_boundary_local_id.entry(engine_id).or_insert(1);
            let assigned = *next;
            *next = next
                .checked_add(1)
                .ok_or_else(|| io::Error::other("boundary-local id space exhausted"))?;
            assigned
        };
        let timestamp_ns = self.conv.to_ns(clock::now_ticks());
        let events = self.cct_events.get(&engine_id).copied().unwrap_or_default();
        let wall_epoch_ns = u64::try_from(
            self.started_at_epoch_ns
                .saturating_add(u128::from(timestamp_ns)),
        )
        .unwrap_or(u64::MAX);
        let (session_dir, session_dir_meta, first_seg_seq) = {
            let writer = self
                .session_writer_for(engine_id)
                .ok_or_else(|| io::Error::other("v2 session writer is unavailable"))?;
            let first_seg_seq = writer.segment_sequence();
            writer.bind_partition(
                PartitionBindRow {
                    partition_id,
                    boundary_local_id,
                    boundary_id: boundary_id.as_bytes(),
                    created_ms: registration.created_ms,
                },
                timestamp_ns,
            )?;
            writer.sync(wall_epoch_ns, timestamp_ns, events)?;
            let session_dir = writer.layout().session_dir.clone();
            let session_dir_meta = session_dir
                .strip_prefix(&writer.layout().project_root)
                .unwrap_or(&session_dir)
                .to_string_lossy()
                .into_owned();
            (session_dir, session_dir_meta, first_seg_seq)
        };
        append_typed_boundary_d2(
            &registration.boundary_dir.join("boundary.bamlmeta"),
            &TypedBoundaryMeta::Bound(BoundaryBoundMeta {
                session_dir: session_dir_meta,
                first_seg_seq,
                partition_id,
                boundary_local_id,
            }),
        )?;
        self.boundaries.insert(
            boundary_id,
            BoundBoundary {
                engine_id,
                partition_id,
                boundary_local_id,
                boundary_dir: registration.boundary_dir,
                created_ms: registration.created_ms,
                first_seg_seq,
                root_thread_id: root_thread.thread_id.0,
                latency_triggers: TriggerSet::new(false, self.latency_trigger_ns, false),
                latency_dump_count: 0,
                last_latency_dump_ns: None,
                dropped_dumps: 0,
                last_drop_detail: None,
                root_latency_handled: false,
                root_latency_dumped: false,
                trigger_dump_refs: Vec::new(),
            },
        );
        Ok(BoundaryBinding {
            session_dir,
            first_seg_seq,
            partition_id,
            boundary_local_id,
        })
    }

    fn observe_call_close_latency(&mut self, engine_id: u64, partition_id: u32, call: RecentCall) {
        let Some(boundary_id) = self.boundaries.iter().find_map(|(&boundary_id, binding)| {
            (binding.engine_id == engine_id && binding.partition_id == partition_id)
                .then_some(boundary_id)
        }) else {
            return;
        };
        let decision = self
            .boundaries
            .get_mut(&boundary_id)
            .expect("boundary found above")
            .observe_latency(&call);
        let fired = match decision {
            LatencyDumpDecision::Ignore => return,
            LatencyDumpDecision::Drop(detail) => {
                self.boundaries
                    .get_mut(&boundary_id)
                    .expect("boundary found above")
                    .record_dropped_dump(detail);
                self.stats.counters.dropped_dumps =
                    self.stats.counters.dropped_dumps.saturating_add(1);
                return;
            }
            LatencyDumpDecision::Fire(fired) => fired,
        };
        let binding = self
            .boundaries
            .get(&boundary_id)
            .expect("boundary found above")
            .clone();
        let is_root_call = call.thread_id == binding.root_thread_id && call.parent_call_id == 0;
        let segment_seq = self
            .session_writers
            .get(&engine_id)
            .and_then(Option::as_ref)
            .map_or(binding.first_seg_seq, SessionStreamWriter::segment_sequence);
        let trigger = format!(
            "{}:thread={}:call={}:node={}",
            fired.reason.wire_name(),
            fired.thread_id,
            fired.call_id,
            fired.node_id
        );
        match self.write_trigger_dump(
            boundary_id,
            &binding,
            &trigger,
            fired.timestamp_ns,
            segment_seq,
            Some(fired.node_id),
        ) {
            Ok(Some(dump_ref)) => self
                .boundaries
                .get_mut(&boundary_id)
                .expect("boundary found above")
                .record_latency_dump(&fired, is_root_call, dump_ref),
            Ok(None) => {
                self.boundaries
                    .get_mut(&boundary_id)
                    .expect("boundary found above")
                    .record_dropped_dump(
                        "call-close latency trigger fired before the flight recorder retained an event"
                            .to_owned(),
                    );
                self.stats.counters.dropped_dumps =
                    self.stats.counters.dropped_dumps.saturating_add(1);
            }
            Err(err) => {
                self.boundaries
                    .get_mut(&boundary_id)
                    .expect("boundary found above")
                    .record_dropped_dump(format!("call-close latency dump failed: {err}"));
                self.stats.counters.dropped_dumps =
                    self.stats.counters.dropped_dumps.saturating_add(1);
                let detail = format!("call-close latency dump failed: {err}");
                let _ = append_typed_boundary_d2(
                    &binding.boundary_dir.join("boundary.bamlmeta"),
                    &TypedBoundaryMeta::Loss(BoundaryLossMeta {
                        timestamp_ns: fired.timestamp_ns,
                        kind: "flight_dump".to_owned(),
                        count: 1,
                        detail,
                    }),
                );
            }
        }
    }

    fn complete_boundary(
        &mut self,
        boundary_id: BoundaryId,
        mut completion: BoundaryCompletion,
    ) -> io::Result<()> {
        let binding = self.boundaries.get(&boundary_id).cloned().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "boundary has not been bound")
        })?;
        completion
            .dump_refs
            .extend(binding.trigger_dump_refs.iter().cloned());
        let engine_id = binding.engine_id;
        let timestamp_ns = self.conv.to_ns(clock::now_ticks());

        if let Some(cct) = self.cct_engines.get_mut(&engine_id) {
            cct.finish_sweep();
            let terminal_ns = cct
                .snapshot()
                .recent_calls
                .last()
                .map_or(timestamp_ns, |call| call.end_ns);
            cct.close_final_window_through(terminal_ns);
            let windows = cct.take_windows();
            let snapshot = cct.snapshot();
            self.persist_cct_windows(engine_id, &windows, &snapshot);
        }

        let events = self.cct_events.get(&engine_id).copied().unwrap_or_default();
        let wall_epoch_ns = u64::try_from(
            self.started_at_epoch_ns
                .saturating_add(u128::from(timestamp_ns)),
        )
        .unwrap_or(u64::MAX);
        let (snapshot_header, last_seg_seq) = {
            let writer = self
                .session_writers
                .get_mut(&engine_id)
                .and_then(Option::as_mut)
                .ok_or_else(|| io::Error::other("boundary session writer is unavailable"))?;
            writer.sync(wall_epoch_ns, timestamp_ns, events)?;
            (writer.boundary_snapshot_header(), writer.segment_sequence())
        };
        let snapshot = self
            .cct_engines
            .get(&engine_id)
            .and_then(|cct| cct.partition_snapshot(binding.partition_id))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "boundary partition still has live threads",
                )
            })?;
        let models = metadata::get_engine_metadata(engine_id)
            .map(|meta| meta.models.entries_after(0))
            .unwrap_or_default();
        write_boundary_snapshot(
            &binding.boundary_dir,
            &snapshot_header,
            boundary_id,
            &binding,
            &snapshot,
            &models,
        )?;

        if let Some(trace) = self.full_traces.remove(&engine_id) {
            let exhausted = trace.exhausted();
            let dump = trace.transcode(engine_id, &self.conv);
            if dump.events != 0 {
                let mut header = build_header(
                    self.process_id,
                    engine_id,
                    self.started_at_epoch_ns,
                    metadata::get_engine_metadata(engine_id).as_ref(),
                    &self.conv,
                );
                header.boundary_id = Some(boundary_id.as_bytes().to_vec());
                header.trigger_reason = Some("full_trace".to_owned());
                header.cct_segment_seq = Some(last_seg_seq);
                let publication = boundary_value_cids(&binding.boundary_dir, boundary_id).and_then(
                    |pinned_cids| {
                        write_exact_artifact(
                            &binding.boundary_dir.join("trace"),
                            &format!("{timestamp_ns}-full-trace"),
                            boundary_id,
                            &header,
                            &dump,
                            pinned_cids,
                        )
                    },
                );
                match publication {
                    Ok(paths) => {
                        completion.dump_refs.push(
                            paths
                                .profile
                                .strip_prefix(&binding.boundary_dir)
                                .unwrap_or(&paths.profile)
                                .to_string_lossy()
                                .into_owned(),
                        );
                    }
                    Err(err) => completion
                        .diagnostics
                        .push(format!("full trace publication failed: {err}")),
                }
            }
            if let Some(exhausted) = exhausted {
                let detail = format!(
                    "TraceBudgetExhausted accepted_bytes={} dropped_bytes={} at_ticks={}",
                    exhausted.accepted_bytes, exhausted.dropped_bytes, exhausted.at_ticks
                );
                completion.diagnostics.push(detail.clone());
                let _ = append_typed_boundary_d2(
                    &binding.boundary_dir.join("boundary.bamlmeta"),
                    &TypedBoundaryMeta::Loss(BoundaryLossMeta {
                        timestamp_ns,
                        kind: "TraceBudgetExhausted".to_owned(),
                        count: exhausted.dropped_bytes,
                        detail,
                    }),
                );
            }
        }

        let mut trigger = completion.trigger.take().or_else(|| {
            let status = completion.status.trim();
            (!status.eq_ignore_ascii_case("ok")
                && !status.eq_ignore_ascii_case("completed")
                && !status.eq_ignore_ascii_case("success"))
            .then(|| "error".to_owned())
        });
        if trigger
            .as_deref()
            .is_some_and(|trigger| trigger.starts_with("latency:"))
        {
            if binding.root_latency_handled {
                completion.diagnostics.push(if binding.root_latency_dumped {
                    "root latency trigger already captured at call close".to_owned()
                } else {
                    "root latency trigger already handled at call close without a dump".to_owned()
                });
                trigger = None;
            } else if binding.latency_dump_count >= MAX_BOUNDARY_DUMPS {
                let detail =
                    format!("boundary dump cap reached ({MAX_BOUNDARY_DUMPS}) at completion");
                self.boundaries
                    .get_mut(&boundary_id)
                    .expect("boundary exists until completion")
                    .record_dropped_dump(detail);
                self.stats.counters.dropped_dumps =
                    self.stats.counters.dropped_dumps.saturating_add(1);
                trigger = None;
            } else if binding.last_latency_dump_ns.is_some_and(|last| {
                timestamp_ns.saturating_sub(last) < LATENCY_DUMP_MIN_INTERVAL_NS
            }) {
                let detail = format!(
                    "completion latency dump rate-limited; minimum interval is \
                     {LATENCY_DUMP_MIN_INTERVAL_NS}ns"
                );
                self.boundaries
                    .get_mut(&boundary_id)
                    .expect("boundary exists until completion")
                    .record_dropped_dump(detail);
                self.stats.counters.dropped_dumps =
                    self.stats.counters.dropped_dumps.saturating_add(1);
                trigger = None;
            }
        }
        if let Some(trigger) = trigger {
            let trigger_node = snapshot
                .nodes
                .iter()
                .filter(|node| node.counters.ends_err != 0)
                .max_by_key(|node| node.counters.ends_err)
                .map(|node| node.node_id);
            match self.write_trigger_dump(
                boundary_id,
                &binding,
                &trigger,
                timestamp_ns,
                last_seg_seq,
                trigger_node,
            ) {
                Ok(Some(dump_ref)) => completion.dump_refs.push(dump_ref),
                Ok(None) => completion
                    .diagnostics
                    .push("trigger fired before the flight recorder retained an event".to_owned()),
                Err(err) => {
                    let detail = format!("trigger dump failed: {err}");
                    completion.diagnostics.push(detail.clone());
                    let _ = append_typed_boundary_d2(
                        &binding.boundary_dir.join("boundary.bamlmeta"),
                        &TypedBoundaryMeta::Loss(BoundaryLossMeta {
                            timestamp_ns,
                            kind: "flight_dump".to_owned(),
                            count: 1,
                            detail,
                        }),
                    );
                }
            }
        }

        let (dropped_dumps, last_drop_detail) = self
            .boundaries
            .get(&boundary_id)
            .map_or((0, None), |binding| {
                (binding.dropped_dumps, binding.last_drop_detail.clone())
            });
        if dropped_dumps != 0 {
            let detail = format!(
                "dropped_dumps={dropped_dumps}; last={}",
                last_drop_detail.as_deref().unwrap_or("unspecified")
            );
            completion.diagnostics.push(detail.clone());
            append_typed_boundary_d2(
                &binding.boundary_dir.join("boundary.bamlmeta"),
                &TypedBoundaryMeta::Loss(BoundaryLossMeta {
                    timestamp_ns,
                    kind: "dropped_dumps".to_owned(),
                    count: dropped_dumps,
                    detail,
                }),
            )?;
        }

        let calls = snapshot
            .nodes
            .iter()
            .map(|node| node.counters.enters)
            .fold(0_u64, u64::saturating_add);
        let errors = snapshot
            .nodes
            .iter()
            .map(|node| node.counters.ends_err)
            .fold(0_u64, u64::saturating_add);
        if snapshot.health.degraded_partitions != 0 {
            completion
                .diagnostics
                .push("partition completed in degraded CCT mode".to_owned());
        }
        append_typed_boundary_d2(
            &binding.boundary_dir.join("boundary.bamlmeta"),
            &TypedBoundaryMeta::Complete(BoundaryCompleteMeta {
                status: completion.status,
                completed_ms: wall_epoch_ns / 1_000_000,
                last_seg_seq,
                counts: BoundaryCounts {
                    events: calls.saturating_mul(2),
                    nodes: u64::try_from(snapshot.nodes.len()).unwrap_or(u64::MAX),
                    calls,
                    errors,
                    captures: 0,
                },
                diagnostics: completion.diagnostics,
                dump_refs: completion.dump_refs,
            }),
        )?;
        let released = self
            .cct_engines
            .get_mut(&engine_id)
            .is_some_and(|cct| cct.release_partition(binding.partition_id));
        if !released {
            report(format_args!(
                "boundary {} was durable but partition {} could not be released",
                boundary_id.to_wire_string(),
                binding.partition_id
            ));
        }
        self.boundaries.remove(&boundary_id);
        boundary::finish_registration(boundary_id);
        Ok(())
    }

    fn write_trigger_dump(
        &self,
        boundary_id: BoundaryId,
        binding: &BoundBoundary,
        trigger: &str,
        timestamp_ns: u64,
        segment_seq: u32,
        trigger_node: Option<u32>,
    ) -> io::Result<Option<String>> {
        let dump = self
            .flight_recorder
            .transcode_engine(binding.engine_id, &self.conv);
        if dump.events == 0 {
            return Ok(None);
        }
        let mut header = build_header(
            self.process_id,
            binding.engine_id,
            self.started_at_epoch_ns,
            metadata::get_engine_metadata(binding.engine_id).as_ref(),
            &self.conv,
        );
        header.boundary_id = Some(boundary_id.as_bytes().to_vec());
        header.trigger_reason = Some(trigger.to_owned());
        header.trigger_node_id = trigger_node;
        header.cct_segment_seq = Some(segment_seq);
        let stem = format!("{timestamp_ns}-{trigger}-{}", binding.boundary_local_id);
        let pinned_cids = boundary_value_cids(&binding.boundary_dir, boundary_id)?;
        let paths = write_exact_artifact(
            &binding.boundary_dir.join("flight"),
            &stem,
            boundary_id,
            &header,
            &dump,
            pinned_cids,
        )?;
        let dump_ref = paths
            .profile
            .strip_prefix(&binding.boundary_dir)
            .unwrap_or(&paths.profile)
            .to_string_lossy()
            .into_owned();
        append_typed_boundary_d2(
            &binding.boundary_dir.join("boundary.bamlmeta"),
            &TypedBoundaryMeta::Trigger(BoundaryTriggerMeta {
                trigger: trigger.to_owned(),
                timestamp_ns,
                dump_ref: Some(dump_ref.clone()),
            }),
        )?;
        Ok(Some(dump_ref))
    }

    fn maybe_heartbeat(&mut self) {
        if self.last_heartbeat.elapsed() < HEARTBEAT_INTERVAL {
            return;
        }
        self.last_heartbeat = Instant::now();
        // Heartbeat cadence is the designated ≥1 s refinement slot for the
        // x86 TSC rate (no-op for exact-rate sources / once refined).
        self.conv.maybe_refine();
        let ts = self.conv.to_ns(clock::now_ticks());
        let mut closed = Vec::new();
        for (&engine_id, cct) in &mut self.cct_engines {
            cct.close_windows_through(ts);
            let windows = cct.take_windows();
            if !windows.is_empty() {
                closed.push((engine_id, windows, cct.snapshot()));
            }
        }
        for (engine_id, windows, snapshot) in closed {
            self.persist_cct_windows(engine_id, &windows, &snapshot);
        }
        let wall_epoch_ns = u64::try_from(self.started_at_epoch_ns.saturating_add(u128::from(ts)))
            .unwrap_or(u64::MAX);
        let mut session_failed = Vec::new();
        for (&engine_id, slot) in &mut self.session_writers {
            if let Some(writer) = slot {
                let events = self.cct_events.get(&engine_id).copied().unwrap_or_default();
                if writer.watermark(wall_epoch_ns, ts, events, false).is_err() {
                    session_failed.push(engine_id);
                }
            }
        }
        for engine_id in session_failed {
            self.fail_session_writer(
                engine_id,
                &io::Error::other("session heartbeat watermark failed"),
            );
        }
        let event = pb::DiskEventV1 {
            event: Some(pb::disk_event_v1::Event::Heartbeat(pb::Heartbeat {
                timestamp_ns: ts,
            })),
        };
        let mut failed = Vec::new();
        for (&engine_id, slot) in &mut self.writers {
            if let Some(writer) = slot {
                writer.encode_event(&event);
                self.stats.counters.flushes = self.stats.counters.flushes.saturating_add(1);
                if writer.flush_buffered().is_err() {
                    failed.push(engine_id);
                }
            }
        }
        for engine_id in failed {
            self.fail_writer(engine_id, &io::Error::other("heartbeat write failed"));
        }
    }

    /// Sync (engine close / explicit flush) and close one engine's file;
    /// later records for it are tombstoned.
    fn close_engine(&mut self, engine_id: u64) {
        if let Some(mut cct) = self.cct_engines.remove(&engine_id) {
            let end_ns = cct
                .snapshot()
                .recent_calls
                .last()
                .map_or_else(|| self.conv.to_ns(clock::now_ticks()), |call| call.end_ns);
            cct.close_final_window_through(end_ns);
            let windows = cct.take_windows();
            let snapshot = cct.snapshot();
            self.persist_cct_windows(engine_id, &windows, &snapshot);
        }
        if let Some(Some(writer)) = self.session_writers.remove(&engine_id) {
            let bytes = writer.bytes_written();
            let ended_epoch_ns = u64::try_from(
                self.started_at_epoch_ns
                    .saturating_add(u128::from(self.conv.to_ns(clock::now_ticks()))),
            )
            .unwrap_or(u64::MAX);
            self.stats.counters.fsyncs = self.stats.counters.fsyncs.saturating_add(1);
            if let Err(err) = writer.finish(ended_epoch_ns, "engine_closed") {
                self.stats.counters.writer_failures =
                    self.stats.counters.writer_failures.saturating_add(1);
                report(format_args!(
                    "seal of v2 CCT session for engine {engine_id} failed: {err}"
                ));
            }
            self.closed_session_bytes = self.closed_session_bytes.saturating_add(bytes);
        }
        if let Some(Some(mut writer)) = self.writers.remove(&engine_id) {
            self.stats.counters.fsyncs = self.stats.counters.fsyncs.saturating_add(1);
            if let Err(err) = writer.sync() {
                self.stats.counters.writer_failures =
                    self.stats.counters.writer_failures.saturating_add(1);
                report(format_args!(
                    "sync of {} on engine close failed: {err}",
                    writer.path().display()
                ));
            }
            self.closed_profile_bytes = self
                .closed_profile_bytes
                .saturating_add(writer.bytes_written());
        }
        let _ = metadata::remove_engine_metadata(engine_id);
        self.cct_events.remove(&engine_id);
        self.pending_latency_calls.remove(&engine_id);
        self.closed_engines.insert(engine_id);
    }

    /// Durable flush of every open file (`fsync`); the explicit-flush path.
    fn sync_files(&mut self) {
        let ts = self.conv.to_ns(clock::now_ticks());
        let mut closed = Vec::new();
        for (&engine_id, cct) in &mut self.cct_engines {
            cct.close_windows_through(ts);
            let windows = cct.take_windows();
            if !windows.is_empty() {
                closed.push((engine_id, windows, cct.snapshot()));
            }
        }
        for (engine_id, windows, snapshot) in closed {
            self.persist_cct_windows(engine_id, &windows, &snapshot);
        }

        let mut failed = Vec::new();
        for (&engine_id, slot) in &mut self.writers {
            if let Some(writer) = slot {
                self.stats.counters.fsyncs = self.stats.counters.fsyncs.saturating_add(1);
                if writer.sync().is_err() {
                    failed.push(engine_id);
                }
            }
        }
        for engine_id in failed {
            self.fail_writer(engine_id, &io::Error::other("sync failed"));
        }

        let wall_epoch_ns = u64::try_from(self.started_at_epoch_ns.saturating_add(u128::from(ts)))
            .unwrap_or(u64::MAX);
        let mut session_failed = Vec::new();
        for (&engine_id, slot) in &mut self.session_writers {
            if let Some(writer) = slot {
                let events = self.cct_events.get(&engine_id).copied().unwrap_or_default();
                self.stats.counters.fsyncs = self.stats.counters.fsyncs.saturating_add(1);
                if writer.sync(wall_epoch_ns, ts, events).is_err() {
                    session_failed.push(engine_id);
                }
            }
        }
        for engine_id in session_failed {
            self.fail_session_writer(engine_id, &io::Error::other("session sync failed"));
        }
    }

    fn flush_files(&mut self) {
        let mut failed = Vec::new();
        for (&engine_id, slot) in &mut self.writers {
            if let Some(writer) = slot {
                self.stats.counters.flushes = self.stats.counters.flushes.saturating_add(1);
                if writer.flush().is_err() {
                    failed.push(engine_id);
                }
            }
        }
        for engine_id in failed {
            self.fail_writer(engine_id, &io::Error::other("flush failed"));
        }
        let mut session_failed = Vec::new();
        for (&engine_id, slot) in &mut self.session_writers {
            if let Some(writer) = slot
                && writer.flush().is_err()
            {
                session_failed.push(engine_id);
            }
        }
        for engine_id in session_failed {
            self.fail_session_writer(engine_id, &io::Error::other("session flush failed"));
        }
    }

    fn profile_bytes(&self) -> u64 {
        self.closed_profile_bytes
            .saturating_add(self.closed_session_bytes)
            .saturating_add(
                self.writers
                    .values()
                    .filter_map(Option::as_ref)
                    .map(ProfileWriter::bytes_written)
                    .fold(0u64, u64::saturating_add),
            )
            .saturating_add(
                self.session_writers
                    .values()
                    .filter_map(Option::as_ref)
                    .map(SessionStreamWriter::bytes_written)
                    .fold(0u64, u64::saturating_add),
            )
    }

    fn write_stats(&mut self, reason: &'static str, ctx: &RingCtx) {
        if !self.stats.enabled() {
            return;
        }
        let process_id = uuid::Uuid::from_bytes(self.process_id).simple().to_string();
        let snapshot = StatsSnapshot {
            process_id: &process_id,
            profile_bytes: self.profile_bytes(),
            ring_live_bytes: ctx.live_bytes(),
            ring_peak_bytes: ctx.peak_bytes(),
            reason,
        };
        if let Err(err) = self.stats.write(snapshot)
            && !self.stats_write_failure_reported
        {
            self.stats_write_failure_reported = true;
            report(format_args!("cannot write BAML_OBS_STATS snapshot: {err}"));
        }
    }
}

fn append_typed_boundary_d2(path: &Path, value: &TypedBoundaryMeta) -> io::Result<u64> {
    let (kind, payload) = encode_typed_boundary_meta(value)?;
    append_meta_d2(path, kind, &payload)
}

fn write_boundary_snapshot(
    boundary_dir: &Path,
    header: &BcctHeader,
    boundary_id: BoundaryId,
    binding: &BoundBoundary,
    snapshot: &crate::prof::cct::CctSnapshot,
    models: &[crate::prof::models::ModelMetadataEntry],
) -> io::Result<()> {
    let final_path = boundary_dir.join("cct.bamlcct");
    if final_path.is_file() {
        let scan = scan_bcct_bytes(&fs::read(&final_path)?)?;
        return if matches!(scan.state, SegmentState::Sealed(_)) {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "existing boundary CCT snapshot is not sealed",
            ))
        };
    }
    let mut artifact = BoundarySnapshot::create(boundary_dir, header)?;
    let writer = artifact.writer_mut();

    let mut nodes = snapshot.nodes.iter().collect::<Vec<_>>();
    nodes.sort_by_key(|node| node.node_id);
    let node_map = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.node_id, u32::try_from(index + 1).unwrap_or(u32::MAX)))
        .collect::<HashMap<_, _>>();
    let births = nodes
        .iter()
        .map(|node| NodeBirthRow {
            node_id: node_map[&node.node_id],
            parent_node_id: if node.identity.parent == 0 {
                0
            } else {
                node_map.get(&node.identity.parent).copied().unwrap_or(0)
            },
            function_id: node.identity.function_id.0,
            logical_thread_id: node.identity.first_thread_id,
            partition_id: 1,
        })
        .collect::<Vec<_>>();
    if !births.is_empty() {
        writer.append(&BlockRows::NodeBirth(births), 0, 0)?;
    }
    let totals = nodes
        .iter()
        .flat_map(|node| snapshot_counter_rows(node_map[&node.node_id], node.counters))
        .collect::<Vec<_>>();
    if !totals.is_empty() {
        writer.append(&BlockRows::NodeTotal(totals), 0, 0)?;
    }
    let histograms = nodes
        .iter()
        .filter(|node| node.histogram.iter().any(|count| *count != 0))
        .map(|node| CctHistogramRow {
            node_id: node_map[&node.node_id],
            duration_buckets: node.histogram,
        })
        .collect::<Vec<_>>();
    if !histograms.is_empty() {
        writer.append(&BlockRows::CctHistogram(histograms), 0, 0)?;
    }

    let relevant_models = snapshot
        .llm
        .iter()
        .map(|row| row.model_id)
        .collect::<HashSet<_>>();
    let model_births = models
        .iter()
        .filter(|model| relevant_models.contains(&model.model_id))
        .map(|model| ModelBirthRow {
            model_id: model.model_id,
            name: model.name.clone(),
        })
        .collect::<Vec<_>>();
    if !model_births.is_empty() {
        writer.append(&BlockRows::ModelBirth(model_births), 0, 0)?;
    }
    let llm = snapshot
        .llm
        .iter()
        .flat_map(|row| {
            node_map
                .get(&row.node_id)
                .copied()
                .map(|node_id| snapshot_llm_rows(node_id, row.model_id, row.counters))
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    if !llm.is_empty() {
        writer.append(&BlockRows::LlmDelta(llm), 0, 0)?;
    }

    let mut edges = snapshot.spawn_edges.iter().collect::<Vec<_>>();
    edges.sort_by_key(|edge| edge.edge_id);
    let edge_map = edges
        .iter()
        .enumerate()
        .map(|(index, edge)| (edge.edge_id, u32::try_from(index + 1).unwrap_or(u32::MAX)))
        .collect::<HashMap<_, _>>();
    let spawn_edges = edges
        .iter()
        .flat_map(|edge| {
            snapshot_spawn_rows(
                edge_map[&edge.edge_id],
                node_map
                    .get(&edge.identity.spawn_context)
                    .copied()
                    .unwrap_or(0),
                edge.identity.child_entry_function,
                node_map
                    .get(&edge.identity.child_root_node)
                    .copied()
                    .unwrap_or(0),
                edge.counters,
            )
        })
        .collect::<Vec<_>>();
    if !spawn_edges.is_empty() {
        writer.append(&BlockRows::SpawnEdge(spawn_edges), 0, 0)?;
    }

    writer.append(
        &BlockRows::PartitionBind(vec![PartitionBindRow {
            partition_id: 1,
            boundary_local_id: binding.boundary_local_id,
            boundary_id: boundary_id.as_bytes(),
            created_ms: binding.created_ms,
        }]),
        0,
        0,
    )?;
    artifact.seal_and_commit()?;
    Ok(())
}

fn snapshot_counter_rows(
    node_id: u32,
    mut counters: crate::prof::cct::NodeCounters,
) -> Vec<CctDeltaRow> {
    let mut rows = Vec::new();
    while counters.enters != 0
        || counters.ends_ok != 0
        || counters.ends_err != 0
        || counters.ends_cancel != 0
        || counters.ends_exit != 0
        || counters.total_ns != 0
        || counters.self_ns != 0
        || counters.await_ns != 0
    {
        rows.push(CctDeltaRow {
            node_id,
            enters: take_snapshot_u32(&mut counters.enters),
            ends_ok: take_snapshot_u32(&mut counters.ends_ok),
            ends_err: take_snapshot_u32(&mut counters.ends_err),
            ends_cancel: take_snapshot_u32(&mut counters.ends_cancel),
            ends_exit: take_snapshot_u32(&mut counters.ends_exit),
            total_ns: std::mem::take(&mut counters.total_ns),
            self_ns: std::mem::take(&mut counters.self_ns),
            await_ns: std::mem::take(&mut counters.await_ns),
        });
    }
    rows
}

/// Snapshot all value roots durably attributable to this boundary. Exact
/// artifacts pin the conservative boundary set because the profile wire does
/// not repeat value references; the boundary manifest/value log is the sole
/// durable join authority. Reading both also closes the brief D1 ordering
/// window where a value record is committed just before its manifest append.
fn boundary_value_cids(boundary_dir: &Path, boundary_id: BoundaryId) -> io::Result<Vec<Cid>> {
    let mut roots = BTreeSet::new();
    match CidManifestReader::read(boundary_dir.join("manifest.bamlcids")) {
        Ok(outcome) => {
            if outcome.manifest.boundary_id != boundary_id {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "value CID manifest boundary does not match exact artifact boundary",
                ));
            }
            roots.extend(outcome.manifest.cids);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    roots.extend(derive_unsealed_bamlvalue_roots(boundary_dir)?);
    Ok(roots.into_iter().collect())
}

fn take_snapshot_u32(value: &mut u64) -> u32 {
    let taken = (*value).min(u64::from(u32::MAX));
    *value -= taken;
    u32::try_from(taken).expect("bounded to u32")
}

fn snapshot_llm_rows(
    node_id: u32,
    model_id: u32,
    mut counters: crate::prof::cct::LlmCounters,
) -> Vec<LlmDeltaRow> {
    let mut rows = Vec::new();
    while counters.calls != 0
        || counters.tokens_in != 0
        || counters.tokens_out != 0
        || counters.provider_errs != 0
        || counters.parse_errs != 0
    {
        rows.push(LlmDeltaRow {
            node_id,
            llm_calls_delta: take_snapshot_u32(&mut counters.calls),
            tokens_in_delta: std::mem::take(&mut counters.tokens_in),
            tokens_out_delta: std::mem::take(&mut counters.tokens_out),
            provider_errs_delta: take_snapshot_u32(&mut counters.provider_errs),
            parse_errs_delta: take_snapshot_u32(&mut counters.parse_errs),
            model_id,
        });
    }
    rows
}

fn snapshot_spawn_rows(
    edge_id: u32,
    parent_node: u32,
    entry_fn: u32,
    child_root_node: u32,
    mut counters: crate::prof::cct::SpawnCounters,
) -> Vec<SpawnEdgeRow> {
    let mut rows = Vec::new();
    while counters.spawned != 0
        || counters.completed != 0
        || counters.errored != 0
        || counters.cancelled != 0
        || counters.running_ns != 0
        || counters.awaiting_ns != 0
    {
        rows.push(SpawnEdgeRow {
            edge_id,
            parent_node,
            entry_fn,
            child_root_node,
            spawn_delta: take_snapshot_u32(&mut counters.spawned),
            completed_delta: take_snapshot_u32(&mut counters.completed),
            errored_delta: take_snapshot_u32(&mut counters.errored),
            cancelled_delta: take_snapshot_u32(&mut counters.cancelled),
            running_ns_delta: std::mem::take(&mut counters.running_ns),
            awaiting_ns_delta: std::mem::take(&mut counters.awaiting_ns),
        });
    }
    rows
}

fn ensure_profile_dir_ignored(profile_dir: &Path) -> io::Result<bool> {
    let Some(baml_dir) = nearest_baml_dir(profile_dir) else {
        return Ok(false);
    };

    fs::create_dir_all(&baml_dir)?;
    let ignore_path = baml_dir.join(".gitignore");
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&ignore_path)
    {
        Ok(mut file) => {
            file.write_all(b"*\n")?;
            Ok(true)
        }
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            let contents = fs::read(&ignore_path)?;
            if has_standalone_star_line(&contents) {
                return Ok(true);
            }

            let mut file = OpenOptions::new().append(true).open(&ignore_path)?;
            if contents.is_empty() || contents.ends_with(b"\n") {
                file.write_all(b"*\n")?;
            } else {
                file.write_all(b"\n*\n")?;
            }
            Ok(true)
        }
        Err(err) => Err(err),
    }
}

fn nearest_baml_dir(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|ancestor| ancestor.file_name().is_some_and(|name| name == ".baml"))
        .map(Path::to_path_buf)
}

fn has_standalone_star_line(contents: &[u8]) -> bool {
    contents
        .split(|byte| *byte == b'\n')
        .any(|line| trim_ascii_space_and_cr(line) == b"*")
}

fn trim_ascii_space_and_cr(mut bytes: &[u8]) -> &[u8] {
    while matches!(bytes.first(), Some(b' ' | b'\t' | b'\r')) {
        bytes = &bytes[1..];
    }
    while matches!(bytes.last(), Some(b' ' | b'\t' | b'\r')) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

/// The process UUID, minted once (uuid v4).
fn process_id() -> [u8; 16] {
    ProcessEuid::current().0
}

/// Consumer-side diagnostics. The consumer must never panic (it would die
/// silently and the rings would grow to the cap), so problems are reported
/// and degraded around — including reporting itself: `eprintln!` panics on a
/// closed stderr, so write errors are swallowed instead.
fn report(msg: std::fmt::Arguments<'_>) {
    use std::io::Write as _;
    let _ = writeln!(std::io::stderr(), "bex-prof-consumer: {msg}");
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

    use pb::disk_event_v1::Event;

    use super::*;
    use crate::{
        ids::{BexCallId, BexThreadId, FunctionId},
        prof::{
            EngineProfileMetadata, FunctionMetaEntry,
            cct::CctEvent,
            file::{header_started_at_epoch_ns, read_bamlprof},
            record::{
                CallSiteSourceSpan, FunctionEndStatus, MAX_RECORD_LEN, RawRecord, ThreadEndStatus,
            },
            register_engine_metadata,
        },
    };

    fn temp_dir(tag: &str) -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        std::env::temp_dir().join(format!(
            "bamlprof-{}-{}-{tag}",
            std::process::id(),
            N.fetch_add(1, AtomicOrdering::Relaxed)
        ))
    }

    fn leak<T>(value: T) -> &'static T {
        Box::leak(Box::new(value))
    }

    #[test]
    fn consumer_process_id_matches_runtime_process_euid() {
        assert_eq!(process_id(), ProcessEuid::current().0);
    }

    #[test]
    fn call_close_latency_policy_rate_limits_caps_and_marks_root() {
        let mut binding = BoundBoundary {
            engine_id: 1,
            partition_id: 2,
            boundary_local_id: 3,
            boundary_dir: PathBuf::new(),
            created_ms: 0,
            first_seg_seq: 0,
            root_thread_id: 7,
            latency_triggers: TriggerSet::new(false, Some(100), false),
            latency_dump_count: 0,
            last_latency_dump_ns: None,
            dropped_dumps: 0,
            last_drop_detail: None,
            root_latency_handled: false,
            root_latency_dumped: false,
            trigger_dump_refs: Vec::new(),
        };
        let call = |call_id, start_ns, end_ns| RecentCall {
            thread_id: 7,
            call_id,
            node_id: u32::try_from(call_id).unwrap(),
            parent_call_id: 0,
            start_ns,
            end_ns,
            status: FunctionEndStatus::Ok,
            dump_ref: 0,
        };

        assert!(matches!(
            binding.observe_latency(&call(1, 10, 109)),
            LatencyDumpDecision::Ignore
        ));
        let first = match binding.observe_latency(&call(1, 100, 200)) {
            LatencyDumpDecision::Fire(fired) => fired,
            decision => panic!("expected first latency dump, got {decision:?}"),
        };
        binding.record_latency_dump(&first, true, "flight/first.bamlprof".to_owned());
        assert!(binding.root_latency_handled);
        assert!(binding.root_latency_dumped);

        let too_soon = call(
            2,
            LATENCY_DUMP_MIN_INTERVAL_NS,
            LATENCY_DUMP_MIN_INTERVAL_NS + 199,
        );
        let detail = match binding.observe_latency(&too_soon) {
            LatencyDumpDecision::Drop(detail) => detail,
            decision => panic!("expected rate-limit drop, got {decision:?}"),
        };
        assert!(detail.contains("rate-limited"));
        binding.record_dropped_dump(detail);

        let mut timestamp = first.timestamp_ns + LATENCY_DUMP_MIN_INTERVAL_NS;
        while binding.latency_dump_count < MAX_CALL_CLOSE_LATENCY_DUMPS {
            let next = call(
                u64::from(binding.latency_dump_count) + 10,
                timestamp - 100,
                timestamp,
            );
            let fired = match binding.observe_latency(&next) {
                LatencyDumpDecision::Fire(fired) => fired,
                decision => panic!("expected spaced latency dump, got {decision:?}"),
            };
            binding.record_latency_dump(&fired, false, format!("flight/{}.bamlprof", fired.id));
            timestamp = timestamp.saturating_add(LATENCY_DUMP_MIN_INTERVAL_NS);
        }
        assert!(matches!(
            binding.observe_latency(&call(999, timestamp - 100, timestamp)),
            LatencyDumpDecision::Drop(detail) if detail.contains("cap reached")
        ));
        assert_eq!(binding.latency_dump_count, MAX_CALL_CLOSE_LATENCY_DUMPS);
        assert_eq!(binding.dropped_dumps, 1);
    }

    #[test]
    fn call_close_latency_dump_carries_exact_trigger_identity() {
        let root = temp_dir("call-close-latency");
        let profile_dir = root.join("project/.baml/profiles");
        let boundary_dir = root.join("boundary");
        std::fs::create_dir_all(&boundary_dir).unwrap();
        let mut state = ConsumerState::new_configured(
            profile_dir,
            TickConverter::identity(),
            ProfilePipeline::Cct,
            ObsLayout::V2,
            None,
        );
        state.latency_trigger_ns = Some(100);
        state.flight_recorder = FlightRecorder::new(4096);

        const ENGINE: u64 = 77;
        let mut raw = Vec::new();
        let mut scratch = [0_u8; MAX_RECORD_LEN];
        for record in [
            RawRecord::CallFunction {
                flags: 0,
                thread_id: BexThreadId(7),
                call_id: BexCallId(9),
                parent_call_id: BexCallId(1),
                function_id: FunctionId(23),
                call_site: None,
                ts_ticks: 100,
            },
            RawRecord::EndFunction {
                status: FunctionEndStatus::Ok,
                thread_id: BexThreadId(7),
                call_id: BexCallId(9),
                ts_ticks: 300,
            },
        ] {
            let len = record.encode(&mut scratch);
            raw.extend_from_slice(&scratch[..len]);
        }
        assert!(state.flight_recorder.retain(ENGINE, &raw));

        let boundary_id = BoundaryId::from_bytes([0x61; 16]);
        state.boundaries.insert(
            boundary_id,
            BoundBoundary {
                engine_id: ENGINE,
                partition_id: 4,
                boundary_local_id: 5,
                boundary_dir: boundary_dir.clone(),
                created_ms: 0,
                first_seg_seq: 3,
                root_thread_id: 7,
                latency_triggers: TriggerSet::new(false, Some(100), false),
                latency_dump_count: 0,
                last_latency_dump_ns: None,
                dropped_dumps: 0,
                last_drop_detail: None,
                root_latency_handled: false,
                root_latency_dumped: false,
                trigger_dump_refs: Vec::new(),
            },
        );
        state.observe_call_close_latency(
            ENGINE,
            4,
            RecentCall {
                thread_id: 7,
                call_id: 9,
                node_id: 23,
                parent_call_id: 1,
                start_ns: 100,
                end_ns: 300,
                status: FunctionEndStatus::Ok,
                dump_ref: 0,
            },
        );

        let binding = &state.boundaries[&boundary_id];
        assert_eq!(binding.latency_dump_count, 1);
        assert_eq!(binding.trigger_dump_refs.len(), 1);
        let parsed = read_bamlprof(&boundary_dir.join(&binding.trigger_dump_refs[0])).unwrap();
        assert_eq!(parsed.header.trigger_node_id, Some(23));
        let reason = parsed.header.trigger_reason.as_deref().unwrap();
        assert!(reason.contains("thread=7"));
        assert!(reason.contains("call=9"));
        assert!(reason.contains("node=23"));
        assert_eq!(parsed.header.cct_segment_seq, Some(3));

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn pending_latency_call_resolves_after_causal_end_deferral() {
        let mut cct = EngineCct::default();
        cct.ingest(CctEvent::StartThread {
            flags: 0,
            thread_id: 7,
            parent_thread_id: 0,
            parent_call_id: 0,
            timestamp_ns: 0,
            name: None,
        });
        let partition = cct.partition_for_thread(7).unwrap();
        cct.ingest(CctEvent::EndFunction {
            status: FunctionEndStatus::Ok,
            thread_id: 7,
            call_id: 9,
            timestamp_ns: 300,
        });
        let mut pending = vec![PendingLatencyCall {
            partition_id: Some(partition),
            thread_id: 7,
            call_id: 9,
        }];
        let mut closed = Vec::new();
        ConsumerState::resolve_pending_latency_calls(&cct, &mut pending, &mut closed);
        assert_eq!(pending.len(), 1);
        assert!(closed.is_empty());

        cct.ingest(CctEvent::CallFunction {
            flags: 0,
            thread_id: 7,
            call_id: 9,
            parent_call_id: 0,
            function_id: FunctionId(23),
            timestamp_ns: 100,
        });
        ConsumerState::resolve_pending_latency_calls(&cct, &mut pending, &mut closed);
        assert!(pending.is_empty());
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].1.node_id, 2);
        assert_eq!(closed[0].1.start_ns, 100);
        assert_eq!(closed[0].1.end_ns, 300);
    }

    /// C12 acceptance: structural overload in shed mode remains bounded,
    /// reaches the CCT degradation marker, and is reflected by the same
    /// consumer counter written to the machine-readable stats snapshot.
    #[test]
    fn c12_shed_saturation_is_bounded_degraded_and_reported() {
        const ENGINE: u64 = 0xC120_0001;
        const CAP_BYTES: usize = 1_024;
        const SEGMENT_BYTES: usize = 64;

        let root = temp_dir("c12-shed");
        let profile_dir = root.join("project/.baml/profiles");
        let stats_path = root.join("obs-stats.json");
        let registry: &'static Registry = leak(Registry::new());
        let ctx: &'static RingCtx = leak(RingCtx::new_with_policy(
            CAP_BYTES,
            config::RingOverflowPolicy::Shed,
        ));
        let handle = registry.acquire(ctx, SEGMENT_BYTES, 0, ENGINE);
        let mut buffer = [0_u8; MAX_RECORD_LEN];
        let mut push = |record: RawRecord<'_>| {
            let len = record.encode(&mut buffer);
            // SAFETY: this test thread is the live ring claimant.
            unsafe { handle.push(&buffer[..len]) };
        };
        push(RawRecord::StartThread {
            flags: 0,
            thread_id: BexThreadId(1),
            parent_thread_id: BexThreadId(0),
            parent_call_id: BexCallId(0),
            ts_ticks: 1,
            name: b"",
        });
        for call_id in 1..=256 {
            push(RawRecord::CallFunction {
                flags: 0,
                thread_id: BexThreadId(1),
                call_id: BexCallId(call_id),
                parent_call_id: BexCallId(call_id - 1),
                function_id: FunctionId(16),
                call_site: None,
                ts_ticks: call_id + 1,
            });
        }

        let mut state = ConsumerState::new_configured(
            profile_dir.clone(),
            TickConverter::identity(),
            ProfilePipeline::Cct,
            ObsLayout::V2,
            Some(stats_path.clone()),
        );
        let env = ConsumerEnv {
            registry,
            ctx,
            dir: profile_dir,
            wake_interval: Duration::from_millis(1),
            clock: ClockMode::Fixed(TickConverter::identity()),
            pipeline: ProfilePipeline::Cct,
            obs_layout: ObsLayout::V2,
            obs_stats_path: None,
        };
        assert!(state.sweep_once(&env));

        let dropped = state.stats.counters.shed_structural_ranges;
        assert!(dropped > 0, "the deterministic saturation must shed");
        assert!(
            state.stats.counters.events > 0,
            "retained prefix must drain"
        );
        assert!(ctx.live_bytes() <= CAP_BYTES);
        assert!(ctx.peak_bytes() <= CAP_BYTES);

        let snapshot = state
            .cct_engines
            .get(&ENGINE)
            .expect("retained structural prefix creates a CCT")
            .snapshot();
        assert_eq!(snapshot.health.shed_ranges, 1);
        assert_eq!(snapshot.health.shed_events, dropped);
        assert!(snapshot.health.degraded_partitions >= 1);

        state.write_stats("c12-acceptance", ctx);
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&stats_path).unwrap()).unwrap();
        assert_eq!(report["shed"]["structural_ranges"].as_u64(), Some(dropped));
        assert_eq!(
            report["ring_peak_bytes"].as_u64(),
            Some(u64::try_from(ctx.peak_bytes()).unwrap())
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn profile_dir_ignore_marker_created_for_baml_artifact_dir() {
        let root = temp_dir("ignore-create");
        let profile_dir = root.join("project/.baml/profiles");

        assert!(ensure_profile_dir_ignored(&profile_dir).unwrap());
        assert_eq!(
            std::fs::read(root.join("project/.baml/.gitignore")).unwrap(),
            b"*\n"
        );
        assert!(
            !profile_dir.exists(),
            "profile dir creation remains ProfileWriter's responsibility"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn profile_dir_ignore_marker_appends_without_deleting_existing_rules() {
        let root = temp_dir("ignore-append");
        let baml_dir = root.join("project/.baml");
        let profile_dir = baml_dir.join("profiles");
        std::fs::create_dir_all(&baml_dir).unwrap();
        std::fs::write(baml_dir.join(".gitignore"), b"# keep this\n!.keep\n").unwrap();

        assert!(ensure_profile_dir_ignored(&profile_dir).unwrap());
        assert_eq!(
            std::fs::read(baml_dir.join(".gitignore")).unwrap(),
            b"# keep this\n!.keep\n*\n"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn profile_dir_ignore_marker_does_not_duplicate_standalone_star() {
        let root = temp_dir("ignore-existing");
        let baml_dir = root.join("project/.baml");
        let profile_dir = baml_dir.join("profiles");
        let original = b"# keep this\n  * \r\n!.keep\n";
        std::fs::create_dir_all(&baml_dir).unwrap();
        std::fs::write(baml_dir.join(".gitignore"), original).unwrap();

        assert!(ensure_profile_dir_ignored(&profile_dir).unwrap());
        assert_eq!(
            std::fs::read(baml_dir.join(".gitignore")).unwrap(),
            original
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn profile_dir_ignore_marker_ignores_custom_dirs_outside_baml() {
        let root = temp_dir("ignore-custom");
        let profile_dir = root.join("profiles");

        assert!(!ensure_profile_dir_ignored(&profile_dir).unwrap());
        assert!(
            !root.exists(),
            "custom dirs outside .baml remain user-managed"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn consumer_state_prepares_baml_ignore_marker_before_writer_creation() {
        let root = temp_dir("state-ignore");
        let profile_dir = root.join("project/.baml/profiles");

        let state = ConsumerState::new(profile_dir.clone(), TickConverter::identity());
        assert!(state.profile_writes_enabled);
        assert_eq!(
            std::fs::read(root.join("project/.baml/.gitignore")).unwrap(),
            b"*\n"
        );
        assert!(
            !profile_dir.exists(),
            "ConsumerState setup must not create profile files or the profiles dir"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn consumer_state_disables_profile_writes_when_ignore_marker_fails() {
        let root = temp_dir("state-ignore-fail");
        let baml_dir = root.join("project/.baml");
        let profile_dir = baml_dir.join("profiles");
        std::fs::create_dir_all(baml_dir.join(".gitignore")).unwrap();

        let state = ConsumerState::new(profile_dir, TickConverter::identity());
        assert!(!state.profile_writes_enabled);

        std::fs::remove_dir_all(&root).ok();
    }

    /// The PR3 gate: a fake producer pushes a known sequence of raw records
    /// through a real ring (tiny segments → constant growth) into a real
    /// consumer thread; the `.bamlprof` it writes must parse back to the
    /// exact event sequence, with the registered header.
    #[test]
    fn e2e_fake_producer_roundtrip() {
        const ENGINE: u64 = 0xE2E0_0001;
        const PAIRS: u64 = if cfg!(miri) { 40 } else { 2_000 };

        let dir = temp_dir("e2e");
        let registry: &'static Registry = leak(Registry::new());
        let ctx: &'static RingCtx = leak(RingCtx::new(1 << 40));
        let (ctl_tx, ctl_rx) = mpsc::channel();
        let env = ConsumerEnv {
            registry,
            ctx,
            dir: dir.clone(),
            wake_interval: Duration::from_millis(1),
            clock: ClockMode::Fixed(TickConverter::identity()),
            pipeline: ProfilePipeline::Legacy,
            obs_layout: ObsLayout::V1,
            obs_stats_path: None,
        };
        std::thread::Builder::new()
            .name("test-prof-consumer".into())
            .spawn(move || consumer_main(&ctl_rx, &env))
            .unwrap();

        register_engine_metadata(
            ENGINE,
            EngineProfileMetadata {
                source_snapshot_id: "snapshot-1".into(),
                revision_id: "revision-1".into(),
                revision_id_bytes: [1; 32],
                function_count: 1,
                revision_dictionary: None,
                models: std::sync::Arc::default(),
                functions: vec![FunctionMetaEntry {
                    function_id: 7,
                    fqn: "pkg.main".into(),
                    source_file: "main.baml".into(),
                    span_start: 10,
                    span_end: 90,
                    kind: "bytecode".into(),
                    definition_key: Some("function:pkg.main".into()),
                    owner_type: None,
                    parent_function: None,
                    lambda_path: None,
                    package_name: Some("pkg".into()),
                    namespace: vec!["pkg".into()],
                }],
            },
        );

        // Expected events, built independently of to_disk_event.
        let mut expected: Vec<Event> = Vec::new();
        expected.push(Event::StartThread(pb::StartThread {
            thread_id: 1,
            parent_thread_id: None,
            parent_call_id: None,
            name: Some("worker".into()),
            timestamp_ns: 1,
        }));
        for seq in 1..=PAIRS {
            expected.push(Event::CallFunction(pb::CallFunction {
                thread_id: 1,
                call_id: seq,
                parent_call_id: (seq > 1).then_some(seq - 1),
                function_id: 7,
                timestamp_ns: seq * 2,
                call_site_file_id: None,
                call_site_start_offset: None,
                call_site_end_offset: None,
                call_site_line: None,
            }));
        }
        expected.push(Event::SetFunctionId(pb::SetFunctionId {
            thread_id: 1,
            call_id: PAIRS,
            id: vec![0xAB; 16],
            timestamp_ns: PAIRS * 2 + 1,
        }));
        for seq in (1..=PAIRS).rev() {
            expected.push(Event::EndFunction(pb::EndFunction {
                thread_id: 1,
                call_id: seq,
                status: pb::FunctionEndStatus::Ok as i32,
                timestamp_ns: 10_000 + seq,
            }));
        }
        expected.push(Event::EndThread(pb::EndThread {
            thread_id: 1,
            status: pb::ThreadEndStatus::Completed as i32,
            timestamp_ns: 99_999,
        }));

        std::thread::spawn(move || {
            // Tiny segments force constant growth + recycling under the
            // live consumer.
            let h = registry.acquire(ctx, 256, 4, ENGINE);
            let mut buf = [0u8; MAX_RECORD_LEN];
            let mut push = |rec: RawRecord<'_>| {
                let len = rec.encode(&mut buf);
                // SAFETY: claiming thread, alive for the whole closure.
                unsafe { h.push(&buf[..len]) };
            };
            push(RawRecord::StartThread {
                flags: 0,
                thread_id: BexThreadId(1),
                parent_thread_id: BexThreadId(0),
                parent_call_id: BexCallId(0),
                ts_ticks: 1,
                name: b"worker",
            });
            for seq in 1..=PAIRS {
                push(RawRecord::CallFunction {
                    flags: 0,
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(seq),
                    parent_call_id: BexCallId(seq - 1),
                    function_id: FunctionId(7),
                    call_site: None,
                    ts_ticks: seq * 2,
                });
            }
            push(RawRecord::SetFunctionId {
                thread_id: BexThreadId(1),
                call_id: BexCallId(PAIRS),
                id: [0xAB; 16],
                ts_ticks: PAIRS * 2 + 1,
            });
            for seq in (1..=PAIRS).rev() {
                push(RawRecord::EndFunction {
                    status: FunctionEndStatus::Ok,
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(seq),
                    ts_ticks: 10_000 + seq,
                });
            }
            push(RawRecord::EndThread {
                status: ThreadEndStatus::Completed,
                thread_id: BexThreadId(1),
                ts_ticks: 99_999,
            });
        })
        .join()
        .unwrap();

        // Flush-with-ack (the same protocol flush_and_join uses).
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        ctl_tx.send(ControlMsg::Flush(ack_tx)).unwrap();
        ctx.wake().force_wake();
        ack_rx
            .recv_timeout(Duration::from_mins(1))
            .expect("consumer never acked the flush");

        let paths: Vec<_> = std::fs::read_dir(&dir)
            .expect("profiles dir missing")
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(
            paths.len(),
            1,
            "expected exactly one engine file: {paths:?}"
        );
        assert_eq!(
            paths[0].extension().and_then(|e| e.to_str()),
            Some("bamlprof")
        );

        let contents = read_bamlprof(&paths[0]).expect("parse .bamlprof");
        // A live heartbeat append can leave a torn tail at read time; our
        // events were synced whole before the flush ack.
        let (header, events) = (contents.header, contents.events);
        assert_eq!(header.engine_id, ENGINE);
        assert_eq!(header.source_snapshot_id, "snapshot-1");
        assert_eq!(header.revision_id, "revision-1");
        assert_eq!(
            header.revision_ref.as_ref().unwrap().revision_id,
            vec![1; 32]
        );
        assert_eq!(header.process_id.len(), 16);
        assert!(header_started_at_epoch_ns(&header).is_some());
        let table = header.function_table.expect("function table");
        assert_eq!(table.functions.len(), 1);
        assert_eq!(table.functions[0].fqn, "pkg.main");
        assert_eq!(table.functions[0].function_id, 7);
        assert_eq!(
            table.functions[0].definition_key.as_deref(),
            Some("function:pkg.main")
        );
        assert_eq!(table.functions[0].package_name.as_deref(), Some("pkg"));
        assert_eq!(table.functions[0].namespace, vec!["pkg"]);

        let got: Vec<Event> = events
            .into_iter()
            .filter_map(|e| e.event)
            .filter(|e| !matches!(e, Event::Heartbeat(_)))
            .collect();
        assert_eq!(got, expected, "event sequence must round-trip exactly");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn e2e_boundary_bind_complete_writes_meta_session_row_and_sealed_snapshot() {
        use crate::prof::{
            boundary::{BoundaryBegin, register_begin},
            storage::{
                BlockRows, SegmentState, TypedBoundaryMeta, decode_typed_boundary_meta,
                scan_bcct_bytes, scan_meta_bytes,
            },
        };

        const ENGINE: u64 = 0xB0A0_DA12;
        let root = temp_dir("boundary-lifecycle");
        let project = root.join("project");
        let dir = project.join(".baml/profiles");
        let registry: &'static Registry = leak(Registry::new());
        let ctx: &'static RingCtx = leak(RingCtx::new(1 << 30));
        let (ctl_tx, ctl_rx) = mpsc::channel();
        let env = ConsumerEnv {
            registry,
            ctx,
            dir,
            wake_interval: Duration::from_millis(1),
            clock: ClockMode::Fixed(TickConverter::identity()),
            pipeline: ProfilePipeline::Cct,
            obs_layout: ObsLayout::V2,
            obs_stats_path: None,
        };
        std::thread::Builder::new()
            .name("test-boundary-consumer".into())
            .spawn(move || consumer_main(&ctl_rx, &env))
            .unwrap();

        register_engine_metadata(
            ENGINE,
            EngineProfileMetadata {
                revision_id_bytes: [9; 32],
                models: std::sync::Arc::default(),
                ..EngineProfileMetadata::default()
            },
        );
        let boundary_id = BoundaryId::from_bytes([0x42; 16]);
        let lifecycle = register_begin(BoundaryBegin {
            boundary_id,
            target: "pkg.main".to_owned(),
            source: "test".to_owned(),
            project_id: "project-1".to_owned(),
            revision_id: [9; 32],
            capture_defaults: 3,
            project_root: project,
            created_ms: Some(123),
        })
        .unwrap();
        let boundary_dir = boundary::registration(boundary_id).unwrap().boundary_dir;

        let handle = registry.acquire(ctx, 64 * 1024, 2, ENGINE);
        let mut buffer = [0_u8; MAX_RECORD_LEN];
        let mut push = |record: RawRecord<'_>| {
            let len = record.encode(&mut buffer);
            // SAFETY: this test thread exclusively owns the ring handle.
            unsafe { handle.push(&buffer[..len]) };
        };
        push(RawRecord::StartThread {
            flags: 0,
            thread_id: BexThreadId(7),
            parent_thread_id: BexThreadId(0),
            parent_call_id: BexCallId(0),
            ts_ticks: 10,
            name: b"",
        });

        let (bind_tx, bind_rx) = mpsc::sync_channel(1);
        ctl_tx
            .send(ControlMsg::BindBoundary {
                boundary_id,
                root_thread: ThreadRef {
                    process_euid: ProcessEuid(process_id()),
                    engine_id: EngineId(ENGINE),
                    thread_id: BexThreadId(7),
                },
                ack: bind_tx,
            })
            .unwrap();
        ctx.wake().force_wake();
        let binding = bind_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(binding.partition_id, 1);
        assert_eq!(binding.boundary_local_id, 1);

        push(RawRecord::CallFunction {
            flags: 0,
            thread_id: BexThreadId(7),
            call_id: BexCallId(1),
            parent_call_id: BexCallId(0),
            function_id: FunctionId(16),
            call_site: None,
            ts_ticks: 20,
        });
        push(RawRecord::EndFunction {
            status: FunctionEndStatus::Errored,
            thread_id: BexThreadId(7),
            call_id: BexCallId(1),
            ts_ticks: 40,
        });
        push(RawRecord::EndThread {
            status: ThreadEndStatus::Errored,
            thread_id: BexThreadId(7),
            ts_ticks: 50,
        });

        let pinned_cid = crate::value_cas::encode_value_dag(
            &crate::value_cas::CanonicalValue::String("promoted evidence".to_owned()),
        )
        .unwrap()
        .root;
        let mut value_manifest = crate::value_cas::CidManifestWriter::create(
            boundary_dir.join("manifest.bamlcids"),
            boundary_id,
        )
        .unwrap();
        value_manifest.append(pinned_cid).unwrap();
        value_manifest.seal().unwrap();

        let (complete_tx, complete_rx) = mpsc::sync_channel(1);
        ctl_tx
            .send(ControlMsg::CompleteBoundary {
                boundary_id,
                completion: BoundaryCompletion::new("error"),
                ack: complete_tx,
            })
            .unwrap();
        ctx.wake().force_wake();
        complete_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert!(lifecycle.is_active());

        let meta = scan_meta_bytes(&std::fs::read(boundary_dir.join("boundary.bamlmeta")).unwrap());
        assert!(!meta.torn_tail);
        let typed = meta
            .records
            .iter()
            .map(decode_typed_boundary_meta)
            .collect::<io::Result<Vec<_>>>()
            .unwrap();
        assert!(matches!(typed[0], TypedBoundaryMeta::Begin(_)));
        assert!(matches!(typed[1], TypedBoundaryMeta::Bound(_)));
        assert!(matches!(typed[2], TypedBoundaryMeta::Trigger(_)));
        assert!(matches!(typed[3], TypedBoundaryMeta::Complete(_)));
        let flight = std::fs::read_dir(boundary_dir.join("flight"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert!(flight.iter().any(|path| {
            path.extension()
                .is_some_and(|extension| extension == "bamlprof")
        }));
        assert!(flight.iter().any(|path| {
            path.extension()
                .is_some_and(|extension| extension == "bamlcids")
        }));
        let exact_manifest = flight
            .iter()
            .find(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "bamlcids")
            })
            .unwrap();
        let exact_pins = crate::value_cas::CidManifestReader::read(exact_manifest).unwrap();
        assert_eq!(exact_pins.manifest.cids, vec![pinned_cid]);
        assert!(exact_pins.manifest.sealed);

        let snapshot =
            scan_bcct_bytes(&std::fs::read(boundary_dir.join("cct.bamlcct")).unwrap()).unwrap();
        assert!(matches!(snapshot.state, SegmentState::Sealed(_)));
        let rows = snapshot
            .blocks
            .iter()
            .map(|block| block.decode_rows().unwrap())
            .collect::<Vec<_>>();
        assert!(rows.iter().any(|rows| {
            matches!(rows, BlockRows::PartitionBind(rows) if rows.len() == 1
                && rows[0].partition_id == 1
                && rows[0].boundary_id == boundary_id.as_bytes())
        }));
        let births = rows.iter().find_map(|rows| match rows {
            BlockRows::NodeBirth(rows) => Some(rows),
            _ => None,
        });
        assert_eq!(
            births
                .unwrap()
                .iter()
                .map(|row| row.node_id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );

        let session_segments = std::fs::read_dir(binding.session_dir.join("cct"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(session_segments.len(), 1);
        let session = scan_bcct_bytes(&std::fs::read(&session_segments[0]).unwrap()).unwrap();
        assert!(session.blocks.iter().any(|block| {
            matches!(
                block.decode_rows().unwrap(),
                BlockRows::PartitionBind(ref rows)
                    if rows[0].boundary_id == boundary_id.as_bytes()
            )
        }));

        std::fs::remove_dir_all(root).ok();
    }

    /// `0 = none` ring conventions become absent optionals on disk.
    #[test]
    fn transcode_none_conventions() {
        let raw = RawRecord::StartThread {
            flags: 0,
            thread_id: BexThreadId(9),
            parent_thread_id: BexThreadId(0),
            parent_call_id: BexCallId(0),
            ts_ticks: 5,
            name: b"",
        };
        match to_disk_event(&raw, &TickConverter::identity())
            .event
            .unwrap()
        {
            Event::StartThread(st) => {
                assert_eq!(st.parent_thread_id, None);
                assert_eq!(st.parent_call_id, None);
                assert_eq!(st.name, None);
            }
            other => panic!("wrong variant {other:?}"),
        }
        let raw = RawRecord::CallFunction {
            flags: 0,
            thread_id: BexThreadId(9),
            call_id: BexCallId(1),
            parent_call_id: BexCallId(0),
            function_id: FunctionId(0),
            call_site: None,
            ts_ticks: 5,
        };
        match to_disk_event(&raw, &TickConverter::identity())
            .event
            .unwrap()
        {
            Event::CallFunction(cf) => {
                assert_eq!(cf.parent_call_id, None);
                assert_eq!(cf.call_site_file_id, None);
                assert_eq!(cf.call_site_start_offset, None);
                assert_eq!(cf.call_site_end_offset, None);
                assert_eq!(cf.call_site_line, None);
            }
            other => panic!("wrong variant {other:?}"),
        }
        let raw = RawRecord::CallFunction {
            flags: 0,
            thread_id: BexThreadId(9),
            call_id: BexCallId(2),
            parent_call_id: BexCallId(1),
            function_id: FunctionId(3),
            call_site: Some(CallSiteSourceSpan {
                file_id: 0,
                start_offset: 17,
                end_offset: 29,
                line: 5,
            }),
            ts_ticks: 6,
        };
        match to_disk_event(&raw, &TickConverter::identity())
            .event
            .unwrap()
        {
            Event::CallFunction(cf) => {
                assert_eq!(cf.parent_call_id, Some(1));
                assert_eq!(cf.call_site_file_id, Some(0));
                assert_eq!(cf.call_site_start_offset, Some(17));
                assert_eq!(cf.call_site_end_offset, Some(29));
                assert_eq!(cf.call_site_line, Some(5));
            }
            other => panic!("wrong variant {other:?}"),
        }
        let raw = RawRecord::EndFunction {
            status: FunctionEndStatus::Errored,
            thread_id: BexThreadId(9),
            call_id: BexCallId(1),
            ts_ticks: 5,
        };
        match to_disk_event(&raw, &TickConverter::identity())
            .event
            .unwrap()
        {
            Event::EndFunction(ef) => {
                assert_eq!(ef.status, pb::FunctionEndStatus::Errored as i32);
            }
            other => panic!("wrong variant {other:?}"),
        }
    }

    /// The PR3 throughput harness (plan §5): events/s for one consumer core
    /// doing drain + transcode + write. Run manually:
    /// `cargo test -p bex_events --release -- --ignored prof_drain_throughput --nocapture`
    #[test]
    #[ignore = "throughput harness; run manually in release with --nocapture"]
    #[expect(clippy::print_stdout, reason = "harness reports its measurement")]
    fn prof_drain_throughput() {
        const EVENTS: u64 = 4_000_000;
        const ENGINE: u64 = 0xBE0C_0001;

        let dir = temp_dir("throughput");
        let registry: &'static Registry = leak(Registry::new());
        let ctx: &'static RingCtx = leak(RingCtx::new(1 << 40));

        // Pre-fill the ring so the measurement sees a saturated drain
        // (producer cost excluded).
        let handle = registry.acquire(ctx, config::DEFAULT_SEG_BYTES, 4, ENGINE);
        let mut buf = [0u8; MAX_RECORD_LEN];
        for seq in 1..=EVENTS {
            let len = RawRecord::CallFunction {
                flags: 0,
                thread_id: BexThreadId(1),
                call_id: BexCallId(seq),
                parent_call_id: BexCallId(seq.saturating_sub(1)),
                function_id: FunctionId(1),
                call_site: None,
                ts_ticks: seq,
            }
            .encode(&mut buf);
            // SAFETY: claiming thread, alive throughout.
            unsafe { handle.push(&buf[..len]) };
        }

        let mut state = ConsumerState::new(dir.clone(), TickConverter::identity());
        let env = ConsumerEnv {
            registry,
            ctx,
            dir: dir.clone(),
            wake_interval: Duration::from_millis(1),
            clock: ClockMode::Fixed(TickConverter::identity()),
            pipeline: ProfilePipeline::Legacy,
            obs_layout: ObsLayout::V1,
            obs_stats_path: None,
        };
        let start = Instant::now();
        while state.sweep_once(&env) {}
        state.flush_files();
        let elapsed = start.elapsed();

        #[expect(clippy::cast_precision_loss, reason = "display only")]
        let rate = EVENTS as f64 / elapsed.as_secs_f64();
        println!(
            "prof_drain_throughput: {EVENTS} events in {elapsed:.2?} = {:.1}M events/s/core",
            rate / 1.0e6
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn pipeline_dual_keeps_legacy_bytes_and_cct_mode_skips_legacy_sink() {
        fn artifact_for(pipeline: ProfilePipeline) -> (Option<Vec<u8>>, usize) {
            const ENGINE: u64 = 0xF0A0_0001;
            let dir = temp_dir(pipeline.as_str());
            let stats_path = dir.join("obs-stats.json");
            let registry: &'static Registry = leak(Registry::new());
            let ctx: &'static RingCtx = leak(RingCtx::new(1 << 30));
            let handle = registry.acquire(ctx, config::MIN_SEG_BYTES, 0, ENGINE);
            let mut buf = [0u8; MAX_RECORD_LEN];
            for record in [
                RawRecord::StartThread {
                    flags: 0,
                    thread_id: BexThreadId(1),
                    parent_thread_id: BexThreadId(0),
                    parent_call_id: BexCallId(0),
                    ts_ticks: 1,
                    name: b"root",
                },
                RawRecord::CallFunction {
                    flags: 0,
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(1),
                    parent_call_id: BexCallId(0),
                    function_id: FunctionId(16),
                    call_site: None,
                    ts_ticks: 10,
                },
                RawRecord::EndFunction {
                    status: FunctionEndStatus::Ok,
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(1),
                    ts_ticks: 20,
                },
                RawRecord::EndThread {
                    status: ThreadEndStatus::Completed,
                    thread_id: BexThreadId(1),
                    ts_ticks: 21,
                },
            ] {
                let len = record.encode(&mut buf);
                // SAFETY: this test thread is the live ring claimant.
                unsafe { handle.push(&buf[..len]) };
            }

            let mut state = ConsumerState::new_configured(
                dir.clone(),
                TickConverter::identity(),
                pipeline,
                ObsLayout::V1,
                Some(stats_path.clone()),
            );
            let env = ConsumerEnv {
                registry,
                ctx,
                dir: dir.clone(),
                wake_interval: Duration::from_millis(1),
                clock: ClockMode::Fixed(TickConverter::identity()),
                pipeline,
                obs_layout: ObsLayout::V1,
                obs_stats_path: None,
            };
            while state.sweep_once(&env) {}
            state.sync_files();
            state.write_stats("test", ctx);

            let stats: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&stats_path).unwrap()).unwrap();
            assert_eq!(stats["pipeline"], pipeline.as_str());
            assert_eq!(stats["events"], 4);
            assert_eq!(stats["drained_ranges"], 1);
            if pipeline == ProfilePipeline::Cct {
                assert_eq!(stats["profile_bytes"], 0);
            } else {
                assert!(stats["profile_bytes"].as_u64().unwrap() > 0);
            }
            assert!(stats["ring_peak_bytes"].as_u64().unwrap() > 0);

            let path = std::fs::read_dir(&dir)
                .unwrap()
                .map(Result::unwrap)
                .map(|entry| entry.path())
                .find(|path| path.extension().is_some_and(|ext| ext == "bamlprof"));
            let bytes = path.map(|path| std::fs::read(path).unwrap());
            let cct_nodes = state
                .cct_engines
                .get(&ENGINE)
                .map_or(0, |cct| cct.snapshot().nodes.len());
            std::fs::remove_dir_all(dir).ok();
            (bytes, cct_nodes)
        }

        let (legacy, legacy_nodes) = artifact_for(ProfilePipeline::Legacy);
        let (dual, dual_nodes) = artifact_for(ProfilePipeline::Dual);
        let (cct, cct_nodes) = artifact_for(ProfilePipeline::Cct);
        assert_eq!(dual, legacy);
        assert!(legacy.is_some());
        assert!(cct.is_none());
        assert_eq!(legacy_nodes, 0);
        assert!(dual_nodes >= 3);
        assert!(cct_nodes >= 3);
    }

    /// P2 correctness oracle: one real `Dual` fan-out must preserve exact
    /// per-function entry and terminal-status counts between the legacy raw
    /// artifact and the CCT aggregation. This deliberately joins raw end
    /// records back to their `(thread, call)` starts rather than deriving the
    /// expected CCT values from the input fixture.
    #[test]
    fn dual_pipeline_raw_and_cct_counters_are_exactly_equivalent() {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        struct ExactCounts {
            enters: u64,
            ends_ok: u64,
            ends_err: u64,
            ends_cancel: u64,
            ends_exit: u64,
        }

        const ENGINE: u64 = 0xD0A1_C07;
        let dir = temp_dir("dual-cct-equivalence");
        let registry: &'static Registry = leak(Registry::new());
        let ctx: &'static RingCtx = leak(RingCtx::new(1 << 30));
        let handle = registry.acquire(ctx, config::MIN_SEG_BYTES, 0, ENGINE);
        let mut buf = [0u8; MAX_RECORD_LEN];
        let records = [
            RawRecord::StartThread {
                flags: 0,
                thread_id: BexThreadId(1),
                parent_thread_id: BexThreadId(0),
                parent_call_id: BexCallId(0),
                ts_ticks: 1,
                name: b"root",
            },
            RawRecord::CallFunction {
                flags: 0,
                thread_id: BexThreadId(1),
                call_id: BexCallId(1),
                parent_call_id: BexCallId(0),
                function_id: FunctionId(16),
                call_site: None,
                ts_ticks: 10,
            },
            RawRecord::CallFunction {
                flags: 0,
                thread_id: BexThreadId(1),
                call_id: BexCallId(2),
                parent_call_id: BexCallId(1),
                function_id: FunctionId(17),
                call_site: None,
                ts_ticks: 20,
            },
            RawRecord::EndFunction {
                status: FunctionEndStatus::Ok,
                thread_id: BexThreadId(1),
                call_id: BexCallId(2),
                ts_ticks: 30,
            },
            RawRecord::CallFunction {
                flags: 0,
                thread_id: BexThreadId(1),
                call_id: BexCallId(3),
                parent_call_id: BexCallId(1),
                function_id: FunctionId(17),
                call_site: None,
                ts_ticks: 40,
            },
            RawRecord::EndFunction {
                status: FunctionEndStatus::Errored,
                thread_id: BexThreadId(1),
                call_id: BexCallId(3),
                ts_ticks: 50,
            },
            RawRecord::CallFunction {
                flags: 0,
                thread_id: BexThreadId(1),
                call_id: BexCallId(4),
                parent_call_id: BexCallId(1),
                function_id: FunctionId(18),
                call_site: None,
                ts_ticks: 60,
            },
            RawRecord::EndFunction {
                status: FunctionEndStatus::Cancelled,
                thread_id: BexThreadId(1),
                call_id: BexCallId(4),
                ts_ticks: 70,
            },
            RawRecord::CallFunction {
                flags: 0,
                thread_id: BexThreadId(1),
                call_id: BexCallId(5),
                parent_call_id: BexCallId(1),
                function_id: FunctionId(18),
                call_site: None,
                ts_ticks: 80,
            },
            RawRecord::EndFunction {
                status: FunctionEndStatus::Exited,
                thread_id: BexThreadId(1),
                call_id: BexCallId(5),
                ts_ticks: 90,
            },
            RawRecord::EndFunction {
                status: FunctionEndStatus::Ok,
                thread_id: BexThreadId(1),
                call_id: BexCallId(1),
                ts_ticks: 100,
            },
            RawRecord::EndThread {
                status: ThreadEndStatus::Completed,
                thread_id: BexThreadId(1),
                ts_ticks: 101,
            },
        ];
        for record in records {
            let len = record.encode(&mut buf);
            // SAFETY: this test thread is the live ring claimant.
            unsafe { handle.push(&buf[..len]) };
        }

        let mut state = ConsumerState::new_configured(
            dir.clone(),
            TickConverter::identity(),
            ProfilePipeline::Dual,
            ObsLayout::V1,
            None,
        );
        let env = ConsumerEnv {
            registry,
            ctx,
            dir: dir.clone(),
            wake_interval: Duration::from_millis(1),
            clock: ClockMode::Fixed(TickConverter::identity()),
            pipeline: ProfilePipeline::Dual,
            obs_layout: ObsLayout::V1,
            obs_stats_path: None,
        };
        while state.sweep_once(&env) {}
        state.sync_files();

        let profile_path = std::fs::read_dir(&dir)
            .unwrap()
            .map(Result::unwrap)
            .map(|entry| entry.path())
            .find(|path| path.extension().is_some_and(|ext| ext == "bamlprof"))
            .expect("dual mode writes the legacy oracle artifact");
        let contents = crate::prof::read::read_bamlprof_from_bytes(
            &std::fs::read(profile_path).expect("read legacy oracle artifact"),
        )
        .expect("parse legacy oracle artifact");
        assert!(!contents.truncated);

        let mut call_functions = HashMap::<(u64, u64), u32>::new();
        let mut raw_counts = HashMap::<u32, ExactCounts>::new();
        for event in contents.events.into_iter().filter_map(|event| event.event) {
            match event {
                Event::CallFunction(call) => {
                    assert!(
                        call_functions
                            .insert((call.thread_id, call.call_id), call.function_id)
                            .is_none(),
                        "duplicate raw call key"
                    );
                    raw_counts.entry(call.function_id).or_default().enters += 1;
                }
                Event::EndFunction(end) => {
                    let function_id = call_functions
                        .remove(&(end.thread_id, end.call_id))
                        .expect("raw end has a matching call start");
                    let counts = raw_counts.entry(function_id).or_default();
                    match pb::FunctionEndStatus::try_from(end.status).unwrap() {
                        pb::FunctionEndStatus::Ok => counts.ends_ok += 1,
                        pb::FunctionEndStatus::Errored => counts.ends_err += 1,
                        pb::FunctionEndStatus::Cancelled => counts.ends_cancel += 1,
                        pb::FunctionEndStatus::Exited => counts.ends_exit += 1,
                    }
                }
                _ => {}
            }
        }
        assert!(call_functions.is_empty(), "every raw call has one end");

        let snapshot = state
            .cct_engines
            .get(&ENGINE)
            .expect("dual mode retains the CCT side of the fan-out")
            .snapshot();
        let mut cct_counts = HashMap::<u32, ExactCounts>::new();
        for node in snapshot.nodes {
            let function_id = node.identity.function_id.0;
            if function_id < 16 {
                continue;
            }
            let counts = cct_counts.entry(function_id).or_default();
            counts.enters += node.counters.enters;
            counts.ends_ok += node.counters.ends_ok;
            counts.ends_err += node.counters.ends_err;
            counts.ends_cancel += node.counters.ends_cancel;
            counts.ends_exit += node.counters.ends_exit;
        }

        assert_eq!(cct_counts, raw_counts);
        assert_eq!(
            raw_counts.get(&16),
            Some(&ExactCounts {
                enters: 1,
                ends_ok: 1,
                ..ExactCounts::default()
            })
        );
        assert_eq!(
            raw_counts.get(&17),
            Some(&ExactCounts {
                enters: 2,
                ends_ok: 1,
                ends_err: 1,
                ..ExactCounts::default()
            })
        );
        assert_eq!(
            raw_counts.get(&18),
            Some(&ExactCounts {
                enters: 2,
                ends_cancel: 1,
                ends_exit: 1,
                ..ExactCounts::default()
            })
        );
        std::fs::remove_dir_all(dir).ok();
    }

    use crate::prof::config;

    /// PR5 orphan-path soak: rounds of short-lived producer threads (the
    /// tokio blocking-pool churn pattern) against a LIVE consumer thread —
    /// orphan → drain-to-empty → pool → claim must hold under the real
    /// consumer loop, the registry must stay bounded by peak concurrency,
    /// and every record must reach disk.
    #[test]
    fn soak_orphan_churn_with_live_consumer() {
        const ENGINE: u64 = 0x50AC_0001;
        let rounds: u64 = if cfg!(miri) { 4 } else { 64 };
        let per_round: u64 = if cfg!(miri) { 20 } else { 500 };

        let dir = temp_dir("soak");
        let registry: &'static Registry = leak(Registry::new());
        let ctx: &'static RingCtx = leak(RingCtx::new(1 << 40));
        let (ctl_tx, ctl_rx) = mpsc::channel();
        let env = ConsumerEnv {
            registry,
            ctx,
            dir: dir.clone(),
            wake_interval: Duration::from_millis(1),
            clock: ClockMode::Fixed(TickConverter::identity()),
            pipeline: ProfilePipeline::Legacy,
            obs_layout: ObsLayout::V1,
            obs_stats_path: None,
        };
        std::thread::Builder::new()
            .name("soak-prof-consumer".into())
            .spawn(move || consumer_main(&ctl_rx, &env))
            .unwrap();

        for round in 0..rounds {
            std::thread::spawn(move || {
                // Tiny segments force growth + recycling every round.
                let h = registry.acquire(ctx, 512, 2, ENGINE);
                let mut buf = [0u8; MAX_RECORD_LEN];
                for seq in 0..per_round {
                    let len = RawRecord::CallFunction {
                        flags: 0,
                        thread_id: BexThreadId(round + 1),
                        call_id: BexCallId(seq + 1),
                        parent_call_id: BexCallId(seq),
                        function_id: FunctionId(1),
                        call_site: None,
                        ts_ticks: round * per_round + seq,
                    }
                    .encode(&mut buf);
                    // SAFETY: claiming thread, alive for the whole closure.
                    unsafe { h.push(&buf[..len]) };
                }
                h.ring().orphan(); // TLS-destructor stand-in
            })
            .join()
            .unwrap();
            // A dead thread's ring is claimable only after the consumer pools
            // it. Flush-with-ack gives this test an observed consumer sweep
            // between sequential churn rounds without relying on scheduler
            // timing.
            let (ack_tx, ack_rx) = mpsc::sync_channel(1);
            ctl_tx.send(ControlMsg::Flush(ack_tx)).unwrap();
            ctx.wake().force_wake();
            ack_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("soak consumer did not flush before the next churn round");
        }

        // Flush through the live consumer and read everything back.
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        ctl_tx.send(ControlMsg::Flush(ack_tx)).unwrap();
        ctx.wake().force_wake();
        ack_rx
            .recv_timeout(Duration::from_mins(1))
            .expect("soak consumer never acked");

        // Sequential churn must reuse one pooled ring, not grow the registry.
        let mut rings = 0;
        registry.for_each(|_| rings += 1);
        assert_eq!(rings, 1, "registry grew under churn: {rings} rings");

        let paths: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(paths.len(), 1);
        let events = read_bamlprof(&paths[0]).expect("parse soak profile").events;
        let calls = events
            .iter()
            .filter(|e| matches!(e.event, Some(pb::disk_event_v1::Event::CallFunction(_))))
            .count();
        assert_eq!(
            calls as u64,
            rounds * per_round,
            "soak lost or duplicated records"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// PR5: hitting `BAML_RING_MAX_OVERFLOW_BYTES` must be a hard process
    /// error with the documented message (D6) — asserted from a subprocess
    /// because the path aborts.
    #[test]
    #[cfg(not(miri))] // spawns a subprocess
    fn overflow_cap_aborts_with_message() {
        if std::env::var("BAML_PROF_OVERFLOW_CHILD").is_ok() {
            // Child mode: a cap smaller than one segment aborts on the
            // first allocation.
            let ctx: &'static RingCtx = leak(RingCtx::new(1024));
            let registry: &'static Registry = leak(Registry::new());
            let _ = registry.acquire(ctx, 64 * 1024, 2, 1);
            unreachable!("the segment allocation above must abort");
        }
        let exe = std::env::current_exe().expect("test binary path");
        let output = std::process::Command::new(exe)
            .args([
                "prof::consumer::tests::overflow_cap_aborts_with_message",
                "--exact",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("BAML_PROF_OVERFLOW_CHILD", "1")
            .output()
            .expect("spawn child test process");
        assert!(
            !output.status.success(),
            "child must die on the overflow abort"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("BAML_RING_MAX_OVERFLOW_BYTES"),
            "abort message missing the knob name: {stderr}"
        );
        assert!(
            stderr.contains("cannot keep up"),
            "abort message missing the diagnosis: {stderr}"
        );
    }
}
