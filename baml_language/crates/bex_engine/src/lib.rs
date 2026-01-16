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

mod resource;

use std::{collections::HashMap, sync::Arc};

use baml_snapshot::BamlSnapshot;
use bex_vm::{BexVm, VmExecState};
use bex_vm_types::{ExternalOp, ObjectIndex, SysOp, Value};
use parking_lot::RwLock;
use thiserror::Error;
use tokio::sync::mpsc;

pub mod ops;

pub use resource::{Resource, ResourceId, ResourceRegistry};

// ============================================================================
// Resolved Values
// ============================================================================

/// A resolved value that external operations can work with directly.
///
/// Unlike `Value` which may contain object indices, `ResolvedValue` contains
/// the actual data that external operations need.
#[derive(Debug, Clone)]
pub enum ResolvedValue {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Array(Vec<ResolvedValue>),
    Map(indexmap::IndexMap<String, ResolvedValue>),
    /// An object index that couldn't be resolved (fallback).
    Object(ObjectIndex),
    /// A resource ID (for file handles, connections, etc.)
    ResourceId(ResourceId),
}

// ============================================================================
// Operation Context and Errors
// ============================================================================

/// Errors that can occur during external operation execution.
#[derive(Debug, Error)]
pub enum OpError {
    #[error("{0}")]
    Other(String),

    #[error("Resource not found: {0}")]
    ResourceNotFound(ResourceId),

    #[error("Resource type mismatch")]
    ResourceTypeMismatch,
}

/// Context passed to external operations.
///
/// Provides access to resources and other engine state.
pub struct OpContext {
    /// Registry for storing/retrieving resources.
    pub resources: Arc<RwLock<ResourceRegistry>>,
}

impl OpContext {
    /// Add a resource and return its ID.
    pub fn add_resource<T: Resource>(&self, resource: T) -> ResourceId {
        self.resources.write().add(resource)
    }

    /// Get a resource by ID and apply a function to it.
    ///
    /// This holds a read lock for the duration of the closure.
    pub fn with_resource<T: 'static, R, F: FnOnce(&T) -> R>(
        &self,
        id: ResourceId,
        f: F,
    ) -> Option<R> {
        let guard = self.resources.read();
        guard.get::<T>(id).map(f)
    }

    /// Check if a resource exists.
    pub fn has_resource(&self, id: ResourceId) -> bool {
        self.resources.read().contains(id)
    }

    /// Remove a resource by ID.
    pub fn remove_resource(&self, id: ResourceId) -> Option<Arc<dyn Resource>> {
        self.resources.write().remove(id)
    }
}

/// Resolved arguments for an external operation.
#[derive(Debug, Clone)]
pub struct ResolvedArgs {
    /// Resolved arguments.
    pub args: Vec<ResolvedValue>,
}

// ============================================================================
// Engine Types
// ============================================================================

/// Result of an external future.
struct FutureResult {
    id: ObjectIndex,
    result: Result<ResolvedValue, EngineError>,
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
}

// ============================================================================
// BexEngine
// ============================================================================

/// The async runtime that drives VM execution.
///
/// `BexEngine` is the main entry point for executing BAML programs.
/// It owns the compiled program.
pub struct BexEngine {
    /// The compiled program (contains bytecode, types, functions, etc.)
    program: BamlSnapshot,
    /// Environment variables passed to VM.
    env_vars: HashMap<String, String>,
}

impl BexEngine {
    /// Create a new engine with the given program.
    pub fn new(program: BamlSnapshot, env_vars: HashMap<String, String>) -> Self {
        Self { program, env_vars }
    }

    /// Get a reference to the program.
    pub fn program(&self) -> &BamlSnapshot {
        &self.program
    }

