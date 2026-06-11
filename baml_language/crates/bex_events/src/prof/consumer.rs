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
    collections::HashMap,
    io,
    path::PathBuf,
    sync::{Mutex, OnceLock, mpsc},
    time::{Duration, Instant},
};

use crate::prof::{
    EngineProfileMetadata, clock,
    config::ProfConfig,
    file::{ProfileWriter, build_header},
    pb,
    record::{self, FunctionEndStatus, RawRecord, ThreadEndStatus},
    registry::Registry,
    ring::{Ring, RingCtx},
};

/// How often the consumer stamps a process-liveness heartbeat into every
/// open file (MVP: consumer-stamped, v2 §4).
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);

fn engine_meta() -> &'static Mutex<HashMap<u64, EngineProfileMetadata>> {
    static META: OnceLock<Mutex<HashMap<u64, EngineProfileMetadata>>> = OnceLock::new();
    META.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registers an engine's header metadata. Call before (or shortly after) the
/// engine starts producing: the header is written when the consumer opens
/// the engine's file, i.e. when its first events are drained.
pub fn register_engine_metadata(engine_id: u64, meta: EngineProfileMetadata) {
    engine_meta()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(engine_id, meta);
}

pub(crate) enum ControlMsg {
    /// Drain everything currently committed, sync files durably, then ack.
    Flush(mpsc::SyncSender<()>),
    /// An engine was dropped: drain its remaining events, sync + close its
    /// file (freeing the fd and stopping its heartbeats), drop its metadata,
    /// and tombstone the id. Sent non-blocking from `BexEngine::drop`.
    EngineClosed(u64),
}

/// Everything the consumer loop needs; owned so tests can run private
/// consumers against private registries and directories.
pub(crate) struct ConsumerEnv {
    pub(crate) registry: &'static Registry,
    pub(crate) ctx: &'static RingCtx,
    pub(crate) dir: PathBuf,
    pub(crate) wake_interval: Duration,
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
/// metadata entry is freed. Non-blocking — safe from `Drop`. A no-op when
/// profiling never started.
pub fn engine_closed(engine_id: u64) {
    let Some(tx) = CONTROL_TX.get() else {
        return;
    };
    if tx.send(ControlMsg::EngineClosed(engine_id)).is_ok() {
        crate::prof::registry::global_ctx().wake().force_wake();
    }
}

/// The consumer loop: §3.4 sweep + §3.6 wake protocol + control messages.
pub(crate) fn consumer_main(control: &mpsc::Receiver<ControlMsg>, env: &ConsumerEnv) {
    env.ctx.wake().register_consumer();
    let mut state = ConsumerState::new(env.dir.clone());
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
}

impl ConsumerState {
    fn new(dir: PathBuf) -> ConsumerState {
        ConsumerState {
            dir,
            writers: HashMap::new(),
            closed_engines: std::collections::HashSet::new(),
            closed_reported: std::collections::HashSet::new(),
            process_id: process_id(),
            started_at_epoch_ns: clock::started_at_epoch_ns(),
            last_heartbeat: Instant::now(),
            corrupt_reported: false,
        }
    }

    fn sweep_once(&mut self, env: &ConsumerEnv) -> bool {
        // SAFETY: consumer_main is the process's (or this test registry's)
        // single consumer thread.
        unsafe {
            env.registry
                .sweep(&mut |ring, bytes| self.transcode(ring, bytes))
        }
    }

