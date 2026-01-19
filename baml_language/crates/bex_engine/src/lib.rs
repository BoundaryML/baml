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

use std::{collections::HashMap, sync::Arc};

use baml_snapshot::BamlSnapshot;
// Re-export bex_sys types for convenience
pub use bex_sys::{
    FileHandle, OpContext, OpError, ResolvedArgs, ResolvedValue, ResourceId, ResourceKind,
    ResourceRegistry, SocketHandle, SysOpResult, ops,
};
use bex_vm::{BexVm, BytecodeProgram, VmExecState};
use bex_vm_types::{ExternalOp, Object, ObjectIndex, SysOp, Value};
use thiserror::Error;
use tokio::sync::mpsc;

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
    /// The original snapshot (for metadata access)
    snapshot: BamlSnapshot,
    /// The converted bytecode program with native functions attached (for VM execution)
    bytecode: BytecodeProgram,
    /// Environment variables passed to VM.
    env_vars: HashMap<String, String>,
}

impl BexEngine {
    /// Create a new engine with the given program.
    pub fn new(
        snapshot: BamlSnapshot,
        env_vars: HashMap<String, String>,
    ) -> Result<Self, EngineError> {
        // Convert the pure bytecode to a VM-ready program with native functions attached
        let bytecode = bex_vm::convert_program(snapshot.bytecode.clone())?;
        Ok(Self {
            snapshot,
            bytecode,
            env_vars,
        })
    }

    /// Get a reference to the program snapshot.
    pub fn program(&self) -> &BamlSnapshot {
        &self.snapshot
    }

    /// Execute a function by name.
    ///
    /// This method is `&self` because:
    /// - VM is created as a local variable (cloned from self.bytecode)
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
        let mut vm = BexVm::new(self.bytecode.clone(), self.env_vars.clone());

        // Set entry point with args
        vm.set_entry_point(function_index, args);

        // Create a resource registry for this call
        let ctx = Arc::new(OpContext::new());

        // Run the event loop
        self.run_event_loop(&mut vm, ctx).await
    }

    /// Look up a function by name and return its bytecode index.
    fn lookup_function(&self, function_name: &str) -> Result<ObjectIndex, EngineError> {
        self.bytecode
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
            Value::Object(idx) => match &vm.objects[*idx] {
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
            },
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
