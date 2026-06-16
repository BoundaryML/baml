//! bridge_cffi - C FFI bindings for BAML using bex_engine.
//!
//! This crate provides the same FFI interface as `engine/language_client_cffi/`
//! but powered by `bex_engine` instead of `baml-runtime`.
//!
//! `lib.rs` is the crate root: it owns global runtime management and the
//! primary `call_function` / `cancel_function_call` entry points. The
//! result-encoding logic lives in [`baml_to_host`]; the remaining C-ABI shims
//! (handles, host values, objects, runtime lifecycle, callbacks) live under
//! `ffi`.

use std::{
    collections::HashMap,
    ffi::CStr,
    panic::AssertUnwindSafe,
    sync::{Arc, RwLock},
};

use bex_project::Bex;
use bridge_ctypes::{DecodeFromBuffer, HANDLE_TABLE, kwargs_to_bex_values};
use futures::future::FutureExt;
use once_cell::sync::OnceCell;
use sys_native::SysOpsExt;
use tokio::runtime::Runtime;

pub mod baml_to_host;
pub mod buffer;
pub mod collector;
pub mod error;
mod ffi;
pub mod host_spans;
mod panic;

pub use baml_to_host::{call_and_encode, error_to_outbound, result_to_outbound};
pub use bridge_ctypes::baml_core;
pub use buffer::Buffer;
pub use error::BridgeError;
pub use ffi::{
    callbacks::{CallbackFn, register_callback},
    handle::{
        __testonly_seed_function_ref, __testonly_seed_generic_media, BamlCffiStatus,
        baml_handle_clone, baml_handle_release, baml_media_base64, baml_media_file,
        baml_media_from_base64, baml_media_from_file, baml_media_from_url, baml_media_mime_type,
        baml_media_url,
    },
    host_value::{
        HostDispatchFn, complete_host_call, register_host_dispatch_callback,
        register_host_release_callback,
    },
    objects::{flush_events, free_buffer},
    runtime::{create_baml_runtime, destroy_baml_runtime, invoke_runtime_cli, version},
};

use crate::ffi::callbacks::send_outbound_result_to_callback;

// ============================================================================
// Global Bex runtime management
// ============================================================================

/// Global Bex runtime. Uses RwLock to allow replacing the runtime.
static RUNTIME_INSTANCE: RwLock<Option<Arc<dyn Bex>>> = RwLock::new(None);

/// Global Tokio runtime for async execution.
static TOKIO_RUNTIME: OnceCell<Arc<Runtime>> = OnceCell::new();

/// Initialize the global Tokio runtime.
pub fn get_tokio_runtime() -> Result<Arc<Runtime>, BridgeError> {
    let result = TOKIO_RUNTIME.get_or_try_init(|| {
        Runtime::new()
            .map_err(|e| BridgeError::Internal(format!("Failed to create Tokio runtime: {e}")))
            .map(Arc::new)
    });
    result.cloned()
}

/// Get a clone of the global runtime, or error if not initialized.
pub fn get_runtime() -> Result<Arc<dyn Bex>, BridgeError> {
    RUNTIME_INSTANCE
        .read()
        .map_err(|_| BridgeError::LockPoisoned)?
        .clone()
        .ok_or(BridgeError::NotInitialized)
}

/// Initialize the global runtime from BAML source files.
///
/// If a runtime is already initialized, it will be replaced with the new one.
///
/// # Arguments
/// * `root_path` - Root path for BAML files
/// * `src_files` - Map of filename to content
pub fn initialize_runtime(
    root_path: &str,
    src_files: HashMap<String, String>,
) -> Result<Arc<dyn Bex>, BridgeError> {
    let physical_fs = vfs::PhysicalFS::new("/");
    let vfs_root = vfs::VfsPath::new(physical_fs);
    let vfs_path = vfs_root
        .join(root_path)
        .map_err(|e| bex_project::RuntimeError::Other(e.to_string()))?;

    let files = src_files
        .into_iter()
        .map(|(k, v)| (bex_project::FsPath::from_str(k), v))
        .collect();

    let rt = bex_project::new(vfs_path, bex_project::SysOps::native(), files)?;

    replace_runtime(rt.clone())?;

    Ok(rt)
}