    fn transcode(&mut self, ring: &'static Ring, bytes: &[u8]) {
        // SAFETY: bytes in hand are drain progress (Ring::engine_id contract).
        let engine_id = unsafe { ring.engine_id() };
        if self.closed_engines.contains(&engine_id) {
            if self.closed_reported.insert(engine_id) {
                report(format_args!(
                    "dropping records for closed engine {engine_id} (post-Drop emission?)"
                ));
            }
            return;
        }
        let Some(writer) = self.writer_for(engine_id) else {
            return;
        };
        let mut io_result = Ok(());
        for rec in record::iter(bytes) {
            match rec {
                Ok(raw) => {
                    let event = to_disk_event(&raw);
                    io_result = writer.write_event(&event);
                    if io_result.is_err() {
                        break;
                    }
                }
                Err(err) => {
                    // A committed range that fails to decode is a producer
                    // bug — report once, drop the rest of the range (the
                    // framing is unrecoverable past this point).
                    if !self.corrupt_reported {
                        self.corrupt_reported = true;
                        report(format_args!(
                            "corrupt profiling record in committed range (engine {engine_id}): {err:?}"
                        ));
                    }
                    break;
                }
            }
        }
        if let Err(err) = io_result {
            self.fail_writer(engine_id, &err);
        }
    }

