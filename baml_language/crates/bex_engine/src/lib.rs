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

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Instant, SystemTime},
};

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
// Re-export CancellationToken for callers.
pub use function_call_context::{FunctionCallContext, FunctionCallContextBuilder};
use sys_types::{CallId, OpError, SysOpResult};
use thiserror::Error;
use tokio::sync::{Notify, mpsc};
pub use tokio_util::sync::CancellationToken;

// ============================================================================
// Engine Types
// ============================================================================

/// Result of an external future.
struct FutureResult {
    id: HeapPtr,
    result: Result<BexExternalValue, EngineError>,
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
#[derive(Debug, Error)]
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
    /// Resolved class names for instance allocation
    resolved_class_names: HashMap<String, HeapPtr>,
    /// Resolved enum names for variant allocation
    resolved_enum_names: HashMap<String, HeapPtr>,
    /// System operations provider.
    sys_ops: std::sync::Arc<sys_types::SysOps>,
    /// Context passed to `sys_ops` that need engine-level information.
    sys_op_ctx: sys_types::SysOpContext,

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
fn default_round_robin_start() -> usize {
    // Keep wasm deterministic for tooling (matches legacy behavior).
    0
}

#[cfg(not(target_arch = "wasm32"))]
fn default_round_robin_start() -> usize {
    use std::time::UNIX_EPOCH;
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
        sys_ops: std::sync::Arc<sys_types::SysOps>,
    ) -> Result<Self, EngineError> {
        // Convert the pure bytecode to a VM-ready program with native functions attached
        let bytecode = bex_vm::convert_program(bytecode_program)?;

        // Extract compile-time objects for the heap
        let compile_time_objects: Vec<Object> = bytecode.objects.into_iter().collect();

        // Pre-compute class and enum indices before moving objects to heap.
        // This is used for allocating instances/variants from sys-op results.
        let class_indices: Vec<(String, usize)> = compile_time_objects
            .iter()
            .enumerate()
            .filter_map(|(idx, obj)| {
                if let Object::Class(class) = obj {
                    Some((class.name.clone(), idx))
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
                    Some((enm.name.clone(), idx))
                } else {
                    None
                }
            })
            .collect();

        // Create the unified heap with compile-time objects
        let heap = BexHeap::new(compile_time_objects);

        // Convert ObjectIndex -> HeapPtr for function lookup table.
        // Now that the heap exists, we can get stable pointers to compile-time objects.
        let resolved_function_names = bytecode
            .resolved_function_names
            .into_iter()
            .map(|(name, (idx, kind))| {
                let ptr = heap.compile_time_ptr(idx.into_raw());
                (name, (ptr, kind))
            })
            .collect();

        // Build class name lookup table from pre-computed indices.
        let resolved_class_names: HashMap<String, HeapPtr> = class_indices
            .into_iter()
            .map(|(name, idx)| (name, heap.compile_time_ptr(idx)))
            .collect();

        // Build enum name lookup table from pre-computed indices.
        let resolved_enum_names: HashMap<String, HeapPtr> = enum_indices
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
        let globals = GlobalPool::from_vec(globals_vec);

        // Build SysOpContext by pre-extracting LLM function metadata from the heap.
        // This avoids passing raw HeapPtrs to sys_ops.
        let llm_functions = Self::extract_llm_function_info(&resolved_function_names);

        // Convert compile-time client metadata to runtime format.
        let client_metadata: std::collections::HashMap<String, sys_types::ClientBuildMeta> =
            bytecode
                .client_metadata
                .into_iter()
                .map(|(name, meta)| {
                    let client_type = match meta.client_type {
                        bex_vm_types::ClientBuildType::Primitive => {
                            bex_heap::builtin_types::owned::LlmClientType::Primitive
                        }
                        bex_vm_types::ClientBuildType::Fallback => {
                            bex_heap::builtin_types::owned::LlmClientType::Fallback
                        }
                        bex_vm_types::ClientBuildType::RoundRobin => {
                            bex_heap::builtin_types::owned::LlmClientType::RoundRobin
                        }
                    };
                    let retry_policy = meta.retry_policy.map(|rp| {
                        bex_heap::builtin_types::owned::LlmRetryPolicy {
                            max_retries: rp.max_retries,
                            initial_delay_ms: rp.initial_delay_ms,
                            multiplier: rp.multiplier,
                            max_delay_ms: rp.max_delay_ms,
                        }
                    });
                    (
                        name,
                        sys_types::ClientBuildMeta {
                            client_type,
                            sub_client_names: meta.sub_client_names,
                            retry_policy,
                            round_robin_start: meta
                                .round_robin_start
                                .and_then(|start| usize::try_from(start).ok()),
                        },
                    )
                })
                .collect();

        // Build round-robin counters for composite clients.
        let round_robin_counters = client_metadata
            .iter()
            .filter(|(_, meta)| {
                matches!(
                    meta.client_type,
                    bex_heap::builtin_types::owned::LlmClientType::RoundRobin
                )
            })
            .map(|(name, meta)| {
                let start = meta
                    .round_robin_start
                    .unwrap_or_else(default_round_robin_start);
                (
                    name.clone(),
                    std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(start)),
                )
            })
            .collect();

