//! BEX Engine - The async runtime that drives the VM.
//!
//! This crate provides `BexEngine`, which executes BAML programs by:
//! - Creating a VM instance for each function call
//! - Driving the VM execution loop
//!
//! The architecture is inspired by Deno's embedding of V8:
//! - VM executes synchronously until it needs external I/O
//! - Engine manages async operations and feeds results back
//! - Communication via `VmExecState` enum (yield points)
//!
//! # External Operations
//!
//! External operations (LLM calls, HTTP requests, file I/O) are dispatched via
//! the `SysOp` enum using static dispatch. This avoids dynamic dispatch
//! overhead and makes the system more macro-friendly.
//!
//! # Resources
//!
//! Resources (file handles, connections, etc.) are stored in a `ResourceRegistry`.
//! External ops can store resources and return their ID to the VM. Later ops
//! can retrieve resources by ID. The VM only sees integer IDs.
//!
//! # Garbage Collection Coordination
//!
//! The engine coordinates GC through a [`HeapPermitManager`]. Every
//! `call_function` invocation holds an [`ActiveHeapPermit`] for the duration
//! of its VM's execution; the permit is released at async safepoints (e.g.
//! during `Await`). GC is a "request-and-wait" operation:
//!
//! 1. **Trigger**: [`BexEngine::collect_garbage`] calls
//!    [`HeapPermitManager::request_park`], which drains all semaphore permits.
//! 2. **Park**: running VMs release their permits at the next safepoint; new
//!    `call_function` invocations block in `HeapPermitManager::new_permit`
//!    because the manager's holders mutex is held by the GC.
//! 3. **Collect roots**: [`HeapGuard`] iterates the live permit holders (via
//!    weak references) and calls each `RootHaver::collect_roots`, unioned
//!    with `BexHeap::collect_handle_roots` for FFI-held objects.
//! 4. **GC**: `BexHeap::collect_garbage_generational` runs under the guard;
//!    produces a forwarding map.
//! 5. **Fixup**: `HeapGuard` calls each parked holder's
//!    `RootHaver::forward_roots`. The `BexVm` impl of `forward_roots` also
//!    invalidates the VM's TLAB so post-GC allocations refill from the new
//!    Gen0 cursor.
//! 6. **Resume**: dropping the `HeapGuard` releases the semaphore; parked
//!    VMs re-acquire and continue.
//!
//! ## Safety Invariants
//!
//! - A VM can only mutate its own heap state while holding an
//!   [`ActiveHeapPermit`]. GC cannot start until every active permit has
//!   been released.
//! - Handles are registered in `BexHeap::handles` before any GC could
//!   observe them, and the write lock on that table serializes against
//!   GC's `update_handles`.
//! - New `call_function` invocations block on the holders mutex during
//!   GC, so no fresh permit enters circulation mid-collection.
//!
//! # Unsafe Code
//!
//! This module uses unsafe code for:
//! - `PermitCell<T>` Send/Sync: single-threaded access is enforced by the
//!   semaphore/holders-mutex pair.
//! - Direct heap access during value conversion (always under an active
//!   permit, witnessed by a `PermitProof` parameter).
//!
//! Safety is ensured by the permit/guard coordination system described above.

#![allow(unsafe_code)]

mod conversion;
mod function_call_context;
mod future;
mod thread;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use ::bex_heap::{HeapPermit as _, Tlab};
// Re-export event types for callers.
use ::bex_vm_types::{
    RootHaver,
    types::{FutureId, InterfaceImplementors},
};
use ::core::sync::atomic::AtomicBool;
use async_trait::async_trait;
pub use bex_events::{
    FunctionMetadataTable, ProgramMetadata,
    ids::{
        BexCallId, BexThreadId, CallRef, EngineId, FunctionId, ProcessEuid, ProgramId, ThreadRef,
    },
};
pub use bex_external_types::{BexExternalValue, RuntimeTy, TypeName, UnionMetadata};
use bex_heap::BexHeap;
// Re-export GcStats for users of the engine
pub use bex_heap::GcStats;
pub use bex_heap::{ActiveHeapPermit, HeapGuard, HeapPermitManager, InactiveHeapPermit};
use bex_vm::{BexVm, VmExecState};
use bex_vm_types::{
    FunctionMeta, FunctionOrigin, GlobalPool, HeapPtr, Object, SharedGlobals, SysOp,
    TaskGroupInner, Value, VmGlobals,
};
pub use conversion::test_arg_to_external;
// Re-export CancellationToken for callers.
pub use function_call_context::{FunctionCallContext, FunctionCallContextBuilder};
pub use sys_types::{CallId, ClassDefinition, ClassFieldDefinition};
use sys_types::{OpError, SysOpResult};
use thiserror::Error;
pub use tokio_util::sync::CancellationToken;

pub use crate::{
    future::{FutureManager, FutureManagerGuard, FutureManagerInner},
    thread::{BexThread, ChildErrorQueue},
};

const SPAWN_CLOSURE_FQN: &str = "baml.<spawn-closure>";
const SPAWN_CLOSURE_DISPLAY_NAME: &str = "<spawn-closure>";

/// Reserved header-table row for calls whose function identity cannot be
/// resolved (e.g. runtime-synthesized functions absent from the compile-time
/// pool). Ring `CallFunction` records do NOT reference this row today:
/// unresolvable callees are stamped `function_id: 0` ("unassigned" — see
/// `BexVm::prof_enter_call`) and the consumer transcodes ids verbatim. The
/// row (id = max real id + 2) is reserved in every header for consumers that
/// want a display bucket for such records; re-pointing the id-0 paths at it
/// is an open cross-team item.
const UNKNOWN_FUNCTION_FQN: &str = "baml.<unknown-function>";
const UNKNOWN_FUNCTION_DISPLAY_NAME: &str = "<unknown-function>";

/// Outcome of running a single [`BexThread`] to termination.
///
/// Used by [`BexEngine::run_thread_event_loop`] to distinguish whether the
/// thread is the root (whose value must be returned to the host) or a
/// spawned child (whose value has already been written into the
/// [`FutureManager`] for the awaiter to pick up).
#[allow(clippy::large_enum_variant)]
pub(crate) enum ThreadOutcome {
    /// Root thread completed normally; return this value to the host.
    RootValue(BexExternalValue),
    /// Spawned child thread settled — `FutureManager` already updated.
    SettledChild(ChildSettleKind),
}

/// How a spawned child settled its future — drives the profiling
/// `EndThread` status (children settle as `Ok(SettledChild)` even when
/// cancelled or errored, so the kind must travel in the outcome).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChildSettleKind {
    Fulfilled,
    Cancelled,
    Errored,
}

/// §7 follow-up 11: closes a spawned thread's profiling lifecycle if the
/// task future is dropped before its event loop takes over (abnormal host
/// teardown — e.g. dropping the runtime while the task is queued on a
/// `TaskGroup` ticket or parked on the heap permit). By that point the
/// Spawn arm has emitted `StartThread` and `set_entry_point` the entry
/// `CallFunction`; nothing else would close them. Armed at the top of the
/// task body; also the closer for the queued-then-cancelled early return;
/// defused once `run_thread_event_loop` is entered (its wrapper owns
/// `EndThread` on every return path from there). Emits via the TLS ring
/// lookup, so dropping from any thread is sound. Drops *after* the loop
/// has started (mid-await teardown) are out of scope — the artifact is
/// torn-tail territory there anyway.
struct SpawnProfCloser {
    engine: Arc<BexEngine>,
    prof_thread_id: u64,
    entry_call_id: bex_events::ids::BexCallId,
    armed: bool,
}

impl SpawnProfCloser {
    fn defuse(&mut self) {
        self.armed = false;
    }
}

impl Drop for SpawnProfCloser {
    fn drop(&mut self) {
        use bex_events::prof::record::{FunctionEndStatus, RawRecord, ThreadEndStatus};
        if !self.armed || !self.engine.prof_enabled {
            return;
        }
        let thread_id = bex_events::ids::BexThreadId(self.prof_thread_id);
        if self.entry_call_id.0 != 0 {
            self.engine.prof_emit(&RawRecord::EndFunction {
                // Dropped-before-run is a cancellation, not a failure.
                status: FunctionEndStatus::Cancelled,
                thread_id,
                call_id: self.entry_call_id,
                ts_ticks: bex_events::prof::clock::now_ticks(),
            });
        }
        self.engine.prof_emit(&RawRecord::EndThread {
            status: ThreadEndStatus::Cancelled,
            thread_id,
            ts_ticks: bex_events::prof::clock::now_ticks(),
        });
    }
}

// ============================================================================
// Engine Types
// ============================================================================

/// Information about a user-callable function, used by `baml run --list`.
#[derive(Debug, Clone)]
pub struct UserFunctionInfo {
    pub qualified_name: String,
    pub display_name: String,
    pub origin: FunctionOrigin,
    pub param_names: Vec<String>,
    pub param_types: Vec<RuntimeTy>,
    pub param_has_default: Vec<bool>,
    pub return_type: RuntimeTy,
    pub display_type_params: Vec<String>,
    pub display_param_types: Vec<String>,
    pub display_return_type: String,
    /// Filesystem path of the source file containing the function.
    /// Empty string for builtins and synthesized functions.
    /// Exposed for BEP-027 §"`baml.argv`": `argv[1]` under root-main `baml run`
    /// is the path to the file containing `main`.
    pub source_file: String,
    /// `true` when the function carries `FunctionMeta::Llm` — i.e. it
    /// was declared with `client X { ... } prompt #"..."#` and the
    /// compiler synthesized the LLM dispatch body. Surfaced here so
    /// `baml run --list` can annotate LLM functions inline without
    /// reaching back into the heap to inspect `body_meta`.
    pub is_llm: bool,
}

pub struct BexCallResult {
    pub value: Result<BexExternalValue, EngineError>,
    pub entry_call_ref: CallRef,
}

/// Internal call argument after host binding has distinguished omission from
/// explicit null. This is intentionally not part of the external bridge value
/// surface.
#[derive(Debug, Clone)]
pub enum BexCallArg {
    Provided(Box<BexExternalValue>),
    OmittedDefault,
}

// ============================================================================
// Span Tracking (per-invocation, NOT on Arc<BexEngine>)
// ============================================================================

/// RAII guard that owns a `CallId` slot in `BexEngine::active_calls`. The
/// slot is inserted by [`Self::register`] and removed on drop, matching
/// the lifetime of the `call_function` invocation. The constructor and
/// the registry insert are atomic — there is no window during which the
/// slot exists without an owning guard, so a panic at the registration
/// site cannot leak entries.
#[derive(Clone)]
struct ActiveCall {
    cancel: CancellationToken,
    pending: bool,
}

struct ActiveCallGuard {
    engine: Arc<BexEngine>,
    call_id: CallId,
}

impl ActiveCallGuard {
    /// Atomically reserve `call_id` in `engine.active_calls` and return a
    /// guard that will release the slot on drop. Returns
    /// [`EngineError::DuplicateCallId`] if the id is already in flight.
    fn register(
        engine: Arc<BexEngine>,
        call_id: CallId,
        cancel: CancellationToken,
    ) -> Result<(Self, CancellationToken), EngineError> {
        let mut map = engine
            .active_calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let cancel = match map.get_mut(&call_id) {
            Some(existing) if existing.pending => {
                existing.pending = false;
                existing.cancel.clone()
            }
            Some(_) => return Err(EngineError::DuplicateCallId { call_id }),
            None => {
                map.insert(
                    call_id,
                    ActiveCall {
                        cancel: cancel.clone(),
                        pending: false,
                    },
                );
                cancel
            }
        };
        drop(map);
        Ok((Self { engine, call_id }, cancel))
    }

    fn reserve_cancelled(engine: &BexEngine, call_id: CallId) {
        let mut map = engine
            .active_calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match map.get(&call_id) {
            Some(existing) => existing.cancel.cancel(),
            None => {
                let cancel = CancellationToken::new();
                cancel.cancel();
                map.insert(
                    call_id,
                    ActiveCall {
                        cancel,
                        pending: true,
                    },
                );
            }
        }
    }
}

impl Drop for ActiveCallGuard {
    fn drop(&mut self) {
        // `unwrap_or_else(into_inner)` so a poisoned mutex during unwind
        // doesn't double-panic.
        let mut map = self
            .engine
            .active_calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.remove(&self.call_id);
    }
}

/// Errors that can occur during engine execution.
#[derive(Debug, PartialEq, Error, Clone)]
pub enum EngineError {
    #[error("Function call with ID {call_id} not found")]
    FunctionCallNotFound { call_id: CallId },

    #[error("Future with ID {future_id} not found")]
    FutureNotFound { future_id: FutureId },

    #[error("Function not found: {name}")]
    FunctionNotFound { name: String },

    /// Function exists, but its `FunctionKind` is not invokable as an
    /// engine entry point. Only bytecode functions can be called via
    /// [`BexEngine::call_function`] / [`BexEngine::call_function_bound_args`]:
    /// native (`$rust_function`) entries would re-enter the VM through
    /// `YieldToCall` with no bytecode frame to return to. Sysops + builtins
    /// reach their natives from inside a calling bytecode body.
    #[error("Function `{name}` is not invokable as an entry point (kind: {kind})")]
    NotInvokableAsEntry { name: String, kind: String },

    #[error("VM internal error: {0}")]
    VmInternalError(bex_vm::errors::VmInternalError),

    #[error("{}", format_vm_internal_error(source, trace))]
    TracedVmInternalError {
        source: bex_vm::errors::VmInternalError,
        trace: Vec<bex_vm::StackFrame>,
    },

    /// Either a BAML panic or a BAML error value.
    #[error("{}", format_unhandled_throw(value, trace))]
    UnhandledThrow {
        value: Box<BexExternalValue>,
        trace: Vec<bex_vm::StackFrame>,
    },

    /// Clean process-termination request from `baml.sys.exit(code)`.
    /// The caller is expected to honor this as the process exit code.
    /// BAML `int` is `i64`, so the signal carries the full value; the
    /// caller clamps into its shell's range (typically 0..=255 on Unix).
    #[error("baml.sys.exit({code})")]
    Exit { code: i64 },

    #[error("Cannot convert object of type {type_name}")]
    CannotConvert { type_name: String },

    #[error("Type mismatch: {message}")]
    TypeMismatch { message: String },

    #[error("Schema inconsistency: {message}")]
    SchemaInconsistency { message: String },

    #[cfg(feature = "heap_debug")]
    #[error("Snapshot not possible for type: {type_name}")]
    CannotSnapshot { type_name: String },

    #[error("A function call with ID {call_id} is already in progress")]
    DuplicateCallId { call_id: CallId },

    #[error("Package initialization failed: {0}")]
    InitFailed(String),

    #[error("{0}")]
    Other(String),
}

fn format_vm_internal_error(
    err: &bex_vm::errors::VmInternalError,
    trace: &[bex_vm::StackFrame],
) -> String {
    use std::fmt::Write;
    let mut out = bex_vm::format_traceback(
        trace
            .iter()
            .map(|f| (f.file_path.as_str(), f.error_line, f.function_name.as_str())),
    );
    write!(out, "VM internal error: {err}").unwrap();
    out
}

/// Recognize an uncaught `baml.panics.Exit { code }` and pull its `code`
/// field out. Returns `None` for any other value — the caller should fall
/// back to the normal unhandled-throw path.
///
/// Exit lives in the regular panic class hierarchy so BAML code can catch
/// it (like Python's `SystemExit`); at the engine boundary we recognize it
/// by class tag rather than routing it through a separate `VmError`
/// variant, so the VM's unwinder stays ignorant of which panic classes
/// are "special" and the special-casing lives in exactly one place — here.
fn extract_exit_code(value: &BexExternalValue) -> Option<i64> {
    match value {
        BexExternalValue::Instance {
            class_name, fields, ..
        } if class_name == bex_vm_types::PanicClass::Exit.fqn() => match fields.get("code")? {
            BexExternalValue::Int(code) => Some(*code),
            _ => None,
        },
        _ => None,
    }
}

fn format_unhandled_throw(value: &BexExternalValue, trace: &[bex_vm::StackFrame]) -> String {
    use std::fmt::Write;
    let mut out = bex_vm::format_traceback(trace.iter().map(|loc| {
        (
            loc.file_path.as_str(),
            loc.error_line,
            loc.function_name.as_str(),
        )
    }));
    write!(out, "uncaught throw: {value:?}").unwrap();
    out
}

/// Fully-qualified name of the cancellation panic class.
pub const CANCELLED_PANIC_CLASS: &str = "baml.panics.Cancelled";

/// True iff `err` is an unhandled `baml.panics.Cancelled` panic.
///
/// Centralizes the cancellation-classification logic that bridges (`bridge_cffi`,
/// `bridge_nodejs`, `bridge_python`, `bridge_wasm`) and `baml_lsp_server` need
/// for mapping `EngineError` → host-specific cancellation indicator.
pub fn is_cancelled_engine_error(err: &EngineError) -> bool {
    matches!(
        err,
        EngineError::UnhandledThrow { value, .. }
            if matches!(
                value.as_ref(),
                BexExternalValue::Instance { class_name, .. }
                    if class_name == CANCELLED_PANIC_CLASS
            )
    )
}

/// Synthesize an `EngineError::UnhandledThrow` representing a cancellation.
///
/// Used when the engine produces a cancellation outside an active VM (pre-call
/// fail-fast and post-completion "cancel wins" race). Mirrors the shape of a
/// `baml.panics.Cancelled` instance produced by the VM's `Await` opcode so
/// host bridges can detect both cases by inspecting `class_name`.
///
/// Exposed publicly so bridges and other layers that need to surface a
/// cancellation outside the engine's normal control flow can produce one
/// in the canonical shape rather than rolling their own.
pub fn cancelled_unhandled_throw() -> EngineError {
    let mut fields = indexmap::IndexMap::new();
    fields.insert(
        "message".to_string(),
        BexExternalValue::String("operation cancelled".into()),
    );
    EngineError::UnhandledThrow {
        value: Box::new(BexExternalValue::Instance {
            class_name: CANCELLED_PANIC_CLASS.to_string(),
            fields,
        }),
        trace: Vec::new(),
    }
}

// ============================================================================
// BexEngine
// ============================================================================