    /// Execute a function by name.
    ///
    /// This method is `&self` because:
    /// - VM is created as a local variable (cloned from self.program.bytecode)
    /// - Each call gets its own VM instance (like legacy)
    ///
    /// Concurrent calls work naturally - each gets its own VM.
    ///
    /// Args are VM `Value` types. Return value is `ResolvedValue` which contains
    /// the actual data (strings, arrays, etc.) since the VM is dropped after execution.
    pub async fn call_function(
        &self,
        function_name: &str,
        args: &[Value],
    ) -> Result<ResolvedValue, EngineError> {
        // Look up the function to verify it exists
        let function_index = self.lookup_function(function_name)?;

        // Create VM by cloning bytecode (like legacy async_vm_runtime.rs)
        let mut vm = BexVm::new(self.program.bytecode.clone(), self.env_vars.clone());

        // Set entry point with args
        vm.set_entry_point(function_index, args);

        // Create a resource registry for this call
        let ctx = Arc::new(OpContext {
            resources: Arc::new(RwLock::new(ResourceRegistry::new())),
        });

        // Run the event loop
        self.run_event_loop(&mut vm, ctx).await
    }

    /// Look up a function by name and return its bytecode index.
    fn lookup_function(&self, function_name: &str) -> Result<ObjectIndex, EngineError> {
        self.program
            .bytecode
            .resolved_function_names
            .get(function_name)
            .map(|(idx, _kind)| *idx)
            .ok_or_else(|| EngineError::FunctionNotFound {
                name: function_name.to_string(),
            })
    }

    /// Run the VM event loop until completion.
    async fn run_event_loop(
        &self,
        vm: &mut BexVm,
        ctx: Arc<OpContext>,
    ) -> Result<ResolvedValue, EngineError> {
        let (pending_futures, mut processed_futures) = mpsc::unbounded_channel::<FutureResult>();

        'vm_exec: loop {
            match vm.exec()? {
                VmExecState::Complete(value) => {
                    // Resolve the value before returning (VM will be dropped after this)
                    return Ok(Self::resolve_value(vm, &value));
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

                    // Clone what we need for the spawned task
                    let pending_futures = pending_futures.clone();
                    let ctx = Arc::clone(&ctx);
                    let operation = pending.operation;

                    // Spawn the operation with static dispatch
                    tokio::spawn(async move {
                        let result = Self::execute_external_op(operation, ctx, resolved_args).await;
                        let _ = pending_futures.send(FutureResult {
                            id,
                            result: result.map_err(EngineError::from),
                        });
                    });
                }

                VmExecState::Await(future_id) => {
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

    /// Execute an external operation via static dispatch.
    async fn execute_external_op(
        op: ExternalOp,
        ctx: Arc<OpContext>,
        args: ResolvedArgs,
    ) -> Result<ResolvedValue, OpError> {
        match op {
            ExternalOp::Llm => {
                // TODO: Implement LLM operations
                Err(OpError::Other("LLM operations not yet implemented".into()))
            }
            ExternalOp::Sys(sys_op) => Self::execute_sys_op(sys_op, ctx, args).await,
        }
    }

    /// Execute a system operation.
    async fn execute_sys_op(
        op: SysOp,
        ctx: Arc<OpContext>,
        args: ResolvedArgs,
    ) -> Result<ResolvedValue, OpError> {
        match op {
            SysOp::FsOpen => ops::fs::open(ctx, args).await,
            SysOp::FsRead => ops::fs::read(ctx, args).await,
            SysOp::Shell => ops::sys::shell(ctx, args).await,
            SysOp::NetConnect => ops::net::connect(ctx, args).await,
            SysOp::NetRead => ops::net::read(ctx, args).await,
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
                use bex_vm_types::Object;
                match &vm.objects[*idx] {
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
                    // For other objects, keep the index as a fallback
                    _ => ResolvedValue::Object(*idx),
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
            ResolvedValue::Object(idx) => Value::Object(idx),
            ResolvedValue::ResourceId(id) => {
                // Store resource ID as an integer value
                Value::Int(id.cast_signed())
            }
        }
    }
}