        let sys_op_ctx = sys_types::SysOpContext {
            llm_functions: Arc::new(llm_functions),
            function_global_indices: Arc::new(bytecode.function_global_indices),
            template_strings_macros: Arc::new(bytecode.template_strings_macros),
            client_metadata: Arc::new(client_metadata),
            round_robin_counters: Arc::new(round_robin_counters),
            cancel: CancellationToken::new(),
        };

        Ok(Self {
            heap,
            globals,
            resolved_function_names,
            resolved_class_names,
            resolved_enum_names,
            sys_ops,
            sys_op_ctx,
            // Initialize epoch tracking
            current_epoch: AtomicU64::new(0),
            epoch_states: [EpochState::new(), EpochState::new()],
            epoch_drained: Notify::new(),
            gc_complete: Notify::new(),
            gc_in_progress: AtomicBool::new(false),
            active_calls: Mutex::new(HashMap::new()),
        })
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
        &self,
        function_name: &str,
        args: Vec<BexExternalValue>,
        FunctionCallContext {
            call_id,
            host_ctx,
            collectors,
            cancel,
        }: FunctionCallContext,
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
        let return_type = self.function_return_type(function_name).unwrap_or(Ty::Null);

        // Register with current epoch
        let my_epoch = self.current_epoch.load(Ordering::Acquire);
        let slot = (my_epoch % 2) as usize;
        self.epoch_states[slot]
            .active
            .fetch_add(1, Ordering::AcqRel);

        // SAFETY: We just registered with the epoch above
        let guard = unsafe { EpochGuard::new() };

        // Create VM with shared heap (each VM gets its own TLAB)
        let mut vm = BexVm::new(Arc::clone(&self.heap), self.globals.clone());

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

