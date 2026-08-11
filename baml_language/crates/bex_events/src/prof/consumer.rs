//! The background profile consumer (plan §5 PR3): a single `std::thread`
//! named `bex-prof-consumer` that sweeps every ring (§3.4) and feeds the
//! drained ranges to the CCT aggregation plane (design §5), the v2 session
//! streams (§6.1), the opt-in raw firehose (§6.2), and the flight recorder
//! (§5.9). The legacy per-engine `.bamlprof` writers and the run-store /
//! history projections were deleted in P9 step 4 — the raw firehose and
//! flight dumps are now the only exact-event `.bamlprof` producers.
//!
//! Invariants (plan §6): this thread never touches the GC heap or heap
//! permits — it reads rings, the registered (immutable) engine metadata, and
//! its own scratch. That is what keeps lossless-by-growth deadlock-free.
//!
//! Engine lifecycle: `BexEngine::drop` sends [`ControlMsg::EngineClosed`],
//! which drains the engine's remaining events, flushes + seals its session
//! stream, and frees its metadata — long-lived engine-churning hosts (LSP
//! recompiles) don't accumulate fds or heartbeat work. Residual growth per
//! closed engine: one tombstoned id (8 bytes). Rings claimed by still-live
//! threads stay registered (idle) until those threads die and the rings
//! pool — bounded by peak concurrency, by design (plan invariant 7).
//!
//! Capacity model (D6): the 100M ev/s figure in the design is the *burst*
//! producer write budget; sustainable rate ≈ consumers × per-core CCT
//! update rate (~50 ns/pair integrated, P2 exit gate), and burst tolerance
//! ≈ `BAML_RING_MAX_OVERFLOW_BYTES / ((produce − drain) × ~30 B)` seconds
//! of backlog growth. Consumer sharding is a tuning knob with a concrete
//! trigger (a bench showing one consumer saturated), not MVP scope.

#![allow(unsafe_code)]

use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{OnceLock, mpsc},
    time::{Duration, Instant},
};

use crate::{
    ids::ProcessEuid,
    prof::{
        clock::{self, TickConverter},
        config::{PipelineMode, ProfConfig},
        encode::build_header,
        metadata, record,
        registry::Registry,
        ring::{Ring, RingCtx},
        stats::{ConsumerCounters, StatsReporter},
        transcode::to_disk_event,
    },
};

/// Cadence of the consumer's clock-refinement tick (the designated ≥1 s
/// slot for the x86 TSC rate; session-stream heartbeats have their own
/// rate limit inside the writer).
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) enum ControlMsg {
    /// Drain everything currently committed, force a window flush of the
    /// session/raw planes, then ack.
    Flush(mpsc::SyncSender<()>),
    /// §10.3 CCT-equivalence oracle tap: totals by (engine, function id).
    CctSnapshot(mpsc::SyncSender<Vec<(u64, u32, u64, u64)>>),
    /// §9.2 `LiveMirrorSource` tap: the whole-engine live fold encoded as
    /// an always-sealed BCCT segment (identical block format to disk).
    CctLiveSegment {
        engine_id: u64,
        reply: mpsc::SyncSender<Option<Vec<u8>>>,
    },
    /// §9.4 exact-recency tier tap: the recent-call rings (completed
    /// calls), function ids pre-joined.
    RecentCalls {
        engine_id: u64,
        reply: mpsc::SyncSender<Option<Vec<RecentCallOut>>>,
    },
    /// §5.9 manual flight-recorder dump: transcode the retained raw window
    /// into `sessions/<sess>/flight/<ts>-<trigger>.bamlprof`.
    FlightDump {
        engine_id: u64,
        trigger: String,
        reply: mpsc::SyncSender<Option<PathBuf>>,
    },
    /// §6.4 host↔consumer handshake: bind a partition (rooted at
    /// `root_thread`) to a boundary. The consumer answers with the `bound`
    /// record's fields having been written (partition_bind row + boundary
    /// meta append).
    BindBoundary {
        engine_id: u64,
        boundary_id: [u8; 16],
        root_thread: u64,
        boundary_dir: PathBuf,
        ack: mpsc::SyncSender<bool>,
    },
    /// §6.5: boundary completed — fold its partition into
    /// `<dir>/cct.bamlcct` (tmp+rename, sealed), append the meta complete
    /// record, and free the partition (§5.7).
    CompleteBoundary {
        boundary_id: [u8; 16],
        status: String,
        ack: mpsc::SyncSender<bool>,
    },
    /// An engine was dropped: drain its remaining events, flush + seal its
    /// session stream (freeing the fd and stopping its heartbeats), drop
    /// its metadata, and tombstone the id. Sent non-blocking from
    /// `BexEngine::drop`.
    EngineClosed(u64),
}

/// Everything the consumer loop needs; owned so tests can run private
/// consumers against private registries and directories.
pub(crate) struct ConsumerEnv {
    pub(crate) registry: &'static Registry,
    pub(crate) ctx: &'static RingCtx,
    pub(crate) dir: PathBuf,
    pub(crate) wake_interval: Duration,
    pub(crate) clock: ClockMode,
    /// Pipeline label for stats lines (`BAML_PROFILE_PIPELINE`; single-mode
    /// post-P9, observability design §10.3).
    pub(crate) pipeline: PipelineMode,
    /// §6.2 raw firehose opt-in (`BAML_PROFILE_RAW`).
    pub(crate) profile_raw: bool,
    /// Self-report NDJSON path (`BAML_OBS_STATS`); `None` = off.
    pub(crate) stats_path: Option<PathBuf>,
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
            profile_raw: cfg.profile_raw,
            stats_path: cfg.obs_stats_path.clone(),
        };
        std::thread::Builder::new()
            .name("bex-prof-consumer".into())
            .spawn(move || consumer_main(&rx, &env))
            .expect("failed to spawn bex-prof-consumer thread");
        tx
    });
}

/// Drains everything committed so far and forces a window flush of the
/// session/raw planes, waiting up to `timeout` for the consumer's ack.
/// Returns whether the ack arrived. A no-op `true` when profiling never
/// started.
///
/// Durability: the ack means every committed record has reached its sinks
/// and the session stream ran its group-commit tick (§6.6 off-thread
/// fsync); the raw firehose is debug telemetry and stays OS-buffer only by
/// design (§6.2).
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

/// §6.4: bind the boundary rooted at `root_thread` to its partition. The
/// host has already written the boundary dir + `begin` record; on `true`,
/// the consumer has appended the `bound` record and the session stream's
/// `partition_bind` row.
pub fn bind_boundary(
    engine_id: u64,
    boundary_id: [u8; 16],
    root_thread: u64,
    boundary_dir: &Path,
    timeout: Duration,
) -> bool {
    let Some(tx) = CONTROL_TX.get() else {
        return false;
    };
    let (ack_tx, ack_rx) = mpsc::sync_channel(1);
    if tx
        .send(ControlMsg::BindBoundary {
            engine_id,
            boundary_id,
            root_thread,
            boundary_dir: boundary_dir.to_path_buf(),
            ack: ack_tx,
        })
        .is_err()
    {
        return false;
    }
    crate::prof::registry::global_ctx().wake().force_wake();
    ack_rx.recv_timeout(timeout).unwrap_or(false)
}

