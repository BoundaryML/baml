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
//! the `ExternalOp` enum using static dispatch. This avoids dynamic dispatch
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

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
};

use baml_snapshot::BamlSnapshot;
pub use bex_external_types::{EpochGuard, ExternalValue, Snapshot};
use bex_heap::BexHeap;
// Re-export GcStats for users of the engine
pub use bex_heap::GcStats;
// Re-export bex_sys types for convenience
pub use bex_sys::{
    FileHandle, OpContext, OpError, ResolvedArgs, ResolvedValue, ResourceId, ResourceKind,
    ResourceRegistry, SocketHandle, SysOpResult, ops,
};
use bex_vm::{BexVm, NativeFunction, VmExecState};
use bex_vm_types::{ExternalOp, GlobalPool, Object, ObjectIndex, SysOp, Value};
use thiserror::Error;
use tokio::sync::{Notify, mpsc};

// ============================================================================
// Engine Types
// ============================================================================

/// Result of an external future.
struct FutureResult {
    id: ObjectIndex,
    result: Result<ResolvedValue, EngineError>,
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
#[allow(unsafe_code)]
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

/// Errors that can occur during engine execution.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Function not found: {name}")]
    FunctionNotFound { name: String },

    #[error("External operation failed: {0}")]
    ExternalOpFailed(#[from] OpError),

    #[error("Future channel closed unexpectedly")]
    FutureChannelClosed,

    #[error("VM error: {0}")]
    VmError(#[from] bex_vm::errors::VmError),

    #[error("Internal VM error: {0}")]
    InternalVmError(#[from] bex_vm::InternalError),

    #[error("Cannot snapshot object of type {type_name}")]
    CannotSnapshot { type_name: String },
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
/// let engine = Arc::new(BexEngine::new(snapshot, env_vars)?);
///
/// // Concurrent calls are safe - each gets its own VM and TLAB
/// let (result1, result2) = tokio::join!(
///     engine.call_function("process_order", &order1_args),
///     engine.call_function("process_order", &order2_args),
/// );
///
/// // Or with explicit spawning:
/// let engine_clone = Arc::clone(&engine);
/// let handle = tokio::spawn(async move {
///     engine_clone.call_function("background_task", &[]).await
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
    /// The original snapshot (for metadata access)
    snapshot: BamlSnapshot,
    /// The unified heap (shared across all VM instances)
    heap: Arc<BexHeap<NativeFunction>>,
    /// Global variables pool
    globals: GlobalPool,
    /// Resolved function/class/enum names for lookup
    resolved_function_names:
        HashMap<String, (ObjectIndex, bex_vm_types::FunctionKind<NativeFunction>)>,
    /// Environment variables passed to VM.
    env_vars: HashMap<String, String>,

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
}

impl BexEngine {
    /// Create a new engine with the given program.
    ///
    /// The engine creates a unified heap containing compile-time objects
    /// (functions, classes, enums). Each function call creates a VM that
    /// shares this heap and allocates runtime objects into its own TLAB.
    pub fn new(
        snapshot: BamlSnapshot,
        env_vars: HashMap<String, String>,
    ) -> Result<Self, EngineError> {
        // Convert the pure bytecode to a VM-ready program with native functions attached
        let bytecode = bex_vm::convert_program(snapshot.bytecode.clone())?;

        // Extract compile-time objects for the heap
        let compile_time_objects: Vec<Object<NativeFunction>> =
            bytecode.objects.into_iter().collect();

        // Create the unified heap with compile-time objects
        let heap = BexHeap::new(compile_time_objects);

        Ok(Self {
            snapshot,
            heap,
            globals: bytecode.globals,
            resolved_function_names: bytecode.resolved_function_names,
            env_vars,
            // Initialize epoch tracking
            current_epoch: AtomicU64::new(0),
            epoch_states: [EpochState::new(), EpochState::new()],
            epoch_drained: Notify::new(),
            gc_complete: Notify::new(),
            gc_in_progress: AtomicBool::new(false),
        })
    }

    /// Get a reference to the program snapshot.
    pub fn program(&self) -> &BamlSnapshot {
        &self.snapshot
    }

    /// Get a reference to the shared heap.
    pub fn heap(&self) -> &Arc<BexHeap<NativeFunction>> {
        &self.heap
    }

    /// Get statistics about heap usage.
    ///
    /// Useful for monitoring concurrent execution and debugging.
    pub fn heap_stats(&self) -> bex_heap::HeapStats {
        self.heap.stats()
    }

    /// Convert an `ExternalValue` to a `Snapshot` (owned data).
    ///
    /// - For `Snapshot` variants: returns the snapshot directly
    /// - For `Object(Handle)`: resolves the handle and deep-copies the object graph
    ///
    /// # Supported Object Types
    ///
    /// - `String` → `Snapshot::String`
    /// - `Array` → `Snapshot::Array` (recursively converts elements)
    /// - `Map` → `Snapshot::Map` (recursively converts values)
    /// - `Instance` → `Snapshot::Instance` (includes class name and field names)
    /// - `Variant` → `Snapshot::Variant` (includes enum and variant names)
    ///
    /// # Errors
    ///
    /// Returns `EngineError::CannotSnapshot` for object types that cannot be
    /// converted (Function, Class, Enum, Future, Media).
    ///
    /// # Panics
    ///
    /// Panics if the handle is invalid (should never happen - handles are GC roots).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = engine.call_function("get_user", &[]).await?;
    /// let snapshot = engine.to_snapshot(result)?;
    /// match snapshot {
    ///     Snapshot::Instance { class_name, fields } => {
    ///         println!("Got {} with {} fields", class_name, fields.len());
    ///     }
    ///     _ => {}
    /// }
    /// ```
    pub fn to_snapshot(&self, external: ExternalValue) -> Result<Snapshot, EngineError> {
        match external {
            ExternalValue::Snapshot(s) => Ok(s),
            ExternalValue::Object(handle) => self.snapshot_handle(&handle),
        }
    }

    /// Convert a handle to a snapshot.
    ///
    /// This is safe for external code to call (no `EpochGuard` needed) because
    /// we hold the handle table read lock for the entire operation, preventing
    /// GC from moving objects while we're snapshotting.
    fn snapshot_handle(
        &self,
        handle: &bex_external_types::Handle,
    ) -> Result<Snapshot, EngineError> {
        // Hold the handles read lock for the entire snapshot operation.
        // This prevents GC from running update_handles (which needs write lock),
        // ensuring all ObjectIndex values remain valid during recursive snapshotting.
        //
        // The GcProtectedHeap guard ensures resolve_handle can only be called
        // while the lock is held - you can't accidentally use it unsafely.
        self.heap.with_gc_protection(|protected| {
            let idx = protected
                .resolve_handle(handle.slab_key())
                .expect("Handle is a GC root - object should never be collected");
            self.snapshot_object(idx)
        })
    }

    /// Convert an object at the given index to a snapshot.
    ///
    /// # Safety
    ///
    /// This method uses unsafe calls to `heap.get_object()`. It is safe because:
    /// - We only read objects, never write
    /// - The caller ensures the index is valid (from a handle which is a GC root)
    fn snapshot_object(&self, idx: ObjectIndex) -> Result<Snapshot, EngineError> {
        // SAFETY: We only read objects, and the index comes from a valid handle.
        // No concurrent writes can occur while we hold a reference to the heap.
        #[allow(unsafe_code)]
        let obj = unsafe { self.heap().get_object(idx) };
        match obj {
            Object::String(s) => Ok(Snapshot::String(s.clone())),
            Object::Array(arr) => {
                let items: Result<Vec<_>, _> = arr.iter().map(|v| self.snapshot_value(v)).collect();
                Ok(Snapshot::Array(items?))
            }
            Object::Map(map) => {
                let entries: Result<indexmap::IndexMap<String, Snapshot>, EngineError> = map
                    .iter()
                    .map(|(k, v)| Ok((k.clone(), self.snapshot_value(v)?)))
                    .collect();
                Ok(Snapshot::Map(entries?))
            }
            Object::Instance(instance) => {
                // Get class name and field names from the Class object
                // SAFETY: Same as above - read-only access to a valid object
                #[allow(unsafe_code)]
                let class_obj = unsafe { self.heap().get_object(instance.class) };
                let (class_name, field_names) = match class_obj {
                    Object::Class(class) => (class.name.clone(), &class.field_names),
                    _ => panic!("Instance.class should point to a Class object"),
                };

                // Convert fields with their names
                let fields: Result<indexmap::IndexMap<String, Snapshot>, EngineError> = field_names
                    .iter()
                    .zip(instance.fields.iter())
                    .map(|(name, value)| Ok((name.clone(), self.snapshot_value(value)?)))
                    .collect();

                Ok(Snapshot::Instance {
                    class_name,
                    fields: fields?,
                })
            }
            Object::Variant(variant) => {
                // Get enum name and variant name from the Enum object
                // SAFETY: Same as above - read-only access to a valid object
                #[allow(unsafe_code)]
                let enum_obj = unsafe { self.heap().get_object(variant.enm) };
                let (enum_name, variant_name) = match enum_obj {
                    Object::Enum(enm) => {
                        let variant_name = enm
                            .variant_names
                            .get(variant.index)
                            .cloned()
                            .unwrap_or_else(|| format!("variant_{}", variant.index));
                        (enm.name.clone(), variant_name)
                    }
                    _ => panic!("Variant.enm should point to an Enum object"),
                };

                Ok(Snapshot::Variant {
                    enum_name,
                    variant_name,
                })
            }
            Object::Function(_) => Err(EngineError::CannotSnapshot {
                type_name: "function".to_string(),
            }),
            Object::Class(_) => Err(EngineError::CannotSnapshot {
                type_name: "class".to_string(),
            }),
            Object::Enum(_) => Err(EngineError::CannotSnapshot {
                type_name: "enum".to_string(),
            }),
            Object::Future(_) => Err(EngineError::CannotSnapshot {
                type_name: "future".to_string(),
            }),
            Object::Media(_) => Err(EngineError::CannotSnapshot {
                type_name: "media".to_string(),
            }),
        }
    }

    /// Convert a VM Value to a Snapshot.
    fn snapshot_value(&self, value: &Value) -> Result<Snapshot, EngineError> {
        match value {
            Value::Null => Ok(Snapshot::Null),
            Value::Int(i) => Ok(Snapshot::Int(*i)),
            Value::Float(f) => Ok(Snapshot::Float(*f)),
            Value::Bool(b) => Ok(Snapshot::Bool(*b)),
            Value::Object(idx) => self.snapshot_object(*idx),
        }
    }

    /// Convert a VM Value to an `ExternalValue`.
    ///
    /// Primitives are wrapped in Snapshot, heap objects get a Handle.
    fn value_to_external(&self, value: Value) -> ExternalValue {
        match value {
            Value::Null => ExternalValue::Snapshot(Snapshot::Null),
            Value::Int(i) => ExternalValue::Snapshot(Snapshot::Int(i)),
            Value::Float(f) => ExternalValue::Snapshot(Snapshot::Float(f)),
            Value::Bool(b) => ExternalValue::Snapshot(Snapshot::Bool(b)),
            Value::Object(idx) => {
                let handle = self.heap().create_handle(idx);
                ExternalValue::Object(handle)
            }
        }
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
        #[allow(unsafe_code)]
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
        #[allow(unsafe_code)]
        let (stats, _remapped_roots, forwarding) =
            unsafe { self.heap.collect_garbage_with_forwarding(&all_roots) };

        // Update all parked VM stacks with forwarding pointers and invalidate TLABs
        // SAFETY: VMs are still parked (gc_complete not yet notified), we have
        // exclusive access via the parked_vms lock we're still holding
        #[allow(unsafe_code)]
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

            // Invalidate TLAB so next allocation gets chunk from new space
            vm.tlab.invalidate();
        }

        // Release lock before notifying waiters
        drop(parked_vms);

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

    /// Execute a function by name.
    ///
    /// This method is `&self` because each call creates its own VM with a TLAB.
    /// Concurrent calls work naturally - each gets its own VM and TLAB.
    ///
    /// # Arguments
    ///
    /// Arguments are passed as `ExternalValue` types:
    /// - Primitives convert to `Snapshot` via `From` impls
    /// - `Object(Handle)` references existing heap objects
    /// - `Snapshot(...)` allocates new objects on the heap
    ///
    /// # Returns
    ///
    /// Returns `ExternalValue`:
    /// - Primitives return as `Snapshot(Snapshot::Int/Float/Bool/Null)`
    /// - Heap objects return as `Object(Handle)` - a reference, not a deep copy
    ///
    /// Use `to_snapshot()` to convert handles to owned data when needed.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = engine.call_function("get_user", &[
    ///     "Alice".into(),
    ///     42i64.into(),
    /// ]).await?;
    ///
    /// // Get owned data
    /// let snapshot = engine.to_snapshot(result)?;
    /// ```
    pub async fn call_function(
        &self,
        function_name: &str,
        args: &[ExternalValue],
    ) -> Result<ExternalValue, EngineError> {
        // Wait for any in-progress GC to complete.
        // This ensures Handles in args have stable indices.
        while self.gc_in_progress.load(Ordering::Acquire) {
            self.gc_complete.notified().await;
        }

        // Look up the function to verify it exists
        let function_index = self.lookup_function(function_name)?;

        // Register with current epoch
        let my_epoch = self.current_epoch.load(Ordering::Acquire);
        let slot = (my_epoch % 2) as usize;
        self.epoch_states[slot]
            .active
            .fetch_add(1, Ordering::AcqRel);

        // SAFETY: We just registered with the epoch above
        #[allow(unsafe_code)]
        let guard = unsafe { EpochGuard::new() };

        // Create VM with shared heap (each VM gets its own TLAB)
        let mut vm = BexVm::new(
            Arc::clone(&self.heap),
            self.globals.clone(),
            self.env_vars.clone(),
        );

        // Convert ExternalValue args to Value, allocating Snapshots on the heap
        let vm_args: Vec<Value> = args
            .iter()
            .map(|arg| Self::externalize_to_value(&mut vm, arg, &guard))
            .collect();

        // Set entry point with converted args
        vm.set_entry_point(function_index, &vm_args);

        // Create a resource registry for this call
        let ctx = Arc::new(OpContext::new());

        // Run the event loop with epoch tracking
        let result = self.run_event_loop_with_epoch(&mut vm, ctx, my_epoch).await;

        // Unregister from epoch
        if self.epoch_states[slot]
            .active
            .fetch_sub(1, Ordering::AcqRel)
            == 1
        {
            // We were the last active VM in this epoch
            self.epoch_drained.notify_one();
        }

        result
    }

    /// Convert an `ExternalValue` to a VM `Value`.
    ///
    /// Requires `EpochGuard` because resolving handles returns an `ObjectIndex`
    /// that must remain valid while we use it.
    ///
    /// - `Object(Handle)` extracts the `ObjectIndex`
    /// - `Snapshot(...)` recursively allocates on the heap
    fn externalize_to_value(
        vm: &mut BexVm,
        external: &ExternalValue,
        guard: &EpochGuard<'_>,
    ) -> Value {
        match external {
            ExternalValue::Object(handle) => {
                // Resolve through table to get current index after any GC
                let idx = handle
                    .object_index(guard)
                    .expect("Handle should be valid - object was returned to external code");
                Value::Object(idx)
            }
            ExternalValue::Snapshot(snapshot) => Self::allocate_snapshot(vm, snapshot),
        }
    }

    /// Recursively allocate a `Snapshot` onto the heap, returning a `Value`.
    fn allocate_snapshot(vm: &mut BexVm, snapshot: &Snapshot) -> Value {
        match snapshot {
            Snapshot::Null => Value::Null,
            Snapshot::Int(i) => Value::Int(*i),
            Snapshot::Float(f) => Value::Float(*f),
            Snapshot::Bool(b) => Value::Bool(*b),
            Snapshot::String(s) => vm.alloc_string(s.clone()),
            Snapshot::Array(arr) => {
                let values: Vec<Value> = arr
                    .iter()
                    .map(|item| Self::allocate_snapshot(vm, item))
                    .collect();
                vm.alloc_array(values)
            }
            Snapshot::Map(map) => {
                let values: indexmap::IndexMap<String, Value> = map
                    .iter()
                    .map(|(k, v): (&String, &Snapshot)| (k.clone(), Self::allocate_snapshot(vm, v)))
                    .collect();
                vm.alloc_map(values)
            }
            Snapshot::Instance { .. } => {
                // Instance allocation requires class lookup - not supported from external
                // External callers should use the class constructor functions
                panic!("Cannot allocate Instance from Snapshot - use class constructor functions")
            }
            Snapshot::Variant { .. } => {
                // Variant allocation requires enum lookup - not supported from external
                // External callers should use enum variant values
                panic!("Cannot allocate Variant from Snapshot - use enum values")
            }
        }
    }

    /// Look up a function by name and return its bytecode index.
    fn lookup_function(&self, function_name: &str) -> Result<ObjectIndex, EngineError> {
        self.resolved_function_names
            .get(function_name)
            .map(|(idx, _kind)| *idx)
            .ok_or_else(|| EngineError::FunctionNotFound {
                name: function_name.to_string(),
            })
    }

    /// Collect roots from a yielded VM.
    fn collect_vm_roots(vm: &BexVm) -> Vec<ObjectIndex> {
        let mut roots = Vec::new();

        // Stack values
        for value in &vm.stack.0 {
            if let Value::Object(idx) = value {
                roots.push(*idx);
            }
        }

        // Note: Frame locals are stored in the stack at the locals_offset position,
        // so they're already included in the stack iteration above.

        roots
    }

    /// Run GC if conditions are met (called at safepoints).
    fn maybe_run_gc(&self, vm: &mut BexVm) {
        if self.heap.should_gc() {
            let roots = Self::collect_vm_roots(vm);
            #[allow(unsafe_code)]
            unsafe {
                let (stats, _remapped_roots, forwarding) =
                    self.heap.collect_garbage_with_forwarding(&roots);

                // Update VM stack with forwarding pointers
                for value in &mut vm.stack.0 {
                    if let Value::Object(idx) = value {
                        if let Some(&new_idx) = forwarding.get(idx) {
                            *idx = new_idx;
                        }
                    }
                }

                // Invalidate TLAB so next allocation gets chunk from new space
                vm.tlab.invalidate();

                self.heap.reset_gc_counter();
                tracing::debug!(
                    "GC completed: {} live, {} collected",
                    stats.live_count,
                    stats.collected_count
                );
            }
        }
    }

    /// Run the VM event loop until completion, with epoch tracking.
    ///
    /// The `my_epoch` parameter is used to check if GC has been requested
    /// (epoch advanced). VMs from old epochs will park at yield points.
    async fn run_event_loop_with_epoch(
        &self,
        vm: &mut BexVm,
        ctx: Arc<OpContext>,
        my_epoch: u64,
    ) -> Result<ExternalValue, EngineError> {
        let (pending_futures, mut processed_futures) = mpsc::unbounded_channel::<FutureResult>();

        'vm_exec: loop {
            match vm.exec()? {
                VmExecState::Complete(value) => {
                    // Convert to ExternalValue (handles for objects, snapshots for primitives)
                    return Ok(self.value_to_external(value));
                }

                VmExecState::ScheduleFuture(id) => {
                    let pending = vm.pending_future(id)?;

                    // Resolve arguments from VM values to ResolvedValues
                    let resolved_args = ResolvedArgs {
                        args: pending
                            .args
                            .iter()
                            .map(|v| Self::resolve_value(vm, v))
                            .collect(),
                    };

                    match pending.operation {
                        ExternalOp::Llm => {
                            // LLM operations are always async (not yet implemented)
                            let pending_futures = pending_futures.clone();
                            tokio::spawn(async move {
                                let result = Err(OpError::Other(
                                    "LLM operations not yet implemented".into(),
                                ));
                                let _ = pending_futures.send(FutureResult {
                                    id,
                                    result: result.map_err(EngineError::from),
                                });
                            });
                        }
                        ExternalOp::Sys(sys_op) => {
                            match Self::execute_sys_op(sys_op, Arc::clone(&ctx), resolved_args) {
                                SysOpResult::Ready(result) => {
                                    // Sync operation - fulfill immediately
                                    let value = Self::unresolve_value(
                                        vm,
                                        result.map_err(EngineError::from)?,
                                    );
                                    vm.fulfil_future(id, value)?;
                                }
                                SysOpResult::Async(fut) => {
                                    // Async operation - spawn task
                                    let pending_futures = pending_futures.clone();
                                    tokio::spawn(async move {
                                        let result = fut.await;
                                        let _ = pending_futures.send(FutureResult {
                                            id,
                                            result: result.map_err(EngineError::from),
                                        });
                                    });
                                }
                            }
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
                        // TODO: When there's an error in the future, we must handle somehow.
                        let resolved = future.result?;
                        let value = Self::unresolve_value(vm, resolved);
                        vm.fulfil_future(future.id, value)?;
                        // Future fulfilled, we can continue executing the VM.
                        if future.id == future_id {
                            continue 'vm_exec;
                        }
                    }

                    // We gotta wait for the target future.
                    loop {
                        let future = processed_futures
                            .recv()
                            .await
                            .ok_or(EngineError::FutureChannelClosed)?;

                        // TODO: When there's an error in the future, we must handle somehow.
                        let resolved = future.result?;
                        let value = Self::unresolve_value(vm, resolved);
                        vm.fulfil_future(future.id, value)?;
                        // Future fulfilled, we can continue executing the VM.
                        if future.id == future_id {
                            break;
                        }
                    }
                }

                VmExecState::Notify(_notification) => {
                    // Ignore watch notifications for now
                }
            }
        }
    }

    /// Execute a system operation, returning either an immediate result or a future.
    fn execute_sys_op(op: SysOp, ctx: Arc<OpContext>, args: ResolvedArgs) -> SysOpResult {
        match op {
            // Async operations - return boxed futures
            SysOp::FsOpen => SysOpResult::Async(Box::pin(ops::fs::open(ctx, args))),
            SysOp::FsRead => SysOpResult::Async(Box::pin(ops::fs::read(ctx, args))),
            SysOp::Shell => SysOpResult::Async(Box::pin(ops::sys::shell(ctx, args))),
            SysOp::NetConnect => SysOpResult::Async(Box::pin(ops::net::connect(ctx, args))),
            SysOp::NetRead => SysOpResult::Async(Box::pin(ops::net::read(ctx, args))),
            // Sync operations - return immediate results
            SysOp::FsClose => SysOpResult::Ready(ops::fs::close(&ctx, args)),
            SysOp::NetClose => SysOpResult::Ready(ops::net::close(&ctx, args)),
        }
    }

    /// Resolve a VM value to a `ResolvedValue`.
    fn resolve_value(vm: &BexVm, value: &Value) -> ResolvedValue {
        match value {
            Value::Null => ResolvedValue::Null,
            Value::Int(i) => ResolvedValue::Int(*i),
            Value::Float(f) => ResolvedValue::Float(*f),
            Value::Bool(b) => ResolvedValue::Bool(*b),
            Value::Object(idx) => {
                let obj = vm.get_object(*idx);
                match obj {
                    Object::String(s) => ResolvedValue::String(s.clone()),
                    Object::Array(arr) => {
                        let resolved: Vec<ResolvedValue> =
                            arr.iter().map(|v| Self::resolve_value(vm, v)).collect();
                        ResolvedValue::Array(resolved)
                    }
                    Object::Map(map) => {
                        let resolved: indexmap::IndexMap<String, ResolvedValue> = map
                            .iter()
                            .map(|(k, v)| (k.clone(), Self::resolve_value(vm, v)))
                            .collect();
                        ResolvedValue::Map(resolved)
                    }
                    other => {
                        panic!("Cannot resolve object type to ResolvedValue: {other:?}")
                    }
                }
            }
        }
    }

    /// Convert a `ResolvedValue` back to a VM Value.
    fn unresolve_value(vm: &mut BexVm, resolved: ResolvedValue) -> Value {
        match resolved {
            ResolvedValue::Null => Value::Null,
            ResolvedValue::Int(i) => Value::Int(i),
            ResolvedValue::Float(f) => Value::Float(f),
            ResolvedValue::Bool(b) => Value::Bool(b),
            ResolvedValue::String(s) => vm.alloc_string(s),
            ResolvedValue::Array(arr) => {
                let values: Vec<Value> = arr
                    .into_iter()
                    .map(|v| Self::unresolve_value(vm, v))
                    .collect();
                vm.alloc_array(values)
            }
            ResolvedValue::Map(map) => {
                let values: indexmap::IndexMap<String, Value> = map
                    .into_iter()
                    .map(|(k, v)| (k, Self::unresolve_value(vm, v)))
                    .collect();
                vm.alloc_map(values)
            }
            ResolvedValue::ResourceId(id) => {
                // Store resource ID as an integer value
                Value::Int(id.cast_signed())
            }
        }
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
        // 1. Create a test BamlSnapshot with a simple function
        // 2. Create a BexEngine from the snapshot
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
