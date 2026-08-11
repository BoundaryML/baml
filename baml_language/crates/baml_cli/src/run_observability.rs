//! Phase H host wiring for `baml run` (observability design §3.2/§6.4):
//! mint a `BoundaryId`, create the boundary dir under
//! `.baml/history/<created_ms>-<target_slug>-<baml_id_1_wire>/`, write the
//! host-side `begin` meta record, enable value capture on the call
//! context, and — after the call resolves — drain captured values into the
//! boundary dir + project store, bind the boundary to the profile
//! consumer, and complete it.
//!
//! Everything in this module is best-effort: an observability failure must
//! never fail the run. Failures are reported through the verbose channel
//! (`baml run -v`), matching `run_command`'s quiet-on-success contract.

use std::path::{Path, PathBuf};

use bex_engine::{
    BexEngine, CallRef, FunctionCallContextBuilder, ProcessEuid,
    value_capture::{CaptureKind, TraceCaptureConfig, TraceCaptureProducer},
};
use bex_events::{
    ids::BoundaryId,
    prof::cct::meta::{MetaRecord, MetaWriter},
    store::{Store, canon, gc},
    value::{
        DagRef, FileValueArtifactSink, LogEventRecord, ValueCapture, ValueCaptureKind, ValueCodec,
        ValueWriter,
    },
};
use sys_native::CallId;

/// How long the host waits on each consumer handshake. The CLI is about to
/// exit — a stuck consumer must never hang the run.
const BIND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
const COMPLETE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const FLUSH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Verbose-gated diagnostics: observability problems are debug evidence,
/// not run output (`baml run` stays quiet on success, and e.g. a consumer
/// running the legacy pipeline makes `bind` fail on every run).
fn obs_debug(args: std::fmt::Arguments<'_>) {
    if crate::reporter::verbose() {
        crate::reporter::print_verbose(format_args!("observability: {args}"));
    }
}

fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// §3.2 gate: history capture is on by default whenever profiling is
/// enabled, and `BAML_HISTORY=0` (or `false`) turns durable capture off
/// wholesale (the named privacy switch).
pub(crate) fn history_enabled() -> bool {
    history_enabled_from(
        bex_events::prof::ProfConfig::global().is_enabled(),
        std::env::var("BAML_HISTORY").ok().as_deref(),
    )
}

/// Injectable core of [`history_enabled`] (testable without touching the
/// process environment).
fn history_enabled_from(profiling_enabled: bool, baml_history: Option<&str>) -> bool {
    if !profiling_enabled {
        return false;
    }
    match baml_history {
        Some(value) => {
            let value = value.trim();
            !(value == "0" || value.eq_ignore_ascii_case("false"))
        }
        None => true,
    }
}

/// Sanitize a run target into a directory-name-safe slug. Mirrors the
/// consumer-side rule in `bex_events::history::path::target_slug`: keep
/// ASCII alphanumerics plus `.`/`_`/`-`, replace everything else with
/// `_`, cap at 80 chars, and never produce an empty slug.
pub(crate) fn target_slug(raw: &str) -> String {
    let mut slug = String::with_capacity(raw.len().min(80));
    for ch in raw.chars().take(80) {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            slug.push(ch);
        } else {
            slug.push('_');
        }
    }
    if slug.is_empty() {
        "boundary".to_string()
    } else {
        slug
    }
}

/// The boundary dir name: `<created_ms>-<target_slug>-<baml_id_1_wire>`
/// (§6.4 layout; same shape `bex_events::history::path::
/// build_boundary_history_path` produces, so every existing reader —
/// `find_boundary_dir`, retention, `obs-bench validate` — lists it).
pub(crate) fn boundary_dir_name(created_ms: u64, target: &str, boundary_id: BoundaryId) -> String {
    format!(
        "{created_ms}-{}-{}",
        target_slug(target),
        boundary_id.to_wire_string()
    )
}

/// One `baml run` boundary: begin → (call runs) → finish.
pub(crate) struct RunBoundary {
    boundary_id: BoundaryId,
    boundary_dir: PathBuf,
    baml_dir: PathBuf,
    created_ms: u64,
    producer: TraceCaptureProducer,
    /// §7.3 continuous off-thread drain (primary path). `None` when the
    /// worker thread could not spawn; finish() then drains inline.
    drain_worker: Option<ValueDrainWorker>,
}

