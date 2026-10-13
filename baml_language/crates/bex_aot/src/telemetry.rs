//! Telemetry for natively compiled BAML: the VM's producer, for the
//! functions the backend observes.
//!
//! A recording from a native program is the VM's recording format, built by
//! the same pipeline: records go into `btel_processor`'s ring, its processor
//! thread folds timing completions into per-call-path aggregates, and
//! `btel_recorder` and `btel_file` write the files. Only the producer (what
//! runs on each call) lives here, and it follows the VM's
//! (`bex_vm::telemetry`) record for record.
//!
//! # What is observed
//!
//! The backend partitions functions when it compiles them. An *observed*
//! function is emitted `#[inline(never)]` and runs [`enter`] and
//! [`Frame::exit`] around its body. Every other function is *folded*: no
//! instrumentation, free to be inlined, and its time is part of its nearest
//! observed ancestor's, as a `trace.hidden` helper's is on the VM. Its
//! metadata says so ([`btel_types::FunctionMetadata::inlined`]).
//!
//! Every call site, folded or observed, names itself with [`site`] before
//! the call: the callee's call path is keyed, as on the VM, by its parent
//! path, the physical caller and the caller's site. A site is an index into
//! the caller's source map (`PC_COORDINATE_COMPILED_SITE`).
//!
//! # Levels
//!
//! `BAML_TELEMETRY`, read once by [`start`]:
//!
//! | level | records |
//! |---|---|
//! | `off` | nothing; no recording is started |
//! | `low` | each root call's thread span |
//! | `medium` (default) | plus a timing completion per observed call |
//! | `high` | plus a span (announcement and completion) per observed call |
//!
//! The observed set is fixed when the program is compiled; a level cannot
//! observe a folded function. A program compiled with every function observed
//! records at `high` what the VM records at `high`.
//!
//! # Threads
//!
//! A native program runs synchronously on one thread. Each call into the
//! program from its host ([`root`]) is one telemetry thread, as each engine
//! call is on the VM. The producer's state is per OS thread.

use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU8, Ordering},
    },
};

use btel_clock::{ClockEpoch, ClockRuntime};
use btel_processor::TelemetryRuntime;
use btel_records::{ContextReference, SpanRecord, TimingRecord};
use btel_types::{
    AwaitDuration, CallPathEdge, CallPathId, ClockInstant, FunctionId, FunctionIdAllocator,
    FunctionMetadata, FunctionMetadataTable, FunctionNames, InvocationOutcome, LAST_CALL_PATH_ID,
    RecordingId, RuntimeFunctionKind, RuntimeFunctionOrigin, SourceCoordinate, SourceMap,
    SourceMapEntry, SourceSpan, TelemetryId, allocate_call_path_id, allocate_telemetry_id,
};
use rustc_hash::FxHashMap;

use crate::{Panic, Thrown};

/// One compiled BAML function, as the backend describes it: what the
/// recording's function table says about it.
#[derive(Debug)]
pub struct Function {
    /// The BAML function's fully qualified name, as the VM names it.
    pub fqn: &'static str,
    /// Whether the function records its calls; a folded one never does.
    pub observed: bool,
    /// Its source file's path and compiler-local ID, and its byte span.
    pub file: &'static str,
    pub file_id: u32,
    pub start: u32,
    pub end: u32,
    /// Its call sites, by site index: the source each [`site`] names.
    pub sites: &'static [Site],
}

/// The source of one call site.
#[derive(Debug)]
pub struct Site {
    pub start: u32,
    pub end: u32,
    /// One-based line of `start`.
    pub line: u32,
}

/// Where a recording is written.
#[derive(Debug, Clone)]
pub enum Destination {
    /// `<dir>/<recording-id>/`, blobs under `<dir>/cas`.
    Directory(PathBuf),
    /// `btel/recordings` under the BAML home (`BAML_HOME`, else `~/.baml`).
    UserFiles,
}

const OFF: u8 = 0;
const LOW: u8 = 1;
const MEDIUM: u8 = 2;
const HIGH: u8 = 3;

/// The running recording's level; `OFF` when none is running.
static LEVEL: AtomicU8 = AtomicU8::new(OFF);
static RECORDING: OnceLock<Recording> = OnceLock::new();

struct Recording {
    runtime: Arc<TelemetryRuntime>,
    delivery: Option<Arc<btel_file::LocalDelivery>>,
    clock: Arc<ClockEpoch>,
    functions: &'static [Function],
    /// Each function's ID, by function index.
    ids: Vec<FunctionId>,
}

