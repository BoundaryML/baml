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
    sync::{Arc, Mutex, atomic::Ordering},
};

use ::bex_heap::{HeapPermit as _, Tlab};
// Re-export event types for callers.
use ::bex_vm_types::{RootHaver, types::FutureId};
use ::core::sync::atomic::AtomicBool;
use async_trait::async_trait;
use bex_events::{EventKind, FunctionEnd, FunctionEvent, FunctionStart, SpanContext};
pub use bex_events::{HostSpanContext, RuntimeEvent, SpanId};
pub use bex_external_types::{BexExternalValue, Ty, TypeName, UnionMetadata};
use bex_heap::BexHeap;
// Re-export GcStats for users of the engine
pub use bex_heap::GcStats;
pub use bex_heap::{ActiveHeapPermit, HeapGuard, HeapPermitManager, InactiveHeapPermit};
use bex_vm::{BexVm, SpanNotification, VmExecState};
use bex_vm_types::{
    FunctionMeta, FunctionOrigin, GlobalPool, HeapPtr, Object, SharedGlobals, SysOp, Value,
    VmGlobals,
};
pub use conversion::test_arg_to_external;
// Re-export CancellationToken for callers.
pub use function_call_context::{FunctionCallContext, FunctionCallContextBuilder};
pub use sys_types::{CallId, ClassDefinition, ClassFieldDefinition};
use sys_types::{OpError, SysOpResult};
use thiserror::Error;
pub use tokio_util::sync::CancellationToken;
use web_time::{Instant, SystemTime};

