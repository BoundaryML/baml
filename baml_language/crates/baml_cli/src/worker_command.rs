//! `baml-cli worker`: one OS process that hosts one segment of a durable run.
//!
//! This is the real worker of the durable functions proof of concept. The
//! protocol is specified in `documents/durable-poc-contracts.md`, section 2:
//!
//! - stdout carries exactly one JSON event per line and nothing else,
//! - stdin carries one JSON command per line,
//! - exit codes are `0` completed, `1` failed, `75` paused, `130` cancelled.
//!
//! The worker compiles the project, installs a log-capturing `baml.io`
//! provider and a [`bex_engine::durable::DurableHost`], and runs one function
//! (`--start`) or continues one from a snapshot (`--resume`).
//!
//! `pause` writes `snap-<n>.bamlsnap` and the state dump `snap-<n>.json` into
//! `--snapshot-dir` at the next clean point, emits `paused`, and exits 75. A
//! run whose function name contains `durable` also takes automatic snapshots
//! (event `snapshot`) and keeps running.
//!
//! A durable run also suspends itself for a long `sleep` (contract section
//! 9.2): when every thread of the run waits in an operation that a resumed
//! process can finish, and the earliest sleep deadline is at least
//! `--sleep-suspend-ms` away, the worker writes a snapshot exactly as for
//! `pause`, emits `paused` with `wake`, and exits 75.
//!
//! The opaque run state inside a snapshot is a JSON object: the function, the
//! durable flag, the original JSON arguments, the `call_id` counter, the
//! segment, the remote calls the run waits on, and remote results that
//! arrived but were not consumed yet.

use std::{
    cell::Cell,
    collections::{BTreeMap, HashMap, HashSet},
    io::{IsTerminal as _, Write as _},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, PoisonError, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, anyhow};
use bex_engine::{
    BexEngine, BexExternalValue, CallId, CancellationToken, EngineError,
    FunctionCallContextBuilder, RuntimeTy,
    durable::{
        DurableHost, DurableThreadId, ParkedKind, PauseProgress, RecordedResult, RemoteCallRequest,
        RemoteCancel, RemoteResultWait, RestoredInfo, ResumeOptions, SnapshotFailure, SnapshotMode,
        SnapshotParts, SnapshotReport, SnapshotRequest, YieldPosition, YieldReason,
    },
};
use clap::Args;
use futures::future::BoxFuture;
use serde_json::{Value as Json, json};
use sys_native::SysOpsExt as _;
use sys_types::{SysOpContext, SysOpOutput, VmBamlError};
use tokio::io::AsyncBufReadExt as _;

/// Where the program comes from: the program store or a compile of
/// `--project` (contract section 9.5).
#[path = "worker_program.rs"]
mod worker_program;

const EXIT_COMPLETED: i32 = 0;
const EXIT_FAILED: i32 = 1;
const EXIT_PAUSED: i32 = 75;
const EXIT_CANCELLED: i32 = 130;

/// Default optimization level of the worker's compile. Level 0 keeps every
/// source-level local in its own slot and keeps exact line tables, which is
/// what the state dump and `position` events show. A snapshot only restores
/// into the identical program, so the level is part of the run state and
/// `--resume` compiles at the level the run started with.
const DEFAULT_OPT_LEVEL: u8 = 0;

fn opt_level(level: u8) -> baml_db::baml_compiler2_emit::OptLevel {
    use baml_db::baml_compiler2_emit::OptLevel;
    match level {
        0 => OptLevel::Zero,
        1 => OptLevel::One,
        _ => OptLevel::Two,
    }
}

/// An automatic snapshot gives up when the run does not reach a yield within
/// this time (an operation that cannot be re-issued is in flight).
const AUTO_SNAPSHOT_PARK_TIMEOUT: Duration = Duration::from_millis(250);

/// How long the worker waits, after the root function returns, for spawned
/// threads to report their end before it emits the terminal event.
const THREAD_DRAIN_GRACE: Duration = Duration::from_secs(2);

/// How long a cancelled run gets to end by itself. After that the worker
/// reports `cancelled` and exits anyway (a run inside a native operation that
/// never yields would otherwise keep the process alive).
const CANCEL_GRACE: Duration = Duration::from_secs(2);

/// Time between two attempts of a pause whose snapshot could not be written
/// or stored.
const PAUSE_RETRY_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Args, Debug)]
pub struct WorkerArgs {
    /// BAML project directory. Filled from the global `--project` option.
    #[arg(skip)]
    pub from: Option<PathBuf>,

    /// Run id (`[a-z0-9-]+`).
    #[arg(long, value_name = "RUN_ID")]
    pub run: String,

    /// Segment number of this worker process in the life of the run.
    #[arg(long, value_name = "N", default_value_t = 1)]
    pub segment: u64,

    /// Directory that receives snapshots of this run.
    #[arg(long = "snapshot-dir", value_name = "DIR")]
    pub snapshot_dir: Option<PathBuf>,

    /// Function to start.
    #[arg(
        long,
        value_name = "FUNCTION",
        conflicts_with = "resume",
        required_unless_present = "resume"
    )]
    pub start: Option<String>,

    /// Arguments as a JSON object keyed by parameter name.
    #[arg(long = "json-args", value_name = "JSON", requires = "start")]
    pub json_args: Option<String>,

    /// Snapshot to resume from.
    #[arg(long, value_name = "SNAPSHOT_PATH")]
    pub resume: Option<PathBuf>,

    /// Result of a remote call that completed while the run had no process:
    /// `{"call_id": "...", "value": <json>}` or `{"call_id": "...", "error": "..."}`.
    #[arg(long = "remote-result", value_name = "JSON")]
    pub remote_result: Vec<String>,

    /// Minimum time between automatic snapshots of a durable run, in
    /// milliseconds. `0` disables them. Runs whose function name does not
    /// contain `durable` never take automatic snapshots.
    #[arg(long = "auto-snapshot-ms", value_name = "MS", default_value_t = 2000)]
    pub auto_snapshot_ms: u64,

    /// A durable run suspends itself (snapshot, `paused` with `wake`, exit 75)
    /// when all of its threads wait and its earliest `sleep` deadline is at
    /// least this many milliseconds away. `0` disables. Runs whose function
    /// name does not contain `durable` never suspend themselves.
    #[arg(long = "sleep-suspend-ms", value_name = "MS", default_value_t = 5000)]
    pub sleep_suspend_ms: u64,

    /// Optimization level of the compile (0, 1 or 2). Only used with
    /// `--start`; a resumed run compiles at the level recorded in its snapshot.
    #[arg(long = "opt-level", value_name = "LEVEL", default_value_t = DEFAULT_OPT_LEVEL, hide = true)]
    pub opt_level: u8,

    /// Content-addressed program store (contract section 9.5). A `--start`
    /// from `--project` stores the compiled program there. A `--start` with
    /// `--program-hash` and every `--resume` load the program from it.
    #[arg(long = "program-store", value_name = "DIR")]
    pub program_store: Option<PathBuf>,

    /// With `--start`: run the program with this hash from the program store
    /// instead of compiling `--project`, which is then optional. A `--resume`
    /// takes the hash from the snapshot header.
    #[arg(
        long = "program-hash",
        value_name = "HEX",
        requires = "program_store",
        conflicts_with = "resume"
    )]
    pub program_hash: Option<String>,

    /// Keep running when stdin reaches end of file. Without this flag a closed
    /// stdin that is not a terminal means the supervisor is gone, and the
    /// worker cancels the run (exit 130).
    #[arg(long = "ignore-stdin-eof")]
    pub ignore_stdin_eof: bool,
}

impl WorkerArgs {
    /// Runs the worker and ends the process with a contract exit code. It
    /// returns only if the tokio runtime cannot be created.
    pub fn run(&self) -> Result<crate::ExitCode> {
        let events = EventSink::start(self.run.clone(), self.segment);
        let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
        let code = rt.block_on(self.run_async(&events));
        events.shutdown();
        // Streams spec §7.5: `main` flushes the profiler on its own exit path;
        // this path leaves through `process::exit`, so flush here.
        bex_events::prof::flush_and_join(Duration::from_secs(5));
        std::process::exit(code);
    }

    async fn run_async(&self, events: &EventSink) -> i32 {
        let started = Instant::now();
        let outcome = match self.resume.as_deref() {
            Some(snapshot) => self.resume_run(snapshot, events, started).await,
            None => {
                let function = self
                    .start
                    .clone()
                    .expect("clap requires --start when --resume is absent");
                // `hello` carries the program hash, so `start_run` emits it
                // once the program is loaded.
                self.start_run(&function, events).await
            }
        };
        match outcome {
            Ok(code) => code,
            Err(error) => {
                events.emit_terminal(
                    "failed",
                    json!({ "error": format!("{error:#}"), "stack": [] }),
                    Vec::new,
                );
                EXIT_FAILED
            }
        }
    }