/// Mutable state of one boundary's value drain: lazily created segment
/// writer + store, plus the roots owed to the §6.7 manifest commit.
#[derive(Default)]
struct DrainState {
    writer: Option<ValueWriter<FileValueArtifactSink>>,
    store: Option<Store>,
    /// Store open failed once — don't retry every drain.
    store_failed: bool,
    roots: Vec<[u8; 32]>,
}

/// The continuous drain worker: drains captured drafts off the
/// application threads at a steady cadence, so a long run's values reach
/// the segment + CAS incrementally instead of accumulating in the trace
/// heap until boundary finish. At stop it runs the same §6.7 root-commit
/// barrier the inline path uses.
struct ValueDrainWorker {
    stop: std::sync::mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ValueDrainWorker {
    const DRAIN_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

    fn spawn(
        boundary_id: BoundaryId,
        boundary_dir: PathBuf,
        baml_dir: PathBuf,
        created_ms: u64,
        producer: TraceCaptureProducer,
    ) -> Option<ValueDrainWorker> {
        let (stop, rx) = std::sync::mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("baml-value-drain".to_string())
            .spawn(move || {
                let mut state = DrainState::default();
                loop {
                    // A send OR a dropped sender both mean "finish now";
                    // only the timeout tick keeps looping.
                    let stopping = !matches!(
                        rx.recv_timeout(Self::DRAIN_INTERVAL),
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                    );
                    drain_step(
                        &producer,
                        &mut state,
                        boundary_id,
                        &boundary_dir,
                        &baml_dir,
                        created_ms,
                    );
                    if stopping {
                        break;
                    }
                }
                drain_finish(&producer, state, boundary_id, &boundary_dir);
            })
            .ok()?;
        Some(ValueDrainWorker {
            stop,
            thread: Some(thread),
        })
    }

    /// Stop the worker and wait for its final drain + commit barrier.
    fn finish(mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl RunBoundary {
    /// Host-side `begin` (§6.4): create the boundary dir and write the
    /// `BoundaryBegin` meta record *before* the run. Returns `None` when
    /// history capture is disabled or any filesystem step fails (reported
    /// verbose; the run proceeds unobserved).
    pub(crate) fn begin(project_root: &Path, target: &str, revision_id: &str) -> Option<Self> {
        if !history_enabled() {
            return None;
        }
        let boundary_id = BoundaryId::new_random();
        let created_ms = epoch_ms();
        let baml_dir = project_root.join(".baml");
        // `.baml/` must be self-ignoring: the legacy profile writer used to
        // stamp this, and it died with the legacy pipeline (P9) — without
        // it every run's artifacts land in `git status`.
        if std::fs::create_dir_all(&baml_dir).is_ok() {
            let gitignore = baml_dir.join(".gitignore");
            if !gitignore.exists() {
                let _ = std::fs::write(&gitignore, "*\n");
            }
        }
        let boundary_dir =
            baml_dir
                .join("history")
                .join(boundary_dir_name(created_ms, target, boundary_id));
        if let Err(err) = std::fs::create_dir_all(&boundary_dir) {
            obs_debug(format_args!(
                "cannot create boundary dir {}: {err}",
                boundary_dir.display()
            ));
            return None;
        }
        let begin = MetaRecord::BoundaryBegin {
            boundary_id: boundary_id.to_wire_string(),
            target: target.to_string(),
            source: "cli".to_string(),
            created_ms,
            project_id: project_root.display().to_string(),
            revision_id: revision_id.to_string(),
            capture_defaults: "llm_boundary".to_string(),
        };
        let written =
            MetaWriter::create(boundary_dir.join("boundary.bamlmeta")).and_then(|mut writer| {
                writer.append(&begin)?;
                // §6.6: begin is a D2 milestone — sync the stream now.
                writer.sync_data()
            });
        if let Err(err) = written {
            obs_debug(format_args!(
                "cannot write boundary begin under {}: {err}",
                boundary_dir.display()
            ));
            return None;
        }
        // Same enabled(16) draft budgets the playground run path uses.
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::enabled(16));
        // §7.3 continuous off-thread drain: values leave the trace heap at
        // a steady cadence instead of accumulating until finish. A failed
        // spawn degrades to the inline finish-time drain.
        let drain_worker = ValueDrainWorker::spawn(
            boundary_id,
            boundary_dir.clone(),
            baml_dir.clone(),
            created_ms,
            producer.clone(),
        );
        if drain_worker.is_none() {
            obs_debug(format_args!(
                "value drain worker did not spawn; draining inline at finish"
            ));
        }
        Some(Self {
            boundary_id,
            boundary_dir,
            baml_dir,
            created_ms,
            producer,
            drain_worker,
        })
    }

    /// Wire the boundary + value capture onto a call-context builder
    /// (mirrors the playground's `handle_function_run` wiring; §3.2 CLI
    /// defaults are values-on/logs-off — logs are the playground extra).
    pub(crate) fn context_builder(&self, call_id: CallId) -> FunctionCallContextBuilder {
        FunctionCallContextBuilder::new(call_id)
            .with_boundary_id(self.boundary_id)
            .with_capture_defaults(bex_engine::CaptureDefaults {
                values_enabled: true,
                logs_enabled: false,
            })
            .with_value_capture(self.producer.clone())
    }

    /// Completion barrier (design §11 Phase H: drain → bind → complete →
    /// flush → exit). `status` is `succeeded` / `failed` / `cancelled`;
    /// `entry_call_ref` carries the run's root logical thread when the
    /// call actually started. Never fails the run.
    pub(crate) fn finish(
        mut self,
        engine: &BexEngine,
        entry_call_ref: Option<CallRef>,
        status: &str,
    ) {
        // 1. Flush the profile rings so the consumer has seen the run's
        //    StartThread before we ask it to bind (playground does the
        //    same before trace attachment).
        if !bex_events::prof::flush_and_join(FLUSH_TIMEOUT) {
            obs_debug(format_args!("profile flush before bind timed out"));
        }

        // Root logical thread: decoded straight off the entry call's
        // CallRef (the engine allocates thread 1 to the root call first).
        let root_thread = entry_call_ref.map_or(1, |call_ref| call_ref.thread_id.0);

        // 2. Values: stop the continuous drain worker (final drain + §6.7
        //    commit barrier run on the worker), or drain inline when the
        //    worker never spawned.
        match self.drain_worker.take() {
            Some(worker) => worker.finish(),
            None => {
                let mut state = DrainState::default();
                drain_step(
                    &self.producer,
                    &mut state,
                    self.boundary_id,
                    &self.boundary_dir,
                    &self.baml_dir,
                    self.created_ms,
                );
                drain_finish(&self.producer, state, self.boundary_id, &self.boundary_dir);
            }
        }

        // 3./4. Bind the run's partition, then complete the boundary (the
        //    consumer folds the sealed `cct.bamlcct` snapshot). Recently
        //    closed engines are retained consumer-side, so binding after
        //    the call completed is valid.
        //
        //    Only when the consumer actually runs the CCT pipeline: under
        //    the (current-default) legacy pipeline there is no CCT state
        //    to bind, and asking anyway makes the consumer print a
        //    diagnostic to stderr — breaking `baml run`'s "stderr is the
        //    program's own output" contract. The boundary stays
        //    begin-only + values, exactly what legacy can support.
        if !bex_events::prof::ProfConfig::global().pipeline.runs_cct() {
            obs_debug(format_args!(
                "CCT pipeline off (BAML_PROFILE_PIPELINE=legacy); boundary {} \
                 stays begin-only",
                self.boundary_id.to_wire_string()
            ));
            return;
        }
        let bound = bex_events::prof::bind_boundary(
            engine.engine_id().0,
            self.boundary_id.as_bytes(),
            root_thread,
            &self.boundary_dir,
            BIND_TIMEOUT,
        );
        if !bound {
            obs_debug(format_args!(
                "bind_boundary declined (legacy pipeline or no CCT state); \
                 boundary {} stays begin-only",
                self.boundary_id.to_wire_string()
            ));
            return;
        }
        if !bex_events::prof::complete_boundary(
            self.boundary_id.as_bytes(),
            status,
            COMPLETE_TIMEOUT,
        ) {
            obs_debug(format_args!(
                "complete_boundary declined for {}",
                self.boundary_id.to_wire_string()
            ));
        }
        // Final durability flush before process exit.
        if !bex_events::prof::flush_and_join(FLUSH_TIMEOUT) {
            obs_debug(format_args!("final profile flush timed out"));
        }
    }
}

/// One drain pass (§7.4): every pending draft goes to the boundary's
/// `.bamlvalue` segment (created lazily under the first draft's thread
/// dir — the history readers scan every thread dir) and, canonically
/// encoded, into the project store `<baml>/store` (dedup by CID). DagRef
/// roots accumulate for the §6.7 manifest commit at finish.
fn drain_step(
    producer: &TraceCaptureProducer,
    state: &mut DrainState,
    boundary_id: BoundaryId,
    boundary_dir: &Path,
    baml_dir: &Path,
    created_ms: u64,
) {
    let DrainState {
        writer,
        store,
        store_failed,
        roots,
    } = state;
    let report = producer.drain_to_value_recorder_report(|draft, body, canonical| {
        // Lazy writer: no captures → no segment file at all.
        if writer.is_none() {
            let thread = draft.call.thread_id.0;
            let value_path = boundary_dir
                .join(format!("thread-{thread}"))
                .join("value-0.bamlvalue");
            let sink = FileValueArtifactSink::create(&value_path)?;
            *writer = Some(ValueWriter::new(sink, boundary_id)?);
        }
        let writer = writer.as_mut().expect("created above");
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
            if store.is_none() && !*store_failed {
                // The store euid is the process euid — the same identity
                // the profile consumer stamps into session dirs and pack
                // headers.
                match Store::open(&baml_dir.join("store"), ProcessEuid::current().0) {
                    Ok(opened) => *store = Some(opened),
                    Err(err) => {
                        obs_debug(format_args!("cannot open value store: {err}"));
                        *store_failed = true;
                    }
                }
            }
            // A failed store write degrades to the legacy record shape
            // (no DagRef); the inline body stays authoritative.
            let dag_ref = store.as_mut().and_then(|store| {
                store
                    .put_encoded(canonical, created_ms)
                    .ok()
                    .map(|_| DagRef {
                        root_cid: canonical.root_cid,
                        node_codec_version: canon::NODE_CODEC_VERSION,
                        logical_len: canonical.logical_len,
                    })
            });
            if let Some(dag_ref) = &dag_ref {
                roots.push(dag_ref.root_cid);
            }
            writer.append_body_with_capture_and_dag(
                ValueCodec::BamlOutboundValue,
                body,
                Some(ValueCapture {
                    kind: value_capture_kind(draft.kind),
                    call: draft.call,
                    function_id: draft.function_id,
                }),
                dag_ref,
            )
        }
    });
    for failure in &report.failures {
        obs_debug(format_args!(
            "value capture drain failed ({:?}): {}",
            failure.kind, failure.diagnostic
        ));
    }
}

/// Final drain + declared loss + the §6.7 root-commit barrier: segment
/// sync BEFORE store sync BEFORE the durable manifest append (a pinned
/// root must never outlive its capture evidence or its pack bytes), then
/// seal the active pack so this short-lived process leaves an idx behind.
fn drain_finish(
    producer: &TraceCaptureProducer,
    mut state: DrainState,
    boundary_id: BoundaryId,
    boundary_dir: &Path,
) {
    // Declared loss instead of silent absence (§7.3). Skips force the
    // segment into existence even when nothing else was captured.
    let stats = producer.stats();
    if stats.skipped_queue_full > 0 {
        if state.writer.is_none() {
            let value_path = boundary_dir.join("thread-1").join("value-0.bamlvalue");
            state.writer = FileValueArtifactSink::create(&value_path)
                .and_then(|sink| ValueWriter::new(sink, boundary_id))
                .map_err(|err| {
                    obs_debug(format_args!("cannot create value segment: {err}"));
                })
                .ok();
        }
        if let Some(writer) = state.writer.as_mut() {
            let loss = bex_events::value::CaptureLossRecord {
                kind: bex_events::value::CaptureLossKind::Value,
                reason: bex_events::value::CaptureLossReason::QueueFull,
                skipped_count: stats.skipped_queue_full,
                call: None,
                message: None,
                timestamp_ms: epoch_ms(),
            };
            if let Err(err) = writer.append_capture_loss(&loss) {
                obs_debug(format_args!("cannot append capture-loss record: {err}"));
            }
        }
    }
    if let Some(writer) = state.writer.as_mut() {
        if let Err(err) = writer.flush() {
            obs_debug(format_args!("value segment flush failed: {err}"));
        }
        // The capture evidence itself must be durable before any root
        // that points into it is pinned.
        if let Err(err) = writer.sync_data() {
            obs_debug(format_args!("value segment sync failed: {err}"));
        }
    }
    if let Some(store) = state.store.as_mut() {
        if let Err(err) = store.sync_data() {
            obs_debug(format_args!("value store sync failed: {err}"));
        } else if !state.roots.is_empty()
            && let Err(err) = gc::append_manifest(boundary_dir, &state.roots)
        {
            obs_debug(format_args!("manifest append failed: {err}"));
        }
        if let Err(err) = store.seal_active() {
            obs_debug(format_args!("value store seal failed: {err}"));
        }
    }
}

/// `bex_engine::value_capture::CaptureKind` → the `.bamlvalue` record
/// kind. (The engine's own mapping is private to its drain helper.)
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── the §3.2 gate ─────────────────────────────────────────────────

