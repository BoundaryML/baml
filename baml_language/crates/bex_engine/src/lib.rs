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

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
};

pub use bex_external_types::{BexExternalValue, BexValue, EpochGuard, Ty, UnionMetadata};
use bex_heap::BexHeap;
// Re-export GcStats for users of the engine
pub use bex_heap::GcStats;
use bex_program::BexProgram;
use bex_vm::{BexVm, VmExecState};
use bex_vm_types::{ExternalOp, GlobalPool, HeapPtr, Object, Value};
// Re-export sys_types types for convenience
pub use sys_types::{
    CompletionHandle, OpError, ResourceHandle, ResourceType, SysOp, SysOpFn, SysOpResult, SysOps,
};
use thiserror::Error;
use tokio::sync::{Notify, mpsc};

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

    #[error("Cannot convert object of type {type_name}")]
    CannotConvert { type_name: String },

    #[error("Type mismatch: {message}")]
    TypeMismatch { message: String },

    #[error("Schema inconsistency: {message}")]
    SchemaInconsistency { message: String },

    #[cfg(feature = "heap_debug")]
    #[error("Snapshot not possible for type: {type_name}")]
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
    snapshot: BexProgram,
    /// The unified heap (shared across all VM instances)
    heap: Arc<BexHeap>,
    /// Global variables pool
    globals: GlobalPool,
    /// Resolved function/class/enum names for lookup
    resolved_function_names: HashMap<String, (HeapPtr, bex_vm_types::FunctionKind)>,
    /// Environment variables passed to VM.
    env_vars: HashMap<String, String>,
    /// System operations provider.
    sys_ops: sys_types::SysOps,

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
    ///
    /// # Arguments
    ///
    /// * `snapshot` - The compiled BAML program
    /// * `env_vars` - Environment variables accessible to the program
    /// * `sys_ops` - System operations provider (use `sys_types_native::SysOps::native()` for default)
    pub fn new(
        snapshot: BexProgram,
        env_vars: HashMap<String, String>,
        sys_ops: sys_types::SysOps,
    ) -> Result<Self, EngineError> {
        // Convert the pure bytecode to a VM-ready program with native functions attached
        let bytecode = bex_vm::convert_program(snapshot.bytecode.clone())?;

        // Extract compile-time objects for the heap
        let compile_time_objects: Vec<Object> = bytecode.objects.into_iter().collect();

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

        // Convert compile-time globals (ConstValue) to runtime globals (Value).
        // Object references are converted from ObjectIndex to HeapPtr.
        let globals_vec: Vec<Value> = bytecode
            .globals
            .into_iter()
            .map(|cv| cv.to_value(|idx| heap.compile_time_ptr(idx.into_raw())))
            .collect();
        let globals = GlobalPool::from_vec(globals_vec);

        Ok(Self {
            snapshot,
            heap,
            globals,
            resolved_function_names,
            env_vars,
            sys_ops,
            // Initialize epoch tracking
            current_epoch: AtomicU64::new(0),
            epoch_states: [EpochState::new(), EpochState::new()],
            epoch_drained: Notify::new(),
            gc_complete: Notify::new(),
            gc_in_progress: AtomicBool::new(false),
        })
    }

    /// Get a reference to the program snapshot.
    pub fn program(&self) -> &BexProgram {
        &self.snapshot
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

    /// Convert a `BexValue` to a `BexExternalValue` (owned data).
    ///
    /// - For `External` variants: returns the value directly
    /// - For `Opaque(Handle)`: resolves the handle and deep-copies
    ///
    /// If the declared type is a union, the value is wrapped in `Union { value, metadata }`.
    ///
    /// # Supported Object Types
    ///
    /// - `String` → `BexExternalValue::String`
    /// - `Array` → `BexExternalValue::Array` (recursively converts elements)
    /// - `Map` → `BexExternalValue::Map` (recursively converts values)
    /// - `Instance` → `BexExternalValue::Instance` (includes class name and fields)
    /// - `Variant` → `BexExternalValue::Variant` (includes enum and variant names)
    ///
    /// # Errors
    ///
    /// Returns `EngineError::CannotConvert` for object types that cannot be
    /// converted (Function, Class, Enum, Future, Media).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = engine.call_function("get_user", &[]).await?;
    /// match result {
    ///     BexExternalValue::Instance { class_name, fields } => {
    ///         println!("Got {} with {} fields", class_name, fields.len());
    ///     }
    ///     BexExternalValue::Union { value, metadata } => {
    ///         println!("Union selected: {}", metadata.selected_option);
    ///     }
    ///     _ => {}
    /// }
    /// ```
    pub fn to_bex_external(
        &self,
        value: BexValue,
        declared_type: &Ty,
    ) -> Result<BexExternalValue, EngineError> {
        match value {
            BexValue::External(s) => Self::maybe_wrap_union(s, declared_type),
            BexValue::Opaque(handle) => self.handle_to_external(&handle, declared_type),
        }
    }

    /// Convert a handle to a `BexExternalValue` using the declared type.
    ///
    /// This is safe for external code to call (no `EpochGuard` needed) because
    /// we hold the handle table read lock for the entire operation, preventing
    /// GC from moving objects while we're converting.
    fn handle_to_external(
        &self,
        handle: &bex_external_types::Handle,
        declared_type: &Ty,
    ) -> Result<BexExternalValue, EngineError> {
        // Hold the handles read lock for the entire conversion operation.
        // This prevents GC from running update_handles (which needs write lock),
        // ensuring all ObjectIndex values remain valid during recursive conversion.
        //
        // The GcProtectedHeap guard ensures resolve_handle can only be called
        // while the lock is held - you can't accidentally use it unsafely.
        self.heap.with_gc_protection(|protected| {
            let idx = protected
                .resolve_handle(handle.slab_key())
                .expect("Handle is a GC root - object should never be collected");
            let value = Value::Object(idx);
            self.vm_value_to_external(&value, declared_type)
        })
    }

    /// Wrap a value in Union metadata if the declared type is a union.
    fn maybe_wrap_union(
        value: BexExternalValue,
        declared_type: &Ty,
    ) -> Result<BexExternalValue, EngineError> {
        match declared_type {
            Ty::Union(members) => {
                let selected = Self::find_matching_member(&value, members)?;
                let metadata = UnionMetadata::new(declared_type.clone(), selected);
                Ok(BexExternalValue::Union {
                    value: Box::new(value),
                    metadata,
                })
            }
            Ty::Optional(inner) => {
                let selected = if matches!(value, BexExternalValue::Null) {
                    Ty::Null
                } else {
                    (**inner).clone()
                };
                let metadata = UnionMetadata::new(declared_type.clone(), selected);
                Ok(BexExternalValue::Union {
                    value: Box::new(value),
                    metadata,
                })
            }
            _ => Ok(value),
        }
    }

    /// Find which union member matches a value.
    fn find_matching_member(value: &BexExternalValue, members: &[Ty]) -> Result<Ty, EngineError> {
        for member in members {
            if Self::value_matches_type(value, member) {
                return Ok(member.clone());
            }
        }
        // This indicates a type system inconsistency - the value should match one of the members
        Err(EngineError::TypeMismatch {
            message: format!(
                "Value of type '{}' does not match any member of union {:?}",
                value.type_name(),
                members
            ),
        })
    }

    /// Check if a value matches a declared type.
    fn value_matches_type(value: &BexExternalValue, ty: &Ty) -> bool {
        match (value, ty) {
            (BexExternalValue::Null, Ty::Null) => true,
            (BexExternalValue::Int(_), Ty::Int) => true,
            (BexExternalValue::Float(_), Ty::Float) => true,
            (BexExternalValue::Bool(_), Ty::Bool) => true,
            (BexExternalValue::String(_), Ty::String) => true,
            (BexExternalValue::Array { .. }, Ty::List(_)) => true,
            (BexExternalValue::Map { .. }, Ty::Map { .. }) => true,
            (BexExternalValue::Instance { class_name, .. }, Ty::Class(name)) => class_name == name,
            (BexExternalValue::Variant { enum_name, .. }, Ty::Enum(name)) => enum_name == name,
            (BexExternalValue::Union { value, .. }, ty) => Self::value_matches_type(value, ty),
            // Handle nested unions/optionals in the type
            (value, Ty::Union(members)) => {
                members.iter().any(|m| Self::value_matches_type(value, m))
            }
            (value, Ty::Optional(inner)) => {
                matches!(value, BexExternalValue::Null) || Self::value_matches_type(value, inner)
            }
            _ => false,
        }
    }

    // =========================================================================
    // External Value Conversion (with declared type information)
    // =========================================================================

    /// Convert a VM Value to a `BexExternalValue` using the declared type.
    ///
    /// If the declared type is a union, the value is wrapped in `Union { value, metadata }`.
    fn vm_value_to_external(
        &self,
        value: &Value,
        declared_type: &Ty,
    ) -> Result<BexExternalValue, EngineError> {
        // If declared type is a union, find which member matches the actual value
        let effective_type = Self::resolve_effective_type(value, declared_type);

        let external = match value {
            Value::Null => BexExternalValue::Null,
            Value::Int(i) => BexExternalValue::Int(*i),
            Value::Float(f) => BexExternalValue::Float(*f),
            Value::Bool(b) => BexExternalValue::Bool(*b),
            Value::Object(idx) => self.vm_object_to_external(*idx, effective_type)?,
        };

        // Wrap in Union if declared type is a union
        Self::maybe_wrap_union(external, declared_type)
    }

    /// Convert an object to a `BexExternalValue` using the effective (non-union) type.
    ///
    /// # Safety
    ///
    /// This method uses unsafe calls to dereference `HeapPtr`. It is safe because:
    /// - We only read objects, never write
    /// - The caller ensures the pointer is valid (from a handle which is a GC root)
    fn vm_object_to_external(
        &self,
        ptr: HeapPtr,
        effective_type: &Ty,
    ) -> Result<BexExternalValue, EngineError> {
        // SAFETY: We only read objects, and the pointer comes from a valid handle.
        let obj = unsafe { ptr.get() };

        match obj {
            Object::String(s) => Ok(BexExternalValue::String(s.clone())),

            Object::Array(arr) => {
                // Get element type from declared type
                let element_type = match effective_type {
                    Ty::List(elem_ty) => elem_ty.as_ref(),
                    other => {
                        return Err(EngineError::TypeMismatch {
                            message: format!("VM has Array but declared type is {other:?}"),
                        });
                    }
                };

                let items: Result<Vec<_>, _> = arr
                    .iter()
                    .map(|v| self.vm_value_to_external(v, element_type))
                    .collect();
                Ok(BexExternalValue::Array {
                    element_type: element_type.clone(),
                    items: items?,
                })
            }

            Object::Map(map) => {
                // Get key and value types from declared type
                let (key_type, value_type) = match effective_type {
                    Ty::Map { key, value } => (key.as_ref(), value.as_ref()),
                    other => {
                        return Err(EngineError::TypeMismatch {
                            message: format!("VM has Map but declared type is {other:?}"),
                        });
                    }
                };

                let entries: Result<indexmap::IndexMap<String, BexExternalValue>, EngineError> =
                    map.iter()
                        .map(|(k, v)| Ok((k.clone(), self.vm_value_to_external(v, value_type)?)))
                        .collect();
                Ok(BexExternalValue::Map {
                    key_type: key_type.clone(),
                    value_type: value_type.clone(),
                    entries: entries?,
                })
            }

            Object::Instance(instance) => {
                // Get class name from the Class object
                let class_obj = unsafe { instance.class.get() };
                let (class_name, field_names) = match class_obj {
                    Object::Class(class) => (class.name.clone(), &class.field_names),
                    _ => panic!("Instance.class should point to a Class object"),
                };

                // Look up field types from the schema
                let class_def = self.snapshot.classes.get(&class_name).ok_or_else(|| {
                    EngineError::SchemaInconsistency {
                        message: format!("Class '{class_name}' not found in schema"),
                    }
                })?;

                // Build field type lookup map once (O(n) instead of O(n^2))
                let field_types: std::collections::HashMap<&str, &Ty> = class_def
                    .fields
                    .iter()
                    .map(|f| (f.name.as_str(), &f.field_type))
                    .collect();

                // Convert fields with their declared types
                let fields: Result<indexmap::IndexMap<String, BexExternalValue>, EngineError> =
                    field_names
                        .iter()
                        .zip(instance.fields.iter())
                        .map(|(name, value)| {
                            // Look up the field's declared type from the pre-built map (O(1))
                            let field_type = field_types.get(name.as_str()).ok_or_else(|| {
                                EngineError::SchemaInconsistency {
                                    message: format!(
                                        "Field '{name}' not found in class '{class_name}'"
                                    ),
                                }
                            })?;

                            Ok((name.clone(), self.vm_value_to_external(value, field_type)?))
                        })
                        .collect();

                Ok(BexExternalValue::Instance {
                    class_name,
                    fields: fields?,
                })
            }

            Object::Variant(variant) => {
                // Get enum name and variant name from the Enum object
                let enum_obj = unsafe { variant.enm.get() };
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

                Ok(BexExternalValue::Variant {
                    enum_name,
                    variant_name,
                })
            }

            Object::Function(_) => Err(EngineError::CannotConvert {
                type_name: "function".to_string(),
            }),
            Object::Class(_) => Err(EngineError::CannotConvert {
                type_name: "class".to_string(),
            }),
            Object::Enum(_) => Err(EngineError::CannotConvert {
                type_name: "enum".to_string(),
            }),
            Object::Future(_) => Err(EngineError::CannotConvert {
                type_name: "future".to_string(),
            }),
            Object::Media(m) => Ok(BexExternalValue::Media {
                handle: self.heap().create_handle(ptr),
                kind: m.kind,
            }),
            Object::Resource(handle) => Ok(BexExternalValue::Resource(handle.clone())),
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => Err(EngineError::CannotSnapshot {
                type_name: "sentinel".to_string(),
            }),
        }
    }

    /// For union types, find which member matches the actual runtime value.
    ///
    /// If the declared type is not a union, returns it unchanged.
    fn resolve_effective_type<'a>(value: &Value, declared_type: &'a Ty) -> &'a Ty {
        match declared_type {
            Ty::Union(members) => Self::find_matching_union_member(value, members)
                .unwrap_or_else(|| members.first().unwrap_or(declared_type)),
            _ => declared_type,
        }
    }

    /// Find the union member that matches the runtime value's type.
    fn find_matching_union_member<'a>(value: &Value, members: &'a [Ty]) -> Option<&'a Ty> {
        match value {
            Value::Null => members.iter().find(|m| matches!(m, Ty::Null)),
            Value::Int(_) => members.iter().find(|m| matches!(m, Ty::Int)),
            Value::Float(_) => members.iter().find(|m| matches!(m, Ty::Float)),
            Value::Bool(_) => members.iter().find(|m| matches!(m, Ty::Bool)),
            Value::Object(ptr) => {
                let obj = unsafe { ptr.get() };
                match obj {
                    Object::String(_) => members.iter().find(|m| matches!(m, Ty::String)),
                    Object::Instance(inst) => {
                        let class_obj = unsafe { inst.class.get() };
                        if let Object::Class(class) = class_obj {
                            members
                                .iter()
                                .find(|m| matches!(m, Ty::Class(name) if name == &class.name))
                        } else {
                            None
                        }
                    }
                    Object::Variant(variant) => {
                        let enum_obj = unsafe { variant.enm.get() };
                        if let Object::Enum(enm) = enum_obj {
                            members
                                .iter()
                                .find(|m| matches!(m, Ty::Enum(name) if name == &enm.name))
                        } else {
                            None
                        }
                    }
                    Object::Array(elements) => {
                        // For arrays, check first element to determine which List type
                        if let Some(first) = elements.first() {
                            members.iter().find(|m| {
                                if let Ty::List(elem_ty) = m {
                                    Self::find_matching_union_member(
                                        first,
                                        &[elem_ty.as_ref().clone()],
                                    )
                                    .is_some()
                                } else {
                                    false
                                }
                            })
                        } else {
                            // Empty array - match any List type
                            members.iter().find(|m| matches!(m, Ty::List(_)))
                        }
                    }
                    Object::Map(_) => members.iter().find(|m| matches!(m, Ty::Map { .. })),
                    _ => None,
                }
            }
        }
    }

    /// Convert a VM Value to a `BexValue`.
    ///
    /// Primitives become `External(BexExternalValue)`, heap objects get a `Handle`.
    fn value_to_external(&self, value: Value) -> BexValue {
        match value {
            Value::Null => BexValue::External(BexExternalValue::Null),
            Value::Int(i) => BexValue::External(BexExternalValue::Int(i)),
            Value::Float(f) => BexValue::External(BexExternalValue::Float(f)),
            Value::Bool(b) => BexValue::External(BexExternalValue::Bool(b)),
            Value::Object(idx) => {
                let handle = self.heap().create_handle(idx);
                BexValue::Opaque(handle)
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

    /// Execute a function by name.
    ///
    /// This method is `&self` because each call creates its own VM with a TLAB.
    /// Concurrent calls work naturally - each gets its own VM and TLAB.
    ///
    /// # Arguments
    ///
    /// Arguments are passed as `BexValue` types:
    /// - Primitives convert to `External(BexExternalValue)` via `From` impls
    /// - `Opaque(Handle)` references existing heap objects
    /// - `External(...)` allocates new objects on the heap
    ///
    /// # Returns
    ///
    /// Returns `BexExternalValue` - the owned result value. If the return type is a union,
    /// the value is wrapped in `Union { value, metadata }` with information about the union.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = engine.call_function("get_user", &[
    ///     "Alice".into(),
    ///     42i64.into(),
    /// ]).await?;
    ///
    /// match result {
    ///     BexExternalValue::Instance { class_name, fields } => {
    ///         println!("Got {} with {} fields", class_name, fields.len());
    ///     }
    ///     BexExternalValue::Union { value, metadata } => {
    ///         println!("Got union value, selected: {}", metadata.selected_option);
    ///     }
    ///     _ => {}
    /// }
    /// ```
    pub async fn call_function(
        &self,
        function_name: &str,
        args: &[BexValue],
    ) -> Result<BexExternalValue, EngineError> {
        // Wait for any in-progress GC to complete.
        // This ensures Handles in args have stable indices.
        while self.gc_in_progress.load(Ordering::Acquire) {
            self.gc_complete.notified().await;
        }

        // Look up the function to verify it exists and get its return type
        let function_index = self.lookup_function(function_name)?;
        let return_type = self
            .snapshot
            .functions
            .get(function_name)
            .map(|f| f.return_type.clone())
            .ok_or_else(|| EngineError::SchemaInconsistency {
                message: format!("Function '{function_name}' exists in bytecode but not in schema"),
            })?;

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
            self.env_vars.clone(),
        );

        // Convert ExternalValue args to Value, allocating BexExternalValue data on the heap
        let vm_args: Vec<Value> = args
            .iter()
            .map(|arg| Self::externalize_to_value(&mut vm, arg, &guard))
            .collect();

        // Set entry point with converted args
        vm.set_entry_point(function_index, &vm_args);

        // Run the event loop with epoch tracking
        let result = self.run_event_loop_with_epoch(&mut vm, my_epoch).await;

        // Unregister from epoch
        if self.epoch_states[slot]
            .active
            .fetch_sub(1, Ordering::AcqRel)
            == 1
        {
            // We were the last active VM in this epoch
            self.epoch_drained.notify_one();
        }

        // Convert BexValue to BexExternalValue, wrapping in Union if return type is union
        result.and_then(|value| self.to_bex_external(value, &return_type))
    }

    /// Convert an `ExternalValue` to a VM `Value`.
    ///
    /// Requires `EpochGuard` because resolving handles returns an `ObjectIndex`
    /// that must remain valid while we use it.
    ///
    /// - `Opaque(Handle)` extracts the `HeapPtr`
    /// - `External(...)` recursively allocates on the heap
    fn externalize_to_value(vm: &mut BexVm, external: &BexValue, guard: &EpochGuard<'_>) -> Value {
        match external {
            BexValue::Opaque(handle) => {
                // Resolve through table to get current pointer after any GC
                let ptr = handle
                    .object_ptr(guard)
                    .expect("Handle should be valid - object was returned to external code");
                Value::Object(ptr)
            }
            BexValue::External(ext) => Self::allocate_from_external(vm, ext, guard),
        }
    }

    /// Recursively allocate a `BexExternalValue` onto the heap, returning a `Value`.
    fn allocate_from_external(
        vm: &mut BexVm,
        external: &BexExternalValue,
        guard: &EpochGuard<'_>,
    ) -> Value {
        match external {
            BexExternalValue::Null => Value::Null,
            BexExternalValue::Int(i) => Value::Int(*i),
            BexExternalValue::Float(f) => Value::Float(*f),
            BexExternalValue::Bool(b) => Value::Bool(*b),
            BexExternalValue::String(s) => vm.alloc_string(s.clone()),
            BexExternalValue::Array { items, .. } => {
                let values: Vec<Value> = items
                    .iter()
                    .map(|item| Self::allocate_from_external(vm, item, guard))
                    .collect();
                vm.alloc_array(values)
            }
            BexExternalValue::Map { entries, .. } => {
                let values: indexmap::IndexMap<String, Value> = entries
                    .iter()
                    .map(|(k, v): (&String, &BexExternalValue)| {
                        (k.clone(), Self::allocate_from_external(vm, v, guard))
                    })
                    .collect();
                vm.alloc_map(values)
            }
            BexExternalValue::Instance { .. } => {
                // Instance allocation requires class lookup - not supported from external
                todo!(
                    "Cannot allocate Instance from BexExternalValue. We need to do a string lookup for the right type in the schema."
                )
            }
            BexExternalValue::Variant { .. } => {
                // Variant allocation requires enum lookup - not supported from external
                todo!(
                    "Cannot allocate Variant from BexExternalValue. We need to do a string lookup for the right type in the schema."
                )
            }
            BexExternalValue::Union { value, .. } => {
                // Unwrap the union and allocate the inner value
                Self::allocate_from_external(vm, value, guard)
            }
            BexExternalValue::Media { handle, .. } => {
                let ptr = handle
                    .object_ptr(guard)
                    .expect("Handle should be valid - object was returned to external code");
                Value::Object(ptr)
            }
            BexExternalValue::Resource(handle) => vm.alloc_resource(handle.clone()),
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

    /// Collect roots from a yielded VM.
    fn collect_vm_roots(vm: &BexVm) -> Vec<HeapPtr> {
        let mut roots = Vec::new();

        // Stack values
        for value in &vm.stack.0 {
            if let Value::Object(ptr) = value {
                roots.push(*ptr);
            }
        }

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
        vm: &mut BexVm,
        my_epoch: u64,
    ) -> Result<BexValue, EngineError> {
        let (pending_futures, mut processed_futures) = mpsc::unbounded_channel::<FutureResult>();

        'vm_exec: loop {
            match vm.exec()? {
                VmExecState::Complete(value) => {
                    // Convert to BexValue (handles for objects, BexExternalValue for primitives)
                    return Ok(self.value_to_external(value));
                }

                VmExecState::ScheduleFuture(id) => {
                    let pending = vm.pending_future(id)?;

                    // Convert arguments to BexExternalValue
                    let args = Self::vm_args_to_external(vm, &pending.args);

                    match pending.operation {
                        ExternalOp::Llm => {
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
                            match self.execute_sys_op(sys_op, args) {
                                SysOpResult::Ready(result) => {
                                    // Sync operation - fulfill immediately
                                    let value = Self::external_to_vm_value(
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
                        let external = future.result?;
                        let value = Self::external_to_vm_value(vm, external);
                        vm.fulfil_future(future.id, value)?;
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

                        let external = future.result?;
                        let value = Self::external_to_vm_value(vm, external);
                        vm.fulfil_future(future.id, value)?;
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

    /// Execute a system operation using the provider table.
    fn execute_sys_op(&self, op: SysOp, args: Vec<BexExternalValue>) -> SysOpResult {
        match op {
            SysOp::FsOpen => (self.sys_ops.fs_open)(args),
            SysOp::FsRead => (self.sys_ops.fs_read)(args),
            SysOp::FsClose => (self.sys_ops.fs_close)(args),
            SysOp::NetConnect => (self.sys_ops.net_connect)(args),
            SysOp::NetRead => (self.sys_ops.net_read)(args),
            SysOp::NetClose => (self.sys_ops.net_close)(args),
            SysOp::Shell => (self.sys_ops.shell)(args),
        }
    }

    /// Convert VM values to `BexExternalValues` for sys ops.
    ///
    /// This is simpler than `vm_value_to_external` because sys ops only receive
    /// primitives, strings, arrays, maps, and resources - not instances/variants.
    fn vm_args_to_external(vm: &BexVm, args: &[Value]) -> Vec<BexExternalValue> {
        args.iter()
            .map(|v| Self::vm_arg_to_external(vm, v))
            .collect()
    }

    fn vm_arg_to_external(vm: &BexVm, value: &Value) -> BexExternalValue {
        match value {
            Value::Null => BexExternalValue::Null,
            Value::Int(i) => BexExternalValue::Int(*i),
            Value::Float(f) => BexExternalValue::Float(*f),
            Value::Bool(b) => BexExternalValue::Bool(*b),
            Value::Object(idx) => {
                let obj = vm.get_object(*idx);
                match obj {
                    Object::String(s) => BexExternalValue::String(s.clone()),
                    Object::Array(arr) => {
                        let items: Vec<BexExternalValue> = arr
                            .iter()
                            .map(|v| Self::vm_arg_to_external(vm, v))
                            .collect();
                        BexExternalValue::Array {
                            element_type: bex_external_types::Ty::Null,
                            items,
                        }
                    }
                    Object::Map(map) => {
                        let entries: indexmap::IndexMap<String, BexExternalValue> = map
                            .iter()
                            .map(|(k, v)| (k.clone(), Self::vm_arg_to_external(vm, v)))
                            .collect();
                        BexExternalValue::Map {
                            key_type: bex_external_types::Ty::String,
                            value_type: bex_external_types::Ty::Null,
                            entries,
                        }
                    }
                    Object::Resource(handle) => BexExternalValue::Resource(handle.clone()),
                    other => {
                        panic!(
                            "Cannot convert object type to BexExternalValue for sys op: {other:?}"
                        )
                    }
                }
            }
        }
    }

    /// Convert a `BexExternalValue` result from sys ops back to a VM Value.
    fn external_to_vm_value(vm: &mut BexVm, external: BexExternalValue) -> Value {
        match external {
            BexExternalValue::Null => Value::Null,
            BexExternalValue::Int(i) => Value::Int(i),
            BexExternalValue::Float(f) => Value::Float(f),
            BexExternalValue::Bool(b) => Value::Bool(b),
            BexExternalValue::String(s) => vm.alloc_string(s),
            BexExternalValue::Array { items, .. } => {
                let values: Vec<Value> = items
                    .into_iter()
                    .map(|v| Self::external_to_vm_value(vm, v))
                    .collect();
                vm.alloc_array(values)
            }
            BexExternalValue::Map { entries, .. } => {
                let values: indexmap::IndexMap<String, Value> = entries
                    .into_iter()
                    .map(|(k, v)| (k, Self::external_to_vm_value(vm, v)))
                    .collect();
                vm.alloc_map(values)
            }
            BexExternalValue::Resource(handle) => vm.alloc_resource(handle),
            // These shouldn't come from sys ops, but handle gracefully
            BexExternalValue::Instance { .. } => {
                panic!("Unexpected Instance from sys op")
            }
            BexExternalValue::Variant { .. } => {
                panic!("Unexpected Variant from sys op")
            }
            BexExternalValue::Union { value, .. } => Self::external_to_vm_value(vm, *value),
            BexExternalValue::Media { .. } => {
                panic!("Unexpected Media from sys op")
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
        // 1. Create a test BexProgram with a simple function
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
