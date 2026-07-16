//! bridge_cffi - C FFI bindings for BAML using bex_engine.
//!
//! This crate provides the same FFI interface as `engine/language_client_cffi/`
//! but powered by `bex_engine` instead of `baml-runtime`.
//!
//! `lib.rs` defines the platform-neutral bridge surface. Native implementation
//! details live in `lib_native.rs`, while Wasm implementation details live
//! in `lib_wasm.rs`.

#[cfg(not(target_arch = "wasm32"))]
#[path = "lib_native.rs"]
mod platform;
#[cfg(target_arch = "wasm32")]
#[path = "lib_wasm.rs"]
mod platform;

use std::sync::Arc;

use bex_project::Bex;

pub mod baml_to_host;
pub mod buffer;
pub mod error;

pub use baml_to_host::{call_and_encode, error_to_outbound, result_to_outbound};
pub use bridge_ctypes::baml_bridge;
pub use buffer::{Buffer, free_buffer};
pub use error::BridgeError;
pub use platform::*;

/// Get a clone of the target's global runtime, or error if not initialized.
pub fn get_runtime() -> Result<Arc<dyn Bex>, BridgeError> {
    platform::get_runtime()
}

/// Initialize the global runtime from serialized BAML bytecode.
///
/// The payload is the same borsh-encoded `bex_vm_types::Program` that
/// `baml pack` embeds in its pack envelope. Decoding and engine construction
/// live behind `bex_project::new_from_bytecode` so the bridge stays on the
/// `bex_project` surface rather than reaching into bex internals.
pub fn initialize_runtime_from_bytecode_with_sys_ops(
    bytecode: &[u8],
    sys_ops: sys_ops::SysOps,
) -> Result<Arc<dyn Bex>, BridgeError> {
    let runtime: Arc<dyn Bex> = bex_project::new_from_bytecode(bytecode, sys_ops)?;
    platform::replace_runtime(runtime.clone())?;
    Ok(runtime)
}

/// Initialize the native process-global runtime from serialized BAML bytecode.
#[cfg(not(target_arch = "wasm32"))]
pub fn initialize_runtime_from_bytecode(bytecode: &[u8]) -> Result<Arc<dyn Bex>, BridgeError> {
    use sys_native::SysOpsExt as _;

    initialize_runtime_from_bytecode_with_sys_ops(bytecode, sys_ops::SysOps::native())
}

/// Allocate a new process-unique function-call ID.
pub fn new_function_call_id() -> u64 {
    bex_project::CallId::next().0
}

/// Build a function-call context builder for a CFFI-owned call id.
pub fn function_call_context_builder(
    call_id: bex_project::CallId,
) -> bex_project::FunctionCallContextBuilder {
    bex_project::FunctionCallContextBuilder::new(call_id)
}

/// Cancel an in-flight function call by ID.
///
/// Returns true on success, false if the runtime is not initialized.
pub fn cancel_function_call_by_id(id: u64) -> bool {
    if id == 0 {
        return false;
    }
    get_runtime()
        .and_then(|runtime| {
            runtime
                .cancel_function_call(bex_project::CallId(id))
                .map_err(BridgeError::from)
        })
        .is_ok()
}

/// Allocate a new process-unique function-call ID.
#[unsafe(no_mangle)]
pub extern "C" fn new_function_call() -> u64 {
    new_function_call_id()
}

/// Cancel an in-flight function call.
///
/// Returns 0 on success, 1 if the call ID is unknown or already completed.
#[unsafe(no_mangle)]
pub extern "C" fn cancel_function_call(id: u64) -> i32 {
    if cancel_function_call_by_id(id) { 0 } else { 1 }
}