    #[test]
    fn history_defaults_on_when_profiling_is_enabled() {
        assert!(history_enabled_from(true, None));
    }

    #[test]
    fn history_off_when_profiling_is_disabled() {
        assert!(!history_enabled_from(false, None));
        assert!(!history_enabled_from(false, Some("1")));
    }

    #[test]
    fn baml_history_zero_and_false_disable_capture() {
        assert!(!history_enabled_from(true, Some("0")));
        assert!(!history_enabled_from(true, Some(" 0 ")));
        assert!(!history_enabled_from(true, Some("false")));
        assert!(!history_enabled_from(true, Some("FALSE")));
    }

    #[test]
    fn baml_history_other_values_keep_capture_on() {
        assert!(history_enabled_from(true, Some("1")));
        assert!(history_enabled_from(true, Some("")));
        assert!(history_enabled_from(true, Some("yes")));
    }

    // ── slug + dir naming (§6.4 layout) ───────────────────────────────

    #[test]
    fn target_slug_keeps_safe_chars_and_replaces_the_rest() {
        assert_eq!(target_slug("scripts.Backfill"), "scripts.Backfill");
        assert_eq!(target_slug("llm.Summarize-v2_x"), "llm.Summarize-v2_x");
        assert_eq!(target_slug("weird name/⚡"), "weird_name__");
    }