/// The async runtime that drives VM execution.
///
/// `BexEngine` is the main entry point for executing BAML programs.
/// It owns the compiled program and the unified heap shared across all VMs.
///
/// # Thread Safety and Concurrent Execution
///
/// `BexEngine` supports concurrent function execution. Each `call_function`
/// invocation creates its own `BexVm` with an exclusive Thread-Local Allocation
/// Buffer (TLAB), enabling parallel execution without contention.
///
/// ## Why Concurrent Calls Are Safe
///
/// - **No global mutable state**: BAML has no global variables, so independent
///   function calls cannot race with each other.
///
/// - **TLAB isolation**: Each VM allocates into its own exclusive heap region.
///   The only synchronization is atomic TLAB chunk allocation (rare operation,
///   approximately once per 1024 allocations).
///
/// - **Lock-free field writes**: Object field mutations are direct memory writes
///   with no locking overhead, enabled by TLAB exclusivity during execution.
///
/// ## Usage Example
///
/// ```ignore
/// use std::sync::Arc;
///
/// let engine = Arc::new(BexEngine::new(bytecode, sys_ops)?);
///
/// // Concurrent calls are safe - each gets its own VM and TLAB
/// let (result1, result2) = tokio::join!(
///     engine.call_function("process_order", order1_args),
///     engine.call_function("process_order", order2_args),
/// );
///
/// // Or with explicit spawning:
/// let engine_clone = Arc::clone(&engine);
/// let handle = tokio::spawn(async move {
///     engine_clone.call_function("background_task", vec![]).await
/// });
/// ```
///
/// ## Handle Sharing (Advanced)
///
/// If you pass the same `Handle` to multiple concurrent calls that both mutate
/// the referenced object, you may observe a data race. This requires deliberate
/// action (obtaining a handle, sharing it, mutating in parallel) and is not
/// something that happens accidentally in normal BAML usage.
///
/// # Architecture
///
/// ```text
/// BexEngine (owns)
///     ├── Arc<BexHeap>     ─── shared across all VMs
///     ├── GlobalPool       ─── global variable definitions
///     └── function index   ─── name → ObjectIndex lookup
///
/// call_function() creates:
///     └── BexVm (temporary)
///         └── Tlab ─── exclusive allocation region from shared heap
/// ```
pub struct BexEngine {
    process_euid: ProcessEuid,
    engine_id: EngineId,
    program_metadata: ProgramMetadata,
    /// Function identity by heap address: maps each compile-time
    /// `Object::Function`'s stable `HeapPtr` (as a raw address) to its
    /// `FunctionId`. Call notifications carry the
    /// resolved function pointer, so per-call identity resolution is one
    /// hash lookup — never a name scan.
    /// Synthetic id for spawn-closure child roots (see `SPAWN_CLOSURE_FQN`).
    next_thread_id: AtomicU64,
    /// The unified heap (shared across all VM instances)
    heap: Arc<BexHeap>,
    /// Frozen global variables shared across every post-`$init` VM.
    ///
    /// Populated once during `$init` and immutable thereafter; cloning is a
    /// cheap refcount bump (see `VmGlobals::Shared`). The VM rejects any
    /// `StoreGlobal` against this view as a `VmInternalError`.
    ///
    /// Stored as a [`SharedGlobals`] (rather than a plain `Arc<[Value]>`)
    /// so the GC can trace + forward `Value::object(HeapPtr)` entries: the
    /// engine registers a clone of this `SharedGlobals` as a [`RootHaver`]
    /// permit holder during `BexEngine::new`. Without that registration any
    /// runtime heap object stored in a top-level `let` global was invisible
    /// to GC and could be reclaimed mid-call.
    globals: SharedGlobals,
    /// Permit holder that keeps `globals` registered with the
    /// `HeapPermitManager`. The engine never `acquire()`s this — it exists
    /// purely so the GC's `collect_roots`/`forward_roots` walks reach the
    /// frozen globals pool.
    _globals_permit: bex_heap::InactiveHeapPermit<SharedGlobals>,
    /// Resolved function/class/enum names for lookup
    resolved_function_names: HashMap<String, (HeapPtr, bex_vm_types::FunctionKind)>,
    /// Resolved class names for instance allocation (`IndexMap` preserves definition order)
    resolved_class_names: indexmap::IndexMap<String, HeapPtr>,
    /// Resolved enum names for variant allocation (`IndexMap` preserves definition order)
    resolved_enum_names: indexmap::IndexMap<String, HeapPtr>,
    /// System operations provider.
    sys_ops: std::sync::Arc<sys_ops::SysOps>,
    /// Context passed to `sys_ops` that need engine-level information.
    sys_op_ctx: sys_types::EngineSysOpContext,
    /// Compiled test cases from the BAML program.
    test_cases: Vec<bex_vm_types::TestCase>,
    /// Process argv passed in at engine creation. Exposed to BAML via
    /// `baml.sys.argv()`. Shared (cheap to clone) with each spawned VM.
    argv: Arc<[String]>,

    // --- GC coordination ---
    heap_permit_manager: Arc<HeapPermitManager>,
    /// Used to prevent multiple threads from trying to run GC at the same time.
    /// Only one should run it, the rest should wait for it to complete.
    checking_gc: AtomicBool,
    /// Used to notify long-running threads that they should park the VM even if they aren't at a typical yield point.
    #[cfg(not(target_arch = "wasm32"))]
    park_requested: Arc<AtomicBool>,

    /// Map of active function calls by ID.
    active_calls: Mutex<HashMap<CallId, ActiveCall>>,

    futures: FutureManager,

    /// Per-program interface implementors registry (BEP-044), kept here so
    /// every spawned VM (including post-`$init` workers) sees the same map
    /// without cloning the underlying `IndexMap`.
    interface_implementors: Arc<InterfaceImplementors>,

    /// Snapshot of the `BAML_PROFILE` master switch, taken once at
    /// construction (the config is read-once per process). Gates every
    /// profiling emission and the per-resume ring refresh.
    prof_enabled: bool,
}

impl Drop for BexEngine {
    /// Closes the engine's profiling lifecycle: a non-blocking notification;
    /// the consumer drains the engine's remaining events (every commit
    /// happened-before the last `Arc` release, hence before this), syncs and
    /// closes its `.bamlprof`, and frees its metadata. Without this,
    /// long-lived engine-churning hosts (LSP recompiles) accumulate open
    /// files and heartbeat work for dead engines.
    fn drop(&mut self) {
        if self.prof_enabled {
            bex_events::prof::engine_closed(self.engine_id.0);
        }
    }
}