/// §6.5: complete a bound boundary — folds `cct.bamlcct`, appends the
/// `complete` record, frees the partition (§5.7).
pub fn complete_boundary(boundary_id: [u8; 16], status: &str, timeout: Duration) -> bool {
    let Some(tx) = CONTROL_TX.get() else {
        return false;
    };
    let (ack_tx, ack_rx) = mpsc::sync_channel(1);
    if tx
        .send(ControlMsg::CompleteBoundary {
            boundary_id,
            status: status.to_string(),
            ack: ack_tx,
        })
        .is_err()
    {
        return false;
    }
    crate::prof::registry::global_ctx().wake().force_wake();
    ack_rx.recv_timeout(timeout).unwrap_or(false)
}

/// §10.3 oracle tap: ask the live consumer for CCT totals by
/// `(engine_id, function_id)`. `None` when profiling/consumer never
/// started or the request timed out. Test/diagnostic surface — one
/// bounded round-trip, never on a hot path.
pub fn cct_totals_snapshot(timeout: Duration) -> Option<Vec<(u64, u32, u64, u64)>> {
    let tx = CONTROL_TX.get()?;
    let (reply_tx, reply_rx) = mpsc::sync_channel(1);
    tx.send(ControlMsg::CctSnapshot(reply_tx)).ok()?;
    crate::prof::registry::global_ctx().wake().force_wake();
    reply_rx.recv_timeout(timeout).ok()
}

/// §9.2 `LiveMirrorSource` tap: the live engine's whole-CCT state encoded
/// as an always-sealed BCCT segment (same block format as disk — a query
/// engine folds it exactly like a `.bamlcct`). `None` when the engine (and
/// its bounded closed retention) is unknown, or on timeout.
pub fn cct_live_segment(engine_id: u64, timeout: Duration) -> Option<Vec<u8>> {
    let tx = CONTROL_TX.get()?;
    let (reply_tx, reply_rx) = mpsc::sync_channel(1);
    tx.send(ControlMsg::CctLiveSegment {
        engine_id,
        reply: reply_tx,
    })
    .ok()?;
    crate::prof::registry::global_ctx().wake().force_wake();
    reply_rx.recv_timeout(timeout).ok().flatten()
}

/// §9.4 exact-recency tier row: one completed call from a partition's
/// recent ring, function id pre-joined through the node table.
#[derive(Debug, Clone, Copy)]
pub struct RecentCallOut {
    pub partition: u32,
    pub thread_idx: u32,
    pub call_id: u64,
    pub parent_call_id: u64,
    pub function: u32,
    pub start_ns: u64,
    pub end_ns: u64,
    pub status: u8,
}

/// §9.4 exact-recency tier tap: completed calls from the engine's recent
/// rings (last 4096 per partition), oldest→newest per partition. `None`
/// when the engine is unknown or the request timed out.
pub fn recent_calls(engine_id: u64, timeout: Duration) -> Option<Vec<RecentCallOut>> {
    let tx = CONTROL_TX.get()?;
    let (reply_tx, reply_rx) = mpsc::sync_channel(1);
    tx.send(ControlMsg::RecentCalls {
        engine_id,
        reply: reply_tx,
    })
    .ok()?;
    crate::prof::registry::global_ctx().wake().force_wake();
    reply_rx.recv_timeout(timeout).ok().flatten()
}

/// §5.9/§3.1 Manual trigger: dump the flight recorder's retained window
/// for one engine. Returns the dump path, or `None` when nothing is
/// retained, rate limits bind (≥5 s spacing, ≤16 dumps/engine), or the
/// request timed out.
pub fn flight_dump(engine_id: u64, trigger: &str, timeout: Duration) -> Option<PathBuf> {
    let tx = CONTROL_TX.get()?;
    let (reply_tx, reply_rx) = mpsc::sync_channel(1);
    tx.send(ControlMsg::FlightDump {
        engine_id,
        trigger: trigger.to_string(),
        reply: reply_tx,
    })
    .ok()?;
    crate::prof::registry::global_ctx().wake().force_wake();
    reply_rx.recv_timeout(timeout).ok().flatten()
}

/// Notifies the consumer that an engine was dropped (called from
/// `BexEngine::drop`): its remaining events are drained, its session
/// stream is flushed and sealed (freeing the fd and stopping its
/// heartbeats), and its metadata entry is freed. Non-blocking — safe from
/// `Drop`. If profiling never started, only the shared metadata entry is
/// removed.
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

