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
mod test_registry;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
};

use async_trait::async_trait;
pub use bex_events::HostSpanContext;
use bex_events::{EventKind, FunctionEnd, FunctionEvent, FunctionStart, SpanContext};
// Re-export event types for callers.
pub use bex_events::{RuntimeEvent, SpanId};
pub use bex_external_types::{BexExternalValue, EpochGuard, Ty, TypeName, UnionMetadata};
use bex_heap::BexHeap;
// Re-export GcStats for users of the engine
pub use bex_heap::GcStats;
use bex_vm::{BexVm, SpanNotification, VmExecState};
use bex_vm_types::{FunctionMeta, GlobalPool, HeapPtr, Object, SysOp, Value};
pub use conversion::test_arg_to_external;
// Re-export CancellationToken for callers.
pub use function_call_context::{FunctionCallContext, FunctionCallContextBuilder};
use sys_types::{CallId, OpError, SysOpResult};
pub use test_registry::{TestInfo, TestRegistry, TestSetInfo, TestSetResult};
use thiserror::Error;
use tokio::sync::{Notify, mpsc};
pub use tokio_util::sync::CancellationToken;
use web_time::{Instant, SystemTime};

// ============================================================================
// Engine Types
// ============================================================================

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

/// Wrapper for VM pointer that implements Send.
///
/// # Safety
///
/// This is safe because:
/// - The pointer is only used while holding the `parked_vms` lock
/// - We only dereference when all VMs are parked at safepoints
/// - The VM lives on the async task's stack and won't move/drop while parked
struct VmPtr(*const BexVm);

// SAFETY: We control all access through the mutex and only use while VMs are parked
unsafe impl Send for VmPtr {}

/// State for a single epoch slot.
/// Used to track VMs that started in a particular epoch.
struct EpochState {
    /// Number of VMs started in this epoch that haven't completed.
    active: AtomicUsize,
    /// Number of VMs parked waiting for GC.
    parked: AtomicUsize,
    /// Pointers to parked VMs for root collection during GC.
    ///
    /// # Safety
    ///
    /// These raw pointers are valid because:
    /// - VM is borrowed from `call_function`'s stack frame
    /// - `.await` on `gc_complete` suspends but doesn't drop the VM
    /// - GC only reads/writes while all VMs are parked
    /// - VM unregisters before resuming execution
    parked_vms: Mutex<Vec<VmPtr>>,
}

impl EpochState {
    fn new() -> Self {
        Self {
            active: AtomicUsize::new(0),
            parked: AtomicUsize::new(0),
            parked_vms: Mutex::new(Vec::new()),
        }
    }
}

/// RAII guard: inserts (`call_id`, cancel) on construction and removes `call_id` on drop,
/// so `active_calls` is cleaned up on all exit paths (success, early return, or panic).
struct ActiveCallGuard<'a> {
    active_calls: &'a Mutex<HashMap<CallId, CancellationToken>>,
    call_id: CallId,
}

impl<'a> ActiveCallGuard<'a> {
    fn new(
        active_calls: &'a Mutex<HashMap<CallId, CancellationToken>>,
        call_id: CallId,
        cancel: &CancellationToken,
    ) -> Result<Self, EngineError> {
        let mut map = active_calls.lock().unwrap();
        if map.contains_key(&call_id) {
            return Err(EngineError::DuplicateCallId { call_id });
        }
        map.insert(call_id, cancel.clone());
        Ok(Self {
            active_calls,
            call_id,
        })
    }
}