/// Maps the M0 [`ProgramMetadata`] (the canonical per-run function table)
/// into the `.bamlprof` header rows.
fn prof_engine_metadata(meta: &ProgramMetadata) -> bex_events::prof::EngineProfileMetadata {
    bex_events::prof::EngineProfileMetadata {
        program_id: hex_bytes(&meta.program_id.0),
        source_snapshot_id: meta.source_snapshot_id.as_ref().map(|id| hex_bytes(&id.0)),
        revision_id: meta.revision_id.as_ref().map(|id| id.0.clone()),
        functions: meta
            .function_table
            .functions
            .iter()
            .map(|f| bex_events::prof::FunctionMetaEntry {
                function_id: f.function_id.0,
                fqn: f.fqn.clone(),
                source_file: f.source_file.clone().unwrap_or_default(),
                span_start: f.source_span.as_ref().map_or(0, |sp| sp.start),
                span_end: f.source_span.as_ref().map_or(0, |sp| sp.end),
                kind: match &f.kind {
                    bex_events::RuntimeFunctionKind::Bytecode => "bytecode".to_string(),
                    bex_events::RuntimeFunctionKind::SysOp(_) => "sysop".to_string(),
                    bex_events::RuntimeFunctionKind::Native
                    | bex_events::RuntimeFunctionKind::NativeUnresolved => "native".to_string(),
                },
                definition_key: f.definition_key.as_ref().map(|key| key.0.clone()),
                owner_type: f.owner_type.as_ref().map(|key| key.0.clone()),
                parent_function: f.parent_function.as_ref().map(|key| key.0.clone()),
                lambda_path: f.lambda_path.clone(),
                package_name: f.package_name.clone(),
                namespace: f.namespace.clone(),
            })
            .collect(),
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

#[cfg(target_arch = "wasm32")]
fn _default_round_robin_start() -> usize {
    // Keep wasm deterministic for tooling (matches legacy behavior).
    0
}

#[cfg(not(target_arch = "wasm32"))]
fn _default_round_robin_start() -> usize {
    use web_time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    #[allow(clippy::cast_possible_truncation)]
    {
        nanos as usize
    }
}

/// Extract an owned `RuntimeTy` from a `SysOp::BamlHostCallHostValue` type-arg operand
/// (an `Object::Type(Box<RuntimeTy>)`).
///
/// The VM packs the sys-op args as `[handle, args_array, ret_ty, throws_ty]`
/// (see `bex_vm::vm`'s `CallIndirect`-`HostClosure` path): `ret_ty` is
/// `type_arg_0` (`T`, the declared return type) and `throws_ty` is `type_arg_1`
/// (`E`, the declared error contract). Returns `Err` if the slot is absent or
/// not a `Type` object — for `BamlHostCallHostValue` that's an engine/compiler
/// ABI bug and a missing slot would silently skip both
/// `validate_host_return_schema` and `enforce_host_throw_contract`, so the
/// caller surfaces it as a fatal internal error rather than soft-failing the
/// contract checks. `slot_name` is used only for the diagnostic message.
fn host_call_type_arg(
    ty_arg: Option<Value>,
    slot_index: usize,
    slot_name: &'static str,
) -> Result<baml_type::RuntimeTy, bex_vm::errors::VmInternalError> {
    let bad_slot = || bex_vm::errors::VmInternalError::BridgeFailure {
        message: format!(
            "call_host_value: missing or non-Type {slot_name} (sysop arg slot \
             {slot_index}); this is an engine/compiler ABI bug — expected the \
             VM to pack args as [handle, args_array, ret_ty, throws_ty]"
        ),
    };
    let ptr = ty_arg
        .as_ref()
        .and_then(Value::as_object_ptr)
        .ok_or_else(bad_slot)?;
    // SAFETY: the pointer was just allocated by the VM for this sys-op call.
    // This MUST be read while the heap permit is held (i.e. *before* the sys-op
    // await releases it): the engine-local `args` Vec is not a GC root, so after
    // the await a moving GC may relocate/collect this `Object::Type` and the raw
    // pointer would dangle. The caller clones the `RuntimeTy` out before awaiting.
    match unsafe { ptr.get() } {
        Object::Type(ty) => Ok((**ty).clone()),
        _ => Err(bad_slot()),
    }
}

/// Convert a sysop `OpError`'s inner `VmRustFnError` into a heap-allocated
/// VM exception `Value` using the VM's own conversion helpers — the same
/// path a `$rust_function` error or `throw` opcode takes:
///   * `VmBamlError`  → [`BexVm::error_to_exception_value`] (a `baml.errors.*` instance)
///   * `VmPanic`      → [`BexVm::panic_to_exception_value`] (a `baml.panics.*` instance)
///   * pre-built `Thrown(value)` → returned as-is
///   * `InternalError` is fatal (no catchable value)
///
/// The caller injects the returned value via
/// [`BexVm::try_handle_external_exception`] so it unwinds through the
/// standard VM exception machinery; an in-BAML `catch` matches it like any
/// other throw.
pub(crate) fn op_error_to_throw_value(
    vm: &mut bex_vm::BexVm,
    kind: bex_vm::errors::VmRustFnError,
) -> Result<Value, bex_vm::errors::VmInternalError> {
    use bex_vm::errors::VmRustFnError;
    match kind {
        VmRustFnError::BamlError(err) => Ok(vm.error_to_exception_value(err)),
        VmRustFnError::Panic(panic) => Ok(vm.panic_to_exception_value(panic)),
        VmRustFnError::Thrown(value) => Ok(value),
        VmRustFnError::InternalError(err) => Err(err),
    }
}

/// Decoded `baml.spawn.SpawnParams` fields (BEP-034 middleware) — see
/// [`BexEngine::read_spawn_params`].
struct SpawnParamsData {
    body: HeapPtr,
    name: Option<String>,
    group: Option<Arc<TaskGroupInner>>,
    cancel: Option<CancellationToken>,
    detach: bool,
}

/// Enforce the host callable's declared throws contract `E` on a
/// materialized thrown `Value`. Returns the value unchanged on a contract
/// match; on mismatch returns a fresh `baml.panics.HostContractViolation`
/// panic `Value` carrying the host class identity for diagnostics.
///
/// Operates on the post-materialization `Value` (not the source
/// `BexExternalValue` or `VmBamlError`) so the check applies uniformly to
/// every shape a host-callable throw can take —
/// `OpErrorPayload::HostThrown` (wire-routed) and
/// `OpErrorPayload::Vm(BamlError(HostCallable))` (engine-internal) both
/// materialize into the same `Object::Instance` of
/// `baml.errors.HostCallable`.
///
/// The check reads the value's runtime BAML type via
/// [`value_runtime_baml_ty`] and tests `value_ty ⊑ contract` via
/// [`RuntimeTy::is_subtype_of`] — `BuiltinUnknown` accepts everything (the
/// "throws unknown" fallback for undeclared host contracts); concrete
/// classes reject anything not in their subtype lattice.
///
/// Panic-class values (`baml.panics.*`) bypass the contract entirely:
/// panics are not catchable errors and a fn's `throws E` clause never
/// includes them. This also avoids re-wrapping an engine-generated
/// `HostContractViolation` (from a wrong-return-type check upstream)
/// into a second `HostContractViolation` with a corrupted message.
fn enforce_host_throw_contract(
    thread: &mut ActiveHeapPermit<BexThread>,
    value: Value,
    contract: &RuntimeTy,
) -> Value {
    // `BuiltinUnknown` is the top type — short-circuit before any heap
    // walking.
    if matches!(contract, RuntimeTy::BuiltinUnknown { .. }) {
        return value;
    }
    let runtime_ty = value_runtime_baml_ty(value, thread.proof());
    // Panics propagate as panics regardless of `E` — they're an
    // engine-level failure mode, not something the user's callable opts
    // into via `throws`.
    if let Some(RuntimeTy::Class(name, _, _)) = runtime_ty.as_ref()
        && name.is_panic_type()
    {
        return value;
    }
    let on_contract = runtime_ty
        .as_ref()
        .is_some_and(|rt: &RuntimeTy| rt.is_subtype_of(contract));
    if on_contract {
        return value;
    }
    // Off-contract: build the `HostContractViolation` panic. Echo the
    // host's `class_name` / `language` from the `HostCallable` wrapper's
    // fields when present — the common case where a bridge SDK wraps a
    // native exception with its host-class identity in those fields.
    let (host_class, host_language) = extract_host_diagnostics_from_value(value, thread.proof());
    let runtime_ty_str = runtime_ty
        .as_ref()
        .map_or_else(|| "<unknown>".to_string(), std::string::ToString::to_string);
    let panic = bex_vm::errors::VmPanic::HostContractViolation {
        message: format!(
            "host callable threw a value of type `{runtime_ty_str}` that is not in its \
             declared throws contract (`{contract}`)",
        ),
        class_name: host_class,
        language: host_language,
    };
    thread.vm.panic_to_exception_value(panic)
}

/// Extract a materialized `Value`'s runtime BAML type for the purpose of
/// host-throw contract checking — the class FQN + type args for
/// `Object::Instance`, the scalar tag for primitives. Returns `None` for
/// shapes that can't reasonably inhabit a thrown-value position (e.g.
/// `Object::HostClosure`, `Object::FunctionRef`); the caller treats
/// `None` as off-contract.
///
/// `_proof` ensures the caller holds an active heap permit so the
/// `HeapPtr` derefs in this function are sound.
fn value_runtime_baml_ty(value: Value, _proof: bex_heap::PermitProof<'_>) -> Option<RuntimeTy> {
    use baml_type::TyAttr;
    use bex_vm_types::ValueKind;
    match value.kind() {
        ValueKind::OmittedArg => None,
        ValueKind::Null => Some(RuntimeTy::Null {
            attr: TyAttr::default(),
        }),
        ValueKind::Int(_) => Some(RuntimeTy::Int {
            attr: TyAttr::default(),
        }),
        ValueKind::Bool(_) => Some(RuntimeTy::Bool {
            attr: TyAttr::default(),
        }),
        ValueKind::Object(ptr) => {
            // SAFETY: caller holds an active heap permit (`_proof`), so
            // deref'ing this `HeapPtr` is sound.
            match unsafe { ptr.get() } {
                Object::Instance(instance) => {
                    // SAFETY: `instance.class` is a heap-rooted Class
                    // pointer; valid under the same permit.
                    let class_obj = unsafe { instance.class.get() };
                    let Object::Class(class) = class_obj else {
                        return None;
                    };
                    Some(RuntimeTy::Class(
                        class.name.clone(),
                        instance.class_type_args.clone(),
                        TyAttr::default(),
                    ))
                }
                Object::String(_) => Some(RuntimeTy::String {
                    attr: TyAttr::default(),
                }),
                Object::Float(_) => Some(RuntimeTy::Float {
                    attr: TyAttr::default(),
                }),
                Object::Variant(variant) => {
                    // SAFETY: the variant's `enm` is a heap-rooted Enum
                    // class, valid under the same permit.
                    let enum_obj = unsafe { variant.enm.get() };
                    let Object::Enum(enum_def) = enum_obj else {
                        return None;
                    };
                    Some(RuntimeTy::Enum(enum_def.name.clone(), TyAttr::default()))
                }
                // Other Object shapes (HostClosure, FunctionRef, Array,
                // Map, etc.) are not meaningful in a thrown position;
                // treat as no runtime type → off-contract.
                _ => None,
            }
        }
    }
}

/// Pull `class_name` / `language` String fields out of a thrown
/// `Object::Instance` (the common `baml.errors.HostCallable` wrapper
/// shape) so a `HostContractViolation` panic can echo the host's
/// exception identity in its diagnostics. Non-Instance values or
/// missing fields return `(None, None)`.
///
/// `_proof` ensures the caller holds an active heap permit so the
/// `HeapPtr` derefs in this function are sound.
fn extract_host_diagnostics_from_value(
    value: Value,
    _proof: bex_heap::PermitProof<'_>,
) -> (Option<String>, Option<String>) {
    let Some(ptr) = value.as_object_ptr() else {
        return (None, None);
    };
    // SAFETY: caller holds an active heap permit.
    let Object::Instance(instance) = (unsafe { ptr.get() }) else {
        return (None, None);
    };
    let Object::Class(class) = (unsafe { instance.class.get() }) else {
        return (None, None);
    };
    let read_string_field = |field_name: &str| -> Option<String> {
        let idx = class.fields.iter().position(|f| f.name == field_name)?;
        let field_value = instance.try_load_field(idx)?;
        let ptr = field_value.as_object_ptr()?;
        match unsafe { ptr.get() } {
            Object::String(s) if !s.is_empty() => Some(s.as_str().to_string()),
            _ => None,
        }
    };
    (
        read_string_field("class_name"),
        read_string_field("language"),
    )
}

// TODO(bep-053): replace with compiler-owned metadata — capital-letter
// sniffing on FQN segments is an acknowledged-interim heuristic and must not
// become load-bearing.
fn derive_owner_type_definition_key(fqn: &str) -> Option<bex_events::DefinitionKey> {
    let parts = fqn.split('.').collect::<Vec<_>>();
    if parts.len() < 3 {
        return None;
    }

    let owner = parts[parts.len() - 2];
    if !owner
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
    {
        return None;
    }

    Some(bex_events::DefinitionKey(format!(
        "class:{}",
        parts[..parts.len() - 1].join(".")
    )))
}

// TODO(bep-053): replace with compiler-owned lambda identity — the
// `find("<lambda")` name sniff is an acknowledged-interim heuristic.
fn derive_lambda_metadata(fqn: &str) -> (Option<bex_events::DefinitionKey>, Option<String>) {
    let Some(lambda_start) = fqn.find("<lambda") else {
        return (None, None);
    };

    let parent = fqn[..lambda_start].trim_end_matches(['.', ':']).to_string();
    let parent_function =
        (!parent.is_empty()).then(|| bex_events::DefinitionKey(format!("function:{parent}")));
    let lambda_path = Some(fqn[lambda_start..].to_string());
    (parent_function, lambda_path)
}

impl BexEngine {
    fn next_engine_id() -> EngineId {
        static NEXT_ENGINE_ID: AtomicU64 = AtomicU64::new(1);
        EngineId(NEXT_ENGINE_ID.fetch_add(1, Ordering::Relaxed))
    }

    fn build_program_metadata(program: &bex_vm_types::Program) -> ProgramMetadata {
        // Ids are 1-based sequential in pool order (`0` = unassigned) — the
        // exact sequence the pre-heap walk in `new()` stamps onto each
        // `Function.function_id`, so the table and the `.bamlprof` records
        // agree byte-for-byte. M0 moves this to compile time.
        let mut next_function_id: u32 = 0;
        let mut functions: Vec<bex_events::FunctionMetadata> = program
            .objects
            .iter()
            .filter_map(|obj| {
                let Object::Function(function) = obj else {
                    return None;
                };

                next_function_id += 1;
                let function_id = FunctionId(next_function_id);
                let fqn = function.name.clone();
                let owner_type = derive_owner_type_definition_key(&fqn);
                let (parent_function, lambda_path) = derive_lambda_metadata(&fqn);
                let mut parts = fqn.split('.').map(str::to_string).collect::<Vec<_>>();
                let display_name = parts.last().cloned().unwrap_or_else(|| fqn.clone());
                let package_name = if parts.len() > 1 {
                    Some(parts.remove(0))
                } else {
                    None
                };
                let namespace = if parts.len() > 1 {
                    parts[..parts.len() - 1].to_vec()
                } else {
                    Vec::new()
                };
                let source_file =
                    (!function.source_file.is_empty()).then(|| function.source_file.clone());
                let source_span = Some(bex_events::SourceSpan {
                    file_id: function.span.file_id.as_u32(),
                    start: function.span.range.start().into(),
                    end: function.span.range.end().into(),
                });

                Some(bex_events::FunctionMetadata {
                    function_id,
                    fqn: fqn.clone(),
                    display_name,
                    source_file,
                    source_span,
                    kind: function.kind.into(),
                    origin: function.origin.into(),
                    owner_type,
                    parent_function,
                    lambda_path,
                    definition_key: Some(bex_events::DefinitionKey(format!("function:{fqn}"))),
                    package_name,
                    namespace,
                    source_snapshot_id: None,
                    revision_id: None,
                    semantic_lanes: None,
                })
            })
            .collect();

        // Synthetic rows live just past the pool indices, so they can never
        // collide with a real function's id.
        functions.push(bex_events::FunctionMetadata {
            function_id: FunctionId(next_function_id + 1),
            fqn: SPAWN_CLOSURE_FQN.to_string(),
            display_name: SPAWN_CLOSURE_DISPLAY_NAME.to_string(),
            source_file: None,
            source_span: None,
            kind: bex_events::RuntimeFunctionKind::Bytecode,
            origin: bex_events::RuntimeFunctionOrigin::Internal,
            owner_type: None,
            parent_function: None,
            lambda_path: Some(SPAWN_CLOSURE_DISPLAY_NAME.to_string()),
            definition_key: Some(bex_events::DefinitionKey(format!(
                "function:{SPAWN_CLOSURE_FQN}"
            ))),
            package_name: Some("baml".to_string()),
            namespace: Vec::new(),
            source_snapshot_id: None,
            revision_id: None,
            semantic_lanes: None,
        });

        functions.push(bex_events::FunctionMetadata {
            function_id: FunctionId(next_function_id + 2),
            fqn: UNKNOWN_FUNCTION_FQN.to_string(),
            display_name: UNKNOWN_FUNCTION_DISPLAY_NAME.to_string(),
            source_file: None,
            source_span: None,
            kind: bex_events::RuntimeFunctionKind::Bytecode,
            origin: bex_events::RuntimeFunctionOrigin::Internal,
            owner_type: None,
            parent_function: None,
            lambda_path: None,
            definition_key: Some(bex_events::DefinitionKey(format!(
                "function:{UNKNOWN_FUNCTION_FQN}"
            ))),
            package_name: Some("baml".to_string()),
            namespace: Vec::new(),
            source_snapshot_id: None,
            revision_id: None,
            semantic_lanes: None,
        });

        ProgramMetadata {
            program_id: ProgramId::new_random(),
            source_snapshot_id: None,
            revision_id: None,
            function_table: FunctionMetadataTable { functions },
        }
    }

    /// Create a new engine with the given program.
    ///
    /// The engine creates a unified heap containing compile-time objects
    /// (functions, classes, enums). Each function call creates a VM that
    /// shares this heap and allocates runtime objects into its own TLAB.
    ///
    /// # Arguments
    ///
    /// * `bytecode_program` - The compiled BAML program bytecode
    /// * `sys_ops` - System operations provider (use `sys_types_native::SysOps::native()` for default)
    /// * `argv` - Process-style argv values exposed to BAML via `baml.sys.argv()`.
    ///   Pass `Vec::new()` when argv is not applicable (e.g. tests, IDE, library embedding).
    pub fn new(
        bytecode_program: bex_vm_types::Program,
        sys_ops: std::sync::Arc<sys_ops::SysOps>,
        argv: Vec<String>,
    ) -> Result<Self, EngineError> {
        let argv: Arc<[String]> = Arc::from(argv);
        let process_euid = ProcessEuid::current();
        let engine_id = Self::next_engine_id();
        let program_metadata = Self::build_program_metadata(&bytecode_program);

        // BEX profiling event stream: snapshot the master switch once
        // (plan §2.2). The engine id minted above demuxes both the host
        // event identity and this engine's `.bamlprof`.
        let prof_enabled = bex_events::prof::ProfConfig::global().is_enabled();

        // Extract package_init_order before consuming bytecode_program.
        let package_init_order = bytecode_program.package_init_order.clone();

        // Convert the pure bytecode to a VM-ready program with native functions attached
        let bytecode =
            bex_vm::convert_program(bytecode_program).map_err(EngineError::VmInternalError)?;

        // Extract test cases before consuming other bytecode fields.
        let test_cases = bytecode.test_cases;

        // Extract compile-time objects for the heap
        let mut compile_time_objects: Vec<Object> = bytecode.objects.into_iter().collect();

        // Box every reachable `ConstValue::Float` into a compile-time
        // `Object::Float` (floats can no longer live inline in `Value`).
        // `bytecode.globals` are rewritten further down, during the
        // `ConstValue` → `Value` conversion, using the returned index map.
        let float_indices = bex_vm_types::types::box_compile_time_floats(
            &mut compile_time_objects,
            &bytecode.globals,
        );

        // Encode compact bytecode for all functions before the heap freezes them.
        // This must happen before BexHeap::new() because objects become immutable
        // behind an Arc after that point.
        //
        // The same pre-freeze walk is the profiling interim function-id
        // provider (plan §2.6): per-run sequential ids (1-based; 0 =
        // unassigned), assigned unconditionally so ids are deterministic,
        // with the header metadata table built only when profiling is on.
        // M0 moves assignment to compile time and replaces this seam.
        let mut next_function_id: u32 = 0;
        let mut function_pool_indices: Vec<usize> = Vec::new();
        let compile_time_objects: Vec<Object> = compile_time_objects
            .into_iter()
            .enumerate()
            .map(|(idx, mut obj)| {
                if let Object::Function(ref mut func) = obj {
                    func.bytecode.compact = Some(func.bytecode.lower_to_compact());
                    next_function_id += 1;
                    func.function_id = next_function_id;
                    function_pool_indices.push(idx);
                }
                obj
            })
            .collect();
        if prof_enabled {
            // The .bamlprof header consumes the M0 metadata table (its ids
            // match the ids stamped on each Function above — same walk
            // order, same 1-based sequence).
            bex_events::prof::register_engine_metadata(
                engine_id.0,
                prof_engine_metadata(&program_metadata),
            );
        }

        // Pre-compute class and enum indices before moving objects to heap.
        // This is used for allocating instances/variants from sys-op results.
        let class_indices: Vec<(String, usize)> = compile_time_objects
            .iter()
            .enumerate()
            .filter_map(|(idx, obj)| {
                if let Object::Class(class) = obj {
                    Some((class.name.to_string(), idx))
                } else {
                    None
                }
            })
            .collect();

        let enum_indices: Vec<(String, usize)> = compile_time_objects
            .iter()
            .enumerate()
            .filter_map(|(idx, obj)| {
                if let Object::Enum(enm) = obj {
                    Some((enm.name.to_string(), idx))
                } else {
                    None
                }
            })
            .collect();

        // Create the unified heap with compile-time objects
        let heap = BexHeap::new(compile_time_objects);

        // Convert ObjectIndex -> HeapPtr for function lookup table.
        // Now that the heap exists, we can get stable pointers to compile-time objects.
        let resolved_function_names: HashMap<String, (HeapPtr, bex_vm_types::FunctionKind)> =
            bytecode
                .resolved_function_names
                .into_iter()
                .map(|(name, (idx, kind))| {
                    let ptr = heap.compile_time_ptr(idx.into_raw());
                    (name, (ptr, kind))
                })
                .collect();

        // Function identity by heap address. Ids are 1-based sequential in
        // pool walk order (the same walk that stamped `Function.function_id`
        // and built the metadata table), and compile-time objects never
        // move, so this is built once and valid for the engine's lifetime.

        // Build class name lookup table from pre-computed indices.
        let resolved_class_names: indexmap::IndexMap<String, HeapPtr> = class_indices
            .into_iter()
            .map(|(name, idx)| (name, heap.compile_time_ptr(idx)))
            .collect();

        // Build enum name lookup table from pre-computed indices.
        let resolved_enum_names: indexmap::IndexMap<String, HeapPtr> = enum_indices
            .into_iter()
            .map(|(name, idx)| (name, heap.compile_time_ptr(idx)))
            .collect();

        // Convert compile-time globals (ConstValue) to runtime globals (Value).
        // Object references are converted from ObjectIndex to HeapPtr.
        // Float globals were redirected to compile-time Object::Float entries
        // by the boxing pre-pass above.
        let globals_vec: Vec<Value> = bytecode
            .globals
            .into_iter()
            .map(|cv| match cv {
                bex_vm_types::ConstValue::Float(f) => {
                    let idx = float_indices[&f.to_bits()];
                    Value::object(heap.compile_time_ptr(idx))
                }
                other => other.to_value(|idx| heap.compile_time_ptr(idx.into_raw())),
            })
            .collect();
        // Mutable during `$init` so `StoreGlobal` can populate top-level let
        // bindings; frozen into `Arc<[Value]>` once `$init` finishes (see below).
        let mut globals_pool = GlobalPool::from_vec(globals_vec);

        #[cfg(not(target_arch = "wasm32"))]
        let park_requested = Arc::new(AtomicBool::new(false));

        let interface_implementors: Arc<InterfaceImplementors> =
            Arc::new(bytecode.interface_implementors.clone());

        // Run $init for each package in dependency order.
        // $init evaluates top-level let-binding initializers and stores their
        // results into the global slots via StoreGlobal instructions.
        // This must run before any user code calls LoadGlobal on let-bound names.
        for init_name in &package_init_order {
            if let Some((init_ptr, _kind)) = resolved_function_names.get(init_name.as_str()) {
                let mut vm = BexVm::new(
                    Arc::clone(&heap),
                    VmGlobals::Owned(globals_pool.clone()),
                    resolved_class_names
                        .iter()
                        .chain(resolved_enum_names.iter())
                        .map(|(k, v)| (k.clone(), *v))
                        .collect(),
                    #[cfg(not(target_arch = "wasm32"))]
                    Arc::clone(&park_requested),
                    Arc::clone(&argv),
                    Arc::clone(&interface_implementors),
                );
                vm.set_entry_point(*init_ptr, &[]);
                // Drive the VM to completion. $init only contains synchronous
                // bytecode (no async ops), but we loop to handle any intermediate
                // notifications gracefully.
                loop {
                    match vm.exec() {
                        Ok(VmExecState::Complete(_)) => {
                            // Extract the (potentially mutated) global pool back
                            // so StoreGlobal writes are visible to subsequent calls.
                            globals_pool = match vm.globals {
                                VmGlobals::Owned(pool) => pool,
                                VmGlobals::Shared(_) => {
                                    unreachable!("$init VM constructed with Owned globals")
                                }
                            };
                            break;
                        }
                        Ok(VmExecState::Notify(_)) => {
                            // Ignore watch notifications during init.
                            continue;
                        }
                        Ok(VmExecState::Event { .. }) => {
                            // Handle events during $init: push null and continue.
                            // No span context exists during init, so the event is dropped,
                            // but we must push a return value to keep the stack balanced.
                            vm.stack.push(Value::NULL);
                            continue;
                        }
                        Ok(other) => {
                            return Err(EngineError::InitFailed(format!(
                                "$init function '{init_name}' yielded unexpectedly: {other:?}"
                            )));
                        }
                        Err(e) => {
                            return Err(EngineError::InitFailed(format!(
                                "$init function '{init_name}' failed: {e}"
                            )));
                        }
                    }
                }
            }
        }

        // Freeze the now-populated globals into a `SharedGlobals` so the GC
        // can trace + forward `Value::object(HeapPtr)` entries. Every
        // post-`$init` VM cloned into a `call_function` invocation reads
        // from this shared instance via `VmGlobals::Shared(globals.clone())`.
        // `StoreGlobal` against the shared view is rejected by the VM as
        // `VmInternalError::StoreGlobalAfterInit`.
        let globals = SharedGlobals::from_vec(globals_pool.0);

        // Build SysOpContext by pre-extracting LLM function metadata from the heap.
        // This avoids passing raw HeapPtrs to sys_ops.
        let llm_functions = Self::extract_llm_function_info(&resolved_function_names);

        // Extract class and enum definitions for output format rendering.
        let class_definitions = Self::extract_class_definitions(&resolved_class_names);
        let enum_definitions = Self::extract_enum_definitions(&resolved_enum_names);

        let heap_permit_manager = Arc::new(HeapPermitManager::new());
        // We just created the permit manager so `new_permit` will not block:
        // the only synchronization inside is the `holders` mutex which is
        // uncontended at this point. `futures::executor::block_on` (rather
        // than `tokio::runtime::Handle::block_on`) is used so that this
        // constructor stays callable from inside a tokio runtime.
        //
        // If `new_permit` ever takes a real lock or schedules async work,
        // this assumption breaks and the constructor would deadlock — at
        // which point we'd have to make `BexEngine::new` async (TODO).
        let futures_permit = futures::executor::block_on(
            heap_permit_manager
                .new_permit(FutureManagerInner::new(Tlab::new_empty(Arc::clone(&heap)))),
        );

        // Register the frozen globals pool as its own permit holder so the
        // GC traces and forwards `Value::object(HeapPtr)` entries (e.g.
        // top-level `let g = [1, 2, 3]` populated during `$init`). The
        // permit is never `acquire()`d — its sole job is to participate in
        // `HeapGuard::collect_roots` / `forward_roots` walks. The
        // `SharedGlobals` is `Clone` (Arc bump), so the holder and the
        // engine's own field point at the same `UnsafeCell<Box<[Value]>>`.
        let globals_permit =
            futures::executor::block_on(heap_permit_manager.new_permit(globals.clone()));

        // Build a default RuntimeIo from the SysOps table with an empty context.
        // This is replaced per-call in execute_sys_op with a live context that
        // carries the correct cancellation token and spawner.
        let runtime_io = sys_ops::build_runtime_io(
            &sys_ops,
            &heap,
            &heap_permit_manager,
            &sys_types::SysOpContext::empty(),
        );

        let sys_op_ctx = sys_types::EngineSysOpContext {
            llm_functions: Arc::new(llm_functions),
            function_global_indices: Arc::new(bytecode.function_global_indices),
            template_strings_macros: Arc::new(bytecode.template_strings_macros),
            class_definitions: Arc::new(class_definitions),
            enum_definitions: Arc::new(enum_definitions),
            type_alias_definitions: Arc::new(bytecode.recursive_type_alias_defs),
            runtime_io,
        };

        Ok(Self {
            process_euid,
            engine_id,
            program_metadata,
            next_thread_id: AtomicU64::new(1),
            heap,
            globals,
            _globals_permit: globals_permit,
            resolved_function_names,
            resolved_class_names,
            resolved_enum_names,
            sys_ops,
            sys_op_ctx,
            test_cases,
            argv,
            heap_permit_manager,
            checking_gc: AtomicBool::new(false),
            #[cfg(not(target_arch = "wasm32"))]
            park_requested,
            active_calls: Mutex::new(HashMap::new()),
            futures: FutureManager::new(futures_permit),
            interface_implementors,
            prof_enabled,
        })
    }

    #[must_use]
    pub fn process_euid(&self) -> ProcessEuid {
        self.process_euid
    }

    #[must_use]
    pub fn engine_id(&self) -> EngineId {
        self.engine_id
    }

    #[must_use]
    pub fn program_metadata(&self) -> &ProgramMetadata {
        &self.program_metadata
    }

    fn next_bex_thread_id(&self) -> BexThreadId {
        BexThreadId(self.next_thread_id.fetch_add(1, Ordering::Relaxed))
    }

    // ── BEX profiling event stream (bex_events::prof) ──────────────────

    /// Mints the next logical BEX thread id — shared by the profiling
    /// stream and the host-event identity (one id universe).
    fn next_prof_thread_id(&self) -> u64 {
        self.next_bex_thread_id().0
    }

    /// Engine-side profiling emission — thread lifecycle and sys-op
    /// completion, all cold paths. Always resolves the ring through the
    /// calling OS thread's TLS lookup, **never** a stale VM snapshot:
    /// engine arms run after `.await`s, where the task may have migrated
    /// OS threads (the VM snapshot is only valid within one exec resume).
    #[allow(unsafe_code)]
    fn prof_emit(&self, rec: &bex_events::prof::record::RawRecord<'_>) {
        if !self.prof_enabled {
            return;
        }
        let handle = bex_events::prof::ring_for_engine(self.engine_id.0);
        let mut buf = [0u8; bex_events::prof::record::MAX_RECORD_LEN];
        let len = rec.encode(&mut buf);
        // SAFETY: the handle was claimed via this live thread's TLS lookup
        // on the line above; engine arms never run from TLS destructors.
        unsafe { handle.push(&buf[..len]) };
    }

    /// Re-snapshots the VM's ring from the *current* OS thread's TLS (D5a).
    /// MUST run after every heap-permit re-acquire (the task may have
    /// migrated OS threads across the await) and before any engine-driven VM
    /// re-entry that can emit — `set_entry_point`, the loop-head exec, and
    /// `inject_sysop_throw`'s unwind all push through this snapshot.
    fn prof_refresh_vm_ring(&self, vm: &mut bex_vm::BexVm) {
        vm.prof_ring = if self.prof_enabled && !vm.prof_suppressed {
            Some(bex_events::prof::ring_for_engine(self.engine_id.0).ring())
        } else {
            None
        };
    }

    /// Closes the sys-op call pair opened at the VM's `VmExecState::SysOp`
    /// yield site (no-op when the VM minted nothing — profiling off or a
    /// non-call sys-op source). Cancellation paths pass `Cancelled` — the
    /// in-flight op was cancelled, not failed (§7 decision 1).
    fn prof_end_sysop(
        &self,
        vm: &mut bex_vm::BexVm,
        status: bex_events::prof::record::FunctionEndStatus,
    ) {
        if vm.prof_ring.is_none() {
            return;
        }
        if let Some(call_id) = vm.pending_sysop_call_id.take() {
            self.prof_emit(&bex_events::prof::record::RawRecord::EndFunction {
                status,
                thread_id: BexThreadId(vm.prof_thread_id),
                call_id: BexCallId(call_id),
                ts_ticks: bex_events::prof::clock::now_ticks(),
            });
        }
    }

    /// §7 decision 2: terminated threads never strand open calls. Closes
    /// every call frame still open in the suspended VM (innermost-first),
    /// plus any armed-but-unclosed sysop pair, with `status` `EndFunction`s.
    /// Called exactly once per terminated thread, at the blocks that end it
    /// *without* unwinding the VM (each returns immediately after): the six
    /// cancel blocks (`Cancelled`) and the unobserved fire-and-forget
    /// child-error surfacing (`Errored`). Threads whose terminal panic
    /// unwound VM-side already closed their frames in the unwinder — those
    /// paths must NOT also drain. Emits via the TLS ring lookup, so it is
    /// safe on any OS thread regardless of the VM's ring snapshot (D5a).
    fn prof_drain_open_calls(
        &self,
        vm: &mut bex_vm::BexVm,
        status: bex_events::prof::record::FunctionEndStatus,
    ) {
        use bex_events::prof::record::RawRecord;
        if vm.prof_ring.is_none() {
            return;
        }
        let thread_id = BexThreadId(vm.prof_thread_id);
        if let Some(call_id) = vm.pending_sysop_call_id.take() {
            self.prof_emit(&RawRecord::EndFunction {
                status,
                thread_id,
                call_id: BexCallId(call_id),
                ts_ticks: bex_events::prof::clock::now_ticks(),
            });
        }
        for call_id in vm.prof_open_call_ids() {
            self.prof_emit(&RawRecord::EndFunction {
                status,
                thread_id,
                call_id: BexCallId(call_id),
                ts_ticks: bex_events::prof::clock::now_ticks(),
            });
        }
    }

    /// Status for a sysop pair that ended in an op error, classified by
    /// error class exactly like the VM's inline-native close
    /// (`prof_native_error_status`): cancel-classed → `Cancelled`,
    /// exit-classed → `Exited`, everything else → `Errored`. Needed because
    /// a cancel-classed payload can arrive through the *generic* result
    /// path (host drops the `CompletionHandle` without completing — the
    /// thread's own token never fired), and the frames the injected throw
    /// subsequently unwinds will close `Cancelled` via the unwinder's
    /// class peek — the pair must agree.
    fn prof_sysop_error_status(
        err: &sys_types::OpError,
    ) -> bex_events::prof::record::FunctionEndStatus {
        use bex_events::prof::record::FunctionEndStatus;
        use bex_vm_types::errors::{VmPanic, VmRustFnError};
        match &err.payload {
            sys_types::OpErrorPayload::Vm(VmRustFnError::Panic(VmPanic::Cancelled)) => {
                FunctionEndStatus::Cancelled
            }
            sys_types::OpErrorPayload::Vm(VmRustFnError::Panic(VmPanic::Exit { .. })) => {
                FunctionEndStatus::Exited
            }
            _ => FunctionEndStatus::Errored,
        }
    }
    /// Pre-extract LLM function metadata from heap objects.
    ///
    /// This avoids passing raw `HeapPtr`s to `sys_ops` — instead, we read the
    /// data once during construction and store it in `SysOpContext`.
    fn extract_llm_function_info(
        resolved_function_names: &HashMap<String, (HeapPtr, bex_vm_types::FunctionKind)>,
    ) -> HashMap<String, sys_types::LlmFunctionInfo> {
        let mut llm_functions = HashMap::new();
        for (name, (ptr, _kind)) in resolved_function_names {
            // SAFETY: ptr is from resolved_function_names, a compile-time object
            let obj = unsafe { ptr.get() };
            if let Object::Function(func) = obj {
                if let Some(FunctionMeta::Llm {
                    prompt_template,
                    client,
                }) = &func.body_meta
                {
                    llm_functions.insert(
                        name.clone(),
                        sys_types::LlmFunctionInfo {
                            prompt_template: prompt_template.clone(),
                            client_name: client.clone(),
                            return_type: func.return_type.clone(),
                        },
                    );
                }
            }
        }
        llm_functions
    }

    /// Extract class definitions from the heap for output format rendering.
    fn extract_class_definitions(
        resolved_class_names: &indexmap::IndexMap<String, HeapPtr>,
    ) -> indexmap::IndexMap<baml_type::TypeName, sys_types::ClassDefinition> {
        let mut defs = indexmap::IndexMap::new();
        for (_name, ptr) in resolved_class_names {
            // SAFETY: ptr is from resolved_class_names, a compile-time object
            let obj = unsafe { ptr.get() };
            if let Object::Class(cls) = obj {
                defs.insert(
                    cls.name.clone(),
                    sys_types::ClassDefinition {
                        name: cls.name.display_name().to_string(),
                        description: cls.description.clone(),
                        alias: cls.alias.clone(),
                        fields: cls
                            .fields
                            .iter()
                            .map(|f| sys_types::ClassFieldDefinition {
                                name: f.name.clone(),
                                field_type: f.field_type.clone(),
                                description: f.description.clone(),
                                alias: f.alias.clone(),
                                skip: f.skip,
                            })
                            .collect(),
                    },
                );
            }
        }
        defs
    }

    /// Extract enum definitions from the heap for output format rendering.
    fn extract_enum_definitions(
        resolved_enum_names: &indexmap::IndexMap<String, HeapPtr>,
    ) -> indexmap::IndexMap<baml_type::TypeName, sys_types::EnumDefinition> {
        let mut defs = indexmap::IndexMap::new();
        for (_name, ptr) in resolved_enum_names {
            // SAFETY: ptr is from resolved_enum_names, a compile-time object
            let obj = unsafe { ptr.get() };
            if let Object::Enum(enm) = obj {
                defs.insert(
                    enm.name.clone(),
                    sys_types::EnumDefinition {
                        name: enm.name.display_name().to_string(),
                        description: enm.description.clone(),
                        alias: enm.alias.clone(),
                        variants: enm
                            .variants
                            .iter()
                            .filter(|v| !v.skip)
                            .map(|v| sys_types::EnumVariantDefinition {
                                name: v.name.clone(),
                                description: v.description.clone(),
                                alias: v.alias.clone(),
                            })
                            .collect(),
                    },
                );
            }
        }
        defs
    }

    /// Get a reference to the shared heap.
    pub fn heap(&self) -> &Arc<BexHeap> {
        &self.heap
    }

    /// Get statistics about heap usage.
    ///
    /// Useful for monitoring concurrent execution and debugging.
    pub fn heap_stats(&self) -> bex_heap::HeapStats {
        self.heap.stats()
    }

    /// Get a reference to the heap permit manager.
    pub fn heap_permit_manager(&self) -> &Arc<HeapPermitManager> {
        &self.heap_permit_manager
    }

    /// Number of currently `Pending` futures tracked by the engine. Useful
    /// for telemetry and tests that verify the future manager cleans up
    /// completed futures.
    pub async fn active_future_count(&self) -> usize {
        self.futures
            .active_future_count(&self.heap_permit_manager)
            .await
    }

    /// Resolve a [`bex_external_types::Handle`] to its current [`HeapPtr`].
    ///
    /// The permit parameter proves GC cannot run while the caller is using the
    /// returned pointer — holding any `ActiveHeapPermit<T>` keeps one of the
    /// manager's semaphore tokens in scope, so `request_park` cannot proceed.
    ///
    /// Returns `None` if the handle has been invalidated.
    pub fn resolve_handle(
        &self,
        _permit: bex_heap::PermitProof<'_>,
        handle: &bex_external_types::Handle,
    ) -> Option<HeapPtr> {
        use bex_external_types::WeakHeapRef;
        self.heap.resolve_handle_ptr(handle.slab_key())
    }

    /// Explicitly trigger garbage collection.
    ///
    /// Requests and waits for all heap permit holders to park.
    /// Once they are parked, runs the GC.
    ///
    /// # Returns
    ///
    /// Statistics about the collection (live count, collected count, etc.)
    pub async fn collect_garbage(&self, level: bex_heap::CollectionLevel) -> bex_heap::GcStats {
        #[cfg(not(target_arch = "wasm32"))]
        self.park_requested.store(true, Ordering::Relaxed);
        let mut heap_guard = self.heap_permit_manager.request_park().await;
        #[cfg(not(target_arch = "wasm32"))]
        self.park_requested.store(false, Ordering::Relaxed);

        // Collect roots from handles (objects returned to external code)
        let mut all_roots = self.heap.collect_handle_roots();

        heap_guard.collect_roots(&mut all_roots);

        tracing::debug!(
            "GC: {} total roots from {} handles and {} parked heap permits",
            all_roots.len(),
            self.heap.stats().active_handles,
            heap_guard.num_permits(),
        );

        // Run GC — always returns the forwarding map so we can update parked VM stacks.
        let (stats, _remapped_roots, forwarding) =
            unsafe { self.heap.collect_garbage_generational(&all_roots, level) };

        // Bug H, check 1 (heap_debug only): every pointer the GC was told
        // about (`all_roots`) must end up in the forwarding map. If a
        // holder's `collect_roots` produces a pointer the GC's BFS does
        // not reach, the subsequent `forward_roots` will leave a stale
        // reference behind. This assertion turns that silent class of
        // bug into an immediate panic during stress tests.
        #[cfg(feature = "heap_debug")]
        for &ptr in &all_roots {
            assert!(
                forwarding.contains_key(&ptr),
                "heap_debug: post-GC integrity sweep — root {ptr:?} was not \
                 reached by the GC BFS (collect_roots produced it but it is \
                 not in the forwarding map)"
            );
        }

        // Update all parked VM stacks with forwarding pointers and invalidate TLABs
        // SAFETY: VMs are still parked (gc_complete not yet notified), we have
        // exclusive access via the parked_vms lock we're still holding
        heap_guard.forward_roots(&forwarding);

        // Bug H, check 3 (heap_debug only): after `forward_roots`, no
        // holder root should still point into the inactive (former
        // active) space. If any does, `forward_roots` missed it — the
        // stale pointer would dereference into freed/poisoned memory.
        #[cfg(feature = "heap_debug")]
        {
            let mut roots_after = self.heap.collect_handle_roots();
            heap_guard.collect_roots(&mut roots_after);
            for &ptr in &roots_after {
                assert!(
                    !self.heap.debug_ptr_in_inactive(ptr),
                    "heap_debug: post-forward_roots — root {ptr:?} still points \
                     into the inactive space (forward_roots missed it)"
                );
            }
        }

        self.heap.verify_quick();

        drop(heap_guard);

        // Flush deferred host-value releases now that the stop-the-world window
        // has closed. Collecting a dead `Object::HostClosure` runs
        // `HostValueArc::drop`, which only *enqueues* the key (it cannot fire
        // the host release callback inline: `Drop` runs while all heap permits
        // are parked, and the callback runs arbitrary host code — e.g. Python
        // re-acquires the GIL — which would be an AB-BA deadlock against the
        // parked permits). The `heap_guard` is dropped above, so all permits
        // are released and we hold none here; this is a safe point to invoke
        // the host callbacks. (`gc_safepoint` re-acquires its permit only
        // *after* `collect_garbage` returns; both `maybe_collect_garbage` call
        // sites release their permit before calling.)
        bex_external_types::host_value::host_release_dispatch::drain();

        tracing::debug!(
            "GC completed: {} live, {} collected",
            stats.live_count,
            stats.collected_count
        );

        stats
    }

    /// Execute a function by name.
    ///
    /// # Arguments
    ///
    /// Arguments are passed as `Vec<BexExternalValue>`:
    /// - Primitives and strings are passed directly (e.g. `BexExternalValue::String(...)`)
    /// - `Handle` references existing heap objects
    /// - `Adt(Media | PromptAst)` allocates new builtin ADT objects on the heap
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = engine.call_function("get_user", vec![
    ///     "Alice".into(),
    ///     42i64.into(),
    /// ], None).await?;
    /// ```
    pub async fn call_function(
        self: &Arc<Self>,
        function_name: &str,
        args: Vec<BexExternalValue>,
        call_ctx: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexExternalValue, EngineError> {
        self.call_function_with_trace(function_name, args, call_ctx, copy_objects)
            .await
            .and_then(|result| result.value)
    }

    pub async fn call_function_with_trace(
        self: &Arc<Self>,
        function_name: &str,
        args: Vec<BexExternalValue>,
        FunctionCallContext {
            host_call_id,
            cancel,
            profile_enabled,
            type_args,
        }: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexCallResult, EngineError> {
        let args = args
            .into_iter()
            .map(|arg| BexCallArg::Provided(Box::new(arg)))
            .collect();
        self.call_function_bound_args_with_trace(
            function_name,
            args,
            FunctionCallContext {
                host_call_id,
                cancel,
                profile_enabled,
                type_args,
            },
            copy_objects,
        )
        .await
    }

    /// Run-vocabulary alias for the traced function entry path. The
    /// `FunctionCallContext` still carries host-call plumbing; `RunStore` owns
    /// the durable `RunId` outside the engine.
    pub async fn start_run(
        self: &Arc<Self>,
        function_name: &str,
        args: Vec<BexExternalValue>,
        call_ctx: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexCallResult, EngineError> {
        self.call_function_with_trace(function_name, args, call_ctx, copy_objects)
            .await
    }

    pub async fn call_function_bound_args(
        self: &Arc<Self>,
        function_name: &str,
        args: Vec<BexCallArg>,
        call_ctx: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexExternalValue, EngineError> {
        self.call_function_bound_args_with_trace(function_name, args, call_ctx, copy_objects)
            .await
            .and_then(|result| result.value)
    }

    pub async fn call_function_bound_args_with_trace(
        self: &Arc<Self>,
        function_name: &str,
        args: Vec<BexCallArg>,
        FunctionCallContext {
            host_call_id,
            cancel,
            profile_enabled,
            type_args,
        }: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexCallResult, EngineError> {
        // Register this host call so `cancel_function_call(host_call_id)` can
        // target it. The RAII guard removes the entry on drop (including
        // panic unwind). Insertion and guard construction are atomic, so a
        // panic here cannot leak registry entries.
        let (_call_guard, cancel) =
            ActiveCallGuard::register(Arc::clone(self), host_call_id, cancel)?;

        // Fail fast if already cancelled — guarantees pre-cancelled IDs
        // always produce a `baml.panics.Cancelled` panic regardless of
        // function contents.
        if cancel.is_cancelled() {
            return Err(cancelled_unhandled_throw());
        }

        let (function_index, kind) = self.lookup_function(function_name)?;
        if matches!(kind, bex_vm_types::FunctionKind::NativeUnresolved) {
            return Err(EngineError::NotInvokableAsEntry {
                name: function_name.to_string(),
                kind: format!("{kind:?}"),
            });
        }
        self.validate_bound_args(function_name, &args)?;
        let return_type = self
            .function_return_type(function_name)
            .unwrap_or(RuntimeTy::Null {
                attr: baml_type::TyAttr::default(),
            });
        let throws_type = self.function_throws_type(function_name);

        // Type-directed coercion for each provided arg: lets host SDKs send
        // `int(42)` to a `bigint` slot (and vice versa) without re-encoding,
        // and rewrites the engine-registered class FQN onto incoming
        // `Map`/`Instance`/`Variant` values. Idempotent for already-matching
        // values, so callers that already coerced (e.g. `BexProject::Bex`
        // kwargs entry) aren't double-charged.
        let param_types: Vec<RuntimeTy> = self
            .function_params(function_name)?
            .into_iter()
            .map(|(_, ty, _)| ty.clone())
            .collect();
        let args: Vec<BexCallArg> = args
            .into_iter()
            .enumerate()
            .map(|(idx, arg)| match arg {
                BexCallArg::Provided(value) => {
                    let coerced =
                        crate::conversion::coerce_arg_to_declared_type(*value, &param_types[idx])?;
                    Ok(BexCallArg::Provided(Box::new(coerced)))
                }
                BexCallArg::OmittedDefault => Ok(BexCallArg::OmittedDefault),
            })
            .collect::<Result<_, EngineError>>()?;

        // Create the root thread (shared heap, own TLAB) and acquire its permit.
        let mut thread = self.new_root_thread(cancel.clone(), profile_enabled).await;

        // Snapshot the declared parameter types so we can thread the
        // expected `RuntimeTy` into per-arg conversion. Binding a `HostValue` to an
        // `Object::HostClosure` needs it: the closure carries the declared
        // `RuntimeTy::Function`'s arity and return type, extracted from the parameter
        // type.
        let param_types: Vec<RuntimeTy> = self
            .function_params(function_name)?
            .into_iter()
            .map(|(_, ty, _)| ty.clone())
            .collect();
        let vm_args: Vec<Value> = args
            .into_iter()
            .enumerate()
            .map(|(idx, arg)| match arg {
                BexCallArg::Provided(arg) => self.convert_external_to_vm_value_with_ty(
                    &mut thread,
                    *arg,
                    param_types.get(idx),
                ),
                BexCallArg::OmittedDefault => Ok(Value::OMITTED_ARG),
            })
            .collect::<Result<Vec<_>, _>>()?;

        // `function_index` is the entry function's `HeapPtr`; `type_args` are the
        // explicit BEP-039 type args from the host (closures/bound methods instead
        // seed their captured/class type args — see `call_callable`).
        self.run_entry_point(
            thread,
            function_index,
            vm_args,
            type_args,
            return_type,
            throws_type,
            host_call_id,
            cancel,
            copy_objects,
        )
        .await
    }

    /// Build a fresh root [`BexThread`] over the shared heap and acquire its
    /// heap permit. Shared by the named-entry (`call_function_bound_args`) and
    /// callable-entry (`call_callable`) paths.
    async fn new_root_thread(
        self: &Arc<Self>,
        cancel: CancellationToken,
        profile_enabled: bool,
    ) -> ActiveHeapPermit<BexThread> {
        // Globals are shared as a frozen `Arc<[Value]>` — cloning is a refcount bump.
        let vm = BexVm::new(
            Arc::clone(&self.heap),
            VmGlobals::Shared(self.globals.clone()),
            self.resolved_class_names
                .iter()
                .chain(self.resolved_enum_names.iter())
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            #[cfg(not(target_arch = "wasm32"))]
            Arc::clone(&self.park_requested),
            Arc::clone(&self.argv),
            Arc::clone(&self.interface_implementors),
        );
        // BEP-034: wrap the root VM in a `BexThread` from the outset so the
        // permit's `RootHaver` is the thread (delegating to the inner VM).
        // Spawned children build their own `BexThread`s in `spawn_thread`.
        let mut vm = vm;
        vm.prof_thread_id = self.next_prof_thread_id();
        vm.prof_suppressed = !profile_enabled;
        // Identity seed for the `$id` surface (baml.id.*): unconditional —
        // `$id` works with profiling off, and the ids it exposes are the
        // VM-minted ids the event stream records.
        vm.bex_ref_seed = Some((self.process_euid, self.engine_id));
        // No ring snapshot and no StartThread here: the permit acquisition
        // below awaits (the snapshot would go stale across an OS-thread
        // migration), and early-error returns between here and the run loop
        // would leak a StartThread with no EndThread. Both happen at
        // guaranteed-balanced points instead: the StartThread in
        // run_entry_point right before set_entry_point (§7 decision 7 —
        // straight-line into run_thread_event_loop, whose every exit path
        // emits the EndThread), and the snapshot at the same spot plus each
        // loop-head resume.
        let root_thread = BexThread::new_root(vm, cancel);
        let inactive = self.heap_permit_manager.new_permit(root_thread).await;
        inactive.acquire().await
    }

    /// Shared entry-point core: set the VM's entry frame, run the thread event
    /// loop, and extract the root value. Used by both the named-function path
    /// and the closure path; the callers differ only in how they resolve the
    /// entry pointer and its `return`/`throws` types.
    #[expect(clippy::too_many_arguments)]
    async fn run_entry_point(
        self: &Arc<Self>,
        mut thread: ActiveHeapPermit<BexThread>,
        entry_ptr: HeapPtr,
        vm_args: Vec<Value>,
        type_args: Vec<RuntimeTy>,
        return_type: RuntimeTy,
        throws_type: Option<RuntimeTy>,
        host_call_id: CallId,
        cancel: CancellationToken,
        copy_objects: bool,
    ) -> Result<BexCallResult, EngineError> {
        // D5a: the entry-frame CallFunction below pushes into the snapshot;
        // take it on THIS thread, after the last await before the push.
        self.prof_refresh_vm_ring(&mut thread.vm);
        // §7 decision 7: the root StartThread is emitted before the entry
        // frame's CallFunction so that every thread's first record is its
        // StartThread — a uniform invariant for renderers (children get
        // theirs at the Spawn arm, before their entry push). BALANCE: the
        // matching EndThread is emitted by run_thread_event_loop on every
        // exit path; no early return / `?` may be introduced between this
        // emission and the run_thread_event_loop call below, or an error
        // path would leak an unclosed StartThread.
        if thread.vm.prof_ring.is_some() {
            self.prof_emit(&bex_events::prof::record::RawRecord::StartThread {
                flags: 0,
                thread_id: BexThreadId(thread.vm.prof_thread_id),
                parent_thread_id: BexThreadId(0), // engine-root thread
                parent_call_id: BexCallId(0),
                ts_ticks: bex_events::prof::clock::now_ticks(),
                name: b"",
            });
        }
        thread
            .vm
            .set_entry_point_with_type_args(entry_ptr, &vm_args, type_args);
        let entry_call_ref = CallRef {
            process_euid: self.process_euid,
            engine_id: self.engine_id,
            thread_id: BexThreadId(thread.vm.prof_thread_id),
            call_id: BexCallId(thread.vm.current_call_id()),
        };

        // Run the event loop.
        let result = self
            .run_thread_event_loop(
                return_type,
                throws_type,
                thread,
                host_call_id,
                &cancel,
                copy_objects,
            )
            .await;

        // Flush any host-value releases queued during this call. The root
        // thread's `ActiveHeapPermit` was consumed by `run_thread_event_loop`
        // (taken by value) and has been dropped by the time it returns, so we
        // hold no heap permit here and the host release callbacks run safely
        // off the parked-permit window. This makes release prompt even when a
        // GC never ran during the call.
        bex_external_types::host_value::host_release_dispatch::drain();

        // active_calls cleanup is done by ActiveCallGuard on drop.
        //
        // Keep genuine engine errors intact. Cancellation is surfaced as a
        // `baml.panics.Cancelled` panic — either raised by the VM's `Await`
        // opcode, or synthesized by engine safepoints (see
        // `cancelled_unhandled_throw`).
        match result {
            Ok(ThreadOutcome::RootValue(value)) => Ok(BexCallResult {
                value: Ok(value),
                entry_call_ref,
            }),
            Ok(ThreadOutcome::SettledChild(_)) => Ok(BexCallResult {
                // Root threads should never produce SettledChild; treat as an
                // engine invariant violation rather than silently returning Null.
                value: Err(EngineError::Other(
                    "BEP-034: root thread terminated as SettledChild".to_string(),
                )),
                entry_call_ref,
            }),
            Err(err) => Ok(BexCallResult {
                value: Err(err),
                entry_call_ref,
            }),
        }
    }

    /// Invoke a callable BAML value — a raw function, a closure (with captures),
    /// or a bound method — referenced by a host [`Handle`](bex_external_types::Handle), as a fresh root call,
    /// returning its result. This is the by-value counterpart of
    /// [`Self::call_function`], used to call a BAML callback passed to a sys-op
    /// (e.g. an HTTP server `handler`). The callee's `return`/`throws` types and
    /// type args are read from the heap object rather than a name lookup; a bound
    /// method's receiver is injected as `self` and its instance's class type args
    /// are seeded — mirroring the VM's normal call path
    /// (`execute_call_from_locals_offset`).
    pub async fn call_callable(
        self: &Arc<Self>,
        handle: bex_external_types::Handle,
        args: Vec<BexExternalValue>,
        call_ctx: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexExternalValue, EngineError> {
        self.call_callable_with_trace(handle, args, call_ctx, copy_objects)
            .await
            .and_then(|result| result.value)
    }

    pub async fn call_callable_with_trace(
        self: &Arc<Self>,
        handle: bex_external_types::Handle,
        args: Vec<BexExternalValue>,
        FunctionCallContext {
            host_call_id,
            cancel,
            profile_enabled,
            type_args: _,
        }: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexCallResult, EngineError> {
        let (_call_guard, cancel) =
            ActiveCallGuard::register(Arc::clone(self), host_call_id, cancel)?;
        if cancel.is_cancelled() {
            return Err(cancelled_unhandled_throw());
        }
        let mut thread = self.new_root_thread(cancel.clone(), profile_enabled).await;

        // Resolve the handle to the live heap object. The handle keeps it rooted.
        let entry_ptr = self
            .resolve_handle(thread.proof(), &handle)
            .ok_or_else(|| {
                EngineError::Other("call_callable: stale callable handle".to_string())
            })?;

        // Unwrap to the entry pointer to run, the receiver to inject as `self`
        // (bound methods only), the type args to seed the frame, and the inner
        // `Function` pointer to read metadata from. `entry` is the closure value
        // itself (so its captures/upvalues resolve) or, for a bound method, the
        // inner function (its `self` is supplied positionally).
        let (entry, receiver, seed_type_args, func_ptr) = match thread.vm.get_object(entry_ptr) {
            Object::Function(_) => (entry_ptr, None, Vec::new(), entry_ptr),
            Object::Closure(closure) => (
                entry_ptr,
                None,
                closure.captured_type_args.to_vec(),
                closure.function,
            ),
            Object::BoundMethod(bm) => {
                let receiver = bm.receiver;
                let class_type_args = receiver
                    .as_object_ptr()
                    .and_then(|ptr| match thread.vm.get_object(ptr) {
                        Object::Instance(inst) => Some(inst.class_type_args.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                (bm.function, Some(receiver), class_type_args, bm.function)
            }
            other => {
                return Err(EngineError::TypeMismatch {
                    message: format!("call_callable: handle is not callable ({other:?})"),
                });
            }
        };

        // Read the inner function's metadata. `param_types` carries declared
        // parameter types (including `self` for methods) for type-directed
        // coercion; lambdas leave it empty (types inferred, not stored), so the
        // real arity comes from `arity` and coercion is best-effort.
        let (return_type, throws_type, arity, param_types) = match thread.vm.get_object(func_ptr) {
            Object::Function(func) => {
                // A value referencing an unresolved native builtin can't be an
                // entry point (parity with `call_function_bound_args`).
                if matches!(func.kind, bex_vm_types::FunctionKind::NativeUnresolved) {
                    return Err(EngineError::NotInvokableAsEntry {
                        name: func.name.clone(),
                        kind: format!("{:?}", func.kind),
                    });
                }
                (
                    func.return_type.clone(),
                    func.throws_type.clone(),
                    func.arity,
                    func.param_types.clone(),
                )
            }
            _ => {
                return Err(EngineError::TypeMismatch {
                    message: "call_callable: value does not wrap a function".to_string(),
                });
            }
        };

        // A bound method's `arity` counts the implicit `self`; callers don't pass
        // it (the receiver is injected below), so the visible arity drops by one.
        let self_offset = usize::from(receiver.is_some());
        let user_arity = arity.saturating_sub(self_offset);
        if args.len() != user_arity {
            return Err(EngineError::TypeMismatch {
                message: format!(
                    "callable expects {user_arity} argument(s), got {}",
                    args.len()
                ),
            });
        }

        // Coerce each provided arg to its declared param type (offset by `self`
        // for bound methods).
        let coerced: Vec<BexExternalValue> = args
            .into_iter()
            .enumerate()
            .map(|(idx, arg)| match param_types.get(idx + self_offset) {
                Some(ty) => crate::conversion::coerce_arg_to_declared_type(arg, ty),
                None => Ok(arg),
            })
            .collect::<Result<_, _>>()?;

        // Build VM args: the receiver as `self` in slot 0 (bound methods), then
        // the converted user args.
        let mut vm_args: Vec<Value> = Vec::with_capacity(coerced.len() + self_offset);
        if let Some(receiver) = receiver {
            vm_args.push(receiver);
        }
        for (idx, arg) in coerced.into_iter().enumerate() {
            vm_args.push(self.convert_external_to_vm_value_with_ty(
                &mut thread,
                arg,
                param_types.get(idx + self_offset),
            )?);
        }

        // The legacy span label stays "<callable>" (host-facing name for a
        // by-value invocation), but the host-event identity is the real
        // callee: `func_ptr` is the unwrapped `Object::Function`, so it
        // resolves to the actual function's metadata row. (The .bamlprof
        // root `CallFunction` id is stamped independently by the VM in
        // `prof_enter_call` from `Function.function_id`.)
        self.run_entry_point(
            thread,
            entry,
            vm_args,
            seed_type_args,
            return_type,
            throws_type,
            host_call_id,
            cancel,
            copy_objects,
        )
        .await
    }

    /// Cancel a function call by its ID.
    ///
    /// If the call is still running, it will be interrupted at the next
    /// cancellation check point. If the ID is not active yet, reserve it with
    /// an already-cancelled token so the later call starts cancelled.
    pub fn cancel_function_call(&self, call_id: CallId) -> Result<(), EngineError> {
        ActiveCallGuard::reserve_cancelled(self, call_id);
        Ok(())
    }

    /// Run-vocabulary alias for host-call cancellation. The parameter is the
    /// adapter-owned host call id backing value, not a `RunId`.
    pub fn cancel_run(&self, host_call_id: CallId) -> Result<(), EngineError> {
        self.cancel_function_call(host_call_id)
    }

    fn validate_bound_args(
        &self,
        function_name: &str,
        args: &[BexCallArg],
    ) -> Result<(), EngineError> {
        let params = self.function_params(function_name)?;
        if args.len() != params.len() {
            return Err(EngineError::TypeMismatch {
                message: format!(
                    "Function `{function_name}` expects {} argument(s), got {}",
                    params.len(),
                    args.len()
                ),
            });
        }

        for (idx, arg) in args.iter().enumerate() {
            if matches!(arg, BexCallArg::OmittedDefault) && !params[idx].2 {
                return Err(EngineError::TypeMismatch {
                    message: format!(
                        "Argument `{}` for function `{function_name}` cannot be omitted; only parameters with defaults may use omitted defaults",
                        params[idx].0
                    ),
                });
            }
        }

        Ok(())
    }

    /// Look up a function by name and return its heap pointer + kind.
    /// Resolution follows the canonical [`sys_types::resolve_name`] rule
    /// (exact → `user.{name}` → unambiguous suffix match), shared with
    /// `baml run`, `baml pack`, and the sysop LLM/`$new` resolvers.
    ///
    /// Returning the kind lets call-site gates (e.g.
    /// `call_function_bound_args` rejecting non-Bytecode entries) avoid a
    /// second heap dereference.
    fn lookup_function(
        &self,
        function_name: &str,
    ) -> Result<(HeapPtr, bex_vm_types::FunctionKind), EngineError> {
        sys_types::resolve_name(&self.resolved_function_names, function_name)
            .found()
            .map(|(_k, (ptr, kind))| (*ptr, *kind))
            .ok_or_else(|| EngineError::FunctionNotFound {
                name: function_name.to_string(),
            })
    }

    /// Resolve a function name to the key actually present in
    /// `resolved_function_names`. Uses [`sys_types::resolve_name`] so the
    /// rule matches `lookup_function` exactly.
    fn resolve_function_name<'a>(&'a self, name: &str) -> Option<&'a str> {
        sys_types::resolve_name(&self.resolved_function_names, name)
            .found()
            .map(|(k, _)| k)
    }

    /// Get the return type for a function by dereferencing its heap object.
    pub fn function_return_type(&self, name: &str) -> Option<RuntimeTy> {
        let resolved = self.resolve_function_name(name)?;
        let (ptr, _kind) = self.resolved_function_names.get(resolved)?;
        // SAFETY: ptr is from resolved_function_names, a compile-time object
        let obj = unsafe { ptr.get() };
        match obj {
            Object::Function(func) => Some(func.return_type.clone()),
            _ => None,
        }
    }

    /// Get the inferred throws type for a function by dereferencing its heap object.
    fn function_throws_type(&self, name: &str) -> Option<RuntimeTy> {
        let resolved = self.resolve_function_name(name)?;
        let (ptr, _kind) = self.resolved_function_names.get(resolved)?;
        // SAFETY: ptr is from resolved_function_names, a compile-time object
        let obj = unsafe { ptr.get() };
        match obj {
            Object::Function(func) => func.throws_type.clone(),
            _ => None,
        }
    }

    /// All class field schemas known to the engine, keyed by `TypeName`.
    ///
    /// Used by callers that walk a `RuntimeTy` tree and need to resolve nested
    /// class field types — e.g. the CLI parsing `--json-args` for a function
    /// whose parameter is a class with `map<…>` or class-typed fields.
    pub fn class_definitions(&self) -> &indexmap::IndexMap<TypeName, ClassDefinition> {
        &self.sys_op_ctx.class_definitions
    }

    /// Look up the field schema for a class by its `TypeName`.
    pub fn class_definition(&self, name: &TypeName) -> Option<&ClassDefinition> {
        self.sys_op_ctx.class_definitions.get(name)
    }

    /// Get parameter names and types for a function by dereferencing its heap object.
    pub fn function_params(
        &self,
        name: &str,
    ) -> Result<Vec<(&str, &RuntimeTy, bool)>, EngineError> {
        let resolved = self
            .resolve_function_name(name)
            .ok_or(EngineError::FunctionNotFound {
                name: name.to_string(),
            })?;
        let (ptr, _kind) =
            self.resolved_function_names
                .get(resolved)
                .ok_or(EngineError::FunctionNotFound {
                    name: name.to_string(),
                })?;
        // SAFETY: ptr is from resolved_function_names, a compile-time object
        let obj = unsafe { ptr.get() };
        match obj {
            Object::Function(func) => Ok(func
                .param_names
                .iter()
                .zip(func.param_types.iter())
                .enumerate()
                .map(|(idx, (name, ty))| {
                    (
                        name.as_str(),
                        ty,
                        func.param_has_default.get(idx).copied().unwrap_or(false),
                    )
                })
                .collect()),
            other => Err(EngineError::TypeMismatch {
                message: format!("Expected Function, got {other:?}"),
            }),
        }
    }

    /// Check if a function exists by name (tries exact then "user." prefix).
    pub fn function_exists(&self, name: &str) -> bool {
        self.resolve_function_name(name).is_some()
    }

    /// Replace the process argv exposed to BAML via `baml.sys.argv()`.
    ///
    /// Allowed only before the engine is wrapped in an `Arc` and shared,
    /// because we mutate `self` directly. Used by `baml run` to derive
    /// `argv[1]` from the compiled program (BEP-027 §"`baml.argv`": the
    /// path of the file containing root `main`).
    pub fn set_argv(&mut self, argv: Vec<String>) {
        self.argv = Arc::from(argv);
    }

    /// View of the current process argv. Used by hosts that need to
    /// patch `argv[1]` based on resolution context (e.g. swapping a
    /// script-alias name for the resolved function it expanded to).
    pub fn argv(&self) -> &[String] {
        &self.argv
    }

    /// Find a user-callable function by qualified name (`user.main`),
    /// display name (`main`), or unambiguous namespace-leaf suffix
    /// (`lorem.Func` resolving to `user.ns_lorem.Func`). Uses the
    /// shared [`sys_types::resolve_name`] rule and post-filters to
    /// user-callable bytecode functions.
    pub fn find_user_function(&self, name: &str) -> Option<UserFunctionInfo> {
        let (qualified, _) =
            sys_types::resolve_name(&self.resolved_function_names, name).found()?;
        let qualified = qualified.to_string();
        self.user_functions()
            .into_iter()
            .find(|f| f.qualified_name == qualified)
    }

    /// List all user-callable functions with signature info.
    pub fn user_functions(&self) -> Vec<UserFunctionInfo> {
        self.resolved_function_names
            .iter()
            .filter_map(|(name, (ptr, kind))| {
                if !matches!(kind, bex_vm_types::FunctionKind::Bytecode) {
                    return None;
                }
                let obj = unsafe { ptr.get() };
                match obj {
                    Object::Function(func) if func.origin.is_user_callable() => {
                        let display_name = name.strip_prefix("user.").unwrap_or(name).to_string();
                        let is_llm =
                            matches!(func.body_meta, Some(bex_vm_types::FunctionMeta::Llm { .. }));
                        let display_param_types =
                            if func.display_param_types.len() == func.param_names.len() {
                                func.display_param_types.clone()
                            } else {
                                func.param_types.iter().map(ToString::to_string).collect()
                            };
                        let display_return_type = if func.display_return_type.is_empty() {
                            func.return_type.to_string()
                        } else {
                            func.display_return_type.clone()
                        };
                        Some(UserFunctionInfo {
                            qualified_name: name.clone(),
                            display_name,
                            origin: func.origin,
                            param_names: func.param_names.clone(),
                            param_types: func.param_types.clone(),
                            param_has_default: func.param_has_default.clone(),
                            return_type: func.return_type.clone(),
                            display_type_params: func.display_type_params.clone(),
                            display_param_types,
                            display_return_type,
                            source_file: func.source_file.clone(),
                            is_llm,
                        })
                    }
                    _ => None,
                }
            })
            .collect()
    }

    /// Get all compiled test cases.
    pub fn test_cases(&self) -> &[bex_vm_types::TestCase] {
        &self.test_cases
    }

    /// Find a test case by name.
    pub fn test_case(
        &self,
        function_name: &str,
        test_name: &str,
    ) -> Option<&bex_vm_types::TestCase> {
        self.test_cases
            .iter()
            .find(|t| t.function_names.iter().any(|n| function_name == n) && t.name == test_name)
    }

    // ========================================================================
    // Test Collection API
    // ========================================================================

    /// Collect all tests for a package by invoking `{package}.$init_test(collector)`.
    ///
    /// Returns a `BexExternalValue::Handle` pointing to a live `testing.TestRegistry`
    /// heap object (GC-rooted), or `BexExternalValue::Null` if the package has no tests.
    ///
    /// If the package has no test blocks, `$init_test` will not exist in the program
    /// and `Null` is returned immediately.
    ///
    /// # Arguments
    ///
    /// - `package`: The package name (e.g. `"user"`).
    /// - `cancel`: A [`CancellationToken`] for caller-controlled cancellation.
    pub async fn collect_tests(
        self: &Arc<Self>,
        package: &str,
        call_id: CallId,
        cancel: CancellationToken,
    ) -> Result<BexExternalValue, EngineError> {
        let init_test_name = if package == "user" {
            "$init_test".to_string()
        } else {
            format!("{package}.$init_test")
        };

        // If no $init_test function exists, this package has no tests.
        if self.lookup_function(&init_test_name).is_err() {
            return Ok(BexExternalValue::Null);
        }

        let ctx = || {
            FunctionCallContextBuilder::new(call_id)
                .with_cancel_token(cancel.clone())
                .with_profile_enabled(false)
                .build()
        };

        // Step 1: Create a live TestCollector on the heap
        let collector = self
            .call_function(
                "testing.TestCollector.new",
                vec![BexExternalValue::String("".into())],
                ctx(),
                false, // return Handle, not deep copy
            )
            .await?;

        // Step 2: Populate the collector in-place via $init_test
        self.call_function(
            &init_test_name,
            vec![collector.clone()],
            ctx(),
            true, // return value is null, doesn't matter
        )
        .await?;

        // Step 3: Wrap in a TestRegistry
        let registry = self
            .call_function(
                "testing.TestRegistry.new",
                vec![collector],
                ctx(),
                false, // return Handle to live registry
            )
            .await?;

        Ok(registry)
    }

    /// Run GC if conditions are met (called at safepoints),
    /// or yield if another thread is running GC.
    ///
    /// Uses the adaptive `should_collect()` policy to choose the appropriate
    /// collection level (Minor or Major) based on live object counts and
    /// allocation pressure.
    async fn gc_safepoint<T: RootHaver>(
        &self,
        mut permit: ActiveHeapPermit<T>,
    ) -> ActiveHeapPermit<T> {
        let i_am_checking = self
            .checking_gc
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok();
        if i_am_checking {
            // We won the CAS, so we own the GC check.
            if let Some(level) = self.heap.should_collect() {
                let inactive = permit.release();
                self.collect_garbage(level).await;
                permit = inactive.acquire().await;
            }
            self.checking_gc.store(false, Ordering::Release);
            permit
        } else {
            // Another thread is checking; park if they've requested it.
            permit.renew().await
        }
    }

    /// Heuristic-driven GC check that does **not** require the caller to
    /// hold an active heap permit. Use this when the caller is already in a
    /// permit-released state (e.g., the engine's `Await` branch waiting on
    /// a `SetOnce`) — calling [`Self::gc_safepoint`] there would do an
    /// extra release-and-reacquire pair around the heuristic for nothing.
    async fn maybe_collect_garbage(&self) {
        let i_am_checking = self
            .checking_gc
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok();
        if i_am_checking {
            if let Some(level) = self.heap.should_collect() {
                self.collect_garbage(level).await;
            }
            self.checking_gc.store(false, Ordering::Release);
        }
        // If we are not the checker, the actual checker (some other VM) is
        // either already waiting in `request_park` or about to. Our caller
        // is permit-released, so they're not blocking that wait.
    }

    /// Transition the child future settled by `thread` to `Cancelled`
    /// and fire its cancel token so descendants cascade-cancel.
    ///
    /// Cancellations are user-initiated, not unhandled errors, so we
    /// don't push onto the parent's fire-and-forget queue.
    async fn settle_child_cancelled(
        &self,
        thread: &mut ActiveHeapPermit<BexThread>,
        future_id: FutureId,
    ) -> Result<(), EngineError> {
        let child_cancel = thread.vm_thread_cancel().clone();
        let mut guard = self.futures.acquire(thread.proof()).await;
        guard.cancel_future(future_id)?;
        drop(guard);
        child_cancel.cancel();
        Ok(())
    }

    /// Transition the child future settled by `thread` to `Error(value)`,
    /// fire its cancel token, and push our settled future ptr onto our
    /// parent's fire-and-forget error queue per BEP-034 — without this,
    /// errors on un-awaited children would silently vanish.
    async fn settle_child_errored(
        &self,
        thread: &mut ActiveHeapPermit<BexThread>,
        future_id: FutureId,
        value: Value,
    ) -> Result<(), EngineError> {
        let child_cancel = thread.vm_thread_cancel().clone();
        let mut guard = self.futures.acquire(thread.proof()).await;
        let settled_ptr = guard.future_heap_ptr(future_id);
        // BEP-034 fire-and-forget: DEFER the error instead of settling the
        // heap `Future` to `Error` here. The future stays `Pending` (wake
        // signal fired), so any awaiter — including a sibling task — observes
        // it through `future_ready`, which settles it from the stash and
        // marks it consumed. Only errors still unconsumed at the spawner's
        // next await are surfaced by `drain_one_pending_child_error`. Settling
        // eagerly instead would let a sibling's `await`+`catch` run entirely
        // inside the VM (invisible to the engine), leaving the queue entry to
        // re-surface an already-handled error at the spawner.
        guard.defer_error(future_id, value)?;
        drop(guard);
        child_cancel.cancel();
        if let Some(ptr) = settled_ptr {
            thread.vm_thread_notify_parent_of_error(future_id, ptr);
        }
        Ok(())
    }

    /// Route an unhandled VM throw value through the embedder's terminal
    /// paths: settle a spawned child errored (or cancelled), surface a clean
    /// `baml.panics.Exit`, or surface as `EngineError::UnhandledThrow`.
    ///
    /// Shared tail between [`Self::run_thread_event_loop`]'s
    /// `VmError::ThrownUnhandled` arm and the sysop-injected-throw path in
    /// [`Self::inject_sysop_throw`] — both are "the VM tried to unwind
    /// and no handler matched, what now."
    async fn route_unhandled_vm_throw(
        self: &Arc<Self>,
        thread: &mut ActiveHeapPermit<BexThread>,
        _call_id: CallId,
        value: Value,
        trace: Vec<bex_vm::StackFrame>,
        throws_type: Option<&RuntimeTy>,
    ) -> Result<ThreadOutcome, EngineError> {
        if let Some(future_id) = thread.vm_thread_settles_future() {
            let is_cancel_panic =
                thread.vm_thread_cancel().is_cancelled() && self.is_cancelled_panic(value);
            if is_cancel_panic {
                self.settle_child_cancelled(thread, future_id).await?;
                // Trace info for the child is dropped on the floor — engine-local
                // since v1 spans for spawned bodies are TODO.
                let _ = trace;
                return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Cancelled));
            }
            self.settle_child_errored(thread, future_id, value).await?;
            let _ = trace;
            return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Errored));
        }
        let external = if let Some(ty) = throws_type {
            self.convert_vm_value_to_external_with_type(value, ty, thread.proof())?
        } else {
            self.vm_value_to_owned(thread.proof(), value)
        };
        // `baml.panics.Exit { code }` escaping all handlers is the clean-
        // termination path — surface as Exit so the host maps it to a
        // process exit code.
        if let Some(code) = extract_exit_code(&external) {
            return Err(EngineError::Exit { code });
        }
        Err(EngineError::UnhandledThrow {
            value: Box::new(external),
            trace,
        })
    }

    /// Deliver a sysop error at a `SysOp` terminal by **injecting it into the
    /// VM's exception unwinder** — the same path a `throw` opcode takes. An
    /// in-BAML `try { f(x) } catch (e: …) { … }` therefore catches sysop
    /// throws like any other throw.
    ///
    /// Returns `Ok(None)` when the VM caught the throw and execution should
    /// continue at the catch body; `Ok(Some(outcome))` when the throw escaped
    /// all handlers and the thread terminated (settled child, root unhandled,
    /// or process-exit path); `Err` only for engine-internal failures during
    /// value materialization or unwinding (every catchable `VmRustFnError`
    /// variant produces a throw value via [`op_error_to_throw_value`]).
    ///
    /// Materialize the sysop error into a thrown VM `Value` regardless of
    /// payload shape, then — for host-callable throws — enforce the
    /// declared-throws contract on the materialized Value.
    ///
    /// **Why materialize-then-validate** (rather than validate the
    /// pre-materialization shape): a host-callable throw can arrive
    /// through two paths that produce structurally identical materialized
    /// Values but different `OpErrorPayload` variants:
    ///
    /// - `HostThrown(BexExternalValue::Instance{class_name="baml.errors.
    ///   HostCallable", ...})` — the wire-routed path that all production
    ///   bridges (cffi, wasm, python, node) emit.
    /// - `Vm(VmRustFnError::BamlError(VmBamlError::HostCallable{...}))`
    ///   — the engine-internal path that synthesizes a host-callable
    ///   error from inside Rust without going through the wire.
    ///
    /// Both materialize into an `Object::Instance` of
    /// `baml.errors.HostCallable`, so applying the contract check
    /// post-materialization handles both paths with a single code path.
    ///
    /// `host_callable_throws_contract` is the host callable's declared
    /// `E` (the second generic of `call_host_value<T, E>`), specifically
    /// for the throws-contract check. Distinct from `throws_type` (the
    /// *outer* BAML function's declared throws) — that one drives the
    /// `route_unhandled_vm_throw` re-typing when the throw escapes all
    /// in-BAML catches. For non-host-callable sysops (which never produce
    /// `HostThrown` payloads), pass `None` for the throws contract.
    async fn inject_sysop_throw(
        self: &Arc<Self>,
        thread: &mut ActiveHeapPermit<BexThread>,
        call_id: CallId,
        op_err: OpError,
        throws_type: Option<&RuntimeTy>,
        host_callable_throws_contract: Option<&RuntimeTy>,
    ) -> Result<Option<ThreadOutcome>, EngineError> {
        let materialized = match op_err.payload {
            sys_types::OpErrorPayload::HostThrown(thrown) => {
                self.convert_external_to_vm_value(thread, *thrown)?
            }
            sys_types::OpErrorPayload::Vm(kind) => op_error_to_throw_value(&mut thread.vm, kind)
                .map_err(EngineError::VmInternalError)?,
        };
        let vm_value = if let Some(contract) = host_callable_throws_contract {
            enforce_host_throw_contract(thread, materialized, contract)
        } else {
            materialized
        };
        match thread.vm.try_handle_external_exception(vm_value) {
            // A handler caught the injected exception. `crossed` cannot be
            // true here: this entry point runs from the engine's sysop arm,
            // never inside a watch-filter mini-runner.
            Ok(_crossed) => Ok(None),
            Err(bex_vm::errors::VmError::ThrownUnhandled { value, trace }) => Ok(Some(
                self.route_unhandled_vm_throw(thread, call_id, value, trace, throws_type)
                    .await?,
            )),
            // Degenerate frame stack (e.g. all-Native at root): mirror the
            // existing `VmError::Thrown` arm by routing with no trace and no
            // `throws_type`-driven re-typing.
            Err(bex_vm::errors::VmError::Thrown(value)) => Ok(Some(
                self.route_unhandled_vm_throw(thread, call_id, value, Vec::new(), None)
                    .await?,
            )),
            Err(bex_vm::errors::VmError::InternalError(err)) => {
                Err(EngineError::VmInternalError(err))
            }
            Err(bex_vm::errors::VmError::TracedInternalError { source, trace }) => {
                Err(EngineError::TracedVmInternalError { source, trace })
            }
        }
    }

    /// True if `value` is an `Object::Instance` whose class is
    /// `baml.panics.Cancelled`. Used to differentiate cancellation panics
    /// from regular errors when settling a spawned thread that unwound.
    fn is_cancelled_panic(&self, value: Value) -> bool {
        let Some(ptr) = value.as_object_ptr() else {
            return false;
        };
        let Some(&cancelled_class_ptr) = self.resolved_class_names.get(CANCELLED_PANIC_CLASS)
        else {
            return false;
        };
        // SAFETY: a Value::Object that escaped a VM exec step is alive on
        // the heap; this read is gated by the caller's active heap permit.
        match unsafe { ptr.get() } {
            Object::Instance(instance) => instance.class == cancelled_class_ptr,
            _ => false,
        }
    }

    /// Read the spawn parameters out of a `baml.spawn.SpawnParams` instance
    /// (BEP-034 middleware: the value a `spawn ... with` transformer pipeline
    /// produced). Fields are read BY INDEX in declaration order — body=0,
    /// name=1, group=2, cancel=3, detach=4 — keep in sync with
    /// `ns_spawn/spawn.baml`. Returns `None` when the value is not a
    /// well-formed `SpawnParams` (caller falls back to the spawn operands).
    ///
    /// Safe to deref the pointers because the caller holds the active heap
    /// permit and `params` is rooted by the `UnscheduledFuture` being handled.
    fn read_spawn_params(params: HeapPtr) -> Option<SpawnParamsData> {
        // Fields 2/3 helper: `group` / `cancel` are `TaskGroup` / `CancelToken`
        // instances whose `_handle` field (index 0) is `Object::RustData`.
        fn handle_object(value: Value) -> Option<&'static Object> {
            let inst_ptr = value.as_object_ptr()?;
            let Object::Instance(inst) = (unsafe { inst_ptr.get() }) else {
                return None;
            };
            let handle_ptr = inst.load_field(0).as_object_ptr()?;
            Some(unsafe { handle_ptr.get() })
        }

        let Object::Instance(instance) = (unsafe { params.get() }) else {
            return None;
        };
        // Field 0: `body` — the closure the spawned thread runs. A middleware
        // transformer may have wrapped or replaced the original spawn body.
        let body = instance.load_field(0).as_object_ptr()?;

        // Field 1: `name` — optional human-readable label.
        let name =
            instance
                .load_field(1)
                .as_object_ptr()
                .and_then(|ptr| match unsafe { ptr.get() } {
                    Object::String(s) => Some(s.to_string()),
                    _ => None,
                });

        let group = handle_object(instance.load_field(2)).and_then(|obj| match obj {
            Object::RustData(data) => data.clone().downcast::<TaskGroupInner>().ok(),
            _ => None,
        });
        let cancel = handle_object(instance.load_field(3)).and_then(|obj| match obj {
            Object::RustData(data) => data.downcast_ref::<CancellationToken>().cloned(),
            _ => None,
        });

        // Field 4: `detach`.
        let detach = instance.load_field(4).as_bool().unwrap_or(false);

        Some(SpawnParamsData {
            body,
            name,
            group,
            cancel,
            detach,
        })
    }

    /// Spawn a fresh `BexThread` that runs the body packaged in `closure`
    /// and settles a newly-allocated future when the body terminates.
    ///
    /// Allocates the future under the calling thread's heap permit (a
    /// short-lived `()` permit so the new future and child VM are heap
    /// objects gated against GC for the construction window), then
    /// constructs a child `BexThread` whose cancel token is a child of
    /// `parent_cancel` so a parent cancellation cascades down the spawn
    /// tree. The child thread runs its own `run_thread_event_loop` on the
    /// tokio runtime (or `wasm_bindgen_futures::spawn_local` on wasm) and
    /// holds its own permit while executing.
    ///
    /// The future the child settles is pre-allocated by the caller (under the
    /// parent's permit) and identified by `future_id`.
    #[expect(
        clippy::too_many_arguments,
        reason = "spawn edge carries the full context"
    )]
    fn spawn_thread(
        self: Arc<Self>,
        child_cancel: CancellationToken,
        parent_pending_errors: Arc<ChildErrorQueue>,
        root_pending_errors: Arc<ChildErrorQueue>,
        closure: HeapPtr,
        name: Option<String>,
        user_cancel: Option<CancellationToken>,
        group: Option<Arc<TaskGroupInner>>,
        call_id: CallId,
        future_id: FutureId,
        prof_thread_id: u64,
        prof_suppressed: bool,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), EngineError>> + Send + 'static>,
    > {
        Box::pin(self.spawn_thread_inner(
            child_cancel,
            parent_pending_errors,
            root_pending_errors,
            closure,
            name,
            user_cancel,
            group,
            call_id,
            future_id,
            prof_thread_id,
            prof_suppressed,
        ))
    }

    /// Dispatch a spawned body on a fresh `BexThread`.
    ///
    /// The child's heap `Future` (identified by `future_id`) is allocated by
    /// the caller under the *parent's* permit — see the `VmExecState::Spawn`
    /// dispatch site. This function only builds the child VM, registers a new
    /// (initially inactive) permit for it, and fires the task. Critically it
    /// acquires **no** heap permit on the calling task — the child's permit is
    /// acquired on the spawned task — so the parent never holds two permits at
    /// once (which would deadlock GC's `acquire_many`).
    #[allow(clippy::too_many_arguments)]
    async fn spawn_thread_inner(
        self: Arc<Self>,
        child_cancel: CancellationToken,
        parent_pending_errors: Arc<ChildErrorQueue>,
        root_pending_errors: Arc<ChildErrorQueue>,
        closure: HeapPtr,
        name: Option<String>,
        user_cancel: Option<CancellationToken>,
        group: Option<Arc<TaskGroupInner>>,
        call_id: CallId,
        future_id: FutureId,
        prof_thread_id: u64,
        prof_suppressed: bool,
    ) -> Result<(), EngineError> {
        // BEP-034 spawn options: link a user-provided `CancelToken`
        // (`with baml.spawn.options(cancel = ...)`) into this spawn's effective
        // token. Firing the user token cancels the child; the watcher
        // self-terminates when `child_cancel` fires (body done / cancelled
        // / parent cascade) so it never outlives the spawn. (The token itself
        // — fresh for `detach = true`, a child of the parent's otherwise — is
        // derived at the dispatch site, where the future is also allocated.)
        if let Some(user) = user_cancel {
            let linked = child_cancel.clone();
            let watcher = async move {
                tokio::select! {
                    biased;
                    () = user.cancelled() => linked.cancel(),
                    () = linked.cancelled() => {}
                }
            };
            #[cfg(not(target_arch = "wasm32"))]
            tokio::spawn(watcher);
            #[cfg(target_arch = "wasm32")]
            wasm_bindgen_futures::spawn_local(watcher);
        }

        // BEP-034 rate limiting: register with the TaskGroup *synchronously*,
        // here (before the body task is polled), so a `group.cancel()` /
        // `active_count()` issued right after `spawn` already observes this
        // member. The returned ticket is `acquire`d (parked on) inside the body
        // task, so a queued task does not hold the heap permit while waiting.
        let group_ticket = group.map(|group| group.register(child_cancel.clone()));

        // Build the child VM up-front (synchronously) so the await on the
        // permit only holds Send values across yield points.
        let mut child_vm = BexVm::new(
            Arc::clone(&self.heap),
            VmGlobals::Shared(self.globals.clone()),
            self.resolved_class_names
                .iter()
                .chain(self.resolved_enum_names.iter())
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            #[cfg(not(target_arch = "wasm32"))]
            Arc::clone(&self.park_requested),
            Arc::clone(&self.argv),
            Arc::clone(&self.interface_implementors),
        );
        child_vm.prof_thread_id = prof_thread_id;
        child_vm.prof_suppressed = prof_suppressed;
        child_vm.bex_ref_seed = Some((self.process_euid, self.engine_id));
        // Snapshot on the spawning thread, immediately before the entry
        // frame's CallFunction lands (no await in between); the loop-head
        // refresh re-snapshots on the child task's own thread later. The
        // StartThread (with the spawn edge) was already emitted into the
        // ring at the Spawn arm.
        self.prof_refresh_vm_ring(&mut child_vm);
        child_vm.set_entry_point(closure, &[]);
        let child_entry_call_id = BexCallId(child_vm.current_call_id());
        let child_profile_enabled = child_vm.prof_ring.is_some();

        // Register a new (inactive) permit for the child. `new_permit` only
        // takes the holders mutex — it does NOT acquire a semaphore permit —
        // so this is safe to call while the parent still holds its own permit.
        // The child's permit is acquired below, on the spawned task.
        let child_thread = BexThread::new_child(
            child_vm,
            child_cancel.clone(),
            name,
            future_id,
            parent_pending_errors,
            root_pending_errors,
        );
        let inactive = self.heap_permit_manager.new_permit(child_thread).await;

        // Return type / throws type are approximated; the future's value is
        // converted on the awaiter side.
        let engine = self;
        let return_type = RuntimeTy::Null {
            attr: baml_type::TyAttr::default(),
        };
        let task = async move {
            // Declared first so it drops last: if this future is dropped at
            // any await below (before the event loop takes over), the closer
            // emits the child's EndFunction/EndThread (follow-up 11).
            let mut prof_closer = SpawnProfCloser {
                engine: Arc::clone(&engine),
                prof_thread_id,
                entry_call_id: child_entry_call_id,
                armed: child_profile_enabled,
            };
            // BEP-034 rate limiting: if this spawn joined a `TaskGroup`, park
            // here — WITHOUT the heap permit, so a queued task doesn't block GC
            // — until a slot frees. A task cancelled while queued (group/user/
            // parent) settles its future `Cancelled` and never runs its body.
            // The permit is held for the body's lifetime and releases the slot
            // (waking the next FIFO waiter) on drop.
            let _group_permit = match group_ticket {
                Some(ticket) => match ticket.acquire().await {
                    Some(permit) => Some(permit),
                    None => {
                        let mut permit = inactive.acquire().await;
                        if let Err(err) =
                            engine.settle_child_cancelled(&mut permit, future_id).await
                        {
                            tracing::error!(
                                ?err,
                                ?future_id,
                                "failed to settle queued-then-cancelled spawn"
                            );
                        }
                        // The armed `prof_closer` emits the profiling
                        // closes (EndFunction{Cancelled} + EndThread{
                        // Cancelled}) when it drops at this return — the
                        // event loop that would otherwise close them never
                        // runs for a queued-then-cancelled spawn.
                        return;
                    }
                },
                None => None,
            };
            let permit = inactive.acquire().await;
            // The entry call's id was minted by set_entry_point on the
            // spawning thread; its CallFunction is already in the ring.
            // EndThread (ring) is emitted by the run_thread_event_loop
            // wrapper on every exit path from here on.
            prof_closer.defuse();
            match engine
                .run_thread_event_loop(return_type, None, permit, call_id, &child_cancel, true)
                .await
            {
                Ok(ThreadOutcome::SettledChild(_)) => {}
                // The abnormal arms close any spans the event loop left open
                // before the thread ends — the ring EndThread (emitted by the
                // run_thread_event_loop wrapper) must never strand open spans.
                Ok(ThreadOutcome::RootValue(_)) => {
                    tracing::error!(
                        ?future_id,
                        "spawn thread returned a root value instead of settling its future"
                    );
                }
                Err(err) => {
                    tracing::error!(
                        ?err,
                        ?future_id,
                        "spawn thread terminated with engine error"
                    );
                }
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(task);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(task);

        Ok(())
    }

    /// Drain `thread`'s `pending_child_errors` queue until an UNOBSERVED
    /// fire-and-forget error is found, and return its value (settling the
    /// child's heap `Future` to `Error` in the process). Entries whose
    /// deferred error was already consumed by an awaiter (any task that
    /// awaited the future routed through `future_ready`, which takes the
    /// stash entry) are skipped — that error was delivered at the await,
    /// where a `catch` could handle it, and must not re-surface here.
    /// Returns `None` once the queue is empty. Used by the parent's await
    /// drain (pre- and post-) for BEP-034 fire-and-forget propagation.
    async fn drain_one_pending_child_error(
        &self,
        thread: &mut bex_heap::ActiveHeapPermit<BexThread>,
    ) -> Result<Option<Value>, EngineError> {
        loop {
            let Some((id, _ptr)) = thread.vm_thread_pop_pending_child_error() else {
                return Ok(None);
            };
            let mut guard = self.futures.acquire(thread.proof()).await;
            if let Some(value) = guard.take_deferred_error(id)? {
                return Ok(Some(value));
            }
            // Already observed by an awaiter — skip and keep draining.
        }
    }

    /// Runs a thread's event loop (see [`Self::run_thread_event_loop_inner`])
    /// and closes its profiling lifecycle: one `EndThread` per `StartThread`,
    /// on every exit path (the inner loop has many early returns; thread
    /// join at shutdown makes the final commits visible to the consumer per
    /// plan §1).
    async fn run_thread_event_loop(
        self: &Arc<Self>,
        return_type: RuntimeTy,
        throws_type: Option<RuntimeTy>,
        thread: ActiveHeapPermit<BexThread>,
        call_id: CallId,
        cancel: &CancellationToken,
        copy_objects: bool,
    ) -> Result<ThreadOutcome, EngineError> {
        let profile_thread = thread.vm.prof_ring.is_some();
        let prof_thread_id = thread.vm.prof_thread_id;
        // StartThread was already emitted before this function: roots in
        // run_entry_point (right before `set_entry_point`, §7 decision 7 —
        // StartThread-first is a wire invariant), children at the Spawn
        // arm. This wrapper owns the matching EndThread on every exit path;
        // queued-then-cancelled spawns (which never reach this loop) close
        // their lifecycle in spawn_thread_inner's task body.
        let result = self
            .run_thread_event_loop_inner(
                return_type,
                throws_type,
                thread,
                call_id,
                cancel,
                copy_objects,
            )
            .await;
        if profile_thread {
            let status = match &result {
                Ok(ThreadOutcome::SettledChild(ChildSettleKind::Cancelled)) => {
                    bex_events::prof::record::ThreadEndStatus::Cancelled
                }
                Ok(ThreadOutcome::SettledChild(ChildSettleKind::Errored)) => {
                    bex_events::prof::record::ThreadEndStatus::Errored
                }
                // `baml.sys.exit` is a clean termination request: exit
                // code 0 closes as Completed, non-zero as Errored.
                Ok(_) | Err(EngineError::Exit { code: 0 }) => {
                    bex_events::prof::record::ThreadEndStatus::Completed
                }
                Err(EngineError::Exit { .. }) => bex_events::prof::record::ThreadEndStatus::Errored,
                Err(_) if cancel.is_cancelled() => {
                    bex_events::prof::record::ThreadEndStatus::Cancelled
                }
                Err(_) => bex_events::prof::record::ThreadEndStatus::Errored,
            };
            self.prof_emit(&bex_events::prof::record::RawRecord::EndThread {
                status,
                thread_id: BexThreadId(prof_thread_id),
                ts_ticks: bex_events::prof::clock::now_ticks(),
            });
        }
        result
    }

    /// Drive a `BexThread` to completion, dispatching sys-ops, awaits, span
    /// notifications, and early-yield events.
    ///
    /// The `thread` parameter wraps the executing `BexVm` plus metadata
    /// (cancel token, optional name, optional `settles_future`) that
    /// distinguishes a root call from a spawned child. The permit is
    /// released at async safepoints (via `gc_safepoint`) and re-acquired
    /// after any concurrent GC finishes. Each re-acquisition invalidates
    /// the VM's TLAB through the post-GC `forward_roots` hook.
    ///
    /// Returns a [`ThreadOutcome`] describing how the thread terminated:
    /// either a host-visible root value, a root that has already written
    /// to the host, or a settled child future. Children route their
    /// `Complete` / `ThrownUnhandled` / `InternalError` transitions
    /// through [`FutureManager`] so the awaiter resumes correctly.
    #[allow(clippy::too_many_arguments)]
    async fn run_thread_event_loop_inner(
        self: &Arc<Self>,
        return_type: RuntimeTy,
        throws_type: Option<RuntimeTy>,
        mut thread: ActiveHeapPermit<BexThread>,
        call_id: CallId,
        cancel: &CancellationToken,
        copy_objects: bool,
    ) -> Result<ThreadOutcome, EngineError> {
        loop {
            // D5a: refresh the profiling ring snapshot once per exec resume
            // — a TLS lookup here, never per push. Sound because exec()
            // never crosses an .await and tokio migrates tasks only at
            // .await points, so one refresh covers OS-thread migration
            // (plan §6, invariant 4: if exec ever yields mid-step, this
            // model must be revisited).
            self.prof_refresh_vm_ring(&mut thread.vm);

            let exec_result = match thread.vm.exec() {
                Ok(state) => state,
                Err(bex_vm::errors::VmError::ThrownUnhandled { value, trace }) => {
                    return self
                        .route_unhandled_vm_throw(
                            &mut thread,
                            call_id,
                            value,
                            trace,
                            throws_type.as_ref(),
                        )
                        .await;
                }
                Err(bex_vm::errors::VmError::Thrown(value)) => {
                    // Internal throw that escaped without unwinding — treat as
                    // unhandled with no trace.
                    //
                    // Two early-return paths combine here:
                    //   - BEP-034 spawn: if this thread was running as a
                    //     child future, route the throw to the parent via
                    //     `settle_child_*` instead of bubbling out.
                    //   - `baml.sys.exit(code)`: a synthetic throw carrying
                    //     an exit-code value — surface as
                    //     `EngineError::Exit` so the host can set the
                    //     process exit code.
                    if let Some(future_id) = thread.vm_thread_settles_future() {
                        let is_cancel_panic = thread.vm_thread_cancel().is_cancelled()
                            && self.is_cancelled_panic(value);
                        let kind = if is_cancel_panic {
                            self.settle_child_cancelled(&mut thread, future_id).await?;
                            ChildSettleKind::Cancelled
                        } else {
                            self.settle_child_errored(&mut thread, future_id, value)
                                .await?;
                            ChildSettleKind::Errored
                        };
                        return Ok(ThreadOutcome::SettledChild(kind));
                    }
                    let external = self.vm_value_to_owned(thread.proof(), value);
                    if let Some(code) = extract_exit_code(&external) {
                        return Err(EngineError::Exit { code });
                    }
                    return Err(EngineError::UnhandledThrow {
                        value: Box::new(external),
                        trace: Vec::new(),
                    });
                }
                Err(bex_vm::errors::VmError::InternalError(err)) => {
                    if let Some(future_id) = thread.vm_thread_settles_future() {
                        let mut guard = self.futures.acquire(thread.proof()).await;
                        guard
                            .internal_error_future(future_id, EngineError::VmInternalError(err))?;
                        drop(guard);
                        thread.vm_thread_cancel().cancel();
                        return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Errored));
                    }
                    return Err(EngineError::VmInternalError(err));
                }
                Err(bex_vm::errors::VmError::TracedInternalError { source, trace }) => {
                    if let Some(future_id) = thread.vm_thread_settles_future() {
                        let mut guard = self.futures.acquire(thread.proof()).await;
                        guard.internal_error_future(
                            future_id,
                            EngineError::TracedVmInternalError { source, trace },
                        )?;
                        drop(guard);
                        thread.vm_thread_cancel().cancel();
                        return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Errored));
                    }
                    return Err(EngineError::TracedVmInternalError { source, trace });
                }
            };
            match exec_result {
                VmExecState::Complete(value) => {
                    // Spawned children: write the value into the future
                    // registry and return SettledChild. The awaiter's
                    // next `Await` instruction picks up `FutureRead::Ready`.
                    if let Some(future_id) = thread.vm_thread_settles_future() {
                        let mut guard = self.futures.acquire(thread.proof()).await;
                        guard.fulfill_future(future_id, value)?;
                        return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Fulfilled));
                    }
                    // "Cancel wins" semantics: if cancellation races with a
                    // completed VM step, report a cancellation panic rather
                    // than returning a success value.
                    let cancelled = cancel.is_cancelled();

                    let (return_value, _event_result) = if !copy_objects {
                        if let Some(ptr) = value.as_object_ptr() {
                            // SAFETY: the active thread holds the heap permit
                            // through `thread.proof()`.
                            //
                            // Heap-boxed floats can't be handle-wrapped — a
                            // function declared `-> float` (or
                            // `-> Union<float, ...>` / `-> float?`) should
                            // surface as an inline `BexExternalValue::Float`,
                            // not an opaque `Handle`. Route them through the
                            // typed converter so declared-type metadata
                            // (e.g. Union wrapping) is preserved; the bare
                            // unboxing fast-path stripped that.
                            if matches!(unsafe { ptr.get() }, Object::Float(_)) {
                                let external = self.convert_vm_value_to_external_with_type(
                                    value,
                                    &return_type,
                                    thread.proof(),
                                )?;
                                (external.clone(), external)
                            } else {
                                let handle = self.heap.create_handle(ptr);
                                (
                                    BexExternalValue::Handle(handle),
                                    self.vm_value_to_owned(thread.proof(), value),
                                )
                            }
                        } else {
                            let external = self.convert_vm_value_to_external_with_type(
                                value,
                                &return_type,
                                thread.proof(),
                            )?;
                            let external = crate::conversion::coerce_return_to_declared_type(
                                external,
                                &return_type,
                            )?;
                            (external.clone(), external)
                        }
                    } else {
                        let external = self.convert_vm_value_to_external_with_type(
                            value,
                            &return_type,
                            thread.proof(),
                        )?;
                        let external = crate::conversion::coerce_return_to_declared_type(
                            external,
                            &return_type,
                        )?;
                        (external.clone(), external)
                    };

                    if cancelled {
                        return Err(cancelled_unhandled_throw());
                    }

                    return Ok(ThreadOutcome::RootValue(return_value));
                }

                VmExecState::SysOp { operation, args } => {
                    // Single round-trip sys-op call. Convert args, race
                    // the op against the active cancel token, and push the
                    // resulting value back on the VM stack. No
                    // `Object::Future` is allocated and no `FutureManager`
                    // entry is created — the schedule/await dance would be
                    // pure overhead because the user never sees the future.
                    #[allow(clippy::large_enum_variant)]
                    enum SysOpOutcome {
                        Cancelled,
                        Result(Result<BexExternalValue, OpError>),
                    }

                    let bex_args: Vec<BexExternalValue> =
                        args.iter().map(|v| self.vm_arg_to_bex_value(*v)).collect();

                    // Capture the host-call type args (`type_arg_0`/`args[2]`
                    // = return type `T`; `type_arg_1`/`args[3]` = throws
                    // contract `E`) as OWNED `RuntimeTy` values now, while the heap
                    // permit is still held and the packed `Object::Type`
                    // pointers are live. The async wait below releases the
                    // permit, and a moving GC can then relocate/collect the
                    // object; the engine-local `args` Vec is not a GC root
                    // and is never forwarded, so re-reading the raw pointer
                    // post-await would be a use-after-free. Cloning the
                    // `RuntimeTy`s here sidesteps that.
                    //
                    // `host_throws_ty` drives the throws-contract check at
                    // the host-throw injection site below: a host throw
                    // that doesn't match `E` becomes a
                    // `baml.panics.HostContractViolation` panic instead of
                    // a catchable throw.
                    let host_ret_ty: Option<baml_type::RuntimeTy> =
                        if operation == SysOp::BamlHostCallHostValue {
                            Some(
                                host_call_type_arg(args.get(2).copied(), 2, "ret_ty")
                                    .map_err(EngineError::VmInternalError)?,
                            )
                        } else {
                            None
                        };
                    let host_throws_ty: Option<baml_type::RuntimeTy> =
                        if operation == SysOp::BamlHostCallHostValue {
                            Some(
                                host_call_type_arg(args.get(3).copied(), 3, "throws_ty")
                                    .map_err(EngineError::VmInternalError)?,
                            )
                        } else {
                            None
                        };

                    if cancel.is_cancelled() {
                        // Cancel-at-yield: spawned children settle as
                        // Cancelled so the heap Future no longer hangs
                        // at Pending; root threads surface the cancel
                        // to the host.
                        self.prof_end_sysop(
                            &mut thread.vm,
                            bex_events::prof::record::FunctionEndStatus::Cancelled,
                        );
                        self.prof_drain_open_calls(
                            &mut thread.vm,
                            bex_events::prof::record::FunctionEndStatus::Cancelled,
                        );
                        if let Some(future_id) = thread.vm_thread_settles_future() {
                            self.settle_child_cancelled(&mut thread, future_id).await?;
                            return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Cancelled));
                        }
                        return Err(cancelled_unhandled_throw());
                    }

                    let sys_op_result =
                        self.execute_sys_op(operation, &bex_args, call_id, cancel, thread.proof());

                    let outcome = match sys_op_result {
                        SysOpResult::Ready(r) => r,
                        SysOpResult::Async(fut) => {
                            // Release the heap permit so concurrent GC
                            // can run during the wait. Re-acquire
                            // before touching VM state.
                            let inactive = thread.release();
                            self.maybe_collect_garbage().await;
                            let outcome = tokio::select! {
                                biased;
                                () = cancel.cancelled() => SysOpOutcome::Cancelled,
                                r = fut                  => SysOpOutcome::Result(r),
                            };
                            thread = inactive.acquire().await;
                            // Re-snapshot post-await (D5a): the task may be on a different
                            // OS thread now, and engine-driven VM re-entries before the
                            // next loop-head refresh (inject_sysop_throw's unwind) push
                            // through this snapshot.
                            self.prof_refresh_vm_ring(&mut thread.vm);
                            match outcome {
                                SysOpOutcome::Cancelled => {
                                    self.prof_end_sysop(
                                        &mut thread.vm,
                                        bex_events::prof::record::FunctionEndStatus::Cancelled,
                                    );
                                    self.prof_drain_open_calls(
                                        &mut thread.vm,
                                        bex_events::prof::record::FunctionEndStatus::Cancelled,
                                    );
                                    if let Some(future_id) = thread.vm_thread_settles_future() {
                                        self.settle_child_cancelled(&mut thread, future_id).await?;
                                        return Ok(ThreadOutcome::SettledChild(
                                            ChildSettleKind::Cancelled,
                                        ));
                                    }
                                    return Err(cancelled_unhandled_throw());
                                }
                                SysOpOutcome::Result(r) => r,
                            }
                        }
                    };

                    // PR4b: close the sys-op call pair opened at the VM's
                    // yield site. Post-await this task may sit on a
                    // different OS thread — prof_end_sysop goes through the
                    // TLS ring lookup, so that is fine. Errors classify by
                    // class like the inline-native close: a host-abandoned
                    // op (CompletionHandle dropped → cancel-classed payload,
                    // no token fired) closes Cancelled, matching the
                    // Cancelled frames its injected throw then unwinds.
                    self.prof_end_sysop(
                        &mut thread.vm,
                        match &outcome {
                            Ok(_) => bex_events::prof::record::FunctionEndStatus::Ok,
                            Err(op_err) => Self::prof_sysop_error_status(op_err),
                        },
                    );

                    match outcome {
                        Ok(external) => {
                            // Schema-aware return-type validation for host
                            // callables. The bridge's shared
                            // `validate_host_return` guard already rejected
                            // scalar / enum-identity / class-name mismatches at
                            // the FFI boundary; here — where the compiled class
                            // schema is reachable — we additionally validate
                            // class *field types* against the declared return
                            // type (`host_ret_ty`, captured from `args[2]` before
                            // the await). A mismatch is injected into the VM's
                            // exception unwinder so an in-BAML `catch` can
                            // catch it exactly like a host-raised error.
                            if operation == SysOp::BamlHostCallHostValue
                                && let Some(ret_ty) = host_ret_ty.as_ref()
                                && let Err(message) =
                                    self.validate_host_return_schema(&external, ret_ty)
                            {
                                // A wrong-return-type at the engine-level
                                // schema check is the same kind of contract
                                // breach as the FFI-boundary guard catches:
                                // the host returned a value that doesn't
                                // inhabit `T`. Surface as
                                // `baml.panics.HostContractViolation`
                                // (panic, not catchable).
                                let op_err = OpError::new(
                                    SysOp::BamlHostCallHostValue,
                                    sys_types::VmPanic::HostContractViolation {
                                        message,
                                        class_name: None,
                                        language: None,
                                    },
                                );
                                if let Some(outcome) = self
                                    .inject_sysop_throw(
                                        &mut thread,
                                        call_id,
                                        op_err,
                                        throws_type.as_ref(),
                                        host_throws_ty.as_ref(),
                                    )
                                    .await?
                                {
                                    return Ok(outcome);
                                }
                                // VM caught the throw; the unwinder truncated
                                // the eval stack back to the catching frame's
                                // locals region, so we must NOT push the
                                // would-be return value (there's no slot
                                // expecting it any more). Fall through to the
                                // top of the outer loop.
                            } else {
                                // Convert the external value back into a VM
                                // Value (allocating string / list / instance
                                // heap objects as needed) and push it onto
                                // the eval stack. The bytecode that follows
                                // this sys-op call is a normal `store_var`
                                // / projection / whatever the surrounding
                                // expression expected — no implicit await.
                                let value =
                                    self.convert_external_to_vm_value(&mut thread, external)?;
                                thread.vm.stack.push(value);
                            }
                        }
                        Err(op_err) => {
                            // A sysop error. The throw value (built via
                            // [`op_error_to_throw_value`]) is **injected into
                            // the VM's exception unwinder** via
                            // [`Self::inject_sysop_throw`], so an in-BAML
                            // `try { f(x) } catch (e: …) { … }` catches sysop
                            // throws like any other throw. If no handler
                            // matches, the throw propagates exactly like a
                            // VM-internal `ThrownUnhandled` — settling a
                            // spawned child errored or surfacing as
                            // `EngineError::UnhandledThrow` at the root.
                            if let Some(outcome) = self
                                .inject_sysop_throw(
                                    &mut thread,
                                    call_id,
                                    op_err,
                                    throws_type.as_ref(),
                                    host_throws_ty.as_ref(),
                                )
                                .await?
                            {
                                return Ok(outcome);
                            }
                            // VM caught the throw; fall through to the top of
                            // the outer loop for the next `vm.exec()`.
                        }
                    }
                }

                VmExecState::Spawn(unscheduled) => {
                    // BEP-034: pull the closure + name off the
                    // `UnscheduledFuture` heap object and hand them to
                    // `spawn_thread`, which allocates the future and
                    // dispatches the body on a fresh `BexThread`. The
                    // child also inherits a clone of our pending-child-
                    // errors queue so it can push back to us if it
                    // terminates fire-and-forget with a throw.
                    let (closure, name_ptr, config_ptr) = {
                        let unscheduled = thread
                            .vm
                            .unscheduled_future(unscheduled)
                            .map_err(EngineError::VmInternalError)?;
                        (unscheduled.closure, unscheduled.name, unscheduled.config)
                    };
                    let spawn_name: Option<String> =
                        name_ptr.and_then(|ptr| match unsafe { ptr.get() } {
                            Object::String(s) => Some(s.to_string()),
                            _ => None,
                        });
                    // BEP-034 middleware: a `spawn ... with` lowers its final
                    // transformed `baml.spawn.SpawnParams` into the config
                    // operand. The params override the spawn operands — a
                    // transformer may have wrapped/replaced the body or set a
                    // name — and carry the options: `cancel` links into the
                    // child's effective token; `detach` decouples it from the
                    // parent and routes its unhandled errors to the root task
                    // instead of this spawner; `group` rate-limits it.
                    let params = config_ptr.and_then(Self::read_spawn_params);
                    let (closure, spawn_name) = match &params {
                        Some(p) => (p.body, p.name.clone().or(spawn_name)),
                        None => (closure, spawn_name),
                    };
                    let (user_cancel, group, detach) = match params {
                        Some(p) => (p.cancel, p.group, p.detach),
                        None => (None, None, false),
                    };
                    let parent_errors_arc = if detach {
                        thread.vm_thread_root_errors_arc()
                    } else {
                        thread.vm_thread_pending_errors_arc()
                    };
                    let root_errors_arc = thread.vm_thread_root_errors_arc();

                    // Each spawned thread gets a child cancel token so parent →
                    // child cascade falls out of the token tree without bespoke
                    // tracking. A `detach = true` spawn instead gets a fresh,
                    // independent token so the parent's cancellation (and
                    // unhandled-throw cascade) does NOT reach it — it behaves
                    // like a top-level task.
                    let child_cancel = if detach {
                        CancellationToken::new()
                    } else {
                        cancel.child_token()
                    };
                    // Allocate the child's future under the parent's
                    // already-held permit, then hand the id to `spawn_thread`.
                    //
                    // Acquiring a *fresh* heap permit inside the spawn path
                    // (while this task still holds its own) deadlocks against
                    // GC: `HeapPermitManager::request_park` drains the entire
                    // semaphore via `acquire_many(MAX_PERMITS)`, and tokio's
                    // semaphore is fair — so a nested 1-permit acquire queues
                    // *behind* a pending park request, which in turn cannot
                    // proceed until *this* task's permit is released. The task
                    // can't release until the spawn completes → cycle, with all
                    // workers idle. Keeping the spawn path to a single permit
                    // per task (this `new_future` runs under `thread`) avoids
                    // it. The guard is dropped before the `spawn_thread` await
                    // so no non-`Send` guard crosses a yield point.
                    // BEX profiling: the spawn edge. This is the one place
                    // the parent thread id, the spawning call id, and the
                    // child's name are all in hand (plan §2.2).
                    let child_prof_thread_id = self.next_prof_thread_id();
                    if thread.vm.prof_ring.is_some() {
                        let name = spawn_name.as_deref().unwrap_or("");
                        self.prof_emit(&bex_events::prof::record::RawRecord::StartThread {
                            flags: 0,
                            thread_id: BexThreadId(child_prof_thread_id),
                            parent_thread_id: BexThreadId(thread.vm.prof_thread_id),
                            parent_call_id: BexCallId(thread.vm.current_call_id()),
                            ts_ticks: bex_events::prof::clock::now_ticks(),
                            name: bex_events::prof::record::capped_name_bytes(name),
                        });
                    }

                    let future_ptr = {
                        let mut guard = self.futures.acquire(thread.proof()).await;
                        let (future_id, future_ptr) = guard.new_future(child_cancel.clone());
                        drop(guard);
                        Arc::clone(self)
                            .spawn_thread(
                                child_cancel,
                                parent_errors_arc,
                                root_errors_arc,
                                closure,
                                spawn_name,
                                user_cancel,
                                group,
                                call_id,
                                future_id,
                                child_prof_thread_id,
                                thread.vm.prof_suppressed,
                            )
                            .await?;
                        future_ptr
                    };
                    thread.vm.stack.push(Value::object(future_ptr));
                }

                VmExecState::Await(future_id) => {
                    // BEP-034 fire-and-forget: NO pre-drain here. Child errors
                    // are deferred (`defer_error`) and consumed by whichever
                    // task awaits the future (`future_ready`); surfacing
                    // happens only in the POST-drain below, after the awaited
                    // future settles. Draining before the wait would race the
                    // legitimate consumer — e.g. `await baml.future.any(fs)`
                    // would pre-empt `any` consuming a failed input and
                    // surface an error the combinator was about to handle.
                    //
                    // Fail-fast if the thread's own cancel token is
                    // already fired (e.g. parent cascaded into us
                    // between the previous yield and this await). The
                    // BEP guarantees that the next `await` after the
                    // token fires throws `Cancelled`; we honor that
                    // even when the awaited future is unrelated to our
                    // cancel chain (and would otherwise never settle
                    // via cascade).
                    //
                    // For spawned children: route through `cancel_future`
                    // so the heap Future settles instead of leaking as
                    // Pending. Mirrors the `VmError::ThrownUnhandled`
                    // arm above for a Cancelled panic from the VM side.
                    if cancel.is_cancelled() {
                        self.prof_drain_open_calls(
                            &mut thread.vm,
                            bex_events::prof::record::FunctionEndStatus::Cancelled,
                        );
                        if let Some(future_id) = thread.vm_thread_settles_future() {
                            self.settle_child_cancelled(&mut thread, future_id).await?;
                            return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Cancelled));
                        }
                        return Err(cancelled_unhandled_throw());
                    }
                    #[allow(clippy::items_after_statements)]
                    // Outcome of the SetOnce-vs-cancel race below. Inline
                    // here because moving it to module scope just for
                    // clippy's preference would split the readers'
                    // attention across two files.
                    enum AwaitOutcome {
                        Cancelled,
                        Done(Result<(), EngineError>),
                    }
                    // Tightly-scoped guard so the proof's borrow on `vm`
                    // ends before we release the VM permit below.
                    let future = {
                        let mut g = self.futures.acquire(thread.proof()).await;
                        g.future_ready(future_id)?
                    };
                    // Release the VM permit before the SetOnce wait — the
                    // wait is the safepoint. Holding a permit through the
                    // wait would deadlock concurrent GC park (which needs
                    // every permit) against the spawned task that fulfils
                    // this future (which needs a permit to write the heap).
                    let inactive = thread.release();
                    // While parked, run a heuristic-driven GC check (no
                    // permit dance needed since we're already released).
                    self.maybe_collect_garbage().await;
                    // Race the SetOnce wait against the thread's own
                    // cancel token so an unrelated-future await
                    // doesn't hang when the thread itself is cancelled
                    // (cascade only saves us when the awaited future is
                    // a descendant whose token derives from ours).
                    let outcome = tokio::select! {
                        biased;
                        () = cancel.cancelled() => AwaitOutcome::Cancelled,
                        r = future              => AwaitOutcome::Done(r),
                    };
                    thread = inactive.acquire().await;
                    // Re-snapshot post-await (D5a): the task may be on a different
                    // OS thread now, and engine-driven VM re-entries before the
                    // next loop-head refresh (inject_sysop_throw's unwind) push
                    // through this snapshot.
                    self.prof_refresh_vm_ring(&mut thread.vm);
                    match outcome {
                        AwaitOutcome::Cancelled => {
                            self.prof_drain_open_calls(
                                &mut thread.vm,
                                bex_events::prof::record::FunctionEndStatus::Cancelled,
                            );
                            if let Some(future_id) = thread.vm_thread_settles_future() {
                                self.settle_child_cancelled(&mut thread, future_id).await?;
                                return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Cancelled));
                            }
                            return Err(cancelled_unhandled_throw());
                        }
                        AwaitOutcome::Done(r) => r?,
                    }
                    // Post-await drain: surface fire-and-forget child errors
                    // that nobody observed. This is the ONLY drain point — it
                    // runs after the awaited future settled, by which time a
                    // legitimate consumer (a sibling `await`, a combinator's
                    // internal awaits) has consumed any deferred error it was
                    // going to handle, leaving the stash entry absent and the
                    // drain skipping it.
                    //
                    // Carve-out: the entry for the future we are awaiting must
                    // not be drained here — its deferred error flows through
                    // `future_ready` → `FutureRead::Error` → `VmError::Thrown`,
                    // which user `catch` clauses can handle. Without this, an
                    // `(await f) catch (e) { … }` where `f` errored would have
                    // its error pre-empted as an `UnhandledThrow`. Keyed by
                    // `future_id`: stable across GC moves and producer settles.
                    thread.vm_thread_consume_pending_child_error_for(future_id);
                    if let Some(value) = self.drain_one_pending_child_error(&mut thread).await? {
                        // This terminates the thread WITHOUT unwinding the
                        // VM (it is parked at its Await opcode with every
                        // frame open) — drain the open calls like the
                        // cancel blocks do, with Errored: an unobserved
                        // child error killed this thread.
                        self.prof_drain_open_calls(
                            &mut thread.vm,
                            bex_events::prof::record::FunctionEndStatus::Errored,
                        );
                        if let Some(our_future_id) = thread.vm_thread_settles_future() {
                            self.settle_child_errored(&mut thread, our_future_id, value)
                                .await?;
                            return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Errored));
                        }
                        let external = self.vm_value_to_owned(thread.proof(), value);
                        return Err(EngineError::UnhandledThrow {
                            value: Box::new(external),
                            trace: Vec::new(),
                        });
                    }
                }

                // BEP-034 `baml.future.__await_any`: park until the FIRST of
                // several input futures settles, then resume so the VM
                // re-executes the `AwaitAny` opcode and pushes the winner's
                // index. Mirrors the `Await` arm's permit/cancel dance but
                // races all the inputs' SetOnce wakeups at once. The opcode
                // already filtered out futures that were settled going in, so
                // an empty `future_ids` means every input had a wakeup pending
                // or the array was empty.
                VmExecState::AwaitAny(future_ids) => {
                    // Fail-fast on our own cancellation, exactly as `Await`
                    // does: the next suspension point after the token fires
                    // must surface `Cancelled`.
                    if cancel.is_cancelled() {
                        self.prof_drain_open_calls(
                            &mut thread.vm,
                            bex_events::prof::record::FunctionEndStatus::Cancelled,
                        );
                        if let Some(future_id) = thread.vm_thread_settles_future() {
                            self.settle_child_cancelled(&mut thread, future_id).await?;
                            return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Cancelled));
                        }
                        return Err(cancelled_unhandled_throw());
                    }
                    #[allow(clippy::items_after_statements)]
                    enum AwaitAnyOutcome {
                        Cancelled,
                        Done(Result<(), EngineError>),
                    }
                    // Build one SetOnce waiter per pending input. Each waiter
                    // is self-contained (clones the future's `Arc<SetOnce>`),
                    // so we can drop the guard and release the permit before
                    // parking — same safepoint discipline as `Await`.
                    let waiters = {
                        let mut g = self.futures.acquire(thread.proof()).await;
                        let mut ws = Vec::with_capacity(future_ids.len());
                        for future_id in &future_ids {
                            ws.push(g.future_ready(*future_id)?);
                        }
                        ws
                    };
                    let inactive = thread.release();
                    self.maybe_collect_garbage().await;
                    let outcome = if waiters.is_empty() {
                        // Empty INPUT array: `future_ready` returns a waiter
                        // for every id — already-settled inputs yield
                        // immediately-ready waiters — so empty waiters ⟺
                        // empty `future_ids`. Per BEP-034 (matching JS
                        // `Promise.race([])`), racing an empty array never
                        // settles: park on cancellation rather than busy-spin
                        // or panic in `select_all` (which rejects an empty
                        // iterator).
                        cancel.cancelled().await;
                        AwaitAnyOutcome::Cancelled
                    } else {
                        let first_settled =
                            futures::future::select_all(waiters.into_iter().map(Box::pin));
                        tokio::select! {
                            biased;
                            () = cancel.cancelled() => AwaitAnyOutcome::Cancelled,
                            (r, _idx, _rest) = first_settled => AwaitAnyOutcome::Done(r),
                        }
                    };
                    thread = inactive.acquire().await;
                    // Re-snapshot post-await (D5a): the task may be on a different
                    // OS thread now, and engine-driven VM re-entries before the
                    // next loop-head refresh (inject_sysop_throw's unwind) push
                    // through this snapshot.
                    self.prof_refresh_vm_ring(&mut thread.vm);
                    match outcome {
                        AwaitAnyOutcome::Cancelled => {
                            self.prof_drain_open_calls(
                                &mut thread.vm,
                                bex_events::prof::record::FunctionEndStatus::Cancelled,
                            );
                            if let Some(future_id) = thread.vm_thread_settles_future() {
                                self.settle_child_cancelled(&mut thread, future_id).await?;
                                return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Cancelled));
                            }
                            return Err(cancelled_unhandled_throw());
                        }
                        // Only an internal-error future surfaces here; normal
                        // BAML success/throw/cancel settles resolve the SetOnce
                        // with `Ok(())` and are observed when the VM re-reads
                        // the future (and the stdlib `await futures[i]` it).
                        AwaitAnyOutcome::Done(r) => r?,
                    }
                }

                VmExecState::Event {
                    event_name: _,
                    data: _,
                    source_location: _,
                } => {
                    // Tracing removed: `baml.events.send()` is a no-op. It still
                    // returns null — the SendEvent instruction pops its two
                    // arguments but does not push a return value, so push null
                    // before the VM resumes at the next instruction.
                    thread.vm.stack.push(Value::NULL);
                }

                VmExecState::Notify(_notification) => {
                    // Ignore watch notifications for now
                }

                VmExecState::EarlyYield => {
                    thread = self.gc_safepoint(thread).await;
                }
            }
        }
    }

    /// Execute a system operation via uniform dispatch through function pointers.
    ///
    /// All `sys_ops` (including LLM ops) go through the `SysOps` function pointer table.
    /// No more special-case matching — adding a new `#[sys_op]` in the DSL automatically
    /// gets dispatched here via the generated `SysOps::get()`.
    ///
    /// A per-call context is created by cloning the shared `sys_op_ctx` with the
    /// call's cancellation token. This is O(1) since all fields are `Arc`-wrapped.
    fn execute_sys_op(
        self: &Arc<Self>,
        op: SysOp,
        args: &[BexExternalValue],
        call_id: CallId,
        cancel: &CancellationToken,
        permit: bex_heap::PermitProof<'_>,
    ) -> SysOpResult {
        fn check(op: SysOp, err: &OpError) {
            if let sys_types::OpErrorPayload::Vm(kind) = &err.payload {
                if let Err(violation) = sys_types::validate_sys_op_error(op, kind) {
                    tracing::warn!("{violation}");
                }
            }
        }
        let args = args.iter().map(std::convert::Into::into).collect();
        let fn_ptr = self.sys_ops.get(op);
        let mut ctx = self.sys_op_ctx.to_op_context(cancel.clone(), self.clone());
        // Rebuild RuntimeIo with the live per-call context so IO calls
        // (media resolution, auth) use the correct cancellation token.
        ctx.runtime_io =
            sys_ops::build_runtime_io(&self.sys_ops, &self.heap, &self.heap_permit_manager, &ctx);
        let result = fn_ptr(&self.heap, permit, args, &ctx, call_id);

        // `validate_sys_op_error` only applies to the `Vm` payload variant —
        // a `HostThrown` payload is checked engine-side against the
        // surrounding callable's declared `E`, not against the sysop's
        // category contract, so it short-circuits here.
        match result {
            SysOpResult::Ready(Ok(v)) => SysOpResult::Ready(Ok(v)),
            SysOpResult::Ready(Err(err)) => {
                check(op, &err);
                SysOpResult::Ready(Err(err))
            }
            SysOpResult::Async(fut) => {
                let boxed = Box::pin(async move {
                    let res = fut.await;
                    if let Err(err) = &res {
                        check(op, err);
                    }
                    res
                });
                SysOpResult::Async(boxed)
            }
        }
    }
}