    #[test]
    fn target_slug_caps_at_80_chars_and_never_empties() {
        let long = "f".repeat(200);
        assert_eq!(target_slug(&long).len(), 80);
        assert_eq!(target_slug(""), "boundary");
        assert_eq!(target_slug("⚡"), "_");
    }

    #[test]
    fn boundary_dir_name_matches_the_reader_contract() {
        // Same triple shape build_boundary_history_path emits:
        // `<created_ms>-<slug>-<baml_id_1_wire>` — find_boundary_dir
        // matches on the wire-string suffix.
        let id = BoundaryId::from_bytes([11; 16]);
        let name = boundary_dir_name(1234, "main", id);
        assert_eq!(name, format!("1234-main-{}", id.to_wire_string()));
        assert!(name.ends_with(&id.to_wire_string()));
        // The wire form itself round-trips out of the dir name.
        let suffix = name.rsplit('-').next().unwrap();
        // (wire strings contain no '-'? base64url may contain '-';
        // readers match by suffix, not by split — assert that instead.)
        let _ = suffix;
        assert!(BoundaryId::from_wire_str(name.splitn(3, '-').nth(2).unwrap()).is_some());
    }

    #[test]
    fn boundary_dir_name_slugs_hostile_targets() {
        let id = BoundaryId::from_bytes([7; 16]);
        let name = boundary_dir_name(9, "a/b c", id);
        assert!(name.starts_with("9-a_b_c-baml_id_1_"), "{name}");
        assert!(!name.contains('/'));
        assert!(!name.contains(' '));
    }
}