/// The consumer loop: §3.4 sweep + §3.6 wake protocol + control messages.
pub(crate) fn consumer_main(control: &mpsc::Receiver<ControlMsg>, env: &ConsumerEnv) {
    env.ctx.wake().register_consumer();
    let mut state = ConsumerState::new(env.dir.clone(), env.clock.build(), env.profile_raw);
    let mut reporter = StatsReporter::new(
        env.stats_path.clone(),
        env.pipeline.as_str(),
        "bex-prof-consumer",
    );
    loop {
        while let Ok(msg) = control.try_recv() {
            match msg {
                ControlMsg::BindBoundary {
                    engine_id,
                    boundary_id,
                    root_thread,
                    boundary_dir,
                    ack,
                } => {
                    // Drain so the root thread's StartThread has applied.
                    for _ in 0..1024 {
                        if !state.sweep_once(env) {
                            break;
                        }
                    }
                    state.cct_sweep_tick();
                    let ok =
                        state.bind_boundary(engine_id, boundary_id, root_thread, &boundary_dir);
                    let _ = ack.send(ok);
                }
                ControlMsg::CompleteBoundary {
                    boundary_id,
                    status,
                    ack,
                } => {
                    for _ in 0..1024 {
                        if !state.sweep_once(env) {
                            break;
                        }
                    }
                    state.cct_sweep_tick();
                    let ok = state.complete_boundary(boundary_id, &status);
                    let _ = ack.send(ok);
                }
                ControlMsg::CctSnapshot(reply) => {
                    // Resolve any cross-ring stragglers before answering:
                    // the oracle's totals must reflect every drained
                    // record, not the retry cadence.
                    state.cct_sweep_tick();
                    let _ = reply.send(state.cct_totals());
                }
                ControlMsg::CctLiveSegment { engine_id, reply } => {
                    state.cct_sweep_tick();
                    let _ = reply.send(state.cct_live_segment(engine_id));
                }
                ControlMsg::RecentCalls { engine_id, reply } => {
                    state.cct_sweep_tick();
                    let _ = reply.send(state.recent_calls(engine_id));
                }
                ControlMsg::FlightDump {
                    engine_id,
                    trigger,
                    reply,
                } => {
                    let _ = reply.send(state.flight_dump(engine_id, &trigger));
                }
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
                    state.cct_sweep_tick();
                    // Forced window: the flush ack promises every drained
                    // record has reached the session/raw planes, not just
                    // the in-RAM CCT — live engines mint their session here
                    // if the 250 ms cadence hasn't yet.
                    state.cct_window_tick(true);
                    // Flush is the "everything durable" milestone every host
                    // hits before exit — the natural self-report point.
                    state.report_stats(&mut reporter, env);
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
                    state.report_stats(&mut reporter, env);
                    // All of the engine's events have been delivered; let
                    // observers (run store, history store) release whatever
                    // they buffered for it.
                    crate::run::publish_engine_closed(crate::ids::EngineId(engine_id));
                    crate::history::publish_history_engine_closed(crate::ids::EngineId(engine_id));
                }
            }
        }
        let progress = state.sweep_once(env);
        state.cct_sweep_tick();
        state.cct_window_tick(false);
        state.maybe_heartbeat();
        if !progress {
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
    /// Tick→ns conversion for every disk timestamp.
    conv: TickConverter,
    /// Engines whose sessions were sealed (8 bytes per engine, forever): a
    /// record arriving after the close is a logic error — reported once and
    /// dropped, never silently resurrecting the engine's state.
    closed_engines: std::collections::HashSet<u64>,
    closed_reported: std::collections::HashSet<u64>,
    process_id: [u8; 16],
    started_at_epoch_ns: u128,
    last_heartbeat: Instant,
    /// Cumulative self-report counters (`BAML_OBS_STATS`).
    counters: ConsumerCounters,
    /// Per-producer-engine CCT aggregation (design §5).
    cct: HashMap<u64, crate::prof::cct::CctEngine>,
    /// Recently closed engines' CCT state, retained bounded so late
    /// queries (the §10.3 oracle tap, live UIs racing engine drop) still
    /// resolve. P3 replaces retention with the §6.5 boundary snapshot
    /// fold; the bound keeps engine-churning hosts O(1).
    cct_closed: std::collections::VecDeque<(u64, crate::prof::cct::CctEngine)>,
    /// §6.1 v2 session streams, one per engine (created lazily at the first
    /// non-empty flush window, or at engine close for sub-window engines).
    sessions: HashMap<u64, crate::prof::cct::session::SessionWriter>,
    /// §6.6 off-thread fsync helper (lazy: only when a session exists).
    fsync: Option<crate::prof::cct::session::FsyncService>,
    /// §6.3 window cadence (250 ms).
    last_window: Instant,
    layout: crate::prof::config::ObsLayout,
    /// §6.2 raw firehose (`BAML_PROFILE_RAW=1`): verbatim drained ranges
    /// per engine, flushed under the session's `raw/` dir at window ticks.
    profile_raw: bool,
    raw: HashMap<u64, crate::prof::cct::raw::RawSink>,
    /// §5.9 flight recorder: bounded raw-byte window across engines.
    flight: crate::prof::cct::flight::FlightRecorder,
    /// §3.1 rate limits: per engine (last dump, dumps so far, dropped).
    flight_dumps: HashMap<u64, (Instant, u32, u64)>,
    /// §6.4 bound boundaries: boundary_id → (engine, partition, dir,
    /// boundary-local id).
    boundaries: HashMap<[u8; 16], (u64, u32, PathBuf, u32)>,
    next_boundary_local: u32,
}

impl ConsumerState {
    fn new(dir: PathBuf, conv: TickConverter, profile_raw: bool) -> ConsumerState {
        let profile_writes_enabled = match ensure_profile_dir_ignored(&dir) {
            Ok(_) => true,
            Err(err) => {
                report(format_args!(
                    "cannot prepare .baml/.gitignore for profile dir {}; disabling profile persistence: {err}",
                    dir.display()
                ));
                false
            }
        };
        ConsumerState {
            dir,
            profile_writes_enabled,
            conv,
            closed_engines: std::collections::HashSet::new(),
            closed_reported: std::collections::HashSet::new(),
            process_id: process_id(),
            started_at_epoch_ns: clock::started_at_epoch_ns(),
            last_heartbeat: Instant::now(),
            counters: ConsumerCounters::default(),
            cct: HashMap::new(),
            cct_closed: std::collections::VecDeque::new(),
            sessions: HashMap::new(),
            fsync: None,
            last_window: Instant::now(),
            layout: ProfConfig::global().layout,
            profile_raw,
            raw: HashMap::new(),
            flight: crate::prof::cct::flight::FlightRecorder::new(
                crate::prof::cct::flight::FLIGHT_CAP_BYTES,
            ),
            flight_dumps: HashMap::new(),
            boundaries: HashMap::new(),
            next_boundary_local: 0,
        }
    }

    fn sweep_once(&mut self, env: &ConsumerEnv) -> bool {
        // SAFETY: consumer_main is the process's (or this test registry's)
        // single consumer thread.
        let progress = unsafe {
            env.registry
                .sweep(&mut |ring, bytes| self.transcode(ring, bytes))
        };
        if progress {
            self.counters.sweeps_with_progress += 1;
        }
        progress
    }

    /// Append one cumulative stats line (`BAML_OBS_STATS`). Called at flush
    /// and engine-close milestones — never on the sweep path.
    fn report_stats(&mut self, reporter: &mut StatsReporter, env: &ConsumerEnv) {
        if reporter.active() {
            let mut counters = self.counters.clone();
            let live = self.cct.values();
            let closed = self.cct_closed.iter().map(|(_, e)| e);
            for engine in live.chain(closed) {
                let diag = engine.diagnostics();
                // Records are decoded by the CCT engines (the only decode
                // pass since P9); the report sums live + retained-closed
                // engines, so heavy engine churn can undercount — a
                // diagnostic, not an accounting, number.
                counters.records += diag.records;
                counters.cct_nodes += engine.nodes().len() as u64;
                counters.cct_deferred += diag.deferred;
                counters.cct_synthesized += diag.synthesized_parents;
                counters.cct_evicted_calls += diag.evicted_calls;
            }
            reporter.report(&counters, env.ctx.live_bytes());
        }
    }

    /// The sink fan-out (observability design §10.3): every drained range
    /// passes through exactly this point — flight recorder, raw firehose,
    /// and the CCT aggregation plane. (The legacy `.bamlprof`/run-store/
    /// history fan-out that used to fork here was deleted in P9 step 4.)
    fn transcode(&mut self, ring: &'static Ring, bytes: &[u8]) {
        self.counters.ranges += 1;
        self.counters.bytes_drained += bytes.len() as u64;
        // SAFETY: bytes in hand are drain progress (Ring::engine_id
        // contract).
        let engine_id = unsafe { ring.engine_id() };
        if self.closed_engines.contains(&engine_id) {
            // Every commit happened-before the engine's last Arc release;
            // a range arriving after the close is a logic error — reported
            // once and dropped, never resurrecting the sealed state.
            self.counters.records_after_close_ranges += 1;
            if self.closed_reported.insert(engine_id) {
                report(format_args!(
                    "dropping records for closed engine {engine_id} (post-Drop emission?)"
                ));
            }
            return;
        }
        if self.layout.writes_v2() {
            // §5.9 flight recorder: one memcpy, zero transcode until a
            // trigger fires.
            self.flight.push(engine_id, bytes);
            if self.profile_raw && self.profile_writes_enabled {
                // §6.2 raw firehose: buffer the verbatim range; it lands
                // under the session's raw/ dir at the next window tick.
                self.raw.entry(engine_id).or_default().push_range(bytes);
            }
        }
        self.transcode_cct(ring, bytes);
    }

    /// TickConverter identity quad recorded in session and raw headers.
    fn clock_id(&self) -> (u8, u8, u64, u64) {
        let (numer, denom) = self.conv.rate();
        (
            self.conv.kind() as u8,
            self.conv.quality() as u8,
            numer,
            denom,
        )
    }

    /// The CCT aggregation sink (design §5): raw ring bytes, no protobuf —
    /// the always-on path leaves transcode entirely in `cct` mode. Disk
    /// flushing of windows lands with P3; until then state is RAM-only
    /// (plus the test snapshot for the §10.3 equivalence oracle).
    fn transcode_cct(&mut self, ring: &'static Ring, bytes: &[u8]) {
        // SAFETY: bytes in hand are drain progress (Ring::engine_id contract).
        let engine_id = unsafe { ring.engine_id() };
        let conv = self.conv.clone();
        let engine = self.cct.entry(engine_id).or_insert_with(|| {
            let function_count = metadata::get_engine_metadata(engine_id)
                .map_or(0, |meta| u32::try_from(meta.functions.len()).unwrap_or(0));
            crate::prof::cct::CctEngine::new(function_count)
        });
        engine.consume(bytes, &mut |ticks| conv.to_ns(ticks));
        // §3.1 triggers: root-level errored closes and over-threshold
        // latencies fire rate-limited flight dumps (self.flight_dump
        // enforces spacing/caps).
        let errored = engine.take_errored_roots();
        let slow = engine.take_latency_triggers();
        if self.layout.writes_v2() {
            if errored > 0 {
                let _ = self.flight_dump(engine_id, "error");
            }
            if slow > 0 {
                let _ = self.flight_dump(engine_id, "latency");
            }
        }
    }

    /// §10.3 oracle tap: `(engine_id, function_id, enters, ends)` rows
    /// summed across nodes — the exact quantities the raw-derived oracle
    /// recomputes from `.bamlprof` events in `dual` mode.
    fn cct_totals(&self) -> Vec<(u64, u32, u64, u64)> {
        let mut rows = Vec::new();
        let live = self.cct.iter().map(|(id, e)| (*id, e));
        let closed = self.cct_closed.iter().map(|(id, e)| (*id, e));
        for (engine_id, engine) in live.chain(closed) {
            let nodes = engine.nodes();
            let mut by_function: HashMap<u32, (u64, u64)> = HashMap::new();
            for i in 0..nodes.len() {
                let entry = by_function.entry(nodes.function[i]).or_default();
                entry.0 += nodes.enters[i];
                entry.1 += nodes.ends_ok[i]
                    + nodes.ends_err[i]
                    + nodes.ends_cancel[i]
                    + nodes.ends_exit[i];
            }
            for (function, (enters, ends)) in by_function {
                rows.push((engine_id, function, enters, ends));
            }
        }
        rows
    }

    /// §9.2 `LiveMirrorSource` tap: whole-engine fold encoded as an
    /// always-sealed BCCT segment. Live map first, then bounded closed
    /// retention (live UIs racing engine drop).
    fn cct_live_segment(&self, engine_id: u64) -> Option<Vec<u8>> {
        let engine = self.cct.get(&engine_id).or_else(|| {
            self.cct_closed
                .iter()
                .find(|(id, _)| *id == engine_id)
                .map(|(_, e)| e)
        })?;
        let folded = crate::prof::cct::fold::fold_all(engine);
        let meta = metadata::get_engine_metadata(engine_id);
        let revision_string = meta
            .as_ref()
            .and_then(|m| m.revision_id.clone())
            .unwrap_or_default();
        let revision_bytes =
            bex_vm_types::RevisionId::decode(&revision_string).map_or([0u8; 32], |id| id.0);
        let (numer, denom) = self.conv.rate();
        let header = crate::prof::cct::segment::SegmentHeader {
            process_euid: self.process_id,
            engine_id,
            session_seg_seq: 0,
            started_epoch_ns: u64::try_from(self.started_at_epoch_ns).unwrap_or(u64::MAX),
            clock_kind: self.conv.kind() as u8,
            clock_quality: self.conv.quality() as u8,
            tick_ns_numer: numer,
            tick_ns_denom: denom,
            revision_id: revision_bytes,
        };
        Some(crate::prof::cct::fold::encode_live_snapshot(
            &folded, &header,
        ))
    }

    /// §9.4 recent-ring tap body (live map, then bounded closed retention).
    fn recent_calls(&self, engine_id: u64) -> Option<Vec<RecentCallOut>> {
        let engine = self.cct.get(&engine_id).or_else(|| {
            self.cct_closed
                .iter()
                .find(|(id, _)| *id == engine_id)
                .map(|(_, e)| e)
        })?;
        let nodes = engine.nodes();
        let mut out = Vec::new();
        for partition in 0..engine.partition_count() {
            let Some(ring) = engine.recent_ring(partition) else {
                continue;
            };
            for call in ring.iter() {
                let function = nodes.function.get(call.node as usize).copied().unwrap_or(0);
                out.push(RecentCallOut {
                    partition,
                    thread_idx: call.thread_idx,
                    call_id: call.call_id,
                    parent_call_id: call.parent_call_id,
                    function,
                    start_ns: call.start_ns,
                    end_ns: call.end_ns,
                    status: call.status,
                });
            }
        }
        Some(out)
    }

    /// §5.9 dump: transcode the retained raw window for one engine into
    /// `sessions/<sess>/flight/<ts_ms>-<trigger>.bamlprof`. Rate-limited
    /// per §3.1 (≥5 s spacing, ≤16 dumps/engine, dropped counted).
    fn flight_dump(&mut self, engine_id: u64, trigger: &str) -> Option<PathBuf> {
        use crate::prof::cct::flight::{DUMP_MAX_PER_ENGINE, DUMP_MIN_INTERVAL};
        let now = Instant::now();
        let entry = self.flight_dumps.entry(engine_id).or_insert((now, 0, 0));
        if entry.1 >= DUMP_MAX_PER_ENGINE
            || (entry.1 > 0 && now.duration_since(entry.0) < DUMP_MIN_INTERVAL)
        {
            entry.2 += 1;
            return None;
        }

        let conv = self.conv.clone();
        let meta = metadata::get_engine_metadata(engine_id);
        let header = build_header(
            self.process_id,
            engine_id,
            self.started_at_epoch_ns,
            meta.as_ref(),
            &conv,
        );
        let mut buf = Vec::new();
        crate::prof::encode::encode_length_delimited_message(&mut buf, &header).ok()?;
        let mut count: u64 = 0;
        for chunk in self.flight.retained(engine_id) {
            for rec in record::iter(&chunk.bytes) {
                let Ok(raw) = rec else { break };
                let event = to_disk_event(&raw, &conv);
                crate::prof::encode::encode_disk_event(&mut buf, &event);
                count += 1;
            }
        }
        if count == 0 {
            return None;
        }

        // The session dir name is deterministic (§6.1) — usable whether or
        // not a SessionWriter is currently live.
        let baml_dir = self
            .dir
            .parent()
            .map_or_else(|| PathBuf::from(".baml"), Path::to_path_buf);
        let started_secs =
            u64::try_from(self.started_at_epoch_ns / 1_000_000_000).unwrap_or(u64::MAX);
        let euid_hex: String = self.process_id.iter().map(|b| format!("{b:02x}")).collect();
        let flight_dir = baml_dir
            .join("sessions")
            .join(format!("{started_secs}-{euid_hex}-e{engine_id}"))
            .join("flight");
        std::fs::create_dir_all(&flight_dir).ok()?;
        let ts_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0);
        let slug: String = trigger
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .take(48)
            .collect();
        let path = flight_dir.join(format!("{ts_ms}-{slug}.bamlprof"));
        // Durable tmp → rename: a torn dump must never appear under its
        // final name. Dumps carry only structural events — no CID
        // references — so no `.bamlcids` pin sidecar is owed; if the
        // transcoder ever emits value references, it must write the pin
        // (GC already honors `flight/*.bamlcids`) in this same barrier.
        let tmp = flight_dir.join(format!(".{ts_ms}-{slug}.bamlprof.tmp"));
        crate::fsutil::write_replace_durable(&tmp, &path, &buf).ok()?;
        entry.0 = now;
        entry.1 += 1;
        self.counters.flight_dumps += 1;
        // §5.9: bound boundaries of this engine record the dump reference
        // (BoundaryTrigger) so the UI can jump CCT node → exact evidence.
        let dump_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        for (engine, _, boundary_dir, _) in self.boundaries.values() {
            if *engine != engine_id {
                continue;
            }
            if let Ok(mut meta) =
                crate::prof::cct::meta::MetaWriter::create(&boundary_dir.join("boundary.bamlmeta"))
            {
                let _ = meta.append(&crate::prof::cct::meta::MetaRecord::BoundaryTrigger {
                    trigger: trigger.to_string(),
                    at_ms: ts_ms,
                    detail: format!("flight:{dump_name}"),
                });
            }
        }
        Some(path)
    }

    /// One consumer-loop tick for the CCT engines: ages deferrals (§5.2)
    /// and finalizes quiescent threads.
    fn cct_sweep_tick(&mut self) {
        let conv = self.conv.clone();
        for engine in self.cct.values_mut() {
            engine.sweep_tick(&mut |ticks| conv.to_ns(ticks));
        }
    }

    /// §6.4 bind: resolve the root thread's partition, remember the
    /// binding, append the boundary `bound` record, and stamp the
    /// `partition_bind` row into the session stream.
    fn bind_boundary(
        &mut self,
        engine_id: u64,
        boundary_id: [u8; 16],
        root_thread: u64,
        boundary_dir: &Path,
    ) -> bool {
        // Live state, else the bounded retention of recently closed
        // engines (short CLI runs can bind after the engine dropped).
        let engine = self.cct.get(&engine_id).or_else(|| {
            self.cct_closed
                .iter()
                .find(|(id, _)| *id == engine_id)
                .map(|(_, engine)| engine)
        });
        let Some(engine) = engine else {
            report(format_args!(
                "bind_boundary: engine {engine_id} has no CCT state (pipeline off?)"
            ));
            return false;
        };
        let Some(partition) = engine.partition_of_thread(root_thread) else {
            report(format_args!(
                "bind_boundary: thread {root_thread} unknown to engine {engine_id}"
            ));
            return false;
        };
        let boundary_local_id = self.next_boundary_local;
        self.next_boundary_local += 1;
        let created_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0);
        // The bound record into the boundary's own meta stream.
        let meta_ok =
            crate::prof::cct::meta::MetaWriter::create(&boundary_dir.join("boundary.bamlmeta"))
                .and_then(|mut writer| {
                    writer.append(&crate::prof::cct::meta::MetaRecord::BoundaryBound {
                        session_dir: self
                            .sessions
                            .get(&engine_id)
                            .map(|s| s.session_dir().display().to_string())
                            .unwrap_or_default(),
                        first_seg_seq: 0,
                        partition_id: partition,
                        boundary_local_id,
                    })?;
                    writer.sync_data()
                });
        if let Err(err) = meta_ok {
            report(format_args!(
                "bind_boundary: cannot append bound record under {}: {err}",
                boundary_dir.display()
            ));
            return false;
        }
        // The session stream's partition_bind row (best effort: the stream
        // may not exist yet if nothing flushed — bind again next window is
        // unnecessary since rows are per-bind, so write through if open).
        if let Some(writer) = self.sessions.get_mut(&engine_id) {
            let row = crate::prof::cct::blocks::PartitionBindRow {
                partition_id: partition,
                boundary_local_id,
                boundary_id,
                created_ms,
            };
            if let Err(err) = writer.write_partition_bind(row, engine.max_seen_ns()) {
                report(format_args!("partition_bind write failed: {err}"));
            }
        }
        self.boundaries.insert(
            boundary_id,
            (
                engine_id,
                partition,
                boundary_dir.to_path_buf(),
                boundary_local_id,
            ),
        );
        true
    }

    /// §6.5 completion: fold the partition into a sealed `cct.bamlcct`
    /// (tmp+rename, D2), append the meta `complete` record, free the
    /// partition (§5.7).
    fn complete_boundary(&mut self, boundary_id: [u8; 16], status: &str) -> bool {
        let Some((engine_id, partition, dir, boundary_local_id)) =
            self.boundaries.remove(&boundary_id)
        else {
            report(format_args!("complete_boundary: unknown boundary"));
            return false;
        };
        let engine = if let Some(engine) = self.cct.get_mut(&engine_id) {
            engine
        } else if let Some((_, engine)) =
            self.cct_closed.iter_mut().find(|(id, _)| *id == engine_id)
        {
            engine
        } else {
            return false;
        };
        let folded = crate::prof::cct::fold::fold_partition(engine, partition);
        let meta = metadata::get_engine_metadata(engine_id);
        let revision_bytes = meta
            .as_ref()
            .and_then(|m| m.revision_id.as_deref())
            .and_then(bex_vm_types::RevisionId::decode)
            .map_or([0u8; 32], |id| id.0);
        let (numer, denom) = self.conv.rate();
        let header = crate::prof::cct::segment::SegmentHeader {
            process_euid: self.process_id,
            engine_id,
            session_seg_seq: 0,
            started_epoch_ns: u64::try_from(self.started_at_epoch_ns).unwrap_or(u64::MAX),
            clock_kind: self.conv.kind() as u8,
            clock_quality: self.conv.quality() as u8,
            tick_ns_numer: numer,
            tick_ns_denom: denom,
            revision_id: revision_bytes,
        };
        let created_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0);
        let bind = crate::prof::cct::blocks::PartitionBindRow {
            partition_id: partition,
            boundary_local_id,
            boundary_id,
            created_ms,
        };
        let snapshot = crate::prof::cct::fold::encode_boundary_snapshot(&folded, &header, bind);
        // tmp + rename + dir sync (D2).
        let tmp = dir.join(format!(".cct.bamlcct.tmp-{}", std::process::id()));
        let final_path = dir.join("cct.bamlcct");
        let write_ok = std::fs::write(&tmp, &snapshot)
            .and_then(|()| std::fs::File::open(&tmp).and_then(|f| f.sync_data()))
            .and_then(|()| std::fs::rename(&tmp, &final_path))
            .and_then(|()| {
                std::fs::File::open(&dir)
                    .and_then(|d| d.sync_data())
                    .or(Ok(()))
            });
        if let Err(err) = write_ok {
            let _ = std::fs::remove_file(&tmp);
            report(format_args!(
                "boundary snapshot write failed under {}: {err}",
                dir.display()
            ));
            return false;
        }
        let completed_ms = created_ms;
        let meta_ok = crate::prof::cct::meta::MetaWriter::create(&dir.join("boundary.bamlmeta"))
            .and_then(|mut writer| {
                writer.append(&crate::prof::cct::meta::MetaRecord::BoundaryComplete {
                    status: status.to_string(),
                    completed_ms,
                    last_seg_seq: 0,
                    counts: serde_json::json!({
                        "nodes": folded.totals.len(),
                        "spawn_edges": folded.spawns.len(),
                    }),
                    diagnostics: Vec::new(),
                    dump_refs: Vec::new(),
                })?;
                writer.sync_data()
            });
        if let Err(err) = meta_ok {
            report(format_args!("boundary complete record failed: {err}"));
        }
        // §5.7: the boundary's per-partition state frees now.
        engine.free_partition(partition);
        true
    }

    /// §6.3 window cadence: every 250 ms (or immediately when `force` is
    /// set — the explicit-flush path), flush each engine's dirty-node
    /// deltas into its v2 session stream, land buffered raw-firehose
    /// ranges, and run the group-commit / rotation tick.
    fn cct_window_tick(&mut self, force: bool) {
        const WINDOW: Duration = Duration::from_millis(250);
        if !self.layout.writes_v2()
            || !self.profile_writes_enabled
            || (!force && self.last_window.elapsed() < WINDOW)
        {
            return;
        }
        self.last_window = Instant::now();
        // Wall NOW (not the process anchor): heartbeat/watermark rows are
        // liveness attestations.
        let wall_epoch_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
            .unwrap_or(0);
        let baml_dir = self
            .dir
            .parent()
            .map_or_else(|| PathBuf::from(".baml"), Path::to_path_buf);
        let clock = self.clock_id();
        for (&engine_id, engine) in &mut self.cct {
            let flush = engine.flush_window();
            let has_rows = !(flush.birth_rows.is_empty()
                && flush.delta_rows.is_empty()
                && flush.hist_rows.is_empty()
                && flush.llm_rows.is_empty()
                && flush.spawn_rows.is_empty()
                && flush.model_rows.is_empty());
            let writer = match self.sessions.entry(engine_id) {
                std::collections::hash_map::Entry::Occupied(o) => o.into_mut(),
                std::collections::hash_map::Entry::Vacant(v) => {
                    if !has_rows {
                        // No session dir until there is something to say.
                        continue;
                    }
                    let fsync = self
                        .fsync
                        .get_or_insert_with(crate::prof::cct::session::FsyncService::start);
                    match Self::mint_session(
                        &baml_dir,
                        self.process_id,
                        self.started_at_epoch_ns,
                        &self.conv,
                        fsync,
                        engine_id,
                    ) {
                        Some(writer) => v.insert(writer),
                        None => continue,
                    }
                }
            };
            let max_seen = engine.max_seen_ns();
            if has_rows
                && let Err(err) = writer.write_window(&flush, engine.nodes(), 0, max_seen, {
                    let d = engine.diagnostics();
                    d.records
                })
            {
                report(format_args!(
                    "v2 session write failed for engine {engine_id}: {err}"
                ));
            }
            if let Some(fsync) = &self.fsync
                && let Err(err) = writer.tick(fsync, wall_epoch_ns, max_seen)
            {
                report(format_args!(
                    "v2 session tick failed for engine {engine_id}: {err}"
                ));
            }
            // §6.4 crash-detection heartbeat (rate-limited inside).
            let _ = writer.heartbeat(wall_epoch_ns);
            // §6.2 raw firehose: land buffered verbatim ranges under this
            // session's raw/ dir. Follows epoch rotations automatically
            // (the sink resets when the session dir changes).
            if let Some(sink) = self.raw.get_mut(&engine_id)
                && let Err(err) =
                    sink.flush_to(writer.session_dir(), self.process_id, engine_id, clock)
            {
                report(format_args!(
                    "raw firehose write failed for engine {engine_id}: {err}"
                ));
            }
            // §6.1 epoch rotation: close this session (carry-over
            // checkpoint + seal), reset the engine's node table, and let
            // the next window mint a fresh session dir.
            if writer.should_rotate_epoch() {
                let function_hint = u32::try_from(engine.nodes().len()).unwrap_or(0);
                if let Some(writer) = self.sessions.remove(&engine_id)
                    && let Err(err) = writer.close_epoch(engine.nodes(), max_seen)
                {
                    report(format_args!(
                        "epoch close failed for engine {engine_id}: {err}"
                    ));
                }
                engine.rotate_epoch(function_hint);
            }
        }
    }

    /// Mint one §6.1 session writer. Called lazily at window ticks and —
    /// for engines shorter than one 250 ms window (the common fast-CLI
    /// case) — at engine close, so every engine leaves a sealed session.
    fn mint_session(
        baml_dir: &Path,
        process_id: [u8; 16],
        started_at_epoch_ns: u128,
        conv: &TickConverter,
        fsync: &crate::prof::cct::session::FsyncService,
        engine_id: u64,
    ) -> Option<crate::prof::cct::session::SessionWriter> {
        let meta = metadata::get_engine_metadata(engine_id);
        // §4.2 write ordering: the revision dictionary lands (or is
        // confirmed present — idempotent, content-addressed) before any
        // artifact referencing the revision is created. Failure degrades to
        // the embedded function tables, warned once per mint attempt.
        if let Some(dict) = meta.as_ref().and_then(|m| m.dictionary.as_ref()) {
            let dict_dir = baml_dir.join("dict");
            if let Err(err) = crate::dict::ensure_dict_written(&dict_dir, dict) {
                report(format_args!(
                    "cannot write revision dictionary under {}; readers will fall \
                     back to embedded tables: {err}",
                    dict_dir.display()
                ));
            }
        }
        let revision_string = meta
            .as_ref()
            .and_then(|m| m.revision_id.clone())
            .unwrap_or_default();
        let revision_bytes =
            bex_vm_types::RevisionId::decode(&revision_string).map_or([0u8; 32], |id| id.0);
        let (numer, denom) = conv.rate();
        match crate::prof::cct::session::SessionWriter::create(
            baml_dir,
            process_id,
            engine_id,
            u64::try_from(started_at_epoch_ns).unwrap_or(u64::MAX),
            (conv.kind() as u8, conv.quality() as u8, numer, denom),
            revision_bytes,
            &revision_string,
            fsync,
        ) {
            Ok(writer) => Some(writer),
            Err(err) => {
                report(format_args!(
                    "cannot create v2 session stream for engine {engine_id}: {err}"
                ));
                None
            }
        }
    }

    /// ≥1 s consumer tick: the designated refinement slot for the x86 TSC
    /// rate (no-op for exact-rate sources / once refined). The legacy
    /// per-file heartbeat stamping that used to live here died with the
    /// `.bamlprof` writers (P9); session-stream heartbeats are rate-limited
    /// inside `SessionWriter::heartbeat` at window cadence.
    fn maybe_heartbeat(&mut self) {
        if self.last_heartbeat.elapsed() < HEARTBEAT_INTERVAL {
            return;
        }
        self.last_heartbeat = Instant::now();
        self.counters.heartbeats += 1;
        self.conv.maybe_refine();
    }

    /// Flush and seal one engine's session stream (engine close); later
    /// records for it are tombstoned.
    fn close_engine(&mut self, engine_id: u64) {
        // P3 replaces this with the §6.5 boundary snapshot fold; retained
        // (bounded) so oracle taps and live UIs racing the engine drop
        // still read the final state.
        if let Some(mut engine) = self.cct.remove(&engine_id) {
            let conv = self.conv.clone();
            engine.sweep_tick(&mut |ticks| conv.to_ns(ticks));
            // Final window + seal for the v2 session stream (§6.1: rotation
            // at engine close).
            let flush = engine.flush_window();
            let has_rows = !(flush.birth_rows.is_empty()
                && flush.delta_rows.is_empty()
                && flush.hist_rows.is_empty()
                && flush.llm_rows.is_empty()
                && flush.spawn_rows.is_empty()
                && flush.model_rows.is_empty());
            let mut writer = self.sessions.remove(&engine_id);
            if writer.is_none()
                && has_rows
                && self.layout.writes_v2()
                && self.profile_writes_enabled
            {
                // Engines shorter than one 250 ms window (fast CLI runs)
                // never hit a window tick: mint their session now so every
                // engine leaves a sealed session on disk.
                let baml_dir = self
                    .dir
                    .parent()
                    .map_or_else(|| PathBuf::from(".baml"), Path::to_path_buf);
                let fsync = self
                    .fsync
                    .get_or_insert_with(crate::prof::cct::session::FsyncService::start);
                writer = Self::mint_session(
                    &baml_dir,
                    self.process_id,
                    self.started_at_epoch_ns,
                    &self.conv,
                    fsync,
                    engine_id,
                );
            }
            if let Some(mut writer) = writer {
                let max_seen = engine.max_seen_ns();
                if has_rows {
                    let _ = writer.write_window(&flush, engine.nodes(), 0, max_seen, {
                        let d = engine.diagnostics();
                        d.records
                    });
                }
                // §6.2: final raw firehose flush before the session seals.
                if let Some(mut sink) = self.raw.remove(&engine_id) {
                    let clock = self.clock_id();
                    if let Err(err) =
                        sink.flush_to(writer.session_dir(), self.process_id, engine_id, clock)
                    {
                        report(format_args!(
                            "final raw firehose flush for engine {engine_id} failed: {err}"
                        ));
                    }
                }
                if let Err(err) = writer.close(max_seen, "engine_closed") {
                    report(format_args!(
                        "sealing v2 session for engine {engine_id} failed: {err}"
                    ));
                }
            } else {
                // No session (v1 layout, disabled writes, or an engine with
                // nothing to say): drop any buffered raw ranges with it.
                self.raw.remove(&engine_id);
            }
            self.cct_closed.push_back((engine_id, engine));
            while self.cct_closed.len() > 8 {
                self.cct_closed.pop_front();
            }
        }
        let _ = metadata::remove_engine_metadata(engine_id);
        self.closed_engines.insert(engine_id);
        self.counters.engines_closed += 1;
    }
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