/// Start recording the program described by `functions` (the backend's
/// table, by function index) at the level `BAML_TELEMETRY` names. Does
/// nothing at `off`. Returns the level started, or why recording could not
/// start (the program then runs unrecorded). Call once, before any [`root`].
pub fn start(
    functions: &'static [Function],
    destination: Destination,
) -> Result<&'static str, String> {
    let level = match btel_settings::mode::from_env() {
        Ok(None) => return Ok("off"),
        Ok(Some(level)) => level,
        Err(error) => return Err(format!("BAML_TELEMETRY: {error}")),
    };
    let (code, name) = match level {
        btel_settings::mode::AutoTelemetryLevel::Low => (LOW, "low"),
        btel_settings::mode::AutoTelemetryLevel::Medium => (MEDIUM, "medium"),
        btel_settings::mode::AutoTelemetryLevel::High => (HIGH, "high"),
    };
    let allocator = FunctionIdAllocator::default();
    let ids = functions
        .iter()
        .map(|_| allocator.allocate().map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let table = Arc::new(FunctionMetadataTable {
        functions: functions
            .iter()
            .zip(&ids)
            .map(|(function, id)| metadata(function, *id))
            .collect(),
    });
    let (runtime, delivery) = open(destination, table)?;
    let clock = ClockRuntime::new(btel_settings::clock::DEFAULT_MODE).start_run();
    RECORDING
        .set(Recording {
            runtime,
            delivery,
            clock,
            functions,
            ids,
        })
        .map_err(|_| "telemetry already started".to_owned())?;
    LEVEL.store(code, Ordering::Release);
    Ok(name)
}

/// Stop recording and wait until the recording is written: the processor
/// drains, the recording ends and the files are flushed. `Ok` when nothing
/// was recorded or everything was; the error says what failed.
pub fn finish() -> Result<(), String> {
    LEVEL.store(OFF, Ordering::Release);
    let Some(recording) = RECORDING.get() else {
        return Ok(());
    };
    let processed = recording.runtime.finish().map_err(|error| error.0);
    let delivered = match &recording.delivery {
        Some(delivery) => delivery.finish().map_err(|error| error.to_string()),
        None => Err("telemetry recording was disabled at startup".to_owned()),
    };
    processed.and(delivered)
}

/// The recording's metadata for one compiled function.
fn metadata(function: &Function, function_id: FunctionId) -> FunctionMetadata {
    let FunctionNames {
        display_name,
        package_name,
        namespace,
        owner_type,
        parent_function,
        lambda_path,
    } = FunctionNames::from_fqn(function.fqn);
    let sites = u32::try_from(function.sites.len()).unwrap_or(u32::MAX);
    FunctionMetadata {
        function_id,
        fqn: function.fqn.to_owned(),
        display_name,
        source_file: (!function.file.is_empty()).then(|| function.file.to_owned()),
        source_span: Some(SourceSpan {
            file_id: function.file_id,
            start: function.start,
            end: function.end,
        }),
        kind: RuntimeFunctionKind::Compiled,
        origin: RuntimeFunctionOrigin::UserDefined,
        owner_type,
        parent_function,
        lambda_path,
        definition_key: Some(btel_types::DefinitionKey(format!(
            "function:{}",
            function.fqn
        ))),
        package_name,
        namespace,
        argument_layout: None,
        source_map: Some(SourceMap {
            coordinate: SourceCoordinate::CompiledSite,
            extent: sites,
            entries: (0..sites)
                .zip(function.sites)
                .map(|(pc, site)| SourceMapEntry {
                    pc,
                    file_id: function.file_id,
                    start: site.start,
                    end: site.end,
                    line: site.line,
                })
                .collect(),
        }),
        inlined: !function.observed,
    }
}

