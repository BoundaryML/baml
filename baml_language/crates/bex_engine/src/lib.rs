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
//! The engine coordinates GC using an epoch-based system:
//!
//! 1. **Epoch tracking**: Each `call_function` registers with the current epoch
//! 2. **GC trigger**: `collect_garbage()` increments epoch, causing old-epoch VMs to park
//! 3. **Safe collection**: Once all VMs park, GC collects roots from:
//!    - Handle table (objects returned to external code)
//!    - Parked VM stacks (via VM pointer registry)
//! 4. **Stack update**: GC updates parked VM stacks with forwarding pointers
//! 5. **TLAB invalidation**: Parked VMs get TLABs invalidated before resuming
//! 6. **Resume**: `gc_complete.notify_waiters()` releases parked VMs
//!
//! ## Safety Invariants
//!
//! - VMs register pointers before parking, unregister after waking
//! - GC only accesses VM stacks while holding `parked_vms` lock
//! - Handles always resolve through table (no cached indices)
//! - New calls wait for in-progress GC before processing handle args
//!
//! # Unsafe Code
//!
//! This module uses unsafe code for:
//! - `VmPtr` Send implementation: Raw VM pointers stored for GC root collection
//! - Direct heap access: Reading objects during value conversion (index from valid handle)
//! - GC coordination: Dereferencing parked VM pointers to collect/update roots
//! - Epoch guards: Creating guards after registering with the epoch system
//!
//! Safety is ensured by the epoch-based GC coordination system described above.

#![allow(unsafe_code)]

mod conversion;
mod function_call_context;
mod heap_guard;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, atomic::Ordering},
};

// Re-export event types for callers.
use ::bex_vm_types::RootHaver;
use ::core::sync::atomic::AtomicBool;
use async_trait::async_trait;
use bex_events::{EventKind, FunctionEnd, FunctionEvent, FunctionStart, SpanContext};
pub use bex_events::{HostSpanContext, RuntimeEvent, SpanId};
pub use bex_external_types::{BexExternalValue, EpochGuard, Ty, TypeName, UnionMetadata};
use bex_heap::BexHeap;
// Re-export GcStats for users of the engine
pub use bex_heap::GcStats;
use bex_vm::{BexVm, SpanNotification, VmExecState};
use bex_vm_types::{FunctionMeta, GlobalPool, HeapPtr, Object, SysOp, Value};
pub use conversion::test_arg_to_external;
// Re-export CancellationToken for callers.
pub use function_call_context::{FunctionCallContext, FunctionCallContextBuilder};
pub use sys_types::CallId;
use sys_types::{OpError, SysOpResult};
use thiserror::Error;
use tokio::sync::mpsc;
pub use tokio_util::sync::CancellationToken;
use web_time::{Instant, SystemTime};

pub use crate::heap_guard::{ActiveHeapPermit, HeapGuard, HeapPermitManager, InactiveHeapPermit};

// ============================================================================
// Engine Types
// ============================================================================

/// Information about a user-callable function, used by `baml run --list`.
#[derive(Debug, Clone)]
pub struct UserFunctionInfo {
    pub qualified_name: String,
    pub display_name: String,
    pub param_names: Vec<String>,
    pub param_types: Vec<Ty>,
    pub return_type: Ty,
}

/// Result of an external future.
struct FutureResult {
    id: HeapPtr,
    result: Result<BexExternalValue, EngineError>,
}

/// RAII guard for in-flight async sys-op task abort handles.
///
/// On drop, aborts all tracked tasks so early returns (`?`) do not leave
/// spawned work running in the background.
struct AbortHandlesGuard {
    handles: Vec<futures::future::AbortHandle>,
}

impl AbortHandlesGuard {
    fn new() -> Self {
        Self {
            handles: Vec::new(),
        }
    }

    fn push(&mut self, handle: futures::future::AbortHandle) {
        self.handles.push(handle);
    }

    fn abort_all(&self) {
        for handle in &self.handles {
            handle.abort();
        }
    }
}

