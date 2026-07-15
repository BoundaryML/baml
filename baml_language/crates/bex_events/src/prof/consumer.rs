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
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{OnceLock, mpsc},
    time::{Duration, Instant},
};

use crate::{
    ids::{EngineId, ProcessEuid},
    prof::{
        clock::{self, TickConverter},
        config::ProfConfig,
        encode::build_header,
        file::ProfileWriter,
        metadata, pb, record,
        registry::Registry,
        ring::{Ring, RingCtx},
        transcode::to_disk_event,
    },
    run::{ProfileEventSource, RuntimeTarget, profile_event_envelope_from_disk_event},
};

/// How often the consumer stamps a process-liveness heartbeat into every
/// open file (MVP: consumer-stamped, v2 §4).
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);

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
    pub(crate) clock: ClockMode,
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

/// The consumer loop: §3.4 sweep + §3.6 wake protocol + control messages.
pub(crate) fn consumer_main(control: &mpsc::Receiver<ControlMsg>, env: &ConsumerEnv) {
    env.ctx.wake().register_consumer();
    let mut state = ConsumerState::new(env.dir.clone(), env.clock.build());
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
                    // All of the engine's events have been delivered; let
                    // observers (run store, history store) release whatever
                    // they buffered for it.
                    crate::run::publish_engine_closed(crate::ids::EngineId(engine_id));
                    crate::history::publish_history_engine_closed(crate::ids::EngineId(engine_id));
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
}

impl ConsumerState {
    fn new(dir: PathBuf, conv: TickConverter) -> ConsumerState {
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
        for rec in record::iter(bytes) {
            match rec {
                Ok(raw) => {
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
        if !self.profile_writes_enabled {
            return None;
        }
        self.writers
            .entry(engine_id)
            .or_insert_with(|| {
                let meta = metadata::get_engine_metadata(engine_id);
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
        // Heartbeat cadence is the designated ≥1 s refinement slot for the
        // x86 TSC rate (no-op for exact-rate sources / once refined).
        self.conv.maybe_refine();
        let ts = self.conv.to_ns(clock::now_ticks());
        let event = pb::DiskEventV1 {
            event: Some(pb::disk_event_v1::Event::Heartbeat(pb::Heartbeat {
                timestamp_ns: ts,
            })),
        };
        let mut failed = Vec::new();
        for (&engine_id, slot) in &mut self.writers {
            if let Some(writer) = slot {
                writer.encode_event(&event);
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
        if let Some(Some(mut writer)) = self.writers.remove(&engine_id)
            && let Err(err) = writer.sync()
        {
            report(format_args!(
                "sync of {} on engine close failed: {err}",
                writer.path().display()
            ));
        }
        let _ = metadata::remove_engine_metadata(engine_id);
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
        assert_eq!(header.program_id, "test-program");
        assert_eq!(header.source_snapshot_id.as_deref(), Some("snapshot-1"));
        assert_eq!(header.revision_id.as_deref(), Some("revision-1"));
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
            clock: ClockMode::Fixed(TickConverter::identity()),
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
