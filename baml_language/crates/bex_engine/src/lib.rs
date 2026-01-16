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
//! External operations (LLM calls, HTTP requests, file I/O) are handled via
//! the `ExternalOp` trait and `ExternalOpRegistry`. Each operation is identified
//! by a string name (e.g., "`MyLlmFunction`", "`baml.fetch_as`").

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use baml_snapshot::BamlSnapshot;
use bex_vm::{BexVm, VmExecState};
use bex_vm_types::{ObjectIndex, PendingFuture, Value};
use thiserror::Error;
use tokio::sync::mpsc;

// ============================================================================
// External Operation Types
// ============================================================================

/// A boxed future that can be sent across threads.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Errors that can occur during external operation execution.
#[derive(Debug, Error)]
pub enum OpError {
    #[error("{0}")]
    Other(String),
}

/// An external operation that runs asynchronously outside the VM.
///
/// External operations are things like LLM calls, HTTP requests, file I/O,
/// or shell commands. They are identified by a string name and receive
/// arguments from the VM.
pub trait ExternalOp: Send + Sync + 'static {
    /// The operation name (e.g., "`MyLlmFunction`", "`baml.fetch_as`").
    fn name(&self) -> &str;

    /// Execute the operation with the given arguments.
    ///
    /// Returns a `Value` on success or an `OpError` on failure.
    fn execute(&self, pending: &PendingFuture) -> BoxFuture<'static, Result<Value, OpError>>;
}

/// Registry of external operations.
///
/// The engine uses this to look up and execute external operations
/// when the VM yields with `VmExecState::ScheduleFuture`.
pub struct ExternalOpRegistry {
    ops: HashMap<String, Arc<dyn ExternalOp>>,
}

impl ExternalOpRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            ops: HashMap::new(),
        }
    }

    /// Register an external operation.
    pub fn register(&mut self, op: impl ExternalOp) {
        self.ops.insert(op.name().to_string(), Arc::new(op));
    }

    /// Look up an operation by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn ExternalOp>> {
        self.ops.get(name).cloned()
    }
}

impl Default for ExternalOpRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Engine Types
// ============================================================================

/// Result of an external future.
struct FutureResult {
    id: ObjectIndex,
    result: Result<Value, EngineError>,
}

/// Errors that can occur during engine execution.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Function not found: {name}")]
    FunctionNotFound { name: String },

    #[error("External operation not found: {operation}")]
    ExternalOpNotFound { operation: String },

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
/// It owns the compiled program and external operation registry.
pub struct BexEngine {
    /// The compiled program (contains bytecode, types, functions, etc.)
    program: BamlSnapshot,
    /// Registry of external operations.
    external_op_registry: ExternalOpRegistry,
    /// Environment variables passed to VM.
    env_vars: HashMap<String, String>,
}

impl BexEngine {
    /// Create a new engine with the given program.
    pub fn new(program: BamlSnapshot, env_vars: HashMap<String, String>) -> Self {
        Self {
            program,
            external_op_registry: ExternalOpRegistry::new(),
            env_vars,
        }
    }

    /// Get a reference to the program.
    pub fn program(&self) -> &BamlSnapshot {
        &self.program
    }

    /// Get a mutable reference to the external operation registry.
    ///
    /// Use this to register external operations before calling functions.
    pub fn external_op_registry_mut(&mut self) -> &mut ExternalOpRegistry {
        &mut self.external_op_registry
    }

    /// Execute a function by name.
    ///
    /// This method is `&self` because:
    /// - VM is created as a local variable (cloned from self.program.bytecode)
    /// - Each call gets its own VM instance (like legacy)
    ///
    /// Concurrent calls work naturally - each gets its own VM.
    ///
    /// Args and return value are VM `Value` types directly.
    pub async fn call_function(
        &self,
        function_name: &str,
        args: &[Value],
    ) -> Result<Value, EngineError> {
        // Look up the function to verify it exists
        let function_index = self.lookup_function(function_name)?;

        // Create VM by cloning bytecode (like legacy async_vm_runtime.rs)
        let mut vm = BexVm::new(self.program.bytecode.clone(), self.env_vars.clone());

        // Set entry point with args
        vm.set_entry_point(function_index, args);

        // Run the event loop
        self.run_event_loop(&mut vm).await
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
    async fn run_event_loop(&self, vm: &mut BexVm) -> Result<Value, EngineError> {
        let (pending_futures, mut processed_futures) = mpsc::unbounded_channel::<FutureResult>();

        'vm_exec: loop {
            match vm.exec()? {
                VmExecState::Complete(value) => {
                    return Ok(value);
                }

                VmExecState::ScheduleFuture(id) => {
                    let pending = vm.pending_future(id)?;

                    // Look up the external operation
                    let Some(op) = self.external_op_registry.get(&pending.operation) else {
                        let _ = pending_futures.send(FutureResult {
                            id,
                            result: Err(EngineError::ExternalOpNotFound {
                                operation: pending.operation.clone(),
                            }),
                        });
                        continue;
                    };

                    // Spawn the operation
                    let pending_futures = pending_futures.clone();
                    let pending = pending.clone();

                    tokio::spawn(async move {
                        let result = op.execute(&pending).await;
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
                        vm.fulfil_future(future.id, future.result?)?;
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
                        vm.fulfil_future(future.id, future.result?)?;
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
}