    fn writer_for(&mut self, engine_id: u64) -> Option<&mut ProfileWriter> {
        self.writers
            .entry(engine_id)
            .or_insert_with(|| {
                let meta = engine_meta()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .get(&engine_id)
                    .cloned();
                let header = build_header(
                    self.process_id,
                    engine_id,
                    self.started_at_epoch_ns,
                    meta.as_ref(),
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

    fn maybe_heartbeat(&mut self) {
        if self.last_heartbeat.elapsed() < HEARTBEAT_INTERVAL {
            return;
        }
        self.last_heartbeat = Instant::now();
        let ts = clock::now_ns();
        let event = pb::DiskEventV1 {
            event: Some(pb::disk_event_v1::Event::Heartbeat(pb::Heartbeat {
                timestamp_ns: ts,
            })),
        };
        let mut failed = Vec::new();
        for (&engine_id, slot) in &mut self.writers {
            if let Some(writer) = slot
                && writer.write_event(&event).is_err()
            {
                failed.push(engine_id);
            }
        }
        for engine_id in failed {
            self.fail_writer(engine_id, &io::Error::other("heartbeat write failed"));
        }
    }

    /// Sync (engine close / explicit flush) and close one engine's file;
    /// later records for it are tombstoned.
    fn close_engine(&mut self, engine_id: u64) {
        if let Some(Some(mut writer)) = self.writers.remove(&engine_id)
            && let Err(err) = writer.sync()
        {
            report(format_args!(
                "sync of {} on engine close failed: {err}",
                writer.path().display()
            ));
        }
        engine_meta()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&engine_id);
        self.closed_engines.insert(engine_id);
    }

    /// Durable flush of every open file (`fsync`); the explicit-flush path.
    fn sync_files(&mut self) {
        let mut failed = Vec::new();
        for (&engine_id, slot) in &mut self.writers {
            if let Some(writer) = slot
                && writer.sync().is_err()
            {
                failed.push(engine_id);
            }
        }
        for engine_id in failed {
            self.fail_writer(engine_id, &io::Error::other("sync failed"));
        }
    }

    fn flush_files(&mut self) {
        let mut failed = Vec::new();
        for (&engine_id, slot) in &mut self.writers {
            if let Some(writer) = slot
                && writer.flush().is_err()
            {
                failed.push(engine_id);
            }
        }
        for engine_id in failed {
            self.fail_writer(engine_id, &io::Error::other("flush failed"));
        }
    }
}

/// Transcoding is a pure encoding change — content identical to the ring
/// record, with the `0 = none` conventions becoming `Option`s (v2 §4).
fn to_disk_event(raw: &RawRecord<'_>) -> pb::DiskEventV1 {
    use pb::disk_event_v1::Event;
    let event = match *raw {
        RawRecord::CallFunction {
            flags: _,
            thread_id,
            call_id,
            parent_call_id,
            function_id,
            ts_ns,
        } => Event::CallFunction(pb::CallFunction {
            thread_id,
            call_id,
            parent_call_id: (parent_call_id != 0).then_some(parent_call_id),
            function_id,
            timestamp_ns: ts_ns,
        }),
        RawRecord::EndFunction {
            status,
            thread_id,
            call_id,
            ts_ns,
        } => Event::EndFunction(pb::EndFunction {
            thread_id,
            call_id,
            status: match status {
                FunctionEndStatus::Ok => pb::FunctionEndStatus::Ok,
                FunctionEndStatus::Error => pb::FunctionEndStatus::Error,
            } as i32,
            timestamp_ns: ts_ns,
        }),
        RawRecord::StartThread {
            flags: _,
            thread_id,
            parent_thread_id,
            parent_call_id,
            ts_ns,
            name,
        } => Event::StartThread(pb::StartThread {
            thread_id,
            parent_thread_id: (parent_thread_id != 0).then_some(parent_thread_id),
            parent_call_id: (parent_call_id != 0).then_some(parent_call_id),
            name: (!name.is_empty()).then(|| String::from_utf8_lossy(name).into_owned()),
            timestamp_ns: ts_ns,
        }),
        RawRecord::EndThread {
            status,
            thread_id,
            ts_ns,
        } => Event::EndThread(pb::EndThread {
            thread_id,
            status: match status {
                ThreadEndStatus::Completed => pb::ThreadEndStatus::Completed,
                ThreadEndStatus::Cancelled => pb::ThreadEndStatus::Cancelled,
                ThreadEndStatus::Errored => pb::ThreadEndStatus::Errored,
            } as i32,
            timestamp_ns: ts_ns,
        }),
        RawRecord::SetFunctionId {
            thread_id,
            call_id,
            id,
            ts_ns,
        } => Event::SetFunctionId(pb::SetFunctionId {
            thread_id,
            call_id,
            id: id.to_vec(),
            timestamp_ns: ts_ns,
        }),
    };
    pb::DiskEventV1 { event: Some(event) }
}

/// The process UUID, minted once (uuid v4).
fn process_id() -> [u8; 16] {
    static ID: OnceLock<[u8; 16]> = OnceLock::new();
    *ID.get_or_init(|| *uuid::Uuid::new_v4().as_bytes())
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
    use crate::prof::{
        FunctionMetaEntry,
        file::{header_started_at_epoch_ns, read_bamlprof},
        record::MAX_RECORD_LEN,
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
        };
        std::thread::Builder::new()
            .name("test-prof-consumer".into())
            .spawn(move || consumer_main(&ctl_rx, &env))
            .unwrap();

        register_engine_metadata(
            ENGINE,
            EngineProfileMetadata {
                program_id: "test-program".into(),
                functions: vec![FunctionMetaEntry {
                    function_id: 7,
                    fqn: "pkg.main".into(),
                    source_file: "main.baml".into(),
                    span_start: 10,
                    span_end: 90,
                    kind: "bytecode".into(),
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
                thread_id: 1,
                parent_thread_id: 0,
                parent_call_id: 0,
                ts_ns: 1,
                name: b"worker",
            });
            for seq in 1..=PAIRS {
                push(RawRecord::CallFunction {
                    flags: 0,
                    thread_id: 1,
                    call_id: seq,
                    parent_call_id: seq - 1,
                    function_id: 7,
                    ts_ns: seq * 2,
                });
            }
            push(RawRecord::SetFunctionId {
                thread_id: 1,
                call_id: PAIRS,
                id: [0xAB; 16],
                ts_ns: PAIRS * 2 + 1,
            });
            for seq in (1..=PAIRS).rev() {
                push(RawRecord::EndFunction {
                    status: FunctionEndStatus::Ok,
                    thread_id: 1,
                    call_id: seq,
                    ts_ns: 10_000 + seq,
                });
            }
            push(RawRecord::EndThread {
                status: ThreadEndStatus::Completed,
                thread_id: 1,
                ts_ns: 99_999,
            });
        })
        .join()
        .unwrap();

        // Flush-with-ack (the same protocol flush_and_join uses).
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        ctl_tx.send(ControlMsg::Flush(ack_tx)).unwrap();
        ctx.wake().force_wake();
        ack_rx
            .recv_timeout(Duration::from_secs(60))
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
        assert_eq!(header.program_id, "test-program");
        assert_eq!(header.process_id.len(), 16);
        assert!(header_started_at_epoch_ns(&header).is_some());
        let table = header.function_table.expect("function table");
        assert_eq!(table.functions.len(), 1);
        assert_eq!(table.functions[0].fqn, "pkg.main");
        assert_eq!(table.functions[0].function_id, 7);

        let got: Vec<Event> = events
            .into_iter()
            .filter_map(|e| e.event)
            .filter(|e| !matches!(e, Event::Heartbeat(_)))
            .collect();
        assert_eq!(got, expected, "event sequence must round-trip exactly");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// `0 = none` ring conventions become absent optionals on disk.
    #[test]
    fn transcode_none_conventions() {
        let raw = RawRecord::StartThread {
            flags: 0,
            thread_id: 9,
            parent_thread_id: 0,
            parent_call_id: 0,
            ts_ns: 5,
            name: b"",
        };
        match to_disk_event(&raw).event.unwrap() {
            Event::StartThread(st) => {
                assert_eq!(st.parent_thread_id, None);
                assert_eq!(st.parent_call_id, None);
                assert_eq!(st.name, None);
            }
            other => panic!("wrong variant {other:?}"),
        }
        let raw = RawRecord::CallFunction {
            flags: 0,
            thread_id: 9,
            call_id: 1,
            parent_call_id: 0,
            function_id: 0,
            ts_ns: 5,
        };
        match to_disk_event(&raw).event.unwrap() {
            Event::CallFunction(cf) => assert_eq!(cf.parent_call_id, None),
            other => panic!("wrong variant {other:?}"),
        }
        let raw = RawRecord::EndFunction {
            status: FunctionEndStatus::Error,
            thread_id: 9,
            call_id: 1,
            ts_ns: 5,
        };
        match to_disk_event(&raw).event.unwrap() {
            Event::EndFunction(ef) => {
                assert_eq!(ef.status, pb::FunctionEndStatus::Error as i32);
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
                thread_id: 1,
                call_id: seq,
                parent_call_id: seq.saturating_sub(1),
                function_id: 1,
                ts_ns: seq,
            }
            .encode(&mut buf);
            // SAFETY: claiming thread, alive throughout.
            unsafe { handle.push(&buf[..len]) };
        }

        let mut state = ConsumerState::new(dir.clone());
        let env = ConsumerEnv {
            registry,
            ctx,
            dir: dir.clone(),
            wake_interval: Duration::from_millis(1),
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
                        thread_id: round + 1,
                        call_id: seq + 1,
                        parent_call_id: seq,
                        function_id: 1,
                        ts_ns: round * per_round + seq,
                    }
                    .encode(&mut buf);
                    // SAFETY: claiming thread, alive for the whole closure.
                    unsafe { h.push(&buf[..len]) };
                }
                h.ring().orphan(); // TLS-destructor stand-in
            })
            .join()
            .unwrap();
            // A dead thread's ring is claimable only after the consumer
            // pools it; give the sweep a beat, like the seconds-long idle
            // gaps of a real blocking pool. (Back-to-back churn without the
            // gap legally allocates more rings — bounded by peak concurrent
            // un-pooled threads, exercised in orphan_churn_reuses_rings.)
            std::thread::sleep(Duration::from_millis(2));
        }

        // Flush through the live consumer and read everything back.
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        ctl_tx.send(ControlMsg::Flush(ack_tx)).unwrap();
        ctx.wake().force_wake();
        ack_rx
            .recv_timeout(Duration::from_secs(60))
            .expect("soak consumer never acked");

        // Sequential churn must reuse one pooled ring, not grow the registry
        // (the consumer pools each orphan between rounds; a bounded handful
        // is tolerated for rounds that overlapped a sweep).
        let mut rings = 0;
        registry.for_each(|_| rings += 1);
        assert!(rings <= 4, "registry grew under churn: {rings} rings");

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