/// The processor and the file writer, as `bex_engine` starts them for a
/// local-files recording.
fn open(
    destination: Destination,
    functions: Arc<FunctionMetadataTable>,
) -> Result<(Arc<TelemetryRuntime>, Option<Arc<btel_file::LocalDelivery>>), String> {
    let (root, cas) = match destination {
        Destination::Directory(dir) => {
            let cas = dir.join("cas");
            (dir, cas)
        }
        Destination::UserFiles => {
            let home =
                baml_env::baml_home_from(baml_env::os_var("BAML_HOME"), std::env::home_dir());
            let btel = home.join("btel");
            (btel.join("recordings"), btel.join("cas"))
        }
    };
    let id = RecordingId::generate();
    let config = btel_recorder::RecordingConfig::default();
    let transport = btel_settings::transport::ChunkConfig::default();
    config
        .validate_transport(&transport)
        .map_err(str::to_owned)?;
    let mut delivery = None;
    let runtime = TelemetryRuntime::with_publisher_factory(transport, |control| {
        let failure = control.clone();
        let builder = btel_recorder::RecordingBuilder::new(id, config)
            .map_err(std::io::Error::other)?
            .with_function_metadata(functions);
        let writer = btel_file::LocalDelivery::create_with_cas(
            &root,
            &cas,
            id,
            btel_file::LocalDeliveryConfig::default(),
            move |error| failure.disable(btel_processor::RuntimeError(error.to_string())),
        );
        Ok(match writer {
            Ok(writer) => {
                let publisher = btel_file::LocalPublisher::new(builder, writer);
                delivery = publisher.delivery().cloned();
                publisher
            }
            Err(error) => {
                control.disable(btel_processor::RuntimeError(format!(
                    "telemetry recording startup: {error}"
                )));
                btel_file::LocalPublisher::disabled(builder)
            }
        })
    })
    .map_err(|error| format!("telemetry processor startup: {error}"))?;
    Ok((runtime, delivery))
}

// ---------------------------------------------------------------------------
// The per-thread producer state.

// The call site the next call is made from: `(caller index + 1) << 32 |
// site`, zero for the host's call. Kept apart from the rest of the state so
// `site` is one thread-local store.
thread_local! {
    static SITE: Cell<u64> = const { Cell::new(0) };
}

/// What the observed calls on this thread share: the active call path and
/// span, and the interned paths.
struct Producer {
    /// This root call's telemetry thread; `None` outside [`root`].
    thread: Cell<Option<TelemetryId>>,
    /// The innermost observed call's path, and its function (index + 1, or 0).
    active_path: Cell<CallPathId>,
    active_function: Cell<u32>,
    /// The parent of the next span: the innermost observed span, or the thread.
    active_span: Cell<Option<TelemetryId>>,
    /// One slot per callee: the last key it resolved, and the path.
    last: RefCell<Vec<(PathKey, CallPathId)>>,
    paths: RefCell<FxHashMap<PathKey, CallPathId>>,
}

/// A call path's identity, as the VM interns it: parent path, visible caller
/// (index + 1, or 0 for the host), the caller's site and the callee.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
struct PathKey {
    parent: u32,
    caller: u32,
    site: u32,
    callee: u32,
}

thread_local! {
    static PRODUCER: Producer = Producer {
        thread: Cell::new(None),
        active_path: Cell::new(CallPathId::ROOT),
        active_function: Cell::new(0),
        active_span: Cell::new(None),
        last: RefCell::new(Vec::new()),
        paths: RefCell::new(FxHashMap::default()),
    };
}

/// Name the call site the next call is made from: site `site` of the
/// function at index `caller`. Every call site runs this; it is one store.
#[inline]
pub fn site(caller: u32, site: u32) {
    SITE.with(|slot| slot.set((u64::from(caller) + 1) << 32 | u64::from(site)));
}

/// Run one call from the host into the program as a telemetry thread: its
/// span, and the root of every call path under it. Without a recording it
/// is just the call.
pub fn root<R>(call: impl FnOnce() -> Result<R, Thrown>) -> Result<R, Thrown> {
    let Some(recording) = RECORDING.get() else {
        return call();
    };
    if LEVEL.load(Ordering::Acquire) == OFF {
        return call();
    }
    let _scope = recording.runtime.enter();
    let clock = &recording.clock;
    clock.attach_thread();
    let thread = allocate_telemetry_id();
    let started_at = clock.read();
    write_span(
        recording,
        thread,
        SpanRecord::ThreadSpanAnnouncement {
            id: thread,
            parent_id: None,
            spawn_call_path: CallPathId::ROOT,
            started_at,
            clock: Arc::clone(clock),
            name: None,
        },
    );
    let saved = PRODUCER.with(|producer| {
        let saved = (
            producer.thread.replace(Some(thread)),
            producer.active_path.replace(CallPathId::ROOT),
            producer.active_function.replace(0),
            producer.active_span.replace(Some(thread)),
        );
        let mut last = producer.last.borrow_mut();
        if last.len() != recording.functions.len() {
            last.resize(
                recording.functions.len(),
                (PathKey::default(), CallPathId::ROOT),
            );
        }
        saved
    });
    let saved_site = SITE.with(|slot| slot.replace(0));
    let result = call();
    let completed_at = clock.read();
    write_span(
        recording,
        thread,
        SpanRecord::ThreadSpanCompletion {
            id: thread,
            parent_id: None,
            spawn_call_path: CallPathId::ROOT,
            started_at,
            completed_at,
            outcome: outcome(&result),
            clock: Arc::clone(clock),
            name: None,
        },
    );
    PRODUCER.with(|producer| {
        producer.thread.set(saved.0);
        producer.active_path.set(saved.1);
        producer.active_function.set(saved.2);
        producer.active_span.set(saved.3);
    });
    SITE.with(|slot| slot.set(saved_site));
    clock.finish_thread();
    result
}