impl Drop for AbortHandlesGuard {
    fn drop(&mut self) {
        self.abort_all();
    }
}

// ============================================================================
// Span Tracking (per-invocation, NOT on Arc<BexEngine>)
// ============================================================================

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
#[derive(Debug, PartialEq, Error)]
pub enum EngineError {
    #[error("Function call with ID {call_id} not found")]
    FunctionCallNotFound { call_id: CallId },

    #[error("Function not found: {name}")]
    FunctionNotFound { name: String },

    #[error("{0}")]
    ExternalOpFailed(#[from] OpError),

    #[error("Future channel closed unexpectedly")]
    FutureChannelClosed,

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

    #[error("Cannot convert object of type {type_name}")]
    CannotConvert { type_name: String },

    #[error("Type mismatch: {message}")]
    TypeMismatch { message: String },

    #[error("Schema inconsistency: {message}")]
    SchemaInconsistency { message: String },

    #[error("Operation cancelled")]
    Cancelled,

    #[cfg(feature = "heap_debug")]
    #[error("Snapshot not possible for type: {type_name}")]
    CannotSnapshot { type_name: String },

    #[error("A function call with ID {call_id} is already in progress")]
    DuplicateCallId { call_id: CallId },

    #[error("Package initialization failed: {0}")]
    InitFailed(String),
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
    /// Global variables pool
    globals: GlobalPool,
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

    /// Map of active function calls by ID.
    active_calls: Mutex<HashMap<CallId, CancellationToken>>,
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
        let mut globals = GlobalPool::from_vec(globals_vec);

        // Run $init for each package in dependency order.
        // $init evaluates top-level let-binding initializers and stores their
        // results into the global slots via StoreGlobal instructions.
        // This must run before any user code calls LoadGlobal on let-bound names.
        for init_name in &package_init_order {
            if let Some((init_ptr, _kind)) = resolved_function_names.get(init_name.as_str()) {
                let mut vm = BexVm::new(
                    Arc::clone(&heap),
                    globals.clone(),
                    resolved_class_names
                        .iter()
                        .map(|(k, v)| (k.clone(), *v))
                        .collect(),
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
                            globals = vm.globals;
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

        // Build SysOpContext by pre-extracting LLM function metadata from the heap.
        // This avoids passing raw HeapPtrs to sys_ops.
        let llm_functions = Self::extract_llm_function_info(&resolved_function_names);

        // Extract class and enum definitions for output format rendering.
        let class_definitions = Self::extract_class_definitions(&resolved_class_names);
        let enum_definitions = Self::extract_enum_definitions(&resolved_enum_names);

        // Build a default RuntimeIo from the SysOps table with an empty context.
        // This is replaced per-call in execute_sys_op with a live context that
        // carries the correct cancellation token and spawner.
        let runtime_io =
            sys_ops::build_runtime_io(&sys_ops, &heap, &sys_types::SysOpContext::empty());

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
            resolved_function_names,
            resolved_class_names,
            resolved_enum_names,
            sys_ops,
            sys_op_ctx,
            event_sink,
            test_cases,
            argv,
            heap_permit_manager: Arc::new(HeapPermitManager::new()),
            checking_gc: AtomicBool::new(false),
            active_calls: Mutex::new(HashMap::new()),
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

    /// Explicitly trigger garbage collection.
    ///
    /// Requests and waits for all heap permit holders to park.
    /// Once they are parked, runs the GC.
    ///
    /// # Returns
    ///
    /// Statistics about the collection (live count, collected count, etc.)
    pub async fn collect_garbage(&self, level: bex_heap::CollectionLevel) -> bex_heap::GcStats {
        let mut heap_guard = self.heap_permit_manager.request_park().await;

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
        // Update all parked VM stacks with forwarding pointers and invalidate TLABs
        // SAFETY: VMs are still parked (gc_complete not yet notified), we have
        // exclusive access via the parked_vms lock we're still holding
        heap_guard.forward_roots(&forwarding);

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
        }: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexExternalValue, EngineError> {
        // Fail fast if already cancelled — guarantees pre-cancelled tokens
        // always produce Err(Cancelled) regardless of function contents.
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled);
        }

        // Create VM with shared heap (each VM gets its own TLAB)
        let vm = BexVm::new(
            Arc::clone(&self.heap),
            self.globals.clone(),
            self.resolved_class_names
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            Arc::clone(&self.argv),
        );
        let vm = self.heap_permit_manager.new_permit(vm).await;
        let mut vm = vm.acquire().await;

        let function_index = self.lookup_function(function_name)?;
        let return_type = self
            .function_return_type(function_name)
            .unwrap_or(Ty::Null {
                attr: baml_type::TyAttr::default(),
            });
        let throws_type = self.function_throws_type(function_name);

        // Snapshot args for the root FunctionStart event before converting to VM values
        let args_snapshot = args.clone();

        let vm_args: Vec<Value> = args
            .into_iter()
            .map(|arg| self.convert_external_to_vm_value(&mut vm, arg))
            .collect();

        vm.set_entry_point(function_index, &vm_args);

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
                vm.alloc_collector(collector_ref)
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

        // Run the event loop with span tracking
        self.run_event_loop(
            return_type,
            throws_type,
            vm,
            call_id,
            &mut span_state,
            &cancel,
            copy_objects,
        )
        .await

        // active_calls cleanup is done by ActiveCallGuard on drop.
        //
        // Keep genuine engine errors intact. Cancellation is surfaced directly
        // by engine safepoints as `EngineError::Cancelled`.
    }

    /// Cancel a function call by its ID.
    ///
    /// If the call is still running, it will be interrupted at the next
    /// cancellation check point. If the call has already completed or the ID
    /// is unknown, this will return an error.
    pub fn cancel_function_call(&self, call_id: CallId) -> Result<(), EngineError> {
        let mut active_calls = self.active_calls.lock().unwrap();
        if let Some(cancel) = active_calls.remove(&call_id) {
            cancel.cancel();
            Ok(())
        } else {
            Err(EngineError::FunctionCallNotFound { call_id })
        }
    }

    /// Look up a function by name and return its heap pointer.
    ///
    /// Tries the exact name first, then falls back to `"user.{name}"` to handle
    /// the compiler2 pipeline which qualifies user-defined functions with the
    /// package prefix (e.g. `"main"` → `"user.main"`).
    fn lookup_function(&self, function_name: &str) -> Result<HeapPtr, EngineError> {
        // Try exact match first
        if let Some((ptr, _kind)) = self.resolved_function_names.get(function_name) {
            return Ok(*ptr);
        }
        // Fall back to "user." prefix (compiler2 qualifies user functions)
        let qualified = format!("user.{function_name}");
        self.resolved_function_names
            .get(&qualified)
            .map(|(ptr, _kind)| *ptr)
            .ok_or_else(|| EngineError::FunctionNotFound {
                name: function_name.to_string(),
            })
    }

    /// Resolve a function name to the key actually present in `resolved_function_names`.
    ///
    /// Returns `Some(key)` where `key` is either `name` or `"user.{name}"`,
    /// or `None` if neither is found.
    fn resolve_function_name<'a>(&'a self, name: &str) -> Option<&'a str> {
        if self.resolved_function_names.contains_key(name) {
            return Some(
                self.resolved_function_names
                    .get_key_value(name)
                    .map(|(k, _)| k.as_str())
                    .unwrap(),
            );
        }
        let qualified = format!("user.{name}");
        self.resolved_function_names
            .get_key_value(&qualified)
            .map(|(k, _)| k.as_str())
    }