/// Initialize the global runtime from serialized BAML bytecode.
///
/// The payload is the same borsh-encoded `bex_vm_types::Program` that
/// `baml pack` embeds in its pack envelope. Decoding and engine construction
/// live behind `bex_project::new_from_bytecode` so the bridge stays on the
/// `bex_project` surface rather than reaching into bex internals.
pub fn initialize_runtime_from_bytecode(bytecode: &[u8]) -> Result<Arc<dyn Bex>, BridgeError> {
    let rt: Arc<dyn Bex> = bex_project::new_from_bytecode(bytecode, bex_project::SysOps::native())?;

    replace_runtime(rt.clone())?;

    Ok(rt)
}

fn replace_runtime(rt: Arc<dyn Bex>) -> Result<(), BridgeError> {
    let mut guard = RUNTIME_INSTANCE
        .write()
        .map_err(|_| BridgeError::LockPoisoned)?;
    *guard = Some(rt);
    Ok(())
}

// ============================================================================
// Function call entry points
// ============================================================================

/// Call a BAML function asynchronously.
///
/// Returns immediately after spawning the async task.
/// Result/error is delivered via the registered callback as a
/// `BamlOutboundResult` envelope — including pre-call host-boundary failures,
/// which are synthesized into the envelope via [`error_to_outbound`].
#[unsafe(no_mangle)]
pub extern "C" fn call_function(
    function_name: *const libc::c_char,
    encoded_args: *const u8,
    length: usize,
    id: u32,
) {
    if let Err(e) = call_function_inner(function_name, encoded_args, length, id) {
        // Pre-call failure: synthesize the structured envelope and deliver it
        // through the one result channel — no separate error path.
        send_outbound_result_to_callback(id, &error_to_outbound(e));
    }
}

fn call_function_inner(
    function_name: *const libc::c_char,
    encoded_args: *const u8,
    length: usize,
    id: u32,
) -> Result<(), BridgeError> {
    use bridge_ctypes::baml_core::cffi::CallFunctionArgs;

    let runtime = get_runtime()?;

    if function_name.is_null() {
        return Err(BridgeError::NullFunctionName);
    }
    let func_name = unsafe { CStr::from_ptr(function_name) }
        .to_str()
        .map_err(BridgeError::from)?
        .to_owned();

    let args = if encoded_args.is_null() || length == 0 {
        CallFunctionArgs::default()
    } else {
        unsafe { CallFunctionArgs::from_c_buffer(encoded_args, length) }?
    };
    let call_id = decoded_call_id(args.call_id)?;
    let named_type_args = bridge_ctypes::proto_ty_args_to_named(&args.type_args)?;
    let kwargs = kwargs_to_bex_values(args.kwargs, &HANDLE_TABLE)?;

    let call_ctx = function_call_context_builder(call_id).with_named_type_args(named_type_args);

    get_tokio_runtime()?.spawn(async move {
        // `call_and_encode` already wraps the engine call in its own
        // `catch_unwind` and turns a call-time panic into a `SdkPanic`
        // envelope. The outer `catch_unwind` here is the C-ABI safety net for
        // a panic during *encoding* — which must not cross the C boundary.
        let encoded = AssertUnwindSafe(call_and_encode(
            runtime,
            func_name,
            kwargs.into(),
            call_ctx.build(),
        ))
        .catch_unwind()
        .await;

        let bytes = match encoded {
            Ok(bytes) => bytes,
            // An encode-stage panic also rides the envelope as an `SdkPanic`,
            // uniform with every other result.
            Err(panic_info) => baml_to_host::panic_to_outbound(panic_info.as_ref()),
        };
        send_outbound_result_to_callback(id, &bytes);
    });

    Ok(())
}

fn decoded_call_id(id: u64) -> Result<sys_types::CallId, BridgeError> {
    if id == 0 {
        return Err(BridgeError::InvalidCallId);
    }
    Ok(sys_types::CallId(id))
}

/// Allocate a new process-unique function-call ID.
pub fn new_function_call_id() -> u64 {
    sys_types::CallId::next().0
}

/// Build a function-call context builder for a CFFI-owned call id.
pub fn function_call_context_builder(
    call_id: sys_types::CallId,
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
                .cancel_function_call(sys_types::CallId(id))
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