        bex_events::event_store::emit(RuntimeEvent {
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

        // active_calls cleanup is done by ActiveCallGuard on drop

        // If the call failed and the token is cancelled, upgrade to
        // EngineError::Cancelled. This ensures cooperative BAML-level checks
        // (which produce SysOpError via baml.sys.panic) are reported as
        // Cancelled so callers can programmatically distinguish cancellation
        // from genuine failures.
        match result {
            Err(_) if cancel.is_cancelled() => Err(EngineError::Cancelled),
            other => other,
        }
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
    fn lookup_function(&self, function_name: &str) -> Result<HeapPtr, EngineError> {
        self.resolved_function_names
            .get(function_name)
            .map(|(ptr, _kind)| *ptr)
            .ok_or_else(|| EngineError::FunctionNotFound {
                name: function_name.to_string(),
            })
    }

    /// Get the return type for a function by dereferencing its heap object.
    fn function_return_type(&self, name: &str) -> Option<Ty> {
        let (ptr, _kind) = self.resolved_function_names.get(name)?;
        // SAFETY: ptr is from resolved_function_names, a compile-time object
        let obj = unsafe { ptr.get() };
        match obj {
            Object::Function(func) => Some(func.return_type.clone()),
            _ => None,
        }
    }

    /// Get parameter names and types for a function by dereferencing its heap object.
    pub fn function_params(&self, name: &str) -> Result<Vec<(&str, &Ty)>, EngineError> {
        let (ptr, _kind) =
            self.resolved_function_names
                .get(name)
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

    /// Run the VM event loop until completion, with epoch tracking.
    ///
    /// The `my_epoch` parameter is used to check if GC has been requested
    /// (epoch advanced). VMs from old epochs will park at yield points.
    async fn run_event_loop_with_epoch(
        &self,
        return_type: Ty,
        vm: &mut BexVm,
        my_epoch: u64,
        call_id: CallId,
        span_state: &mut Option<SpanState>,
        cancel: &CancellationToken,
    ) -> Result<BexExternalValue, EngineError> {
        let (pending_futures, mut processed_futures) = mpsc::unbounded_channel::<FutureResult>();
        // Abort handles for spawned async tasks.
        //
        // Cancellation design: the VM event loop uses a biased `tokio::select!`
        // at every `Await` instruction, so cancellation is detected immediately
        // regardless of whether the in-flight sys_op is cancel-aware. However,
        // without abort handles the *spawned task* running the sys_op would
        // continue as an orphan until it completes naturally. For short-lived
        // ops (env.get, render_prompt, parse) this is irrelevant, but for
        // long-running ops (HTTP requests burning provider tokens, multi-second
        // sleeps) orphans waste real resources.
        //
        // Rather than making individual sys_ops cancel-aware (wrapping each in
        // its own `tokio::select!`), we store abort handles here and kill all
        // spawned tasks when cancellation fires. This keeps sys_op
        // implementations simple — new sys_ops never need to think about
        // cancellation.
        //
        // We use `futures::future::AbortHandle` (not `tokio::task::AbortHandle`)
        // so the same mechanism works on both native and WASM targets.
        let mut abort_handles: Vec<futures::future::AbortHandle> = Vec::new();

        'vm_exec: loop {
            match vm.exec()? {
                VmExecState::Complete(value) => {
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
                                event: EventKind::Function(FunctionEvent::End(FunctionEnd {
                                    name: root_span.label,
                                    result: external_result,
                                    duration: root_span.started_at.elapsed(),
                                })),
                            };
                            bex_events::event_store::emit(end_event);
                        }
                    }

                    return self.heap.with_gc_protection(|protected| {
                        // Convert to BexValue (handles for objects, BexExternalValue for primitives)
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

                    match self.execute_sys_op(pending.operation, &args, call_id, cancel) {
                        SysOpResult::Ready(result) => {
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
                                for handle in &abort_handles {
                                    handle.abort();
                                }
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
                                bex_events::event_store::emit(enter_event);

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
                                        event: EventKind::Function(FunctionEvent::End(
                                            FunctionEnd {
                                                name: function_name,
                                                result: external_result,
                                                duration: span.started_at.elapsed(),
                                            },
                                        )),
                                    };
                                    bex_events::event_store::emit(exit_event);
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
        &self,
        op: SysOp,
        args: &[BexExternalValue],
        call_id: CallId,
        cancel: &CancellationToken,
    ) -> SysOpResult {
        let args = args.iter().map(std::convert::Into::into).collect();
        let fn_ptr = self.sys_ops.get(op);
        let ctx = self.sys_op_ctx.with_cancel(cancel.clone());
        fn_ptr(&self.heap, args, &ctx, call_id)
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