    /// Get the return type for a function by dereferencing its heap object.
    fn function_return_type(&self, name: &str) -> Option<Ty> {
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

    /// Get parameter names and types for a function by dereferencing its heap object.
    pub fn function_params(&self, name: &str) -> Result<Vec<(&str, &Ty)>, EngineError> {
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
                .map(|(name, ty)| (name.as_str(), ty))
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
                    Object::Function(func) => {
                        let display_name = name.strip_prefix("user.").unwrap_or(name).to_string();
                        Some(UserFunctionInfo {
                            qualified_name: name.clone(),
                            display_name,
                            param_names: func.param_names.clone(),
                            param_types: func.param_types.clone(),
                            return_type: func.return_type.clone(),
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

    /// Run GC if conditions are met (called at safepoints).
    ///
    /// Uses the adaptive `should_collect()` policy to choose the appropriate
    /// collection level (Minor or Major) based on live object counts and
    /// allocation pressure.
    ///
    /// Note: This function is known to be incorrect for multi-VM workloads — it
    /// runs GC without coordinating with other VMs. It is kept working here for
    /// single-VM use but will be replaced by a proper coordinated path later.
    async fn gc_safepoint<T: RootHaver>(
        &self,
        mut permit: ActiveHeapPermit<T>,
    ) -> ActiveHeapPermit<T> {
        let other_thread_checking = self
            .checking_gc
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok();
        if other_thread_checking {
            // Park if they have requested it.
            permit.renew().await
        } else {
            // We set the flag, so we should check
            if let Some(level) = self.heap.should_collect() {
                let inactive = permit.release();
                self.collect_garbage(level).await;
                permit = inactive.acquire().await;
            }
            self.checking_gc.store(false, Ordering::Release);
            permit
        }
    }

    /// Engine-level cancellation safepoint.
    ///
    /// Keeps cancellation handling centralized in the engine loop instead of
    /// requiring individual BAML code paths or `sys_ops` to be cancel-aware.
    fn cancellation_safepoint(
        cancel: &CancellationToken,
        abort_handles: &AbortHandlesGuard,
    ) -> Result<(), EngineError> {
        if cancel.is_cancelled() {
            abort_handles.abort_all();
            return Err(EngineError::Cancelled);
        }
        Ok(())
    }

    /// Run the VM event loop until completion, with epoch tracking.
    ///
    /// The `my_epoch` parameter is used to check if GC has been requested
    /// (epoch advanced). VMs from old epochs will park at yield points.
    #[allow(clippy::too_many_arguments)]
    async fn run_event_loop(
        self: &Arc<Self>,
        return_type: Ty,
        throws_type: Option<Ty>,
        mut vm: ActiveHeapPermit<BexVm>,
        call_id: CallId,
        span_state: &mut Option<SpanState>,
        cancel: &CancellationToken,
        copy_objects: bool,
    ) -> Result<BexExternalValue, EngineError> {
        let (pending_futures, mut processed_futures) = mpsc::unbounded_channel::<FutureResult>();
        // Abort handles for spawned async tasks.
        //
        // Cancellation design: the engine checks cancellation at centralized
        // safepoints (VM loop boundaries + ScheduleFuture boundaries), and uses
        // a biased `tokio::select!` while waiting at `Await`. This keeps
        // cancellation in the engine, so individual sys_ops don't need to be
        // cancellation-aware. Without abort handles, async sys-op tasks would
        // continue as orphans after cancellation until they complete naturally.
        // For long-running ops (HTTP requests, multi-second sleeps), that
        // wastes real resources.
        //
        // Rather than making individual sys_ops cancel-aware (wrapping each in
        // its own `tokio::select!`), we store abort handles here and kill all
        // spawned tasks when cancellation fires. This keeps sys_op
        // implementations simple — new sys_ops never need to think about
        // cancellation.
        //
        // We use `futures::future::AbortHandle` (not `tokio::task::AbortHandle`)
        // so the same mechanism works on both native and WASM targets.
        let mut abort_handles = AbortHandlesGuard::new();

        'vm_exec: loop {
            Self::cancellation_safepoint(cancel, &abort_handles)?;

            // Update the VM's span context so native functions can read it.
            vm.current_span_context = span_state.as_ref().map(Self::build_span_context_from_state);

            let exec_result = match vm.exec() {
                Ok(state) => state,
                Err(bex_vm::errors::VmError::ThrownUnhandled { value, trace }) => {
                    let external = if let Some(ref ty) = throws_type {
                        self.convert_vm_value_to_external_with_type(&value, ty, &vm.epoch_guard())?
                    } else {
                        self.vm_value_to_owned(&value)
                    };
                    return Err(EngineError::UnhandledThrow {
                        value: Box::new(external),
                        trace,
                    });
                }
                Err(bex_vm::errors::VmError::Thrown(value)) => {
                    // Internal throw that escaped without unwinding — treat as
                    // unhandled with no trace.
                    let external = self.vm_value_to_owned(&value);
                    return Err(EngineError::UnhandledThrow {
                        value: Box::new(external),
                        trace: Vec::new(),
                    });
                }
                Err(bex_vm::errors::VmError::InternalError(err)) => {
                    return Err(EngineError::VmInternalError(err));
                }
                Err(bex_vm::errors::VmError::TracedInternalError { source, trace }) => {
                    return Err(EngineError::TracedVmInternalError { source, trace });
                }
            };
            match exec_result {
                VmExecState::Complete(value) => {
                    // "Cancel wins" semantics: if cancellation races with a
                    // completed VM step, report `Cancelled` rather than
                    // returning a success value.
                    //
                    // Still emit FunctionEnd first so tracing consumers see
                    // a paired root FunctionStart/FunctionEnd span.
                    let cancelled = cancel.is_cancelled();
                    if cancelled {
                        abort_handles.abort_all();
                    }

                    // Emit FunctionEnd for the root entry-point span if tracing
                    if let Some(state) = span_state.as_mut() {
                        if let Some(root_span) = state.stack.pop() {
                            let external_result = self.vm_value_to_owned(&value);
                            let mut full_call_stack = state.host_call_stack.clone();
                            full_call_stack.extend(state.stack.iter().map(|s| s.span_id.clone()));
                            full_call_stack.push(root_span.span_id.clone());
                            let end_event = RuntimeEvent {
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
                                        result: external_result,
                                        duration: root_span.started_at.elapsed(),
                                    },
                                ))),
                            };
                            self.emit(end_event);
                        }
                    }

                    if cancelled {
                        return Err(EngineError::Cancelled);
                    }

                    // When copy_objects: false and the result is a heap object,
                    // create a Handle without holding with_gc_protection (which
                    // takes a read lock on handles). create_handle takes a write
                    // lock — the two cannot nest without deadlocking.
                    if !copy_objects {
                        if let Value::Object(ptr) = value {
                            let handle = self.heap.create_handle(ptr);
                            return Ok(BexExternalValue::Handle(handle));
                        }
                        // Non-object primitives fall through to normal conversion below.
                    }

                    return self.heap.with_gc_protection(|protected| {
                        // Normal deep-extraction: convert VM value to fully owned BexExternalValue.
                        self.convert_vm_value_to_external_with_type(
                            &value,
                            &return_type,
                            &protected.epoch_guard(),
                        )
                    });
                }

                VmExecState::ScheduleFuture(id) => {
                    let pending = vm
                        .pending_future(id)
                        .map_err(EngineError::VmInternalError)?;

                    // Convert arguments to BexExternalValue
                    let args: Vec<BexExternalValue> = pending
                        .args
                        .iter()
                        .map(|v| self.vm_arg_to_bex_value(v))
                        .collect();

                    Self::cancellation_safepoint(cancel, &abort_handles)?;
                    let sys_op_result =
                        self.execute_sys_op(pending.operation, &args, call_id, cancel);
                    Self::cancellation_safepoint(cancel, &abort_handles)?;

                    match sys_op_result {
                        SysOpResult::Ready(result) => {
                            // Guard the "commit to VM state" boundary.
                            Self::cancellation_safepoint(cancel, &abort_handles)?;

                            // Sync operation - set future to Ready without touching stack.
                            // The VM will continue to the Await instruction which will
                            // extract the value from the Ready future.
                            let result = result.map_err(EngineError::from)?;
                            let value = self.convert_external_to_vm_value(&mut vm, result);

                            vm.set_future_ready(id, value)
                                .map_err(EngineError::VmInternalError)?;
                        }
                        SysOpResult::Async(fut) => {
                            // Guard the "spawn side effect" boundary.
                            Self::cancellation_safepoint(cancel, &abort_handles)?;

                            // Async operation — wrap in Abortable and spawn.
                            let pending_futures = pending_futures.clone();
                            let (abort_handle, abort_reg) =
                                futures::future::AbortHandle::new_pair();
                            let abortable = futures::future::Abortable::new(
                                async move {
                                    let result = fut.await;
                                    let _ = pending_futures.send(FutureResult {
                                        id,
                                        result: result.map_err(EngineError::from),
                                    });
                                },
                                abort_reg,
                            );
                            #[cfg(not(target_arch = "wasm32"))]
                            tokio::spawn(async move {
                                let _ = abortable.await;
                            });
                            #[cfg(target_arch = "wasm32")]
                            wasm_bindgen_futures::spawn_local(async move {
                                let _ = abortable.await;
                            });
                            abort_handles.push(abort_handle);
                        }
                    }
                }

                VmExecState::Await(future_id) => {
                    Self::cancellation_safepoint(cancel, &abort_handles)?;
                    vm = self.gc_safepoint(vm).await;

                    // First, drain any already-completed futures.
                    while let Ok(future) = processed_futures.try_recv() {
                        let external = future.result?;
                        let value = self.convert_external_to_vm_value(&mut vm, external);
                        vm.fulfil_future(future.id, value)
                            .map_err(EngineError::VmInternalError)?;

                        if future.id == future_id {
                            continue 'vm_exec;
                        }
                    }

                    // We gotta wait for the target future.
                    // Race against cancellation — `biased` ensures the cancel
                    // branch is checked first, matching legacy orchestrator behavior.
                    loop {
                        tokio::select! {
                            biased;
                            () = cancel.cancelled() => {
                                // Abort all in-flight spawned tasks to stop
                                // HTTP requests, sleeps, etc. immediately.
                                abort_handles.abort_all();
                                return Err(EngineError::Cancelled);
                            }
                            future = processed_futures.recv() => {
                                let future = future
                                    .ok_or(EngineError::FutureChannelClosed)?;
                                let external = future.result?;
                                let value = self.convert_external_to_vm_value(
                                    &mut vm,
                                    external,
                                );
                                vm.fulfil_future(future.id, value)
                                    .map_err(EngineError::VmInternalError)?;

                                if future.id == future_id {
                                    break;
                                }
                            }
                        }
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

                        let external_data = self.vm_value_to_owned(&data);

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
                    vm.stack.push(Value::Null);
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
                                let external_args: Vec<BexExternalValue> =
                                    args.iter().map(|v| self.vm_value_to_owned(v)).collect();

                                let enter_event = RuntimeEvent {
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
                                    let external_result = self.vm_value_to_owned(&result);
                                    // call_stack: host prefix + remaining engine spans + exiting span
                                    let mut call_stack = state.host_call_stack.clone();
                                    call_stack
                                        .extend(state.stack.iter().map(|s| s.span_id.clone()));
                                    call_stack.push(span.span_id.clone());
                                    let exit_event = RuntimeEvent {
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
                                            },
                                        ))),
                                    };
                                    self.emit(exit_event);
                                }
                            }
                        }
                    }
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
    ) -> SysOpResult {
        let args = args.iter().map(std::convert::Into::into).collect();
        let fn_ptr = self.sys_ops.get(op);
        let mut ctx = self.sys_op_ctx.to_op_context(cancel.clone(), self.clone());
        // Rebuild RuntimeIo with the live per-call context so IO calls
        // (media resolution, auth) use the correct cancellation token.
        ctx.runtime_io = sys_ops::build_runtime_io(&self.sys_ops, &self.heap, &ctx);
        let result = fn_ptr(&self.heap, args, &ctx, call_id);

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