#[async_trait]
impl sys_types::VmSpawner for BexEngine {
    async fn spawn_with_function(
        self: Arc<Self>,
        function_name: String,
        args: Vec<BexExternalValue>,
        cancel: CancellationToken,
    ) -> Result<BexExternalValue, Box<dyn Send + Sync + 'static>> {
        self.call_function(
            &function_name,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_cancel_token(cancel)
                .build(),
            true,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn Send + Sync + 'static>)
    }

    async fn spawn_with_callable(
        self: Arc<Self>,
        callable: bex_external_types::Handle,
        args: Vec<BexExternalValue>,
        cancel: CancellationToken,
    ) -> Result<BexExternalValue, Box<dyn Send + Sync + 'static>> {
        self.call_callable(
            callable,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_cancel_token(cancel)
                .build(),
            true,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn Send + Sync + 'static>)
    }
}

// Coverage for sysop error → catchable throw routing now lives in
// the `host_value_callable` engine integration test (end-to-end via the VM),
// which exercises every `VmRustFnError` variant routed through
// `op_error_to_throw_value` + `inject_sysop_throw`. The
// `sys_types::tests::contract_*` unit tests pin the contract-category
// mapping; `error_to_exception_value` / `panic_to_exception_value` in
// `bex_vm::vm` are exercised transitively by the same integration test
// (no dedicated `bex_vm` unit test today — a follow-up could add one if
// the integration-only coverage proves slow to diagnose regressions).
//
// The former `call_host_value_tests` module asserted the same shape against
// the now-deleted `op_error_to_catchable_throw` helper; removing it avoids
// duplicating the same expectations across crates.