/// An observed call in progress: what [`Frame::exit`] records and restores.
#[must_use = "an observed call records nothing until `exit`"]
pub struct Frame {
    /// The callee's index + 1; zero when nothing is recorded.
    callee: u32,
    level: u8,
    reentry: bool,
    call_path: CallPathId,
    entered_at: Option<ClockInstant>,
    span: Option<TelemetryId>,
    saved_path: CallPathId,
    saved_function: u32,
    saved_span: Option<TelemetryId>,
    /// The site this call was made from. A builtin that calls back into BAML
    /// (`map`, an `eq` override) names its call site once, before its first
    /// callback; restoring the site when each callback returns keeps every
    /// later callback on the same call path.
    saved_site: u64,
}

impl Frame {
    const NONE: Self = Self {
        callee: 0,
        level: OFF,
        reentry: false,
        call_path: CallPathId::ROOT,
        entered_at: None,
        span: None,
        saved_path: CallPathId::ROOT,
        saved_function: 0,
        saved_span: None,
        saved_site: 0,
    };

    /// Record the call's completion with its outcome, and make its caller's
    /// path active again.
    #[inline]
    pub fn exit<T>(self, result: &Result<T, Thrown>) {
        if self.callee != 0 {
            exit(&self, outcome(result));
        }
    }
}

/// Enter the observed function at index `callee`: resolve its call path,
/// read the clock and make the path active. Nothing below `medium`.
#[inline]
pub fn enter(callee: u32) -> Frame {
    let level = LEVEL.load(Ordering::Relaxed);
    if level < MEDIUM {
        return Frame::NONE;
    }
    enter_observed(callee, level)
}

#[inline(never)]
fn enter_observed(callee: u32, level: u8) -> Frame {
    let Some(recording) = RECORDING.get() else {
        return Frame::NONE;
    };
    let site = SITE.with(Cell::get);
    PRODUCER.with(|producer| {
        let Some(thread) = producer.thread.get() else {
            // A call outside `root`: the host did not enter the program.
            return Frame::NONE;
        };
        let callee_tag = callee + 1;
        let parent = producer.active_path.get();
        let parent_function = producer.active_function.get();
        let caller = u32::try_from(site >> 32).unwrap_or(0);
        let caller_site = u32::try_from(site & u64::from(u32::MAX)).unwrap_or(0);
        // Direct self-recursion from an observed caller reuses the parent's
        // path, flagged as reentry, as the VM does.
        let caller_observed = caller != 0
            && recording
                .functions
                .get(caller as usize - 1)
                .is_some_and(|function| function.observed);
        let reentry = caller_observed && caller == callee_tag && parent_function == callee_tag;
        let call_path = if reentry {
            parent
        } else {
            resolve(
                recording,
                producer,
                thread,
                PathKey {
                    parent: parent.get(),
                    caller,
                    site: caller_site,
                    callee: callee_tag,
                },
            )
        };
        let entered_at = recording.clock.read();
        let saved_span = producer.active_span.get();
        let span = if level >= HIGH {
            let id = allocate_telemetry_id();
            write_span(
                recording,
                thread,
                SpanRecord::FunctionSpanAnnouncement {
                    id,
                    parent_id: saved_span.unwrap_or(thread),
                    call_path,
                    entered_at,
                    captured_inputs: None,
                    captured_type_args: None,
                },
            );
            producer.active_span.set(Some(id));
            Some(id)
        } else {
            None
        };
        producer.active_path.set(call_path);
        producer.active_function.set(callee_tag);
        Frame {
            callee: callee_tag,
            level,
            reentry,
            call_path,
            entered_at: Some(entered_at),
            span,
            saved_path: parent,
            saved_function: parent_function,
            saved_span,
            saved_site: site,
        }
    })
}