    fn project_dir(&self) -> PathBuf {
        self.from.clone().unwrap_or_else(|| PathBuf::from("."))
    }

    fn snapshot_dir(&self) -> PathBuf {
        self.snapshot_dir.clone().unwrap_or_else(|| {
            std::env::temp_dir()
                .join("baml-worker-snapshots")
                .join(&self.run)
        })
    }

    fn new_state(&self, events: &EventSink, run_info: RunInfo) -> Arc<WorkerState> {
        let project_dir = self.project_dir();
        let project_root = project_dir.canonicalize().unwrap_or(project_dir);
        let state = Arc::new(WorkerState::new(
            events.clone(),
            self.run.clone(),
            self.segment,
            project_root,
            self.snapshot_dir(),
            run_info,
        ));
        let _ = state.this.set(Arc::downgrade(&state));
        state
    }

    async fn start_run(&self, function: &str, events: &EventSink) -> Result<i32> {
        let json_args: Json = match self.json_args.as_deref() {
            Some(text) => match serde_json::from_str(text) {
                Ok(args) => args,
                Err(error) => {
                    // `hello` is the first event of every worker, also of one
                    // that cannot start (contract section 9.5).
                    events.emit(
                        "hello",
                        json!({
                            "mode": "start",
                            "function": function,
                            "durable": function.contains("durable"),
                            "program_hash": null,
                        }),
                    );
                    return Err(anyhow!(error).context("--json-args is not valid JSON"));
                }
            },
            None => json!({}),
        };

        let state = self.new_state(
            events,
            RunInfo {
                function: function.to_string(),
                durable: function.contains("durable"),
                json_args: json_args.clone(),
                opt_level: self.opt_level.min(2),
            },
        );
        let engine = Arc::new(self.load_engine_for_start(&state)?.engine);
        state.set_engine(&engine);

        let info = engine
            .find_user_function(function)
            .ok_or_else(|| anyhow!("function `{function}` not found in the project"))?;
        // Argument decoding runs as helper engine calls, before the host is
        // installed, so they produce no thread or position events.
        let args = baml_exec::dispatch::build_args_from_signature(
            &engine,
            HashMap::new(),
            Some(&json_args),
            &info.param_names,
            &info.param_types,
            &info.param_has_default,
        )
        .await?;

        engine.set_durable_host(Some(Arc::clone(&state) as Arc<dyn DurableHost>));
        let cancel = CancellationToken::new();
        let background = self.start_background(&state, &engine, &cancel);

        let call_context = FunctionCallContextBuilder::new(CallId::next())
            .with_cancel_token(cancel.clone())
            .build();
        let outcome = engine
            .call_function_bound_args(&info.qualified_name, args, call_context, true)
            .await;
        Ok(self
            .finish_run(
                &state,
                &engine,
                &cancel,
                background,
                outcome,
                &info.return_type,
            )
            .await)
    }

    /// Continue a run from `snapshot_path` (contract section 2.1, `--resume`).
    async fn resume_run(
        &self,
        snapshot_path: &Path,
        events: &EventSink,
        started: Instant,
    ) -> Result<i32> {
        // Everything `hello` needs is readable without the program.
        let read = || -> Result<(Vec<u8>, bex_engine::durable::SnapshotHeader, SavedRunState)> {
            let bytes = std::fs::read(snapshot_path)
                .with_context(|| format!("cannot read snapshot {}", snapshot_path.display()))?;
            let header = bex_engine::durable::read_header(&bytes)
                .map_err(|e| anyhow!("cannot resume from {}: {e}", snapshot_path.display()))?;
            let run_state = bex_engine::durable::read_run_state(&bytes)
                .map_err(|e| anyhow!("cannot resume from {}: {e}", snapshot_path.display()))?;
            let saved: SavedRunState = serde_json::from_slice(&run_state)
                .context("the snapshot's run state is not a worker run state")?;
            Ok((bytes, header, saved))
        };
        let (bytes, header, saved) = match read() {
            Ok(parts) => parts,
            Err(error) => {
                events.emit(
                    "hello",
                    json!({ "mode": "resume", "function": "", "durable": false, "program_hash": null }),
                );
                return Err(error);
            }
        };
        events.emit(
            "hello",
            json!({
                "mode": "resume",
                "function": saved.function,
                "durable": saved.durable,
                "program_hash": worker_program::hex(&header.program_hash),
                "runtime_build": bex_engine::durable::runtime_build(),
            }),
        );
        let this_build = bex_engine::durable::runtime_build();
        if header.runtime_build != this_build {
            anyhow::bail!(
                "cannot resume: the snapshot was written by runtime build `{}`, this worker is `{this_build}`",
                header.runtime_build
            );
        }

        let state = self.new_state(
            events,
            RunInfo {
                function: saved.function.clone(),
                durable: saved.durable,
                json_args: saved.json_args.clone(),
                opt_level: saved.opt_level,
            },
        );
        let load_started = Instant::now();
        let loaded = self.load_engine(&state, Some(header.program_hash))?;
        let program_stats = loaded.stats();
        let engine = Arc::new(loaded.engine);
        state.set_engine(&engine);
        let program_load_ms = ms_since(load_started);
        if header.program_hash != state.program().hash {
            anyhow::bail!(
                "cannot resume: the project at {} is not the program the snapshot was taken from \
                 (program hash differs)",
                self.project_dir().display()
            );
        }
        let info = engine
            .find_user_function(&saved.function)
            .ok_or_else(|| anyhow!("function `{}` not found in the project", saved.function))?;

        // Results first, so that a thread parked on a remote call finds its
        // result the moment it resumes. A result for a call the run does not
        // wait on is dropped.
        state.restore_remote_calls(&saved);
        state.restore_partial_logs(&saved);
        for text in &self.remote_result {
            match serde_json::from_str::<Json>(text) {
                Ok(result) => state.accept_remote_result(&result),
                Err(error) => {
                    diagnostic(format_args!("ignoring malformed --remote-result: {error}"))
                }
            }
        }

        engine.set_durable_host(Some(Arc::clone(&state) as Arc<dyn DurableHost>));
        let cancel = CancellationToken::new();
        let background = self.start_background(&state, &engine, &cancel);

        // Restored frames keep the call ids of the old process, so the
        // profiler would see calls end that never began here.
        let call_context = FunctionCallContextBuilder::new(CallId::next())
            .with_cancel_token(cancel.clone())
            .suppress_internal_profile()
            .build();
        // The results the run takes in this process arrived at different
        // times while it had no process. The engine replays them in that
        // order, merged with the sleeps whose deadline has passed.
        let options = ResumeOptions {
            recorded_results: state.recorded_results(),
            ..ResumeOptions::default()
        };
        let outcome = engine
            .durable_resume_with(
                &info.qualified_name,
                &bytes,
                state.program().hash,
                call_context,
                true,
                options,
                |restored: &RestoredInfo| {
                    events.emit(
                        "resumed",
                        program_stats.added_to(json!({
                            "process_start_ms": null,
                            "program_load_ms": program_load_ms,
                            "decode_ms": restored.decode_ms,
                            "first_exec_ms": ms_since(started),
                        })),
                    );
                    // A restored thread that waits for a remote result does
                    // not announce the call again. The supervisor learns here
                    // what this process waits on, so that it can answer from a
                    // stored result, attach the run to the child that still
                    // runs, or dispatch the call when neither exists (a fork
                    // taken before the dispatch had returned, for example).
                    for thread in &restored.thread_infos {
                        if let ParkedKind::RemoteCall {
                            call_id, function, ..
                        } = &thread.parked
                        {
                            if thread.cancelled {
                                continue;
                            }
                            events.emit(
                                "remote_wait",
                                json!({
                                    "call_id": call_id,
                                    "thread": thread.thread,
                                    "function": WorkerState::display_function(function),
                                    "has_result": state.has_result(call_id),
                                }),
                            );
                        }
                    }
                },
            )
            .await;
        drop(bytes);
        Ok(self
            .finish_run(
                &state,
                &engine,
                &cancel,
                background,
                outcome,
                &info.return_type,
            )
            .await)
    }