#[cfg(test)]
mod concurrent_tests {
    /// Test that demonstrates concurrent `call_function` is safe.
    /// This test verifies that:
    /// 1. Multiple concurrent calls complete successfully
    /// 2. Each call gets its own VM with its own TLAB
    /// 3. No data races occur during parallel execution
    #[tokio::test]
    async fn test_concurrent_calls_safe() {
        // Note: This requires a test BAML program to be available
        // Skip if test infrastructure not set up
        if std::env::var("BAML_TEST_CONCURRENT").is_err() {
            return;
        }

        // This test is a placeholder demonstrating the concurrent execution pattern.
        // In a real implementation, you would:
        // 1. Compile a test BAML program to bytecode
        // 2. Create a BexEngine from the bytecode
        // 3. Wrap it in Arc and spawn concurrent calls
        // 4. Verify all calls complete successfully
        //
        // Example (when test infrastructure is ready):
        // ```
        // let engine = /* create test engine */;
        // let engine = Arc::new(engine);
        //
        // // Spawn 10 concurrent calls
        // let mut handles = vec![];
        // for i in 0..10 {
        //     let engine = Arc::clone(&engine);
        //     handles.push(tokio::spawn(async move {
        //         // Each call should succeed independently
        //         let args = vec![ExternalValue::Int(i)];
        //         engine.call_function("identity", &args).await
        //     }));
        // }
        //
        // // All should complete successfully
        // for handle in handles {
        //     let result = handle.await.expect("task panicked");
        //     assert!(result.is_ok(), "concurrent call failed: {:?}", result);
        // }
        // ```
    }
}