/// The call path for `key`: the callee's one-slot cache, then the thread's
/// table, then a new path whose definition is recorded.
fn resolve(
    recording: &Recording,
    producer: &Producer,
    thread: TelemetryId,
    key: PathKey,
) -> CallPathId {
    let slot = key.callee as usize - 1;
    if let Some(&(last, path)) = producer.last.borrow().get(slot)
        && last == key
    {
        return path;
    }
    let path = *producer.paths.borrow_mut().entry(key).or_insert_with(|| {
        let call_path = allocate_call_path_id(&LAST_CALL_PATH_ID);
        let function = |tag: u32| recording.ids[tag as usize - 1];
        write_span(
            recording,
            thread,
            SpanRecord::CallPathDefined {
                call_path,
                parent_call_path: CallPathId::new_non_root(key.parent).unwrap_or(CallPathId::ROOT),
                visible_caller: (key.caller != 0).then(|| function(key.caller)),
                caller_pc: key.site,
                callee: function(key.callee),
                edge: CallPathEdge::Synchronous,
            },
        );
        call_path
    });
    if let Some(entry) = producer.last.borrow_mut().get_mut(slot) {
        *entry = (key, path);
    }
    path
}

#[inline(never)]
fn exit(frame: &Frame, outcome: InvocationOutcome) {
    let Some(recording) = RECORDING.get() else {
        return;
    };
    let exited_at = recording.clock.read();
    PRODUCER.with(|producer| {
        let thread = producer.thread.get();
        if let (Some(thread), Some(entered_at)) = (thread, frame.entered_at) {
            match frame.span {
                Some(id) => write_span(
                    recording,
                    thread,
                    span_completion(
                        id,
                        frame.saved_span.unwrap_or(thread),
                        frame.call_path,
                        entered_at,
                        exited_at,
                        outcome,
                        frame.reentry,
                    ),
                ),
                None if frame.level >= MEDIUM => recording.runtime.write_timing(
                    thread,
                    TimingRecord::FunctionTimingCompletion {
                        call_path: frame.call_path,
                        entered_at,
                        exited_at,
                        await_time: AwaitDuration::ZERO,
                        outcome,
                        reentry: frame.reentry,
                    },
                ),
                None => {}
            }
        }
        producer.active_path.set(frame.saved_path);
        producer.active_function.set(frame.saved_function);
        producer.active_span.set(frame.saved_span);
    });
    SITE.with(|slot| slot.set(frame.saved_site));
}

/// A span's completion: the variant for its outcome and reentry.
fn span_completion(
    id: TelemetryId,
    parent_id: TelemetryId,
    call_path: CallPathId,
    entered_at: ClockInstant,
    exited_at: ClockInstant,
    outcome: InvocationOutcome,
    reentry: bool,
) -> SpanRecord<btel_snapshot::Snapshot, btel_snapshot::Snapshot> {
    macro_rules! completion {
        ($variant:ident) => {
            SpanRecord::$variant {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time: AwaitDuration::ZERO,
                captured_value: None,
            }
        };
    }
    match (outcome, reentry) {
        (InvocationOutcome::Ok, false) => completion!(FunctionSpanCompletionOk),
        (InvocationOutcome::Ok, true) => completion!(FunctionSpanCompletionOkReentry),
        (InvocationOutcome::Errored, false) => completion!(FunctionSpanCompletionErrored),
        (InvocationOutcome::Errored, true) => completion!(FunctionSpanCompletionErroredReentry),
        (InvocationOutcome::Cancelled, false) => completion!(FunctionSpanCompletionCancelled),
        (InvocationOutcome::Cancelled, true) => completion!(FunctionSpanCompletionCancelledReentry),
        (InvocationOutcome::Panicked, false) => completion!(FunctionSpanCompletionPanicked),
        (InvocationOutcome::Panicked, true) => completion!(FunctionSpanCompletionPanickedReentry),
    }
}

/// Span records go out with an explicitly empty execution context, as the
/// VM's do when no context is set.
fn write_span(
    recording: &Recording,
    thread: TelemetryId,
    record: SpanRecord<btel_snapshot::Snapshot, btel_snapshot::Snapshot>,
) {
    recording
        .runtime
        .write_span_with_context(thread, ContextReference::Empty, None, record);
}

/// A call's outcome as the VM classifies it: an `Exit` panic ends the run
/// (cancelled), any other panic is `Panicked`, a thrown error `Errored`.
fn outcome<T>(result: &Result<T, Thrown>) -> InvocationOutcome {
    match result {
        Ok(_) => InvocationOutcome::Ok,
        Err(Thrown::Panic(Panic::Exit { .. })) => InvocationOutcome::Cancelled,
        Err(Thrown::Panic(_)) => InvocationOutcome::Panicked,
        Err(Thrown::Error(_)) => InvocationOutcome::Errored,
    }
}