    /// Start the stdin reader and, for a durable run, the automatic snapshots.
    fn start_background(
        &self,
        state: &Arc<WorkerState>,
        engine: &Arc<BexEngine>,
        cancel: &CancellationToken,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        let mut tasks = vec![tokio::spawn(read_commands(
            Arc::clone(state),
            Arc::clone(engine),
            cancel.clone(),
            self.ignore_stdin_eof,
        ))];
        if state.run_info.durable && self.auto_snapshot_ms > 0 {
            tasks.push(tokio::spawn(automatic_snapshots(
                Arc::clone(state),
                Arc::clone(engine),
                Duration::from_millis(self.auto_snapshot_ms),
            )));
        }
        if state.run_info.durable && self.sleep_suspend_ms > 0 {
            let task = tokio::spawn(suspend_for_sleeps(
                Arc::clone(state),
                Arc::clone(engine),
                self.sleep_suspend_ms,
            ));
            *lock(&state.sleep_task) = Some(task);
        }
        tasks
    }

    /// Report how the run ended and pick the exit code.
    async fn finish_run(
        &self,
        state: &Arc<WorkerState>,
        engine: &Arc<BexEngine>,
        cancel: &CancellationToken,
        background: Vec<tokio::task::JoinHandle<()>>,
        outcome: Result<BexExternalValue, EngineError>,
        return_type: &RuntimeTy,
    ) -> i32 {
        state.finished.store(true, Ordering::Release);
        state.run_ended.cancel();
        let sleep_task = lock(&state.sleep_task).take();
        let paused = if matches!(outcome, Err(EngineError::DurableSuspended)) {
            // The task that suspended the run reports what it wrote: the task
            // of a `pause` command, or the self-suspend task. The other one
            // sees that the run has ended and resolves to `None`.
            let pause_task = lock(&state.pause_task).take();
            let requested = match pause_task {
                Some(task) => task.await.ok().flatten(),
                None => None,
            };
            match (requested, sleep_task) {
                (Some(paused), _) => Some(paused),
                (None, Some(task)) => task.await.ok().flatten(),
                (None, None) => None,
            }
        } else {
            if let Some(task) = sleep_task {
                task.abort();
            }
            state.wait_for_threads(THREAD_DRAIN_GRACE).await;
            None
        };
        for task in background {
            task.abort();
        }
        engine.set_durable_host(None);
        // Output without a newline belongs to the run state of a paused run:
        // the resumed segment completes the line. Every other ending reports
        // what is left.
        if !matches!(outcome, Err(EngineError::DurableSuspended)) {
            state.flush_partial_logs();
        }

        let (event_type, fields, code) = match outcome {
            Err(EngineError::DurableSuspended) => match paused {
                Some(paused) => ("paused", paused, EXIT_PAUSED),
                None => (
                    "failed",
                    json!({ "error": "the run was suspended but no snapshot was reported", "stack": [] }),
                    EXIT_FAILED,
                ),
            },
            Ok(value) => match serialize_json(engine, value, return_type).await {
                Ok(value) => ("completed", json!({ "value": value }), EXIT_COMPLETED),
                Err(error) => (
                    "failed",
                    json!({ "error": format!("{error:#}"), "stack": [] }),
                    EXIT_FAILED,
                ),
            },
            // Not part of the contract's event table; readers ignore unknown
            // event types. The exit code carries the outcome.
            Err(_) if cancel.is_cancelled() => ("cancelled", json!({}), EXIT_CANCELLED),
            Err(EngineError::Exit { code: 0 }) => {
                ("completed", json!({ "value": null }), EXIT_COMPLETED)
            }
            Err(error) => {
                let (message, stack) = state.describe_engine_error(&error);
                (
                    "failed",
                    json!({ "error": message, "stack": stack }),
                    EXIT_FAILED,
                )
            }
        };
        // Threads that outlive the root function (a `spawn` nobody awaited)
        // end with the process, and so do the threads of a paused run. They
        // are reported as ended so that every `thread_started` has a
        // `thread_ended`, and nothing they do after this point reaches
        // stdout: the terminal event is the last line.
        events_terminal(state, event_type, fields);
        code
    }
}

fn events_terminal(state: &Arc<WorkerState>, event_type: &str, fields: Json) {
    state.events.emit_terminal(event_type, fields, || {
        // A run that ends (completed, failed or cancelled) while a thread
        // still waits for a remote result abandons that call: nobody will
        // take the result. A paused run keeps its waits in the snapshot.
        let abandoned = if event_type == "paused" {
            Vec::new()
        } else {
            state.take_waiting_calls()
        };
        abandoned
            .into_iter()
            .map(|(call_id, thread, site)| {
                // The engine classified nothing here: the run ended and
                // nobody will take the result. `unknown` is the honest cause.
                let (file, line) = CallSite::fields(site.as_ref());
                (
                    "remote_cancel",
                    json!({
                        "call_id": call_id,
                        "thread": thread,
                        "file": file,
                        "line": line,
                        "cause": "unknown",
                    }),
                )
            })
            .chain(
                state
                    .take_live_threads()
                    .into_iter()
                    .map(|thread| ("thread_ended", json!({ "thread": thread }))),
            )
            .collect()
    });
}

fn ms_since(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// What the worker runs. Part of the snapshot's run state.
#[derive(Clone, Debug)]
struct RunInfo {
    /// The function as given to `--start`.
    function: String,
    durable: bool,
    json_args: Json,
    /// Optimization level the project is compiled at (0, 1 or 2).
    opt_level: u8,
}

/// The JSON object stored as the snapshot's opaque run state.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SavedRunState {
    v: u32,
    function: String,
    durable: bool,
    json_args: Json,
    /// Optimization level the program was compiled at. The resuming worker
    /// uses the same level, or the program hash would differ.
    #[serde(default)]
    opt_level: u8,
    /// Unused since call ids are derived from the calling thread's path and
    /// its own call count, which the engine keeps in the snapshot per thread.
    #[serde(default)]
    call_counter: u64,
    /// Segment that wrote the snapshot.
    segment: u64,
    /// Remote calls that were announced and whose result no thread has taken.
    waiting: Vec<String>,
    /// Results that arrived for calls in `waiting`: `{"value": ..., "ts": n}`
    /// or `{"error": "...", "ts": n}` by call id. `ts` is the time the result
    /// arrived at the supervisor, or at this worker when the supervisor did
    /// not say.
    results: BTreeMap<String, Json>,
    /// Program output that did not end with a newline yet. It has not been
    /// reported as a `log` event; the resumed segment completes the line.
    #[serde(default)]
    partial_stdout: String,
    #[serde(default)]
    partial_stderr: String,
}

/// The compiled program's identity, for snapshot headers and `PauseStats`.
#[derive(Clone, Copy, Debug)]
struct ProgramIdentity {
    hash: [u8; 32],
    bytes: u64,
}

/// Build an engine for `program` whose `baml.io` output is captured as `log`
/// events. [`worker_program`] obtains the program.
fn build_engine(program: bex_vm_types::Program, state: &Arc<WorkerState>) -> Result<BexEngine> {
    // Always start from the native platform: a run is an ordinary program run
    // and needs every operation. Only `baml.io` is replaced.
    let sys_ops = sys_ops::SysOpsBuilder::from_ops(sys_native::SysOps::native())
        .with_io_instance(Arc::new(WorkerIo(Arc::clone(state))))
        .build();
    let argv = vec![
        std::env::current_exe()
            .ok()
            .map_or_else(|| "baml".to_string(), |p| p.to_string_lossy().into_owned()),
    ];
    BexEngine::new_with_runtime_compiler(
        program,
        Arc::new(sys_ops),
        argv,
        bex_project::runtime_compiler(),
    )
    .map_err(|e| anyhow!("failed to create engine: {e:?}"))
}

/// Serialize a value with the stdlib's `baml.json.serialize<T>`, which honors
/// user `baml.ToJson` overrides (the body of `baml_exec`'s output path, minus
/// the print).
async fn serialize_json(
    engine: &Arc<BexEngine>,
    value: BexExternalValue,
    ty: &RuntimeTy,
) -> Result<Json> {
    let result = engine
        .call_function("baml.json.serialize", vec![value], helper_context(ty), true)
        .await
        .map_err(|e| anyhow!("baml.json.serialize failed: {e}"))?;
    match result {
        BexExternalValue::String(text) => {
            serde_json::from_str(&text).context("baml.json.serialize returned invalid JSON")
        }
        other => Err(anyhow!(
            "baml.json.serialize returned a non-string value: {other:?}"
        )),
    }
}

/// Decode JSON into a value of type `ty` with `baml.json.deserialize<T>`.
async fn deserialize_json(
    engine: &Arc<BexEngine>,
    value: &Json,
    ty: &RuntimeTy,
) -> Result<BexExternalValue> {
    engine
        .call_function(
            "baml.json.deserialize",
            vec![BexExternalValue::String(value.to_string().into())],
            helper_context(ty),
            true,
        )
        .await
        .map_err(|e| anyhow!("baml.json.deserialize failed: {e}"))
}