impl Drop for ActiveCallGuard<'_> {
    fn drop(&mut self) {
        let mut active_calls = self.active_calls.lock().unwrap();
        active_calls.remove(&self.call_id);
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

    #[error("VM error: {0}")]
    VmError(#[from] bex_vm::errors::VmError),

    #[error("Internal VM error: {0}")]
    InternalVmError(#[from] bex_vm::InternalError),

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

    // --- Epoch-based GC coordination ---
    /// Current epoch counter (monotonically increasing).
    /// Incremented when GC is requested.
    current_epoch: AtomicU64,
    /// Epoch states - 2 slots indexed by epoch % 2.
    /// (GC is synchronous, so max 2 active epochs at once)
    epoch_states: [EpochState; 2],
    /// Notified when an epoch's VMs have all parked or completed.
    epoch_drained: Notify,
    /// Notified when GC completes and parked VMs can resume.
    gc_complete: Notify,
    /// Flag indicating GC is currently in progress.
    /// Used to prevent handle resolution races.
    gc_in_progress: AtomicBool,

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
    pub fn new(
        bytecode_program: bex_vm_types::Program,
        sys_ops: std::sync::Arc<sys_ops::SysOps>,
        event_sink: Option<std::sync::Arc<dyn bex_events::EventSink>>,
    ) -> Result<Self, EngineError> {
        // Extract package_init_order before consuming bytecode_program.
        let package_init_order = bytecode_program.package_init_order.clone();

        // Convert the pure bytecode to a VM-ready program with native functions attached
        let bytecode = bex_vm::convert_program(bytecode_program)?;

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

        let sys_op_ctx = sys_types::EngineSysOpContext {
            llm_functions: Arc::new(llm_functions),
            function_global_indices: Arc::new(bytecode.function_global_indices),
            template_strings_macros: Arc::new(bytecode.template_strings_macros),
            class_definitions: Arc::new(class_definitions),
            enum_definitions: Arc::new(enum_definitions),
            type_alias_definitions: Arc::new(bytecode.recursive_type_alias_defs),
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
            // Initialize epoch tracking
            current_epoch: AtomicU64::new(0),
            epoch_states: [EpochState::new(), EpochState::new()],
            epoch_drained: Notify::new(),
            gc_complete: Notify::new(),
            gc_in_progress: AtomicBool::new(false),
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
    /// This method:
    /// 1. Increments the epoch (causing old-epoch VMs to park at yield points)
    /// 2. Waits for all old-epoch VMs to park or complete
    /// 3. Runs semi-space copy collection
    /// 4. Releases parked VMs (they will get updated indices on resume)
    ///
    /// # Concurrent Safety
    ///
    /// New calls (epoch N+1) proceed normally while GC waits for epoch N VMs.
    /// This minimizes latency impact - GC doesn't block new work.
    ///
    /// # Returns
    ///
    /// Statistics about the collection (live count, collected count, etc.)
    pub async fn collect_garbage(&self) -> bex_heap::GcStats {
        // Signal GC starting - new calls will wait
        self.gc_in_progress.store(true, Ordering::Release);

        // Increment epoch - new calls get the new epoch
        let gc_epoch = self.current_epoch.fetch_add(1, Ordering::SeqCst);
        let slot = (gc_epoch % 2) as usize;

        // Wait for all VMs from this epoch to park or complete
        loop {
            let active = self.epoch_states[slot].active.load(Ordering::Acquire);
            let parked = self.epoch_states[slot].parked.load(Ordering::Acquire);

            if active == 0 {
                // All VMs completed, nothing to collect
                break;
            }
            if parked >= active {
                // All active VMs are parked, safe to collect
                break;
            }

            // Wait for more VMs to park or complete
            self.epoch_drained.notified().await;
        }

        // Collect roots from handles (objects returned to external code)
        let mut all_roots = self.heap.collect_handle_roots();

        // Acquire parked_vms lock - hold it through GC to update stacks
        let parked_vms = self.epoch_states[slot].parked_vms.lock().unwrap();

        // SAFETY: All VMs are parked (verified above), so we have exclusive read access
        // to their stacks. The parked_vms vec contains valid pointers because VMs
        // register before parking and unregister only after gc_complete is notified.
        for vm_ptr in parked_vms.iter() {
            let vm = unsafe { &*vm_ptr.0 };
            all_roots.extend(Self::collect_vm_roots(vm));
        }

        tracing::debug!(
            "GC: {} total roots from {} handles and {} parked VMs",
            all_roots.len(),
            self.heap.stats().active_handles,
            parked_vms.len()
        );

        // Run GC with forwarding map
        let (stats, _remapped_roots, forwarding) =
            unsafe { self.heap.collect_garbage_with_forwarding(&all_roots) };

        // Update all parked VM stacks with forwarding pointers and invalidate TLABs
        // SAFETY: VMs are still parked (gc_complete not yet notified), we have
        // exclusive access via the parked_vms lock we're still holding
        for vm_ptr in parked_vms.iter() {
            let vm = unsafe { &mut *vm_ptr.0.cast_mut() };

            // Update stack values
            for value in &mut vm.stack.0 {
                if let Value::Object(idx) = value {
                    if let Some(&new_idx) = forwarding.get(idx) {
                        *idx = new_idx;
                    }
                }
            }

            // Update watch state (graph NodeIds, RootState values)
            vm.watch.apply_forwarding(&forwarding);

            // Update frame function pointers (needed for closures)
            vm.apply_frame_forwarding(&forwarding);

            // Invalidate TLAB so next allocation gets chunk from new space
            vm.tlab.invalidate();
        }

        // Release lock before notifying waiters
        drop(parked_vms);

        self.heap.verify_quick();

        // Reset epoch state for reuse
        self.epoch_states[slot].active.store(0, Ordering::Release);
        self.epoch_states[slot].parked.store(0, Ordering::Release);

        // Signal GC complete before releasing parked VMs
        self.gc_in_progress.store(false, Ordering::Release);

        // Release parked VMs
        self.gc_complete.notify_waiters();

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

        // Wait for any in-progress GC to complete.
        while self.gc_in_progress.load(Ordering::Acquire) {
            self.gc_complete.notified().await;
        }

        let _call_guard = ActiveCallGuard::new(&self.active_calls, call_id, &cancel)?;

        let function_index = self.lookup_function(function_name)?;
        let return_type = self
            .function_return_type(function_name)
            .unwrap_or(Ty::Null {
                attr: baml_type::TyAttr::default(),
            });

        // Register with current epoch
        let my_epoch = self.current_epoch.load(Ordering::Acquire);
        let slot = (my_epoch % 2) as usize;
        self.epoch_states[slot]
            .active
            .fetch_add(1, Ordering::AcqRel);

        // SAFETY: We just registered with the epoch above
        let guard = unsafe { EpochGuard::new() };

        // Create VM with shared heap (each VM gets its own TLAB)
        let mut vm = BexVm::new(
            Arc::clone(&self.heap),
            self.globals.clone(),
            self.resolved_class_names
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
        );

        // Snapshot args for the root FunctionStart event before converting to VM values
        let args_snapshot = args.clone();

        let vm_args: Vec<Value> = args
            .into_iter()
            .map(|arg| self.convert_external_to_vm_value(&mut vm, arg, &guard))
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
        let result = self
            .run_event_loop_with_epoch(
                return_type,
                &mut vm,
                my_epoch,
                call_id,
                &mut span_state,
                &cancel,
                copy_objects,
            )
            .await;

        // Unregister from epoch
        if self.epoch_states[slot]
            .active
            .fetch_sub(1, Ordering::AcqRel)
            == 1
        {
            self.epoch_drained.notify_one();
        }

        // active_calls cleanup is done by ActiveCallGuard on drop.
        //
        // Keep genuine engine errors intact. Cancellation is surfaced directly
        // by engine safepoints as `EngineError::Cancelled`.
        result
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
    // Test Metadata Extraction
    // ========================================================================

    /// Extract test names from a live Registry or `TestSetCollector` handle.
    ///
    /// Returns `(tests, testset_stubs)` where `testset_stubs` are `(name, collector_handle)` pairs.
    /// The collector handles are GC-rooted closures that can be invoked to discover nested tests.
    ///
    /// # Safety
    ///
    /// This method uses unsafe pointer dereference. It is safe because:
    /// - We call `with_gc_protection` which holds the GC epoch lock for the duration
    /// - All pointers are valid for the lifetime of the protection guard
    /// - We only read objects, never write (handles are created via `create_handle`)
    #[allow(clippy::type_complexity)]
    fn extract_tests_and_testset_stubs(
        &self,
        container_handle: &BexExternalValue,
    ) -> Result<(Vec<TestInfo>, Vec<(String, BexExternalValue)>), EngineError> {
        let BexExternalValue::Handle(handle) = container_handle else {
            return Ok((vec![], vec![]));
        };

        // Phase 1: extract raw ptrs and raw closure HeapPtrs under GC protection.
        // We CANNOT call create_handle inside with_gc_protection — it would deadlock
        // (with_gc_protection holds handles.read(), create_handle needs handles.write()).
        let (test_raw, raw_stubs) = self.heap.with_gc_protection(|protected| {
            let guard = protected.epoch_guard();
            let container_ptr =
                handle
                    .object_ptr(&guard)
                    .ok_or_else(|| EngineError::SchemaInconsistency {
                        message: "TestCollector handle expired".into(),
                    })?;

            // SAFETY: we hold GC protection via with_gc_protection; pointer is valid
            let container_obj = unsafe { container_ptr.get() };

            let Object::Instance(instance) = container_obj else {
                return Err(EngineError::SchemaInconsistency {
                    message: "expected TestCollector instance".into(),
                });
            };

            // Scan all array fields for TestRegistration and TestSetRegistration items.
            // The compiler may assign field orderings that differ from the class declaration,
            // so we identify fields by their element type rather than by index.
            let mut test_raw: Vec<(String, HeapPtr, Option<HeapPtr>)> = Vec::new();
            let mut raw_stubs = Vec::new();

            for field in &instance.fields {
                let Value::Object(ptr) = field else { continue };
                let obj = unsafe { ptr.get() };
                let Object::Array(items) = obj else {
                    continue;
                };
                if items.is_empty() {
                    continue;
                }

                // Peek at the first element to determine array type.
                let Value::Object(elem_ptr) = &items[0] else {
                    continue;
                };
                let elem_obj = unsafe { elem_ptr.get() };
                let Object::Instance(elem) = elem_obj else {
                    continue;
                };
                let class_obj = unsafe { elem.class.get() };
                let Object::Class(class) = class_obj else {
                    continue;
                };
                let class_name = class.name.display_name.as_str();

                if class_name.ends_with("TestRegistration")
                    && !class_name.ends_with("TestSetRegistration")
                {
                    test_raw = Self::extract_test_raw_ptrs(field)?;
                } else if class_name.ends_with("TestSetRegistration") {
                    raw_stubs = Self::extract_testset_raw_ptrs(field)?;
                }
            }

            // Also handle fields that are empty arrays (they could be either type).
            // If we didn't find any tests/testsets yet from non-empty arrays, try the
            // remaining empty arrays. Since empty arrays don't affect the result, this is safe.
            if test_raw.is_empty() && raw_stubs.is_empty() {
                // Both arrays are empty — nothing to do
            }

            Ok((test_raw, raw_stubs))
        })?;

        // Phase 2: create GC-rooted handles OUTSIDE gc_protection.
        let tests = test_raw
            .into_iter()
            .map(|(name, body_ptr, runner_ptr)| {
                let body = BexExternalValue::Handle(self.heap.create_handle(body_ptr));
                let runner =
                    runner_ptr.map(|p| BexExternalValue::Handle(self.heap.create_handle(p)));
                TestInfo { name, body, runner }
            })
            .collect();

        let testset_stubs = raw_stubs
            .into_iter()
            .map(|(name, closure_ptr)| {
                let handle = self.heap.create_handle(closure_ptr);
                (name, BexExternalValue::Handle(handle))
            })
            .collect();

        Ok((tests, testset_stubs))
    }

    /// Extract raw test data from a `TestRegistration[]` heap value.
    ///
    /// Returns `(name, body_ptr, runner_ptr_or_none)` tuples.
    /// Must be called inside `with_gc_protection`. Handles are created outside.
    fn extract_test_raw_ptrs(
        array_value: &Value,
    ) -> Result<Vec<(String, HeapPtr, Option<HeapPtr>)>, EngineError> {
        let Value::Object(ptr) = array_value else {
            return Ok(vec![]);
        };
        // SAFETY: pointer is valid within the GC epoch protection scope
        let obj = unsafe { ptr.get() };
        let Object::Array(items) = obj else {
            return Ok(vec![]);
        };

        items
            .iter()
            .map(|item| {
                let Value::Object(reg_ptr) = item else {
                    return Err(EngineError::TypeMismatch {
                        message: "expected TestRegistration instance in tests array".into(),
                    });
                };
                // SAFETY: pointer is valid within GC epoch protection scope
                let reg_obj = unsafe { reg_ptr.get() };
                let Object::Instance(reg) = reg_obj else {
                    return Err(EngineError::TypeMismatch {
                        message: "expected TestRegistration instance in tests array".into(),
                    });
                };
                if reg.fields.len() < 2 {
                    return Err(EngineError::SchemaInconsistency {
                        message: "TestRegistration needs at least 2 fields".into(),
                    });
                }
                // TestRegistration fields: [0] name: string, [1] body, [2] runner
                let name = Self::extract_string(&reg.fields[0])?;
                // fields[1] = body: () -> null
                let Value::Object(body_ptr) = &reg.fields[1] else {
                    return Err(EngineError::TypeMismatch {
                        message: "expected Object for body field".into(),
                    });
                };
                // fields[2] = runner: TestRunner? (may be null)
                let runner_ptr = if reg.fields.len() > 2 {
                    match &reg.fields[2] {
                        Value::Object(ptr) => Some(*ptr),
                        Value::Null => None,
                        _ => None,
                    }
                } else {
                    None
                };
                Ok((name, *body_ptr, runner_ptr))
            })
            .collect()
    }

    /// Extract testset names and raw collector closure `HeapPtrs` from a `TestSetRegistration[]`.
    ///
    /// Returns `(name, closure_ptr)` pairs. The raw `HeapPtrs` must be rooted via
    /// `create_handle` OUTSIDE of `with_gc_protection` to avoid deadlock.
    fn extract_testset_raw_ptrs(
        array_value: &Value,
    ) -> Result<Vec<(String, HeapPtr)>, EngineError> {
        let Value::Object(ptr) = array_value else {
            return Ok(vec![]);
        };
        // SAFETY: pointer is valid within the GC epoch protection scope
        let obj = unsafe { ptr.get() };
        let Object::Array(items) = obj else {
            return Ok(vec![]);
        };

        items
            .iter()
            .map(|item| {
                let Value::Object(reg_ptr) = item else {
                    return Err(EngineError::TypeMismatch {
                        message: "expected TestSetRegistration instance in testsets array".into(),
                    });
                };
                // SAFETY: pointer is valid within GC epoch protection scope
                let reg_obj = unsafe { reg_ptr.get() };
                let Object::Instance(reg) = reg_obj else {
                    return Err(EngineError::TypeMismatch {
                        message: "expected TestSetRegistration instance in testsets array".into(),
                    });
                };
                if reg.fields.len() < 2 {
                    return Err(EngineError::SchemaInconsistency {
                        message: "TestSetRegistration instance needs at least 2 fields".into(),
                    });
                }
                // TestSetRegistration fields: [0] name: string, [1] collector, [2] runner
                let name = Self::extract_string(&reg.fields[0])?;
                let Value::Object(closure_ptr) = &reg.fields[1] else {
                    return Err(EngineError::TypeMismatch {
                        message: "expected Object pointer for collector field".into(),
                    });
                };
                Ok((name, *closure_ptr))
            })
            .collect()
    }

    /// Extract a `String` from a heap `Value::Object(ptr)` pointing to an `Object::String`.
    fn extract_string(value: &Value) -> Result<String, EngineError> {
        let Value::Object(ptr) = value else {
            return Err(EngineError::TypeMismatch {
                message: "expected Object pointer for string field".into(),
            });
        };
        // SAFETY: pointer is valid within the GC epoch protection scope
        let obj = unsafe { ptr.get() };
        let Object::String(s) = obj else {
            return Err(EngineError::TypeMismatch {
                message: "expected Object::String for string field".into(),
            });
        };
        Ok(s.clone())
    }

    // ========================================================================
    // Test Collection API
    // ========================================================================

    /// Collect all tests for a package by invoking `{package}.$init_test(registry)`.
    ///
    /// Returns a [`TestRegistry`] with cached metadata (names, hierarchy) and a live
    /// GC-rooted handle to the heap `testing.Registry` object for later execution.
    ///
    /// If the package has no test blocks, `$init_test` will not exist in the program
    /// and an empty [`TestRegistry`] is returned immediately.
    ///
    /// # Arguments
    ///
    /// - `package`: The package name (e.g. `"user"`).
    /// - `cancel`: A [`CancellationToken`] for caller-controlled cancellation.
    pub async fn collect_tests(
        self: &Arc<Self>,
        package: &str,
        cancel: CancellationToken,
        max_testset_load_time: Option<std::time::Duration>,
        skip_testsets: std::collections::HashSet<String>,
    ) -> Result<TestRegistry, EngineError> {
        let init_test_name = if package == "user" {
            "$init_test".to_string()
        } else {
            format!("{package}.$init_test")
        };

        // If no $init_test function exists, this package has no tests.
        if self.lookup_function(&init_test_name).is_err() {
            return Ok(TestRegistry::empty());
        }

        // 1. Create an empty TestCollector via testing.TestCollector.new().
        //    Use copy_objects: false to get a Handle instead of deep-extracting.
        //    The Handle keeps the heap object alive via GC rooting.
        let registry_handle = self
            .call_function(
                "testing.TestCollector.new",
                vec![BexExternalValue::String(String::new())],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_cancel_token(cancel.clone())
                    .build(),
                false, // copy_objects: return a Handle, not a deep-extracted value
            )
            .await?;

        // 2. Call $init_test(registry) — registry mutations happen in-place on
        //    the heap via //baml:mut_self on register_test/register_test_set.
        //    The Handle is converted to Value::Object(ptr) by
        //    convert_external_to_vm_value (conversion.rs:232-236).
        let _result = self
            .call_function(
                &init_test_name,
                vec![registry_handle.clone()],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_cancel_token(cancel.clone())
                    .build(),
                true, // copy_objects: normal deep-extraction for the null return
            )
            .await?;

        // 3. Extract test names and testset stubs (with collector closure handles).
        let (tests, testset_stubs) = self.extract_tests_and_testset_stubs(&registry_handle)?;

        // 4. Expand testsets by invoking each collector closure recursively.
        let testsets = self
            .expand_testset_stubs(
                testset_stubs,
                &cancel,
                max_testset_load_time,
                &skip_testsets,
            )
            .await?;

        Ok(TestRegistry::new(registry_handle, tests, testsets))
    }

    /// Invoke a collector closure: `TestCollector.new(prefix)` → `$invoke_collector` → extract.
    ///
    /// Returns `(tests, nested_stubs, loading_time_ms)`.
    /// `loading_time_ms` covers `TestCollector.new` + `$invoke_collector` + extraction.
    async fn invoke_collector(
        self: &Arc<Self>,
        parent_name: &str,
        collector_closure: BexExternalValue,
        cancel: &CancellationToken,
    ) -> Result<(Vec<TestInfo>, Vec<(String, BexExternalValue)>, u64), EngineError> {
        let start = web_time::Instant::now();

        // Create a fresh TestCollector with the parent's full-path name as prefix
        let collector_handle = self
            .call_function(
                "testing.TestCollector.new",
                vec![BexExternalValue::String(parent_name.to_string())],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_cancel_token(cancel.clone())
                    .build(),
                false, // Handle, not deep-extracted
            )
            .await?;

        // Invoke: $invoke_collector(closure, collector)
        let _invoke_result = self
            .call_function(
                "testing.$invoke_collector",
                vec![collector_closure, collector_handle.clone()],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_cancel_token(cancel.clone())
                    .build(),
                true, // null return
            )
            .await?;

        #[allow(clippy::cast_possible_truncation)]
        let loading_time_ms = start.elapsed().as_millis() as u64;
        let (tests, nested_stubs) = self.extract_tests_and_testset_stubs(&collector_handle)?;
        Ok((tests, nested_stubs, loading_time_ms))
    }

    /// Expand testset stubs by invoking each collector closure via `$invoke_collector`.
    ///
    /// For each `(name, collector_closure_handle)` pair:
    /// 1. If the name is in `skip_names`, push `TestSetResult::Lazy` immediately.
    /// 2. If `max_load_time` is set, race the invocation against a timer; if the
    ///    timer fires first, cancel the invocation and push `Lazy`.
    /// 3. Otherwise expand fully (original behavior).
    async fn expand_testset_stubs(
        self: &Arc<Self>,
        stubs: Vec<(String, BexExternalValue)>,
        cancel: &CancellationToken,
        max_load_time: Option<std::time::Duration>,
        skip_names: &std::collections::HashSet<String>,
    ) -> Result<Vec<TestSetResult>, EngineError> {
        log::info!(
            "[expand_testset_stubs] stubs={} max_load_time={:?} skip_names={:?}",
            stubs.len(),
            max_load_time,
            skip_names
        );
        let mut result = Vec::with_capacity(stubs.len());
        for (name, collector_closure) in stubs {
            // Skip: return lazy immediately, retain closure for later expansion
            if skip_names.contains(&name) {
                log::info!("[expand_testset_stubs] SKIP (in skip_names): {name}");
                result.push(TestSetResult::Lazy {
                    name,
                    collector_closure: Box::new(collector_closure),
                });
                continue;
            }

            let total_start = web_time::Instant::now();

            if let Some(timeout) = max_load_time {
                log::info!("[expand_testset_stubs] racing '{name}' with timeout {timeout:?}");
                // Race the collector invocation against a timer.
                let child_cancel = CancellationToken::new();
                let invoke_fut =
                    self.invoke_collector(&name, collector_closure.clone(), &child_cancel);

                let collector_result = tokio::select! {
                    biased;
                    () = cancel.cancelled() => {
                        child_cancel.cancel();
                        return Err(EngineError::Cancelled);
                    }
                    res = invoke_fut => {
                        log::info!("[expand_testset_stubs] '{name}' completed before timeout");
                        res
                    }
                    () = futures_timer::Delay::new(timeout) => {
                        // Timed out: cancel the in-flight invocation and keep lazy.
                        log::info!("[expand_testset_stubs] '{name}' TIMED OUT, marking lazy");
                        child_cancel.cancel();
                        result.push(TestSetResult::Lazy { name, collector_closure: Box::new(collector_closure) });
                        continue;
                    }
                };

                let (tests, nested_stubs, loading_time_ms) = collector_result?;
                let testsets = Box::pin(self.expand_testset_stubs(
                    nested_stubs,
                    cancel,
                    max_load_time,
                    skip_names,
                ))
                .await?;
                #[allow(clippy::cast_possible_truncation)]
                let total_loading_time_ms = total_start.elapsed().as_millis() as u64;
                result.push(TestSetResult::Expanded(TestSetInfo {
                    name,
                    tests,
                    testsets,
                    loading_time_ms,
                    total_loading_time_ms,
                }));
            } else {
                // No timeout — expand fully (original behavior)
                let (tests, nested_stubs, loading_time_ms) = self
                    .invoke_collector(&name, collector_closure, cancel)
                    .await?;
                let testsets =
                    Box::pin(self.expand_testset_stubs(nested_stubs, cancel, None, skip_names))
                        .await?;
                #[allow(clippy::cast_possible_truncation)]
                let total_loading_time_ms = total_start.elapsed().as_millis() as u64;
                result.push(TestSetResult::Expanded(TestSetInfo {
                    name,
                    tests,
                    testsets,
                    loading_time_ms,
                    total_loading_time_ms,
                }));
            }
        }
        Ok(result)
    }

    /// Expand a single lazy testset by its stored collector closure.
    ///
    /// Called when the user clicks "load" on a lazy testset in the UI.
    /// Uses `max_load_time` for nested child testsets but not for `name` itself
    /// (we always try to expand what the user explicitly requested).
    pub async fn expand_lazy_testset(
        self: &Arc<Self>,
        name: &str,
        collector_closure: BexExternalValue,
        cancel: CancellationToken,
        max_load_time: Option<std::time::Duration>,
    ) -> Result<TestSetInfo, EngineError> {
        let total_start = web_time::Instant::now();
        let (tests, nested_stubs, loading_time_ms) = self
            .invoke_collector(name, collector_closure, &cancel)
            .await?;
        let testsets = self
            .expand_testset_stubs(
                nested_stubs,
                &cancel,
                max_load_time,
                &std::collections::HashSet::new(),
            )
            .await?;
        #[allow(clippy::cast_possible_truncation)]
        let total_loading_time_ms = total_start.elapsed().as_millis() as u64;
        Ok(TestSetInfo {
            name: name.to_string(),
            tests,
            testsets,
            loading_time_ms,
            total_loading_time_ms,
        })
    }

    /// Run a single test by invoking `testing.run_test(body, runner)`.
    ///
    /// Returns a deep-extracted `TestReport` as a `BexExternalValue`.
    pub async fn run_test(
        self: &Arc<Self>,
        test: &TestInfo,
        cancel: CancellationToken,
    ) -> Result<BexExternalValue, EngineError> {
        log::info!(
            "[run_test] test='{}' body={:?} runner={:?}",
            test.name,
            test.body,
            test.runner
        );
        // Check if testing.run_test is resolved
        if let Some((ptr, kind)) = self.resolved_function_names.get("testing.run_test") {
            log::info!("[run_test] testing.run_test found: ptr={ptr:?} kind={kind:?}");
        } else {
            log::warn!("[run_test] testing.run_test NOT found in resolved_function_names");
        }
        let runner_arg = match &test.runner {
            Some(r) => r.clone(),
            None => BexExternalValue::Null,
        };
        self.call_function(
            "testing.run_test",
            vec![test.body.clone(), runner_arg],
            FunctionCallContextBuilder::new(CallId::next())
                .with_cancel_token(cancel)
                .build(),
            true, // deep-extract the TestReport
        )
        .await
    }

    /// Collect roots from a yielded VM.
    fn collect_vm_roots(vm: &BexVm) -> Vec<HeapPtr> {
        let mut roots = Vec::new();

        // Stack values
        for value in &vm.stack.0 {
            if let Value::Object(ptr) = value {
                roots.push(*ptr);
            }
        }

        // Watch state (last_assigned/last_notified values that aren't on the stack)
        vm.watch.collect_roots(&mut roots);

        // Frame function pointers (needed once closures are heap-allocated)
        roots.extend(vm.collect_frame_roots());

        // Note: Frame locals are stored in the stack at the locals_offset position,
        // so they're already included in the stack iteration above.

        roots
    }

    /// Run GC if conditions are met (called at safepoints).
    fn maybe_run_gc(&self, vm: &mut BexVm) {
        self.heap.verify_quick();
        if self.heap.should_gc() {
            let roots = Self::collect_vm_roots(vm);
            unsafe {
                let (stats, _remapped_roots, forwarding) =
                    self.heap.collect_garbage_with_forwarding(&roots);

                // Update VM stack with forwarding pointers
                for value in &mut vm.stack.0 {
                    if let Value::Object(ptr) = value {
                        if let Some(&new_ptr) = forwarding.get(ptr) {
                            *ptr = new_ptr;
                        }
                    }
                }

                // Update watch state (graph NodeIds, RootState values)
                vm.watch.apply_forwarding(&forwarding);

                // Update frame function pointers (needed for closures)
                vm.apply_frame_forwarding(&forwarding);

                // Invalidate TLAB so next allocation gets chunk from new space
                vm.tlab.invalidate();

                self.heap.reset_gc_counter();
                tracing::debug!(
                    "GC completed: {} live, {} collected",
                    stats.live_count,
                    stats.collected_count
                );
            }
            self.heap.verify_quick();
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
    async fn run_event_loop_with_epoch(
        self: &Arc<Self>,
        return_type: Ty,
        vm: &mut BexVm,
        my_epoch: u64,
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

            match vm.exec()? {
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
                    let pending = vm.pending_future(id)?;

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
                            let value = self.heap.with_gc_protection(|protected| {
                                self.convert_external_to_vm_value(
                                    vm,
                                    result,
                                    &protected.epoch_guard(),
                                )
                            });

                            vm.set_future_ready(id, value)?;
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

                    // Check if GC is waiting for our epoch to drain
                    let current = self.current_epoch.load(Ordering::Acquire);
                    if current > my_epoch {
                        // GC has been requested - we need to park
                        let slot = (my_epoch % 2) as usize;

                        // Register VM pointer before parking
                        // SAFETY: VM lives on our async task's stack and won't be dropped
                        // until after we unregister (after gc_complete.notified().await returns)
                        {
                            let mut parked_vms = self.epoch_states[slot].parked_vms.lock().unwrap();
                            parked_vms.push(VmPtr(std::ptr::from_ref(vm)));
                        }

                        // Increment parked count and notify GC
                        self.epoch_states[slot]
                            .parked
                            .fetch_add(1, Ordering::AcqRel);
                        self.epoch_drained.notify_one();

                        // Wait for GC to complete
                        // Note: GC will update our VM's stack with new object indices
                        self.gc_complete.notified().await;

                        // Unregister VM pointer after waking
                        {
                            let mut parked_vms = self.epoch_states[slot].parked_vms.lock().unwrap();
                            let vm_ptr = std::ptr::from_ref(vm);
                            parked_vms.retain(|p| p.0 != vm_ptr);
                        }

                        // Decrement parked count
                        self.epoch_states[slot]
                            .parked
                            .fetch_sub(1, Ordering::AcqRel);
                    }

                    // VM is at a safepoint (yielded) - check if GC should run
                    // (Only the triggering call runs GC, not parked VMs)
                    if self.current_epoch.load(Ordering::Acquire) == my_epoch {
                        self.maybe_run_gc(vm);
                    }

                    // First, drain any already-completed futures.
                    while let Ok(future) = processed_futures.try_recv() {
                        let external = future.result?;
                        let value = self.heap.with_gc_protection(|protected| {
                            self.convert_external_to_vm_value(
                                vm,
                                external,
                                &protected.epoch_guard(),
                            )
                        });
                        vm.fulfil_future(future.id, value)?;
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
                                let value = self.heap.with_gc_protection(|protected| {
                                    self.convert_external_to_vm_value(
                                        vm,
                                        external,
                                        &protected.epoch_guard(),
                                    )
                                });
                                vm.fulfil_future(future.id, value)?;
                                if future.id == future_id {
                                    break;
                                }
                            }
                        }
                    }
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
        let ctx = self.sys_op_ctx.to_op_context(cancel.clone(), self.clone());
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