/// This process's observability euid as the hex-32 string used in session
/// dir names (`<started>-<euid_hex32>-e<engine>`). Hosts compare run keys
/// against it before routing a §9.2 live-mirror tap — engine ids are dense
/// per process, so a bare id lookup could collide across processes.
#[must_use]
pub fn process_euid_hex() -> String {
    process_id().iter().map(|b| format!("{b:02x}")).collect()
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

/// Crate-internal alias for sibling modules (stats self-reporting) that
/// need the same never-panic diagnostic channel.
pub(crate) fn report_public(msg: std::fmt::Arguments<'_>) {
    report(msg);
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
            cct::raw::read_raw_file,
            pb,
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
            "hygiene setup never creates the (legacy, now writer-less) profiles dir"
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

        let state = ConsumerState::new(profile_dir.clone(), TickConverter::identity(), false);
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

        let state = ConsumerState::new(profile_dir, TickConverter::identity(), false);
        assert!(!state.profile_writes_enabled);

        std::fs::remove_dir_all(&root).ok();
    }

    /// The PR3 gate, re-pointed at the raw firehose (P9 step 4): a fake
    /// producer pushes a known sequence of raw records through a real ring
    /// (tiny segments → constant growth) into a real consumer thread; the
    /// session's `raw/` files must replay (via `to_disk_event`, the flight
    /// dump / test-oracle transcode) to the exact event sequence, and the
    /// registered metadata must still build the full header.
    #[test]
    fn e2e_fake_producer_roundtrip() {
        const ENGINE: u64 = 0xE2E0_0001;
        const PAIRS: u64 = if cfg!(miri) { 40 } else { 2_000 };

        let root = temp_dir("e2e");
        // sessions/ lands under the profile dir's parent (.baml).
        let dir = root.join(".baml/profiles");
        let registry: &'static Registry = leak(Registry::new());
        let ctx: &'static RingCtx = leak(RingCtx::new(1 << 40));
        let (ctl_tx, ctl_rx) = mpsc::channel();
        let env = ConsumerEnv {
            registry,
            ctx,
            dir: dir.clone(),
            wake_interval: Duration::from_millis(1),
            clock: ClockMode::Fixed(TickConverter::identity()),
            pipeline: PipelineMode::Cct,
            profile_raw: true,
            stats_path: None,
        };
        std::thread::Builder::new()
            .name("test-prof-consumer".into())
            .spawn(move || consumer_main(&ctl_rx, &env))
            .unwrap();

        register_engine_metadata(
            ENGINE,
            EngineProfileMetadata {
                program_id: "test-program".into(),
                source_snapshot_id: Some("snapshot-1".into()),
                revision_id: Some("revision-1".into()),
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
                dictionary: None,
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

        // Flush-with-ack (the same protocol flush_and_join uses). The
        // forced window flush behind the ack lands the raw firehose.
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        ctl_tx.send(ControlMsg::Flush(ack_tx)).unwrap();
        ctx.wake().force_wake();
        ack_rx
            .recv_timeout(Duration::from_mins(1))
            .expect("consumer never acked the flush");

        // Demux the session dir by engine-id suffix and replay its raw/.
        let sessions = root.join(".baml/sessions");
        let session_dir = std::fs::read_dir(&sessions)
            .expect("sessions root missing")
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(&format!("-e{ENGINE}")))
            })
            .expect("session dir for the test engine");
        let mut raw_files: Vec<_> = std::fs::read_dir(session_dir.join("raw"))
            .expect("raw/ under the session")
            .map(|e| e.unwrap().path())
            .collect();
        raw_files.sort();
        assert!(!raw_files.is_empty(), "at least one raw-NNNNNN.bamlprof");

        let mut got: Vec<Event> = Vec::new();
        for path in &raw_files {
            let parsed = read_raw_file(&std::fs::read(path).unwrap()).expect("raw file parses");
            assert_eq!(parsed.engine_id, ENGINE);
            assert_eq!(parsed.torn_bytes, 0, "flushed run leaves no torn tail");
            assert_eq!(parsed.clock.2, 1, "identity clock numer");
            assert_eq!(parsed.clock.3, 1, "identity clock denom");
            let conv = TickConverter::from_rate(parsed.clock.2, parsed.clock.3);
            for range in &parsed.ranges {
                for rec in record::iter(range) {
                    let raw = rec.expect("raw range decodes");
                    got.extend(to_disk_event(&raw, &conv).event);
                }
            }
        }
        assert_eq!(got, expected, "event sequence must round-trip exactly");

        // The registered metadata still yields the full header (the flight
        // dump / test-oracle header path).
        let meta = metadata::get_engine_metadata(ENGINE).expect("metadata still registered");
        let header = build_header(
            [0xEE; 16],
            ENGINE,
            123,
            Some(&meta),
            &TickConverter::identity(),
        );
        assert_eq!(header.engine_id, ENGINE);
        assert_eq!(header.program_id, "test-program");
        assert_eq!(header.source_snapshot_id.as_deref(), Some("snapshot-1"));
        assert_eq!(header.revision_id.as_deref(), Some("revision-1"));
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

        std::fs::remove_dir_all(&root).ok();
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

        let mut state = ConsumerState::new(dir.clone(), TickConverter::identity(), false);
        let env = ConsumerEnv {
            registry,
            ctx,
            dir: dir.clone(),
            wake_interval: Duration::from_millis(1),
            clock: ClockMode::Fixed(TickConverter::identity()),
            pipeline: PipelineMode::Cct,
            profile_raw: false,
            stats_path: None,
        };
        let start = Instant::now();
        while state.sweep_once(&env) {}
        state.cct_window_tick(true);
        let elapsed = start.elapsed();

        #[expect(clippy::cast_precision_loss, reason = "display only")]
        let rate = EVENTS as f64 / elapsed.as_secs_f64();
        println!(
            "prof_drain_throughput: {EVENTS} events in {elapsed:.2?} = {:.1}M events/s/core",
            rate / 1.0e6
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    use crate::prof::config;

    /// PR5 orphan-path soak: rounds of short-lived producer threads (the
    /// tokio blocking-pool churn pattern) against a LIVE consumer thread —
    /// orphan → drain-to-empty → pool → claim must hold under the real
    /// consumer loop, the registry must stay bounded by peak concurrency,
    /// and every record must reach disk (the raw firehose since P9).
    #[test]
    fn soak_orphan_churn_with_live_consumer() {
        const ENGINE: u64 = 0x50AC_0001;
        let rounds: u64 = if cfg!(miri) { 4 } else { 64 };
        let per_round: u64 = if cfg!(miri) { 20 } else { 500 };

        let root = temp_dir("soak");
        let dir = root.join(".baml/profiles");
        let registry: &'static Registry = leak(Registry::new());
        let ctx: &'static RingCtx = leak(RingCtx::new(1 << 40));
        let (ctl_tx, ctl_rx) = mpsc::channel();
        let env = ConsumerEnv {
            registry,
            ctx,
            dir: dir.clone(),
            wake_interval: Duration::from_millis(1),
            clock: ClockMode::Fixed(TickConverter::identity()),
            pipeline: PipelineMode::Cct,
            profile_raw: true,
            stats_path: None,
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
                // §7 decision 7: a thread's first record is its StartThread
                // (the CCT plane attaches calls through it; bare calls
                // would sit deferred and never flush a session window).
                let len = RawRecord::StartThread {
                    flags: 0,
                    thread_id: BexThreadId(round + 1),
                    parent_thread_id: BexThreadId(0),
                    parent_call_id: BexCallId(0),
                    ts_ticks: round * (per_round + 2),
                    name: b"soak",
                }
                .encode(&mut buf);
                // SAFETY: claiming thread, alive for the whole closure.
                unsafe { h.push(&buf[..len]) };
                for seq in 0..per_round {
                    let len = RawRecord::CallFunction {
                        flags: 0,
                        thread_id: BexThreadId(round + 1),
                        call_id: BexCallId(seq + 1),
                        parent_call_id: BexCallId(seq),
                        function_id: FunctionId(1),
                        call_site: None,
                        ts_ticks: round * (per_round + 2) + seq + 1,
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

        // Every record reached the raw firehose exactly once.
        let sessions = root.join(".baml/sessions");
        let session_dir = std::fs::read_dir(&sessions)
            .expect("sessions root missing")
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(&format!("-e{ENGINE}")))
            })
            .expect("session dir for the soak engine");
        let mut raw_files: Vec<_> = std::fs::read_dir(session_dir.join("raw"))
            .expect("raw/ under the session")
            .map(|e| e.unwrap().path())
            .collect();
        raw_files.sort();
        let mut calls: u64 = 0;
        for path in &raw_files {
            let parsed = read_raw_file(&std::fs::read(path).unwrap()).expect("raw file parses");
            assert_eq!(parsed.engine_id, ENGINE);
            for range in &parsed.ranges {
                for rec in record::iter(range) {
                    if matches!(
                        rec.expect("raw range decodes"),
                        RawRecord::CallFunction { .. }
                    ) {
                        calls += 1;
                    }
                }
            }
        }
        assert_eq!(calls, rounds * per_round, "soak lost or duplicated records");

        std::fs::remove_dir_all(&root).ok();
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