fn helper_context(ty: &RuntimeTy) -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(CallId::next())
        .with_type_args(indexmap::IndexMap::from([("T".to_string(), ty.clone())]))
        .suppress_internal_profile()
        .build()
}

// ============================================================================
// Event output
// ============================================================================

enum SinkMessage {
    Line(String),
    Shutdown(mpsc::Sender<()>),
}

/// The single writer of stdout. Every event is rendered to one line and sent
/// to one thread that writes and flushes it, so lines never interleave.
#[derive(Clone)]
struct EventSink {
    tx: mpsc::Sender<SinkMessage>,
    /// Set by [`Self::emit_terminal`]. Held while a line is queued, so no
    /// event can follow the terminal event.
    closed: Arc<Mutex<bool>>,
    run: Arc<str>,
    segment: u64,
    pid: u32,
}

impl EventSink {
    fn start(run: String, segment: u64) -> Self {
        let (tx, rx) = mpsc::channel::<SinkMessage>();
        std::thread::Builder::new()
            .name("worker-stdout".to_string())
            .spawn(move || {
                let stdout = std::io::stdout();
                for message in rx {
                    match message {
                        SinkMessage::Line(line) => {
                            let mut out = stdout.lock();
                            // A closed stdout means the supervisor is gone;
                            // there is nobody left to report to.
                            let _ = out.write_all(line.as_bytes());
                            let _ = out.write_all(b"\n");
                            let _ = out.flush();
                        }
                        SinkMessage::Shutdown(ack) => {
                            let _ = stdout.lock().flush();
                            let _ = ack.send(());
                            return;
                        }
                    }
                }
            })
            .expect("failed to start the stdout writer thread");
        Self {
            tx,
            closed: Arc::new(Mutex::new(false)),
            run: run.into(),
            segment,
            pid: std::process::id(),
        }
    }

    /// Emit one event. `fields` must be a JSON object; the common fields of
    /// contract section 2.3 are added here.
    fn emit(&self, event_type: &str, fields: Json) {
        let closed = lock(&self.closed);
        if !*closed {
            self.send(event_type, fields);
        }
    }