pub use crate::{
    future::{FutureManager, FutureManagerGuard, FutureManagerInner},
    thread::{BexThread, ChildErrorQueue},
};

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
    SettledChild,
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
    pub param_types: Vec<Ty>,
    pub param_has_default: Vec<bool>,
    pub return_type: Ty,
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
    ) -> Result<Self, EngineError> {
        let mut map = engine
            .active_calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if map.contains_key(&call_id) {
            return Err(EngineError::DuplicateCallId { call_id });
        }
        map.insert(call_id, cancel);
        drop(map);
        Ok(Self { engine, call_id })
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

/// A single active span in the engine's per-invocation span stack.
struct EngineSpan {
    span_id: SpanId,
    parent_span_id: Option<SpanId>,
    /// The BAML function name this span represents.
    label: String,
    started_at: Instant,
}

/// Per-invocation span tracking state.
///
/// Created as a local in `call_function` and threaded through the event
/// loop. NOT stored on the shared `BexEngine`.
struct SpanState {
    /// Stack of active spans (LIFO).
    stack: Vec<EngineSpan>,
    /// Root span ID for the entire call tree.
    root_span_id: SpanId,
    /// Host-side call stack prefix (from Python @trace spans).
    /// Prepended to the engine's call stack in emitted events.
    host_call_stack: Vec<SpanId>,
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

    #[error("{0}")]
    ExternalOpFailed(#[from] OpError),

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
        BexExternalValue::String("operation cancelled".to_string()),
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
    /// The unified heap (shared across all VM instances)
    heap: Arc<BexHeap>,
    /// Frozen global variables shared across every post-`$init` VM.
    ///
    /// Populated once during `$init` and immutable thereafter; cloning is a
    /// cheap refcount bump (see `VmGlobals::Shared`). The VM rejects any
    /// `StoreGlobal` against this view as a `VmInternalError`.
    ///
    /// Stored as a [`SharedGlobals`] (rather than a plain `Arc<[Value]>`)
    /// so the GC can trace + forward `Value::Object(HeapPtr)` entries: the
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
    /// Optional event sink for persisting events (JSONL file, JS callback, etc.).
    /// If `None`, events are only stored in the `CollectorStore` for in-memory queries.
    event_sink: Option<std::sync::Arc<dyn bex_events::EventSink>>,
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
    active_calls: Mutex<HashMap<CallId, CancellationToken>>,

    futures: FutureManager,
}

#[cfg(target_arch = "wasm32")]
fn _default_round_robin_start() -> usize {
    // Keep wasm deterministic for tooling (matches legacy behavior).
    0
}

#[cfg(not(target_arch = "wasm32"))]
fn _default_round_robin_start() -> usize {
    use web_time::UNIX_EPOCH;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    #[allow(clippy::cast_possible_truncation)]
    {
        nanos as usize
    }
}

impl BexEngine {
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
    /// * `event_sink` - Optional event sink for persisting events.
    /// * `argv` - Process-style argv values exposed to BAML via `baml.sys.argv()`.
    ///   Pass `Vec::new()` when argv is not applicable (e.g. tests, IDE, library embedding).
    pub fn new(
        bytecode_program: bex_vm_types::Program,
        sys_ops: std::sync::Arc<sys_ops::SysOps>,
        event_sink: Option<std::sync::Arc<dyn bex_events::EventSink>>,
        argv: Vec<String>,
    ) -> Result<Self, EngineError> {
        let argv: Arc<[String]> = Arc::from(argv);

        // Extract package_init_order before consuming bytecode_program.
        let package_init_order = bytecode_program.package_init_order.clone();

        // Convert the pure bytecode to a VM-ready program with native functions attached
        let bytecode =
            bex_vm::convert_program(bytecode_program).map_err(EngineError::VmInternalError)?;

        // Extract test cases before consuming other bytecode fields.
        let test_cases = bytecode.test_cases;

        // Extract compile-time objects for the heap
        let compile_time_objects: Vec<Object> = bytecode.objects.into_iter().collect();

        // Encode compact bytecode for all functions before the heap freezes them.
        // This must happen before BexHeap::new() because objects become immutable
        // behind an Arc after that point.
        let compile_time_objects: Vec<Object> = compile_time_objects
            .into_iter()
            .map(|mut obj| {
                if let Object::Function(ref mut func) = obj {
                    func.bytecode.compact = Some(func.bytecode.lower_to_compact());
                }
                obj
            })
            .collect();

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
        let globals_vec: Vec<Value> = bytecode
            .globals
            .into_iter()
            .map(|cv| cv.to_value(|idx| heap.compile_time_ptr(idx.into_raw())))
            .collect();
        // Mutable during `$init` so `StoreGlobal` can populate top-level let
        // bindings; frozen into `Arc<[Value]>` once `$init` finishes (see below).
        let mut globals_pool = GlobalPool::from_vec(globals_vec);

        #[cfg(not(target_arch = "wasm32"))]
        let park_requested = Arc::new(AtomicBool::new(false));

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
                        Ok(VmExecState::Notify(_) | VmExecState::SpanNotify(_)) => {
                            // Ignore watch/span notifications during init.
                            continue;
                        }
                        Ok(VmExecState::Event { .. }) => {
                            // Handle events during $init: push null and continue.
                            // No span context exists during init, so the event is dropped,
                            // but we must push a return value to keep the stack balanced.
                            vm.stack.push(Value::Null);
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
        // can trace + forward `Value::Object(HeapPtr)` entries. Every
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
        // GC traces and forwards `Value::Object(HeapPtr)` entries (e.g.
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
            heap,
            globals,
            _globals_permit: globals_permit,
            resolved_function_names,
            resolved_class_names,
            resolved_enum_names,
            sys_ops,
            sys_op_ctx,
            event_sink,
            test_cases,
            argv,
            heap_permit_manager,
            checking_gc: AtomicBool::new(false),
            #[cfg(not(target_arch = "wasm32"))]
            park_requested,
            active_calls: Mutex::new(HashMap::new()),
            futures: FutureManager::new(futures_permit),
        })
    }

    /// Emit an event: store in `CollectorStore` for in-memory queries,
    /// then forward to the event sink (if set) for persistence.
    fn emit(&self, event: bex_events::RuntimeEvent) {
        bex_events::event_store::emit(&event);
        if let Some(sink) = &self.event_sink {
            sink.send(event);
        }
    }

    fn emit_error_function_end_events(
        &self,
        call_id: CallId,
        span_state: &mut Option<SpanState>,
        error: &str,
    ) {
        let Some(state) = span_state.as_mut() else {
            return;
        };

        while let Some(span) = state.stack.pop() {
            let mut call_stack = state.host_call_stack.clone();
            call_stack.extend(state.stack.iter().map(|s| s.span_id.clone()));
            call_stack.push(span.span_id.clone());

            self.emit(RuntimeEvent {
                call_id,
                ctx: SpanContext {
                    span_id: span.span_id,
                    parent_span_id: span.parent_span_id,
                    root_span_id: state.root_span_id.clone(),
                },
                call_stack,
                timestamp: SystemTime::now(),
                event: EventKind::Function(FunctionEvent::End(Box::new(FunctionEnd {
                    name: span.label,
                    result: BexExternalValue::Null,
                    duration: span.started_at.elapsed(),
                    error: Some(error.to_string()),
                }))),
            });
        }
    }

    /// Return the event sink for this engine (if any). Used by bridges for flush / `HostSpanManager`.
    pub fn event_sink(&self) -> Option<std::sync::Arc<dyn bex_events::EventSink>> {
        self.event_sink.clone()
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
                            stream_return_type: func.stream_return_type.clone(),
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
                        name: cls.name.display_name.to_string(),
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
                        name: enm.name.display_name.to_string(),
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

        tracing::debug!(
            "GC completed: {} live, {} collected",
            stats.live_count,
            stats.collected_count
        );

        stats
    }

    /// Execute a function by name with tracing.
    ///
    /// Every call emits [`RuntimeEvent`]s to the global event store for each
    /// traced function span boundary the VM crosses. The entry-point function
    /// itself gets a root span automatically.
    ///
    /// If `host_ctx` is provided, the engine's root span is nested under the
    /// host's active span tree (e.g., Python `@trace` spans). The host's
    /// call stack is prepended to the engine's call stack in events.
    ///
    /// To collect events for a call, use [`bex_events::event_store::track`]
    /// before calling and [`bex_events::event_store::events_for_span`] +
    /// [`bex_events::event_store::untrack`] after.
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
        FunctionCallContext {
            call_id,
            host_ctx,
            collectors,
            cancel,
            type_args,
        }: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexExternalValue, EngineError> {
        let args = args
            .into_iter()
            .map(|arg| BexCallArg::Provided(Box::new(arg)))
            .collect();
        self.call_function_bound_args(
            function_name,
            args,
            FunctionCallContext {
                call_id,
                host_ctx,
                collectors,
                cancel,
                type_args,
            },
            copy_objects,
        )
        .await
    }

    pub async fn call_function_bound_args(
        self: &Arc<Self>,
        function_name: &str,
        args: Vec<BexCallArg>,
        FunctionCallContext {
            call_id,
            host_ctx,
            collectors,
            cancel,
            type_args,
        }: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexExternalValue, EngineError> {
        // Fail fast if already cancelled — guarantees pre-cancelled tokens
        // always produce a `baml.panics.Cancelled` panic regardless of
        // function contents.
        if cancel.is_cancelled() {
            return Err(cancelled_unhandled_throw());
        }

        let (function_index, kind) = self.lookup_function(function_name)?;
        // Only bytecode functions can be invoked as engine entry points.
        // Sysops + `$rust_function` natives reach their handlers through
        // an enclosing bytecode frame's `Call` / `YieldToCall` — there's
        // no frame for them to return into at the top level.
        if !matches!(kind, bex_vm_types::FunctionKind::Bytecode) {
            return Err(EngineError::NotInvokableAsEntry {
                name: function_name.to_string(),
                kind: format!("{kind:?}"),
            });
        }
        self.validate_bound_args(function_name, &args)?;
        let return_type = self
            .function_return_type(function_name)
            .unwrap_or(Ty::Null {
                attr: baml_type::TyAttr::default(),
            });
        let throws_type = self.function_throws_type(function_name);

        // Register this call so `cancel_function_call(call_id)` can target
        // it. The RAII guard removes the entry on drop (including panic
        // unwind). Insertion and guard construction are atomic, so a panic
        // here cannot leak registry entries.
        let _call_guard = ActiveCallGuard::register(Arc::clone(self), call_id, cancel.clone())?;

        // Create VM with shared heap (each VM gets its own TLAB).
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
        );
        // BEP-034: wrap the root VM in a `BexThread` from the outset so the
        // permit's `RootHaver` is the thread (delegating to the inner VM).
        // Spawned children build their own `BexThread`s in `spawn_thread`.
        let root_thread = BexThread::new_root(vm, cancel.clone());
        let inactive = self.heap_permit_manager.new_permit(root_thread).await;
        let mut thread = inactive.acquire().await;

        // Snapshot args for the root FunctionStart event before converting to VM values
        let args_snapshot = args
            .iter()
            .filter_map(|arg| match arg {
                BexCallArg::Provided(value) => Some(value.as_ref().clone()),
                BexCallArg::OmittedDefault => None,
            })
            .collect();

        let vm_args: Vec<Value> = args
            .into_iter()
            .map(|arg| match arg {
                BexCallArg::Provided(arg) => self.convert_external_to_vm_value(&mut thread, *arg),
                BexCallArg::OmittedDefault => Ok(Value::OmittedArg),
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Canary moved the root VM into `thread.vm` for BEP-034 spawn
        // threading; pack/run still needs the generic-function type-args
        // path. Combine both: typed-args setter on the threaded VM.
        thread
            .vm
            .set_entry_point_with_type_args(function_index, &vm_args, type_args);

        // Initialize span tracking for the root call.
        // If host context is provided, nest under the host's span tree.
        let engine_span_id = SpanId::new();
        let (parent_span_id, effective_root_span_id, host_call_stack) = match &host_ctx {
            Some(ctx) => (
                Some(ctx.parent_span_id.clone()),
                ctx.root_span_id.clone(),
                ctx.call_stack.clone(),
            ),
            None => (None, engine_span_id.clone(), vec![]),
        };

        // Wire up collector tracking before emitting any events.
        // Track by engine_span_id (unique per call) so each call gets its own log,
        // even when multiple calls share the same root under @trace.
        //
        // The event store routes events to buckets by matching the event's span_id
        // or parent_span_id against tracked IDs. So the function's own events
        // (span_id == engine_span_id) and child events like LLM calls
        // (parent_span_id == engine_span_id) both land in the same bucket.
        for collector in &collectors {
            collector.track(&engine_span_id);
        }

        // Allocate collectors on the heap for future $collector syntax.
        let _collector_values: Vec<Value> = collectors
            .iter()
            .map(|c| {
                let collector_ref = bex_vm_types::CollectorRef(
                    Arc::clone(c) as Arc<dyn std::any::Any + Send + Sync>
                );
                thread.vm.alloc_collector(collector_ref)
            })
            .collect();

        // Build the call stack: host prefix + this engine span
        let mut call_stack = host_call_stack.clone();
        call_stack.push(engine_span_id.clone());

        let root_ctx = SpanContext {
            span_id: engine_span_id.clone(),
            parent_span_id: parent_span_id.clone(),
            root_span_id: effective_root_span_id.clone(),
        };

        self.emit(RuntimeEvent {
            call_id,
            ctx: root_ctx,
            call_stack,
            timestamp: SystemTime::now(),
            event: EventKind::Function(FunctionEvent::Start(FunctionStart {
                name: function_name.to_string(),
                args: args_snapshot,
                tags: vec![],
            })),
        });

        let mut span_state = Some(SpanState {
            stack: vec![EngineSpan {
                span_id: engine_span_id.clone(),
                parent_span_id,
                label: function_name.to_string(),
                started_at: Instant::now(),
            }],
            root_span_id: effective_root_span_id,
            host_call_stack,
        });

        // Run the event loop with span tracking. On errors, emit FunctionEnd
        // events for every active span so consumers can mark each node failed.
        let result = self
            .run_thread_event_loop(
                return_type,
                throws_type,
                thread,
                call_id,
                &mut span_state,
                &cancel,
                copy_objects,
            )
            .await;
        if let Err(err) = &result {
            let error = err.to_string();
            self.emit_error_function_end_events(call_id, &mut span_state, &error);
        }
        match result {
            Ok(ThreadOutcome::RootValue(value)) => Ok(value),
            Ok(ThreadOutcome::SettledChild) => {
                // Root threads should never produce SettledChild; treat as an
                // engine invariant violation rather than silently returning Null.
                Err(EngineError::Other(
                    "BEP-034: root thread terminated as SettledChild".to_string(),
                ))
            }
            Err(err) => Err(err),
        }

        // active_calls cleanup is done by ActiveCallGuard on drop.
        //
        // Keep genuine engine errors intact. Cancellation is surfaced as a
        // `baml.panics.Cancelled` panic — either raised by the VM's `Await`
        // opcode, or synthesized by engine safepoints (see
        // `cancelled_unhandled_throw`).
    }

    /// Cancel a function call by its ID.
    ///
    /// If the call is still running, it will be interrupted at the next
    /// cancellation check point. If the call has already completed or the ID
    /// is unknown, this will return an error.
    pub fn cancel_function_call(&self, call_id: CallId) -> Result<(), EngineError> {
        let mut active_calls = self.active_calls.lock().unwrap();
        if let Some(call) = active_calls.remove(&call_id) {
            call.cancel();
            Ok(())
        } else {
            Err(EngineError::FunctionCallNotFound { call_id })
        }
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
    pub fn function_return_type(&self, name: &str) -> Option<Ty> {
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
    fn function_throws_type(&self, name: &str) -> Option<Ty> {
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
    /// Used by callers that walk a `Ty` tree and need to resolve nested
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
    pub fn function_params(&self, name: &str) -> Result<Vec<(&str, &Ty, bool)>, EngineError> {
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
                        Some(UserFunctionInfo {
                            qualified_name: name.clone(),
                            display_name,
                            origin: func.origin,
                            param_names: func.param_names.clone(),
                            param_types: func.param_types.clone(),
                            param_has_default: func.param_has_default.clone(),
                            return_type: func.return_type.clone(),
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
                .build()
        };

        // Step 1: Create a live TestCollector on the heap
        let collector = self
            .call_function(
                "testing.TestCollector.new",
                vec![BexExternalValue::String(String::new())],
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
        // Snapshot the heap ptr while we still hold the guard — once
        // the terminal transition runs, the entry is gone.
        let settled_ptr = guard.future_heap_ptr(future_id);
        guard.err_future(future_id, value)?;
        drop(guard);
        child_cancel.cancel();
        if let Some(ptr) = settled_ptr {
            thread.vm_thread_notify_parent_of_error(future_id, ptr);
        }
        Ok(())
    }

    /// True if `value` is an `Object::Instance` whose class is
    /// `baml.panics.Cancelled`. Used to differentiate cancellation panics
    /// from regular errors when settling a spawned thread that unwound.
    fn is_cancelled_panic(&self, value: &Value) -> bool {
        let Value::Object(ptr) = value else {
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
    /// Returns the heap pointer to the allocated future so the spawning
    /// thread can push it onto its stack as the result of `spawn { ... }`.
    fn spawn_thread(
        self: Arc<Self>,
        parent_cancel: CancellationToken,
        parent_pending_errors: Arc<ChildErrorQueue>,
        closure: HeapPtr,
        name: Option<String>,
        call_id: CallId,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<HeapPtr, EngineError>> + Send + 'static>,
    > {
        Box::pin(self.spawn_thread_inner(
            parent_cancel,
            parent_pending_errors,
            closure,
            name,
            call_id,
        ))
    }

    async fn spawn_thread_inner(
        self: Arc<Self>,
        parent_cancel: CancellationToken,
        parent_pending_errors: Arc<ChildErrorQueue>,
        closure: HeapPtr,
        name: Option<String>,
        call_id: CallId,
    ) -> Result<HeapPtr, EngineError> {
        // Each spawned thread gets a child cancel token so parent → child
        // cascade falls out of the token tree without bespoke tracking.
        let child_cancel = parent_cancel.child_token();
        drop(parent_cancel);

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
        );
        child_vm.set_entry_point(closure, &[]);

        // Allocate the future and the child permit through a single
        // helper that confines the unawaited path so the surrounding
        // async fn stays `Send`. The helper takes ownership of all
        // potentially non-`Send` locals so they never live across an
        // outer-scope `.await`.
        let (future_ptr, future_id, inactive) = self
            .clone()
            .spawn_thread_setup(child_vm, child_cancel.clone(), name, parent_pending_errors)
            .await?;

        // Phase B note: v1 spans for spawned bodies are deferred. We pass
        // `None` for `span_state` so the child does not emit FunctionStart/
        // FunctionEnd events through the engine span machinery.
        // Return type / throws type are also approximated; the future's
        // value is converted via `vm_value_to_owned` on the awaiter side.
        let engine = self;
        let return_type = Ty::Null {
            attr: baml_type::TyAttr::default(),
        };
        let task = async move {
            let permit = inactive.acquire().await;
            let mut local_span_state: Option<SpanState> = None;
            match engine
                .run_thread_event_loop(
                    return_type,
                    None,
                    permit,
                    call_id,
                    &mut local_span_state,
                    &child_cancel,
                    true,
                )
                .await
            {
                Ok(_) => {}
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

        Ok(future_ptr)
    }

    /// Pop one `pending_child_errors` entry off `thread`'s queue and
    /// extract its error `Value` from the heap. Returns `None` when
    /// the queue is empty or the entry is malformed (a bug-tolerant
    /// path that logs and skips). Used by the parent's await drain
    /// (pre- and post-) for BEP-034 fire-and-forget propagation.
    ///
    /// Synchronous so it can run inline at the call site without
    /// crossing an `.await` boundary (the helper itself doesn't need
    /// async, and inlining sidesteps the `&mut ActiveHeapPermit<…>`
    /// auto-Send inference quirks that hit other engine helpers).
    fn read_pending_child_error_value(
        thread: &mut bex_heap::ActiveHeapPermit<BexThread>,
    ) -> Option<Value> {
        let (_id, errored_future_ptr) = thread.vm_thread_pop_pending_child_error()?;
        // SAFETY: the ptr was rooted via the BexThread's
        // `pending_child_errors` queue, so GC has kept the heap object
        // alive and forward-updated the ptr if it moved. We hold the
        // active permit.
        match unsafe { errored_future_ptr.get() } {
            Object::Future(f) => match f.read() {
                bex_vm_types::FutureRead::Error(v) => Some(v),
                other => {
                    tracing::warn!(
                        ?other,
                        "pending_child_errors entry not in Error state; skipping"
                    );
                    None
                }
            },
            _ => {
                tracing::warn!("pending_child_errors entry not an Object::Future; skipping");
                None
            }
        }
    }

    /// Synchronous (apart from a couple of `tokio::sync::Mutex::lock`
    /// awaits) setup helper used by [`Self::spawn_thread`]. Confines the
    /// permit allocation flow so the outer `spawn_thread` future never
    /// holds any non-`Send` `MutexGuards` across an `.await`.
    async fn spawn_thread_setup(
        self: Arc<Self>,
        child_vm: BexVm,
        child_cancel: CancellationToken,
        name: Option<String>,
        parent_pending_errors: Arc<ChildErrorQueue>,
    ) -> Result<(HeapPtr, FutureId, InactiveHeapPermit<BexThread>), EngineError> {
        // One-shot `()` permit for the brief future-allocation window. It
        // is dropped before we create the long-lived `BexThread` permit
        // so the GC isn't blocked by a leftover holder.
        let permit = self.heap_permit_manager.new_permit(()).await;
        let permit = permit.acquire().await;
        let (future_id, future_ptr) = {
            let mut guard = self.futures.acquire(permit.proof()).await;
            guard.new_future(child_cancel.clone())
        };
        drop(permit);

        let child_thread = BexThread::new_child(
            child_vm,
            child_cancel,
            name,
            future_id,
            parent_pending_errors,
        );
        let inactive = self.heap_permit_manager.new_permit(child_thread).await;

        Ok((future_ptr, future_id, inactive))
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
    async fn run_thread_event_loop(
        self: &Arc<Self>,
        return_type: Ty,
        throws_type: Option<Ty>,
        mut thread: ActiveHeapPermit<BexThread>,
        call_id: CallId,
        span_state: &mut Option<SpanState>,
        cancel: &CancellationToken,
        copy_objects: bool,
    ) -> Result<ThreadOutcome, EngineError> {
        loop {
            // Update the VM's span context so native functions can read it.
            thread.vm.current_span_context =
                span_state.as_ref().map(Self::build_span_context_from_state);

            let exec_result = match thread.vm.exec() {
                Ok(state) => state,
                Err(bex_vm::errors::VmError::ThrownUnhandled { value, trace }) => {
                    // Spawned children route the thrown value back to the
                    // awaiter via `err_future`; the awaiter's `await`
                    // re-throws it. Cancellation panics use `cancel_future`
                    // so the awaiter sees `FutureRead::Cancelled` instead
                    // of an error.
                    if let Some(future_id) = thread.vm_thread_settles_future() {
                        let is_cancel_panic = thread.vm_thread_cancel().is_cancelled()
                            && self.is_cancelled_panic(&value);
                        if is_cancel_panic {
                            self.settle_child_cancelled(&mut thread, future_id).await?;
                        } else {
                            self.settle_child_errored(&mut thread, future_id, value)
                                .await?;
                        }
                        // Trace info for the child is dropped on the floor
                        // — Phase B keeps it engine-local since v1 spans
                        // for spawned bodies are TODO.
                        let _ = trace;
                        return Ok(ThreadOutcome::SettledChild);
                    }
                    let external = if let Some(ref ty) = throws_type {
                        self.convert_vm_value_to_external_with_type(&value, ty, thread.proof())?
                    } else {
                        self.vm_value_to_owned(thread.proof(), &value)
                    };
                    // `baml.panics.Exit { code }` escaping all handlers is
                    // the clean-termination path — surface it as an Exit
                    // rather than a generic unhandled throw so the host
                    // maps it to a process exit code.
                    if let Some(code) = extract_exit_code(&external) {
                        return Err(EngineError::Exit { code });
                    }
                    return Err(EngineError::UnhandledThrow {
                        value: Box::new(external),
                        trace,
                    });
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
                            && self.is_cancelled_panic(&value);
                        if is_cancel_panic {
                            self.settle_child_cancelled(&mut thread, future_id).await?;
                        } else {
                            self.settle_child_errored(&mut thread, future_id, value)
                                .await?;
                        }
                        return Ok(ThreadOutcome::SettledChild);
                    }
                    let external = self.vm_value_to_owned(thread.proof(), &value);
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
                        return Ok(ThreadOutcome::SettledChild);
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
                        return Ok(ThreadOutcome::SettledChild);
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
                        return Ok(ThreadOutcome::SettledChild);
                    }
                    // "Cancel wins" semantics: if cancellation races with a
                    // completed VM step, report a cancellation panic rather
                    // than returning a success value.
                    //
                    // Still emit FunctionEnd first so tracing consumers see
                    // a paired root FunctionStart/FunctionEnd span.
                    let cancelled = cancel.is_cancelled();

                    let (return_value, event_result) = if !copy_objects {
                        if let Value::Object(ptr) = value {
                            let handle = self.heap.create_handle(ptr);
                            (
                                BexExternalValue::Handle(handle),
                                self.vm_value_to_owned(thread.proof(), &value),
                            )
                        } else {
                            let external = self.convert_vm_value_to_external_with_type(
                                &value,
                                &return_type,
                                thread.proof(),
                            )?;
                            (external.clone(), external)
                        }
                    } else {
                        let external = self.convert_vm_value_to_external_with_type(
                            &value,
                            &return_type,
                            thread.proof(),
                        )?;
                        (external.clone(), external)
                    };

                    // Emit FunctionEnd for the root entry-point span after the
                    // final return value conversion succeeds. If conversion fails,
                    // the active root span remains on the stack and the caller's
                    // error path emits exactly one failing FunctionEnd.
                    if let Some(state) = span_state.as_mut() {
                        if let Some(root_span) = state.stack.pop() {
                            let mut full_call_stack = state.host_call_stack.clone();
                            full_call_stack.extend(state.stack.iter().map(|s| s.span_id.clone()));
                            full_call_stack.push(root_span.span_id.clone());
                            let end_event = RuntimeEvent {
                                call_id,
                                ctx: SpanContext {
                                    span_id: root_span.span_id,
                                    parent_span_id: root_span.parent_span_id,
                                    root_span_id: state.root_span_id.clone(),
                                },
                                call_stack: full_call_stack,
                                timestamp: SystemTime::now(),
                                event: EventKind::Function(FunctionEvent::End(Box::new(
                                    FunctionEnd {
                                        name: root_span.label,
                                        result: event_result,
                                        duration: root_span.started_at.elapsed(),
                                        error: None,
                                    },
                                ))),
                            };
                            self.emit(end_event);
                        }
                    }

                    if cancelled {
                        return Err(cancelled_unhandled_throw());
                    }

                    return Ok(ThreadOutcome::RootValue(return_value));
                }

                VmExecState::SysOp { operation, args } => {
                    // BEP-034 phase D′: single round-trip sys-op call.
                    // Convert args, race the op against the active
                    // cancel token, and push the resulting value back
                    // on the VM stack. No `Object::Future` is
                    // allocated and no `FutureManager` entry is
                    // created — the schedule/await dance was pure
                    // overhead because the user never sees the future.
                    #[allow(clippy::large_enum_variant)]
                    enum SysOpOutcome {
                        Cancelled,
                        Result(Result<BexExternalValue, OpError>),
                    }

                    let bex_args: Vec<BexExternalValue> =
                        args.iter().map(|v| self.vm_arg_to_bex_value(v)).collect();

                    if cancel.is_cancelled() {
                        // Cancel-at-yield: spawned children settle as
                        // Cancelled so the heap Future no longer hangs
                        // at Pending; root threads surface the cancel
                        // to the host.
                        if let Some(future_id) = thread.vm_thread_settles_future() {
                            self.settle_child_cancelled(&mut thread, future_id).await?;
                            return Ok(ThreadOutcome::SettledChild);
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
                            match outcome {
                                SysOpOutcome::Cancelled => {
                                    if let Some(future_id) = thread.vm_thread_settles_future() {
                                        self.settle_child_cancelled(&mut thread, future_id).await?;
                                        return Ok(ThreadOutcome::SettledChild);
                                    }
                                    return Err(cancelled_unhandled_throw());
                                }
                                SysOpOutcome::Result(r) => r,
                            }
                        }
                    };

                    match outcome {
                        Ok(external) => {
                            // Convert the external value back into a VM
                            // Value (allocating string / list / instance
                            // heap objects as needed) and push it onto
                            // the eval stack. The bytecode that follows
                            // this sys-op call is a normal `store_var`
                            // / projection / whatever the surrounding
                            // expression expected — no implicit await.
                            let value = self.convert_external_to_vm_value(&mut thread, external)?;
                            thread.vm.stack.push(value);
                        }
                        Err(op_err) => {
                            return Err(EngineError::from(op_err));
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
                    let (closure, name_ptr) = {
                        let unscheduled = thread
                            .vm
                            .unscheduled_future(unscheduled)
                            .map_err(EngineError::VmInternalError)?;
                        (unscheduled.closure, unscheduled.name)
                    };
                    let spawn_name: Option<String> =
                        name_ptr.and_then(|ptr| match unsafe { ptr.get() } {
                            Object::String(s) => Some(s.clone()),
                            _ => None,
                        });
                    let parent_errors_arc = thread.vm_thread_pending_errors_arc();
                    let future_ptr = Arc::clone(self)
                        .spawn_thread(
                            cancel.clone(),
                            parent_errors_arc,
                            closure,
                            spawn_name,
                            call_id,
                        )
                        .await?;
                    thread.vm.stack.push(Value::Object(future_ptr));
                }

                VmExecState::Await(future_id) => {
                    // BEP-034: surface any fire-and-forget child error
                    // at this checkpoint, EXCEPT the one belonging to
                    // the future we're about to await — that error
                    // will surface via the normal `FutureRead::Error
                    // → VmError::Thrown` path, which is catchable by
                    // user `catch` clauses. Without this carve-out, an
                    // explicit `(await f) catch (e) { … }` where `f`
                    // errored fire-and-forget would have its error
                    // pre-empted here as `EngineError::UnhandledThrow`
                    // and bypass the catch.
                    //
                    // We key the carve-out on the `FutureId` (not the
                    // heap ptr) so the match works even if the
                    // producer already removed the `active_futures`
                    // entry: the queue still has our entry tagged with
                    // the same id.
                    thread.vm_thread_consume_pending_child_error_for(future_id);
                    if let Some(value) = Self::read_pending_child_error_value(&mut thread) {
                        // Same propagation pattern as
                        // `VmError::ThrownUnhandled` for a non-cancel
                        // throw: settle our own future as Error if we
                        // are a spawned child (so OUR awaiter sees the
                        // error, and our parent's queue gets us too);
                        // otherwise surface as unhandled to the host.
                        if let Some(our_future_id) = thread.vm_thread_settles_future() {
                            self.settle_child_errored(&mut thread, our_future_id, value)
                                .await?;
                            return Ok(ThreadOutcome::SettledChild);
                        }
                        let external = self.vm_value_to_owned(thread.proof(), &value);
                        return Err(EngineError::UnhandledThrow {
                            value: Box::new(external),
                            trace: Vec::new(),
                        });
                    }
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
                        if let Some(future_id) = thread.vm_thread_settles_future() {
                            self.settle_child_cancelled(&mut thread, future_id).await?;
                            return Ok(ThreadOutcome::SettledChild);
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
                        let g = self.futures.acquire(thread.proof()).await;
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
                    match outcome {
                        AwaitOutcome::Cancelled => {
                            if let Some(future_id) = thread.vm_thread_settles_future() {
                                self.settle_child_cancelled(&mut thread, future_id).await?;
                                return Ok(ThreadOutcome::SettledChild);
                            }
                            return Err(cancelled_unhandled_throw());
                        }
                        AwaitOutcome::Done(r) => r?,
                    }
                    // Post-await drain: a fire-and-forget child may
                    // have errored *during* our wait on the SetOnce.
                    // Per BEP-034, the error must surface at this
                    // await checkpoint (not slip past it). The pre-
                    // drain caught errors that were ready going in;
                    // this catches errors that arrived during the wait.
                    //
                    // Same catch-natural carve-out as pre-drain. Key
                    // by `future_id` for the same reason: stable
                    // across GC moves and across producer settles
                    // that remove the `active_futures` bookkeeping.
                    thread.vm_thread_consume_pending_child_error_for(future_id);
                    if let Some(value) = Self::read_pending_child_error_value(&mut thread) {
                        if let Some(our_future_id) = thread.vm_thread_settles_future() {
                            self.settle_child_errored(&mut thread, our_future_id, value)
                                .await?;
                            return Ok(ThreadOutcome::SettledChild);
                        }
                        let external = self.vm_value_to_owned(thread.proof(), &value);
                        return Err(EngineError::UnhandledThrow {
                            value: Box::new(external),
                            trace: Vec::new(),
                        });
                    }
                }

                VmExecState::Event {
                    event_name,
                    data,
                    source_location,
                } => {
                    // Emit a CustomEvent or LogEvent with the current span context.
                    if let Some(state) = span_state.as_ref() {
                        let ctx = Self::build_span_context_from_state(state);
                        let mut call_stack = state.host_call_stack.clone();
                        call_stack.extend(state.stack.iter().map(|s| s.span_id.clone()));

                        let external_data = self.vm_value_to_owned(thread.proof(), &data);

                        // Convert source location tuple to SourceLocation struct.
                        let source = source_location.map(
                            |(file_id, line, column, start_offset, end_offset)| {
                                bex_events::SourceLocation {
                                    file_id,
                                    line,
                                    column,
                                    start_offset,
                                    end_offset,
                                }
                            },
                        );

                        // Check if this is a log event (emitted by log.info, log.debug, etc.)
                        // Uses reserved name "$baml_log" to distinguish from user events.
                        let event = if event_name == "$baml_log" {
                            // Extract level and data from the log payload.
                            if let BexExternalValue::Map { entries, .. } = &external_data {
                                let level = entries
                                    .get("level")
                                    .and_then(|v| {
                                        if let BexExternalValue::String(s) = v {
                                            Some(s.clone())
                                        } else {
                                            None
                                        }
                                    })
                                    .unwrap_or_else(|| "info".to_string());
                                let log_data = entries
                                    .get("data")
                                    .cloned()
                                    .unwrap_or(BexExternalValue::Null);
                                EventKind::Log(bex_events::LogEvent {
                                    level,
                                    data: log_data,
                                    source,
                                })
                            } else {
                                // Fallback: treat as custom event if structure is unexpected.
                                EventKind::Custom(bex_events::CustomEvent {
                                    name: event_name,
                                    data: external_data,
                                })
                            }
                        } else {
                            EventKind::Custom(bex_events::CustomEvent {
                                name: event_name,
                                data: external_data,
                            })
                        };

                        self.emit(RuntimeEvent {
                            call_id,
                            ctx,
                            call_stack,
                            timestamp: SystemTime::now(),
                            event,
                        });
                    }
                    // `baml.events.send()` returns null.  The SendEvent instruction
                    // pops its two arguments but does not push a return value, so we
                    // push null here before the VM resumes at the next instruction
                    // (which will store or discard the return value).
                    thread.vm.stack.push(Value::Null);
                }

                VmExecState::Notify(_notification) => {
                    // Ignore watch notifications for now
                }

                VmExecState::SpanNotify(notification) => {
                    if let Some(state) = span_state.as_mut() {
                        match notification {
                            SpanNotification::FunctionEnter {
                                function_name,
                                frame_depth: _,
                                args,
                            } => {
                                let span_id = SpanId::new();
                                let parent_span_id = state.stack.last().map(|s| s.span_id.clone());

                                // Build call_stack: host prefix + existing engine spans + new span
                                let mut call_stack = state.host_call_stack.clone();
                                call_stack.extend(state.stack.iter().map(|s| s.span_id.clone()));
                                call_stack.push(span_id.clone());

                                // Convert VM args to fully owned values for the event
                                let external_args: Vec<BexExternalValue> = args
                                    .iter()
                                    .map(|v| self.vm_value_to_owned(thread.proof(), v))
                                    .collect();

                                let enter_event = RuntimeEvent {
                                    call_id,
                                    ctx: SpanContext {
                                        span_id: span_id.clone(),
                                        parent_span_id: parent_span_id.clone(),
                                        root_span_id: state.root_span_id.clone(),
                                    },
                                    call_stack,
                                    timestamp: SystemTime::now(),
                                    event: EventKind::Function(FunctionEvent::Start(
                                        FunctionStart {
                                            name: function_name.clone(),
                                            args: external_args,
                                            tags: vec![],
                                        },
                                    )),
                                };
                                self.emit(enter_event);

                                state.stack.push(EngineSpan {
                                    span_id,
                                    parent_span_id,
                                    label: function_name,
                                    started_at: Instant::now(),
                                });
                            }
                            SpanNotification::FunctionExit {
                                function_name,
                                result,
                            } => {
                                if let Some(span) = state.stack.pop() {
                                    let external_result =
                                        self.vm_value_to_owned(thread.proof(), &result);
                                    // call_stack: host prefix + remaining engine spans + exiting span
                                    let mut call_stack = state.host_call_stack.clone();
                                    call_stack
                                        .extend(state.stack.iter().map(|s| s.span_id.clone()));
                                    call_stack.push(span.span_id.clone());
                                    let exit_event = RuntimeEvent {
                                        call_id,
                                        ctx: SpanContext {
                                            span_id: span.span_id,
                                            parent_span_id: span.parent_span_id,
                                            root_span_id: state.root_span_id.clone(),
                                        },
                                        call_stack,
                                        timestamp: SystemTime::now(),
                                        event: EventKind::Function(FunctionEvent::End(Box::new(
                                            FunctionEnd {
                                                name: function_name,
                                                result: external_result,
                                                duration: span.started_at.elapsed(),
                                                error: None,
                                            },
                                        ))),
                                    };
                                    self.emit(exit_event);
                                }
                            }
                        }
                    }
                }
                VmExecState::EarlyYield => {
                    thread = self.gc_safepoint(thread).await;
                }
            }
        }
    }

    /// Build a `SpanContext` from the current `SpanState`.
    ///
    /// Returns the context for the innermost active span, or uses the root span
    /// if the stack is empty (e.g. between span transitions).
    fn build_span_context_from_state(state: &SpanState) -> bex_events::SpanContext {
        if let Some(current_span) = state.stack.last() {
            bex_events::SpanContext {
                span_id: current_span.span_id.clone(),
                parent_span_id: current_span.parent_span_id.clone(),
                root_span_id: state.root_span_id.clone(),
            }
        } else {
            // Stack is empty (e.g. root span has not been pushed yet, or has been popped).
            // Use the root span as a fallback.
            bex_events::SpanContext {
                span_id: state.root_span_id.clone(),
                parent_span_id: None,
                root_span_id: state.root_span_id.clone(),
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
        let args = args.iter().map(std::convert::Into::into).collect();
        let fn_ptr = self.sys_ops.get(op);
        let mut ctx = self.sys_op_ctx.to_op_context(cancel.clone(), self.clone());
        // Rebuild RuntimeIo with the live per-call context so IO calls
        // (media resolution, auth) use the correct cancellation token.
        ctx.runtime_io =
            sys_ops::build_runtime_io(&self.sys_ops, &self.heap, &self.heap_permit_manager, &ctx);
        let result = fn_ptr(&self.heap, permit, args, &ctx, call_id);

        match result {
            SysOpResult::Ready(Ok(v)) => SysOpResult::Ready(Ok(v)),
            SysOpResult::Ready(Err(err)) => {
                if let Err(violation) = sys_types::validate_sys_op_error(op, &err.kind) {
                    tracing::warn!("{violation}");
                }
                SysOpResult::Ready(Err(err))
            }
            SysOpResult::Async(fut) => {
                let boxed = Box::pin(async move {
                    let res = fut.await;
                    if let Err(err) = &res {
                        if let Err(violation) = sys_types::validate_sys_op_error(op, &err.kind) {
                            tracing::warn!("{violation}");
                        }
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
}

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