    /// Emit the event `build` returns, unless the terminal event was written.
    /// `build` runs with the sink locked, like the `before` of
    /// [`Self::emit_terminal`]: state that both change (the set of live
    /// threads) is reported by exactly one of them.
    fn emit_with(&self, build: impl FnOnce() -> Option<(&'static str, Json)>) {
        let closed = lock(&self.closed);
        if *closed {
            return;
        }
        if let Some((event_type, fields)) = build() {
            self.send(event_type, fields);
        }
    }

    /// Emit the terminal event (`completed`, `failed`, or `cancelled`) and
    /// drop every later event. `before` runs with the sink locked and returns
    /// events to write immediately before the terminal one; an [`Self::emit`]
    /// on another thread lands either before all of them or not at all.
    fn emit_terminal(
        &self,
        event_type: &str,
        fields: Json,
        before: impl FnOnce() -> Vec<(&'static str, Json)>,
    ) {
        let mut closed = lock(&self.closed);
        if *closed {
            return;
        }
        for (event_type, fields) in before() {
            self.send(event_type, fields);
        }
        self.send(event_type, fields);
        *closed = true;
    }

    fn send(&self, event_type: &str, fields: Json) {
        let mut event = serde_json::Map::new();
        event.insert("v".to_string(), json!(1));
        event.insert("type".to_string(), json!(event_type));
        event.insert("ts".to_string(), json!(now_ms()));
        event.insert("run".to_string(), json!(&*self.run));
        event.insert("segment".to_string(), json!(self.segment));
        event.insert("pid".to_string(), json!(self.pid));
        if let Json::Object(fields) = fields {
            event.extend(fields);
        }
        let _ = self
            .tx
            .send(SinkMessage::Line(Json::Object(event).to_string()));
    }

    /// Write every queued line and stop the writer.
    fn shutdown(&self) {
        let (ack_tx, ack_rx) = mpsc::channel();
        if self.tx.send(SinkMessage::Shutdown(ack_tx)).is_ok() {
            let _ = ack_rx.recv_timeout(Duration::from_secs(5));
        }
    }
}

/// Free-form diagnostics go to stderr (contract section 2.2); the site server
/// forwards them as `log` events with `stream: "worker_stderr"`.
///
/// A write error is ignored. `eprintln!` panics when stderr is a closed pipe,
/// which is the case after the supervisor died. The panic ended the stdin task
/// before it cancelled the run, and the worker stayed alive as an orphan.
fn diagnostic(message: std::fmt::Arguments<'_>) {
    let _ = writeln!(std::io::stderr().lock(), "worker: {message}");
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

// ============================================================================
// Run state: durable host, log capture, pending remote calls
// ============================================================================

thread_local! {
    /// The BAML thread whose `baml.io` sys-op is being dispatched on this OS
    /// thread. Set by [`DurableHost::yielded`] and taken by [`WorkerIo`]; the
    /// engine guarantees that no await point separates the two.
    static IO_THREAD: Cell<Option<DurableThreadId>> = const { Cell::new(None) };
}

/// Remote calls of the run, by `call_id`.
#[derive(Default)]
struct RemoteCalls {
    /// Announced calls whose result no thread has taken yet.
    waiting: HashSet<String>,
    /// Results that arrived and wait for their thread: `{"value": ...}` or
    /// `{"error": "..."}`. Kept as JSON so that they fit into a snapshot.
    results: HashMap<String, Json>,
    /// The thread that waits on a call, once it is known in this process.
    threads: HashMap<String, DurableThreadId>,
    /// Where each call was made, relative to the project directory. Set when
    /// the call is announced and when a restored thread registers its wait,
    /// so that a `remote_cancel` names the call site in either process.
    sites: HashMap<String, CallSite>,
}

/// The frame a remote call was made in, ready for an event: the function that
/// made the call and its `(file, line)`.
#[derive(Clone, Debug)]
struct CallSite {
    function: String,
    file: String,
    line: usize,
}

impl CallSite {
    /// The frame of a yield, with the file made relative to the project and
    /// the function in the spelling of a `position` event.
    fn of(site: &YieldPosition, relative: impl FnOnce(&str) -> String) -> Self {
        Self {
            function: WorkerState::display_function(&site.function).to_string(),
            file: relative(&site.file),
            line: site.line,
        }
    }

    /// `(file, line)` as the event fields, `null` when the site is unknown.
    fn fields(site: Option<&Self>) -> (Json, Json) {
        site.map_or((Json::Null, Json::Null), |site| {
            (json!(site.file), json!(site.line))
        })
    }

    /// Contract section 10.3: the function that made the call, so that the app
    /// names the caller instead of guessing it from the run's entry function.
    /// `null` when the worker could not attribute the call to a user frame.
    fn caller(site: Option<&Self>) -> Json {
        site.map_or(Json::Null, |site| json!(site.function))
    }
}

#[derive(Default)]
struct ThreadTracker {
    /// Threads of the run that started and have not ended.
    live: HashSet<DurableThreadId>,
    /// Root threads of helper engine calls (JSON conversion) made while the
    /// host is installed. They are not part of the run and are not reported.
    helpers: HashSet<DurableThreadId>,
    /// The run's root thread, once it has started.
    root: Option<DurableThreadId>,
    /// Last `(file, line)` reported per thread.
    last_position: HashMap<DurableThreadId, (String, usize)>,
}

#[derive(Default)]
struct LogBuffers {
    stdout: String,
    stderr: String,
    stdout_thread: Option<DurableThreadId>,
    stderr_thread: Option<DurableThreadId>,
}

struct WorkerState {
    events: EventSink,
    run: String,
    segment: u64,
    project_root: PathBuf,
    snapshot_dir: PathBuf,
    run_info: RunInfo,
    /// Set by `WorkerArgs::new_state`; host futures need an owned handle.
    this: OnceLock<Weak<WorkerState>>,
    engine: OnceLock<Weak<BexEngine>>,
    program: OnceLock<ProgramIdentity>,
    remote: Mutex<RemoteCalls>,
    remote_changed: tokio::sync::Notify,
    threads: Mutex<ThreadTracker>,
    threads_changed: tokio::sync::Notify,
    logs: Mutex<LogBuffers>,
    /// The task that serves a `pause` command. It resolves to the fields of
    /// the `paused` event once the run is suspended.
    pause_task: Mutex<Option<tokio::task::JoinHandle<Option<Json>>>>,
    pause_requested: AtomicBool,
    /// The task that suspends the run for a long sleep (contract section
    /// 9.2). Like `pause_task` it resolves to the fields of the `paused`
    /// event when it suspended the run.
    sleep_task: Mutex<Option<tokio::task::JoinHandle<Option<Json>>>>,
    /// Set when the root call has returned.
    finished: AtomicBool,
    /// Cancelled together with `finished`, for tasks that wait.
    run_ended: CancellationToken,
    auto_blocked: AtomicU64,
    /// Serializes snapshot attempts from the moment the snapshot number is
    /// picked until the files are written, so that a `pause` that overlaps an
    /// automatic snapshot cannot reuse its number and overwrite its files.
    snapshot_lock: tokio::sync::Mutex<()>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl WorkerState {
    fn new(
        events: EventSink,
        run: String,
        segment: u64,
        project_root: PathBuf,
        snapshot_dir: PathBuf,
        run_info: RunInfo,
    ) -> Self {
        Self {
            events,
            run,
            segment,
            project_root,
            snapshot_dir,
            run_info,
            this: OnceLock::new(),
            engine: OnceLock::new(),
            program: OnceLock::new(),
            remote: Mutex::new(RemoteCalls::default()),
            remote_changed: tokio::sync::Notify::new(),
            threads: Mutex::new(ThreadTracker::default()),
            threads_changed: tokio::sync::Notify::new(),
            logs: Mutex::new(LogBuffers::default()),
            pause_task: Mutex::new(None),
            pause_requested: AtomicBool::new(false),
            sleep_task: Mutex::new(None),
            finished: AtomicBool::new(false),
            run_ended: CancellationToken::new(),
            auto_blocked: AtomicU64::new(0),
            snapshot_lock: tokio::sync::Mutex::new(()),
        }
    }

    fn set_program(&self, program: ProgramIdentity) {
        let _ = self.program.set(program);
    }

    fn program(&self) -> ProgramIdentity {
        self.program.get().copied().unwrap_or(ProgramIdentity {
            hash: [0; 32],
            bytes: 0,
        })
    }

    /// The opaque run state of a snapshot taken now.
    fn saved_run_state(&self) -> Vec<u8> {
        let (partial_stdout, partial_stderr) = {
            let logs = lock(&self.logs);
            (logs.stdout.clone(), logs.stderr.clone())
        };
        let remote = lock(&self.remote);
        let mut waiting: Vec<String> = remote.waiting.iter().cloned().collect();
        waiting.sort();
        let saved = SavedRunState {
            v: 1,
            function: self.run_info.function.clone(),
            durable: self.run_info.durable,
            json_args: self.run_info.json_args.clone(),
            opt_level: self.run_info.opt_level,
            call_counter: 0,
            segment: self.segment,
            waiting,
            results: remote
                .results
                .iter()
                .map(|(call_id, result)| (call_id.clone(), result.clone()))
                .collect(),
            partial_stdout,
            partial_stderr,
        };
        serde_json::to_vec(&saved).unwrap_or_default()
    }

    /// Take over the unfinished output lines of the segment that wrote the
    /// snapshot.
    fn restore_partial_logs(&self, saved: &SavedRunState) {
        let mut logs = lock(&self.logs);
        logs.stdout.clone_from(&saved.partial_stdout);
        logs.stderr.clone_from(&saved.partial_stderr);
    }

    fn restore_remote_calls(&self, saved: &SavedRunState) {
        let mut remote = lock(&self.remote);
        remote.waiting = saved.waiting.iter().cloned().collect();
        remote.results = saved
            .results
            .iter()
            .filter(|(call_id, _)| remote.waiting.contains(*call_id))
            .map(|(call_id, result)| (call_id.clone(), result.clone()))
            .collect();
    }

    /// Take in a `remote_result` (a stdin command or a `--remote-result`
    /// argument). A result for a call the run does not wait on, or for a call
    /// that already has a result, is ignored.
    fn accept_remote_result(&self, command: &Json) {
        let Some(call_id) = command.get("call_id").and_then(Json::as_str) else {
            diagnostic(format_args!("remote_result without call_id ignored"));
            return;
        };
        // When the result arrived at the supervisor. A supervisor that does
        // not say gets the time the worker saw it.
        let ts = command
            .get("ts")
            .and_then(Json::as_u64)
            .unwrap_or_else(now_ms);
        let result = match (
            command.get("error").and_then(Json::as_str),
            command.get("value"),
        ) {
            (Some(error), _) => json!({ "error": error, "ts": ts }),
            (None, value) => json!({ "value": value.cloned().unwrap_or(Json::Null), "ts": ts }),
        };
        {
            let mut remote = lock(&self.remote);
            if !remote.waiting.contains(call_id) {
                diagnostic(format_args!(
                    "remote_result for `{call_id}` ignored: the run does not wait on that call"
                ));
                return;
            }
            if remote.results.contains_key(call_id) {
                diagnostic(format_args!(
                    "remote_result for `{call_id}` ignored: the call already has a result"
                ));
                return;
            }
            remote.results.insert(call_id.to_string(), result);
        }
        self.remote_changed.notify_waiters();
    }

    /// The results the worker holds, with their arrival times, for the
    /// engine's ordered replay.
    fn recorded_results(&self) -> Vec<RecordedResult> {
        lock(&self.remote)
            .results
            .iter()
            .map(|(call_id, result)| RecordedResult {
                call_id: call_id.clone(),
                at_unix_ms: result.get("ts").and_then(Json::as_u64).unwrap_or(0),
            })
            .collect()
    }

    fn has_result(&self, call_id: &str) -> bool {
        lock(&self.remote).results.contains_key(call_id)
    }

    fn set_engine(&self, engine: &Arc<BexEngine>) {
        let _ = self.engine.set(Arc::downgrade(engine));
    }

    fn engine(&self) -> Option<Arc<BexEngine>> {
        self.engine.get().and_then(Weak::upgrade)
    }

    /// `file` relative to the project directory when it lies inside it.
    fn relative_file(&self, file: &str) -> String {
        let path = Path::new(file);
        path.strip_prefix(&self.project_root)
            .map_or_else(|_| file.to_string(), |p| p.to_string_lossy().into_owned())
    }

    /// The name a caller would write: the qualified name without the `user.`
    /// package prefix. A `spawn { }` body is a lambda whose VM name is
    /// `<lambda(enclosing_function, index)>`; it is reported as the function
    /// that contains it, which is what a source view wants to show.
    fn display_function(name: &str) -> &str {
        let name = name
            .rsplit_once("<lambda(")
            .and_then(|(_, rest)| rest.split_once(','))
            .map_or(name, |(enclosing, _)| enclosing);
        name.strip_prefix("user.").unwrap_or(name)
    }

    fn is_helper(&self, thread: DurableThreadId) -> bool {
        lock(&self.threads).helpers.contains(&thread)
    }

    /// Forget the remote calls the run still waits on and return them with
    /// their threads, ordered by call id. A result that arrives afterwards is
    /// ignored.
    fn take_waiting_calls(&self) -> Vec<(String, Option<DurableThreadId>, Option<CallSite>)> {
        let mut remote = lock(&self.remote);
        remote.results.clear();
        let mut calls: Vec<_> = remote
            .waiting
            .drain()
            .collect::<Vec<_>>()
            .into_iter()
            .map(|call_id| {
                let thread = remote.threads.remove(&call_id);
                let site = remote.sites.remove(&call_id);
                (call_id, thread, site)
            })
            .collect();
        calls.sort_by(|left, right| (&left.0, left.1).cmp(&(&right.0, right.1)));
        calls
    }

    /// Forget the threads that are still running and return them, lowest id
    /// first. A later `thread_ended` for one of them is not reported again.
    fn take_live_threads(&self) -> Vec<DurableThreadId> {
        let mut threads = lock(&self.threads);
        threads.last_position.clear();
        let mut live: Vec<_> = threads.live.drain().collect();
        live.sort_unstable();
        live
    }

    async fn wait_for_threads(&self, grace: Duration) {
        let wait = async {
            loop {
                let notified = self.threads_changed.notified();
                if lock(&self.threads).live.is_empty() {
                    return;
                }
                notified.await;
            }
        };
        let _ = tokio::time::timeout(grace, wait).await;
    }

    /// Append program output and emit one `log` event per complete line.
    fn write_log(&self, stream: &'static str, text: &str) {
        let thread = IO_THREAD.with(Cell::take);
        let mut lines = Vec::new();
        {
            let mut logs = lock(&self.logs);
            let logs = &mut *logs;
            let (buffer, buffer_thread) = if stream == "stdout" {
                (&mut logs.stdout, &mut logs.stdout_thread)
            } else {
                (&mut logs.stderr, &mut logs.stderr_thread)
            };
            buffer.push_str(text);
            *buffer_thread = thread.or(*buffer_thread);
            while let Some(newline) = buffer.find('\n') {
                let line: String = buffer.drain(..=newline).collect();
                lines.push((
                    line.trim_end_matches(['\n', '\r']).to_string(),
                    *buffer_thread,
                ));
            }
            if buffer.is_empty() {
                *buffer_thread = None;
            }
        }
        for (line, thread) in lines {
            self.events.emit(
                "log",
                json!({ "stream": stream, "text": line, "thread": thread }),
            );
        }
    }

    /// Emit output that did not end with a newline.
    fn flush_partial_logs(&self) {
        let (stdout, stdout_thread, stderr, stderr_thread) = {
            let mut logs = lock(&self.logs);
            (
                std::mem::take(&mut logs.stdout),
                logs.stdout_thread.take(),
                std::mem::take(&mut logs.stderr),
                logs.stderr_thread.take(),
            )
        };
        for (stream, text, thread) in [
            ("stdout", stdout, stdout_thread),
            ("stderr", stderr, stderr_thread),
        ] {
            if !text.is_empty() {
                self.events.emit(
                    "log",
                    json!({ "stream": stream, "text": text, "thread": thread }),
                );
            }
        }
    }

    fn describe_engine_error(&self, error: &EngineError) -> (String, Vec<Json>) {
        match error {
            EngineError::UnhandledThrow { value, trace } => {
                // Innermost frame first, like the frames of a state dump.
                let stack = trace
                    .iter()
                    .rev()
                    .map(|frame| {
                        json!({
                            "function": Self::display_function(&frame.function_name),
                            "file": self.relative_file(&frame.file_path),
                            "line": frame.error_line,
                        })
                    })
                    .collect();
                (value.render_readable(), stack)
            }
            other => (other.to_string(), Vec::new()),
        }
    }
}

impl DurableHost for WorkerState {
    fn remote_call(
        &self,
        request: RemoteCallRequest,
    ) -> BoxFuture<'static, Result<String, String>> {
        // The id names the call by the calling thread's place in the spawn
        // tree and by its number among that thread's calls. Both are part of
        // the thread's snapshot state, so an execution that starts again from
        // an older snapshot gives every call the id it had before, however
        // the scheduler interleaves the threads. The supervisor relies on
        // that when it answers a call it has seen from a stored result. The
        // root thread's calls are `<run>-c<n>`; a spawned thread's calls are
        // `<run>-c<path>-<n>`, where the path contains a dot.
        let call_id = if request.thread_path == "0" || request.thread_path.is_empty() {
            format!("{}-c{}", self.run, request.call_index)
        } else {
            format!(
                "{}-c{}-{}",
                self.run, request.thread_path, request.call_index
            )
        };
        // Where the call was made, in the spelling of a `position` event.
        let site = request
            .site
            .as_ref()
            .map(|site| CallSite::of(site, |file| self.relative_file(file)));
        // Registered before the event is written, so a supervisor that answers
        // immediately always finds the call.
        {
            let mut remote = lock(&self.remote);
            remote.waiting.insert(call_id.clone());
            remote.threads.insert(call_id.clone(), request.thread);
            if let Some(site) = site.clone() {
                remote.sites.insert(call_id.clone(), site);
            }
        }
        let engine = self.engine();
        let events = self.events.clone();
        let state = self.this.get().and_then(Weak::upgrade);
        Box::pin(async move {
            let serialized = async {
                let Some(engine) = engine else {
                    return Err("engine is gone".to_string());
                };
                let mut args = serde_json::Map::new();
                for arg in request.args {
                    let value = serialize_json(&engine, arg.value, &arg.ty)
                        .await
                        .map_err(|e| format!("cannot serialize argument `{}`: {e:#}", arg.name))?;
                    args.insert(arg.name, value);
                }
                Ok(args)
            }
            .await;
            let args = match serialized {
                Ok(args) => args,
                Err(message) => {
                    // The call was never announced, so the run does not wait
                    // on it and nobody is told that it was abandoned.
                    if let Some(state) = state {
                        let mut remote = lock(&state.remote);
                        remote.waiting.remove(&call_id);
                        remote.threads.remove(&call_id);
                    }
                    return Err(message);
                }
            };
            let (file, line) = CallSite::fields(site.as_ref());
            events.emit(
                "remote_call",
                json!({
                    "call_id": call_id,
                    "thread": request.thread,
                    "function": WorkerState::display_function(&request.function),
                    "args": args,
                    "file": file,
                    "line": line,
                    "caller": CallSite::caller(site.as_ref()),
                }),
            );
            Ok(call_id)
        })
    }

    fn remote_result(
        &self,
        wait: RemoteResultWait,
    ) -> BoxFuture<'static, Result<BexExternalValue, String>> {
        let Some(state) = self.this.get().and_then(Weak::upgrade) else {
            return Box::pin(async { Err("the worker is shutting down".to_string()) });
        };
        // The call site the snapshot carries, for a `remote_cancel` that this
        // process reports for a call another process made.
        let site = wait
            .site
            .as_ref()
            .map(|site| CallSite::of(site, |file| self.relative_file(file)));
        Box::pin(async move {
            // A resumed thread registers the call it was parked on, in case
            // the snapshot's run state did not list it.
            {
                let mut remote = lock(&state.remote);
                remote.waiting.insert(wait.call_id.clone());
                remote.threads.insert(wait.call_id.clone(), wait.thread);
                if let Some(site) = site {
                    remote.sites.insert(wait.call_id.clone(), site);
                }
            }
            loop {
                let changed = state.remote_changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                let ready = lock(&state.remote).results.get(&wait.call_id).cloned();
                let Some(result) = ready else {
                    changed.await;
                    continue;
                };
                let decoded = match result.get("error").and_then(Json::as_str) {
                    Some(message) => Err(message.to_string()),
                    None => {
                        let value = result.get("value").cloned().unwrap_or(Json::Null);
                        let return_type = wait.return_type.clone();
                        let Some(engine) = state.engine() else {
                            return Err("engine is gone".to_string());
                        };
                        // The engine may stop polling this future while it
                        // attempts a snapshot. The helper call holds a heap
                        // permit, so it runs as its own task: left unpolled
                        // here it would block the snapshot forever.
                        tokio::spawn(async move {
                            deserialize_json(&engine, &value, &return_type).await
                        })
                        .await
                        .map_err(|e| format!("decoding the result failed: {e}"))
                        .and_then(|decoded| {
                            decoded.map_err(|e| {
                                format!("result does not match the return type: {e:#}")
                            })
                        })
                    }
                };
                // The result is consumed in `remote_result_delivered`, once
                // the engine has taken it: a snapshot that parks the thread
                // before that point still carries the result.
                return decoded;
            }
        })
    }

    fn remote_result_available(&self, call_id: &str) -> bool {
        lock(&self.remote).results.contains_key(call_id)
    }

    fn remote_result_delivered(&self, call_id: &str, thread: DurableThreadId) {
        let consumed = {
            let mut remote = lock(&self.remote);
            remote.waiting.remove(call_id);
            remote.threads.remove(call_id);
            remote.sites.remove(call_id);
            remote.results.remove(call_id).is_some()
        };
        if consumed {
            self.events.emit(
                "remote_result_received",
                json!({ "call_id": call_id, "thread": thread }),
            );
        }
    }

    /// Contract 9.2: the run no longer waits on `call_id`. The call leaves the
    /// waiting set, so a result that still arrives is ignored like a result
    /// for an unknown call, and the run state of a later snapshot does not
    /// list it.
    fn remote_cancelled(&self, cancel: RemoteCancel) {
        let recorded = {
            let mut remote = lock(&self.remote);
            remote.waiting.remove(&cancel.call_id);
            remote.results.remove(&cancel.call_id);
            remote.threads.remove(&cancel.call_id);
            remote.sites.remove(&cancel.call_id)
        };
        // The engine's site is the one recorded when the call was made; the
        // worker's own record answers for a call it only heard about.
        let site = cancel
            .site
            .as_ref()
            .map(|site| CallSite::of(site, |file| self.relative_file(file)))
            .or(recorded);
        let (file, line) = CallSite::fields(site.as_ref());
        self.events.emit(
            "remote_cancel",
            json!({
                "call_id": cancel.call_id,
                "thread": cancel.thread,
                "file": file,
                "line": line,
                "cause": cancel.cause.as_str(),
            }),
        );
    }

    fn thread_started(
        &self,
        thread: DurableThreadId,
        parent: Option<DurableThreadId>,
        site: Option<&YieldPosition>,
    ) {
        {
            let mut threads = lock(&self.threads);
            match parent {
                None if threads.root.is_some() => {
                    threads.helpers.insert(thread);
                    return;
                }
                None => threads.root = Some(thread),
                Some(parent) if threads.helpers.contains(&parent) => {
                    threads.helpers.insert(thread);
                    return;
                }
                Some(_) => {}
            }
            threads.live.insert(thread);
        }
        let site = site.map(|site| CallSite::of(site, |file| self.relative_file(file)));
        let (file, line) = CallSite::fields(site.as_ref());
        self.events.emit(
            "thread_started",
            json!({
                "thread": thread,
                "parent_thread": parent,
                "file": file,
                "line": line,
            }),
        );
    }

    fn thread_ended(&self, thread: DurableThreadId) {
        // Several threads of a paused run end at the same time as the
        // terminal event is written. The removal from the live set and the
        // event are one step under the sink lock, so that a thread is
        // reported either here or by `take_live_threads`, never by neither.
        self.events.emit_with(|| {
            let mut threads = lock(&self.threads);
            if threads.helpers.remove(&thread) {
                return None;
            }
            threads.last_position.remove(&thread);
            if !threads.live.remove(&thread) {
                // Already reported by `take_live_threads`.
                return None;
            }
            Some(("thread_ended", json!({ "thread": thread })))
        });
        self.threads_changed.notify_waiters();
    }

    fn yielded(
        &self,
        thread: DurableThreadId,
        reason: YieldReason,
        op: Option<&str>,
        position: Option<&YieldPosition>,
    ) {
        if self.is_helper(thread) {
            return;
        }
        if reason == YieldReason::SysOp && op.is_some_and(|op| op.starts_with("baml.io.")) {
            IO_THREAD.with(|current| current.set(Some(thread)));
        }
        let Some(position) = position else {
            return;
        };
        let file = self.relative_file(&position.file);
        {
            let mut threads = lock(&self.threads);
            let last = threads.last_position.get(&thread);
            if last.is_some_and(|(last_file, last_line)| {
                *last_file == file && *last_line == position.line
            }) {
                return;
            }
            threads
                .last_position
                .insert(thread, (file.clone(), position.line));
        }
        // `op` is the sys-op path for a sys-op yield and null otherwise.
        let op = (reason == YieldReason::SysOp).then_some(op).flatten();
        self.events.emit(
            "position",
            json!({
                "thread": thread,
                "function": Self::display_function(&position.function),
                "file": file,
                "line": position.line,
                "reason": reason.as_str(),
                "op": op,
            }),
        );
    }
}

/// `baml.io` provider that turns program output into `log` events. Process
/// stdout is never written: it carries the event stream.
struct WorkerIo(Arc<WorkerState>);

impl sys_ops::io::IoNamespaceIo for WorkerIo {
    fn input(
        &self,
        _heap: &Arc<bex_heap::BexHeap>,
        _call_id: CallId,
        _prompt: Option<String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        // Drop the attribution `yielded` left for this op, so that it cannot
        // leak to a later print on this OS thread.
        IO_THREAD.with(Cell::take);
        SysOpOutput::err(VmBamlError::Io {
            message: "baml.io.input() is not available in a worker: stdin carries commands".into(),
        })
    }

    fn print(
        &self,
        _heap: &Arc<bex_heap::BexHeap>,
        _call_id: CallId,
        s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        self.0.write_log("stdout", &s);
        SysOpOutput::ok(())
    }

    fn println(
        &self,
        _heap: &Arc<bex_heap::BexHeap>,
        _call_id: CallId,
        mut s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        s.push('\n');
        self.0.write_log("stdout", &s);
        SysOpOutput::ok(())
    }

    fn eprint(
        &self,
        _heap: &Arc<bex_heap::BexHeap>,
        _call_id: CallId,
        s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        self.0.write_log("stderr", &s);
        SysOpOutput::ok(())
    }

    fn eprintln(
        &self,
        _heap: &Arc<bex_heap::BexHeap>,
        _call_id: CallId,
        mut s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        s.push('\n');
        self.0.write_log("stderr", &s);
        SysOpOutput::ok(())
    }
}

// ============================================================================
// Commands (stdin)
// ============================================================================

/// Read one JSON command per line from stdin until EOF. A closed stdin that is
/// not a terminal means the supervisor is gone: the run is cancelled, unless
/// `--ignore-stdin-eof` was given.
async fn read_commands(
    state: Arc<WorkerState>,
    engine: Arc<BexEngine>,
    cancel: CancellationToken,
    ignore_eof: bool,
) {
    let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let command: Json = match serde_json::from_str(line) {
            Ok(command) => command,
            Err(error) => {
                diagnostic(format_args!("ignoring malformed command: {error}"));
                continue;
            }
        };
        match command.get("type").and_then(Json::as_str) {
            Some("pause") => {
                if state.pause_requested.swap(true, Ordering::AcqRel) {
                    diagnostic(format_args!("a pause is already in progress"));
                    continue;
                }
                let task = tokio::spawn(pause_run(Arc::clone(&state), Arc::clone(&engine)));
                *lock(&state.pause_task) = Some(task);
            }
            Some("cancel") => cancel_run(&state, &engine, &cancel),
            Some("remote_result") => state.accept_remote_result(&command),
            other => diagnostic(format_args!("ignoring unknown command type {other:?}")),
        }
    }
    if !ignore_eof && !std::io::stdin().is_terminal() {
        diagnostic(format_args!("stdin closed: cancelling the run"));
        cancel_run(&state, &engine, &cancel);
    }
}

/// Cancel the run and make sure the process ends.
///
/// The engine looks at the cancel token when a thread yields. A thread in a
/// compute loop only yields when it is asked to, so the request is repeated
/// until the root call has returned (a finishing collection clears it). A run
/// that still has not ended after [`CANCEL_GRACE`] is reported as cancelled by
/// this task, and the process exits with code 130.
fn cancel_run(state: &Arc<WorkerState>, engine: &Arc<BexEngine>, cancel: &CancellationToken) {
    if cancel.is_cancelled() {
        return;
    }
    cancel.cancel();
    let state = Arc::clone(state);
    let engine = Arc::clone(engine);
    tokio::spawn(async move {
        let deadline = Instant::now() + CANCEL_GRACE;
        while !state.finished.load(Ordering::Acquire) {
            if Instant::now() >= deadline {
                diagnostic(format_args!(
                    "the cancelled run did not end within {CANCEL_GRACE:?}: exiting"
                ));
                state.flush_partial_logs();
                events_terminal(&state, "cancelled", json!({}));
                state.events.shutdown();
                bex_events::prof::flush_and_join(Duration::from_secs(1));
                std::process::exit(EXIT_CANCELLED);
            }
            engine.durable_request_yield();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
}

// ============================================================================
// Snapshots
// ============================================================================

/// What the commit step of a snapshot wrote.
struct WrittenSnapshot {
    snapshot_path: PathBuf,
    state_path: PathBuf,
    write_ms: f64,
    file_bytes: u64,
}

/// `(highest n among snap-<n>.bamlsnap in dir) + 1`.
fn next_snapshot_number(dir: &Path) -> u64 {
    let highest = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()?
                .strip_prefix("snap-")?
                .strip_suffix(".bamlsnap")?
                .parse::<u64>()
                .ok()
        })
        .max()
        .unwrap_or(0);
    highest + 1
}

fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

impl WorkerState {
    /// Rewrite the engine's state dump the way `position` events name things:
    /// files relative to the project, functions without the `user.` prefix.
    fn localize_state_dump(&self, dump: &mut Json) {
        let Some(threads) = dump.get_mut("threads").and_then(Json::as_array_mut) else {
            return;
        };
        for frame in threads
            .iter_mut()
            .filter_map(|thread| thread.get_mut("frames").and_then(Json::as_array_mut))
            .flatten()
        {
            if let Some(file) = frame.get("file").and_then(Json::as_str) {
                frame["file"] = json!(self.relative_file(file));
            }
            if let Some(function) = frame.get("function").and_then(Json::as_str) {
                frame["function"] = json!(Self::display_function(function));
            }
        }
    }

    /// Rewrite a root-to-value path the way every other event names things:
    /// `frame user.f` becomes `frame f`.
    fn localize_blocked_path(path: Vec<String>) -> Vec<String> {
        path.into_iter()
            .map(|element| {
                for prefix in ["frame ", "native frame "] {
                    if let Some(function) = element.strip_prefix(prefix) {
                        return format!("{prefix}{}", Self::display_function(function));
                    }
                }
                element
            })
            .collect()
    }

    /// Take one snapshot of the run and write `snap-<n>.bamlsnap` and
    /// `snap-<n>.json` into the snapshot directory.
    async fn take_snapshot(
        self: &Arc<Self>,
        engine: &Arc<BexEngine>,
        mode: SnapshotMode,
        on_progress: impl FnMut(PauseProgress) + Send,
    ) -> Result<SnapshotReport<WrittenSnapshot>, SnapshotFailure> {
        let _attempt = self.snapshot_lock.lock().await;
        let dir = self.snapshot_dir.clone();
        let _ = std::fs::create_dir_all(&dir);
        let n = next_snapshot_number(&dir);
        let request = SnapshotRequest {
            run_id: self.run.clone(),
            segment: u32::try_from(self.segment).unwrap_or(u32::MAX),
            seq: n,
            program_hash: self.program().hash,
            embed_program: None,
            compress: true,
            mode,
        };
        let state = Arc::clone(self);
        engine
            .durable_snapshot(
                request,
                || self.saved_run_state(),
                on_progress,
                move |parts: SnapshotParts<'_>| {
                    let started = Instant::now();
                    let snapshot_path = dir.join(format!("snap-{n}.bamlsnap"));
                    let state_path = dir.join(format!("snap-{n}.json"));
                    let mut dump = parts.state_dump.clone();
                    state.localize_state_dump(&mut dump);
                    let dump = serde_json::to_vec(&dump).map_err(|e| e.to_string())?;
                    // The state dump first: a reader that sees the snapshot
                    // can rely on the dump being there.
                    write_atomically(&state_path, &dump)
                        .map_err(|e| format!("cannot write {}: {e}", state_path.display()))?;
                    write_atomically(&snapshot_path, parts.bytes)
                        .map_err(|e| format!("cannot write {}: {e}", snapshot_path.display()))?;
                    Ok(WrittenSnapshot {
                        snapshot_path,
                        state_path,
                        write_ms: ms_since(started),
                        file_bytes: parts.bytes.len() as u64,
                    })
                },
            )
            .await
    }

    /// The fields of a `paused` or `snapshot` event.
    fn snapshot_event_fields(&self, report: &SnapshotReport<WrittenSnapshot>) -> Json {
        let written = &report.committed;
        json!({
            "snapshot_path": written.snapshot_path.to_string_lossy(),
            "state_path": written.state_path.to_string_lossy(),
            "stats": {
                "pause_latency_ms": report.pause_latency_ms,
                "walk_ms": report.stats.walk_ms,
                "encode_ms": report.stats.encode_ms,
                "compress_ms": report.stats.compress_ms,
                "write_ms": written.write_ms,
                "objects": report.stats.objects,
                "raw_bytes": report.stats.raw_bytes,
                "compressed_bytes": report.stats.compressed_bytes,
                "program_bytes": self.program().bytes,
                "blocked_attempts": report.blocked_attempts
                    + self.auto_blocked.load(Ordering::Relaxed),
                // Beyond the contract: the size of the snapshot file.
                "file_bytes": written.file_bytes,
                "threads": report.threads,
            },
        })
    }
}

/// Serve a `pause` command: snapshot the run at its next clean point. Resolves
/// to the fields of the `paused` event, or to `None` when the run ended first.
///
/// Nothing but `paused` or the end of the run ends a pause request (contract
/// section 7.1). A snapshot that cannot be written or stored is reported as
/// `blocked`, once per distinct reason, and the attempt is repeated.
async fn pause_run(state: Arc<WorkerState>, engine: Arc<BexEngine>) -> Option<Json> {
    let mut last_failure: Option<String> = None;
    let mut failed_attempts = 0u64;
    loop {
        if state.finished.load(Ordering::Acquire) {
            return None;
        }
        let result = state
            .take_snapshot(&engine, SnapshotMode::Suspend, |progress| match progress {
                PauseProgress::Pausing { waiting_on } => {
                    state
                        .events
                        .emit("pausing", json!({ "waiting_on": waiting_on }));
                }
                PauseProgress::Blocked { reason, path } => {
                    state.events.emit(
                        "blocked",
                        json!({
                            "reason": reason,
                            "path": WorkerState::localize_blocked_path(path),
                        }),
                    );
                }
            })
            .await;
        match result {
            Ok(mut report) => {
                report.blocked_attempts += failed_attempts;
                let mut fields = state.snapshot_event_fields(&report);
                // Contract section 9.2: a requested pause has no wake time.
                fields["wake"] = Json::Null;
                return Some(fields);
            }
            // The root thread has not entered the engine loop yet.
            Err(SnapshotFailure::RunEnded) if !state.finished.load(Ordering::Acquire) => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(SnapshotFailure::RunEnded) => return None,
            Err(error) => {
                failed_attempts += 1;
                let reason = error.to_string();
                if last_failure.as_deref() != Some(reason.as_str()) {
                    state
                        .events
                        .emit("blocked", json!({ "reason": reason, "path": [] }));
                    last_failure = Some(reason);
                }
                tokio::time::sleep(PAUSE_RETRY_INTERVAL).await;
            }
        }
    }
}

/// Suspend a durable run for a long sleep (contract section 9.2). Resolves to
/// the fields of the `paused` event, or to `None` when the run ended in
/// another way.
///
/// The engine evaluates the self-suspend rule whenever a thread enters or
/// leaves a wait. When the rule holds, one snapshot attempt is made on the
/// same path as `pause`. The engine checks the rule again with every thread
/// parked: when a thread has left its wait in the meantime, nothing is
/// written and the run continues. When the snapshot is blocked, the threads
/// keep waiting in this process, the worker emits one `blocked` event, and
/// the engine does not report the same sleep again.
async fn suspend_for_sleeps(
    state: Arc<WorkerState>,
    engine: Arc<BexEngine>,
    min_remaining_ms: u64,
) -> Option<Json> {
    loop {
        tokio::select! {
            _ = engine.durable_idle_sleep(min_remaining_ms) => {}
            () = state.run_ended.cancelled() => return None,
        }
        if state.pause_requested.load(Ordering::Acquire) {
            // The task of the `pause` command suspends the run.
            state.run_ended.cancelled().await;
            return None;
        }
        let mode = SnapshotMode::SuspendIfIdle { min_remaining_ms };
        match state.take_snapshot(&engine, mode, |_| {}).await {
            Ok(report) => {
                let mut fields = state.snapshot_event_fields(&report);
                // A `pause` command that arrived while this snapshot was
                // written is served by it, and a requested pause has no wake
                // time: the supervisor decides when the run continues.
                fields["wake"] = match report.wake {
                    Some(wake) if !state.pause_requested.load(Ordering::Acquire) => json!({
                        "reason": "sleep",
                        "remaining_ms": wake.remaining_ms,
                        "at_ts": wake.deadline_unix_ms,
                    }),
                    _ => Json::Null,
                };
                return Some(fields);
            }
            Err(SnapshotFailure::NotIdle) => {}
            Err(SnapshotFailure::RunEnded) => {
                tokio::select! {
                    () = tokio::time::sleep(Duration::from_millis(20)) => {}
                    () = state.run_ended.cancelled() => return None,
                }
            }
            Err(SnapshotFailure::Blocked { reason, path, .. }) => {
                state.auto_blocked.fetch_add(1, Ordering::Relaxed);
                state.events.emit(
                    "blocked",
                    json!({
                        "reason": reason,
                        "path": WorkerState::localize_blocked_path(path),
                    }),
                );
            }
            Err(error) => {
                state.auto_blocked.fetch_add(1, Ordering::Relaxed);
                state.events.emit(
                    "blocked",
                    json!({ "reason": error.to_string(), "path": [] }),
                );
            }
        }
    }
}

/// Snapshot a durable run at clean yields, at most once per `interval`, and
/// let it keep running. A blocked attempt only bumps a counter.
async fn automatic_snapshots(state: Arc<WorkerState>, engine: Arc<BexEngine>, interval: Duration) {
    loop {
        tokio::time::sleep(interval).await;
        if state.finished.load(Ordering::Acquire) {
            return;
        }
        if state.pause_requested.load(Ordering::Acquire) {
            continue;
        }
        let mode = SnapshotMode::KeepRunning {
            park_timeout: AUTO_SNAPSHOT_PARK_TIMEOUT,
        };
        match state.take_snapshot(&engine, mode, |_| {}).await {
            Ok(report) => {
                let mut fields = state.snapshot_event_fields(&report);
                fields["automatic"] = json!(true);
                // Contract section 7.1: nobody requested this snapshot, so
                // there is no request time to derive from the event. The time
                // the run took to park is reported under its own name.
                fields["stats"]["park_ms"] = fields["stats"]["pause_latency_ms"].take();
                state.events.emit("snapshot", fields);
            }
            Err(SnapshotFailure::Blocked { .. } | SnapshotFailure::ParkTimeout) => {
                state.auto_blocked.fetch_add(1, Ordering::Relaxed);
            }
            Err(SnapshotFailure::RunEnded) => {}
            Err(error) => diagnostic(format_args!("automatic snapshot failed: {error}")),
        }
    }
}
