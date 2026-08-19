//! Native runtime and C ABI implementation for `bridge_cffi`.

use std::{
    collections::HashMap,
    panic::AssertUnwindSafe,
    sync::{Arc, RwLock},
};

use bex_project::Bex;
use bridge_ctypes::{DecodeFromBuffer, HANDLE_TABLE, kwargs_to_bex_values};
use futures::future::FutureExt;
use once_cell::sync::OnceCell;
use sys_native::SysOpsExt;
use tokio::runtime::Runtime;

use crate::{
    BridgeError, baml_to_host, error_to_outbound, function_call_context_builder,
    register_active_call_route,
};

#[path = "api.rs"]
pub mod api;
#[path = "collector.rs"]
pub mod collector;
#[path = "ffi/mod.rs"]
mod ffi;
#[path = "host_spans.rs"]
pub mod host_spans;
#[path = "panic.rs"]
mod panic;

pub use api::{
    BAML_API_V1_ABI_VERSION, BamlApiV1, BamlCffiHandleType, BamlCffiMediaKind,
    BamlHostDispatchCallback, BamlHostReleaseCallback, BamlResultCallback, baml_get_api_v1,
};
use ffi::callbacks::send_outbound_result_to_callback;
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
    objects::flush_events,
    runtime::{
        BamlBridgeInfoV1, create_baml_runtime, destroy_baml_runtime,
        initialize_runtime_from_bytecode as initialize_runtime_from_bytecode_ffi,
        initialize_runtime_from_bytecode_with_metadata, invoke_runtime_cli, register_bridge_ffi,
        shutdown_runtime as shutdown_runtime_ffi, version,
    },
    unhandled_spawn::register_unhandled_spawn_error_callback,
};

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

pub(crate) fn get_runtime() -> Result<Arc<dyn Bex>, BridgeError> {
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

    let rt: Arc<dyn Bex> = bex_project::new(vfs_path, bex_project::SysOps::native(), files)?;
    crate::install_unhandled_spawn_error_handler(&rt);
    replace_runtime(rt.clone())?;
    Ok(rt)
}

pub(crate) fn replace_runtime(rt: Arc<dyn Bex>) -> Result<(), BridgeError> {
    let mut guard = RUNTIME_INSTANCE
        .write()
        .map_err(|_| BridgeError::LockPoisoned)?;
    let previous = guard.replace(rt);
    drop(guard);
    if let Some(previous) = previous {
        get_tokio_runtime()?.spawn(previous.shutdown());
    }
    Ok(())
}

pub(crate) fn take_runtime() -> Result<Option<Arc<dyn Bex>>, BridgeError> {
    RUNTIME_INSTANCE
        .write()
        .map_err(|_| BridgeError::LockPoisoned)
        .map(|mut runtime| runtime.take())
}

pub(crate) fn dispatch_unhandled_spawn_error(content: Vec<u8>, cancelled: bool) {
    ffi::unhandled_spawn::dispatch(content, cancelled);
}

/// Call a BAML function asynchronously.
///
/// Returns immediately after spawning the async task. Result/error is delivered
/// via the registered callback as a `BamlOutboundResult` envelope.
#[unsafe(no_mangle)]
pub extern "C" fn call_function(encoded_args: *const u8, length: usize, id: u32) {
    if let Err(e) = call_function_inner(encoded_args, length, id) {
        send_outbound_result_to_callback(id, &error_to_outbound(e));
    }
}

fn call_function_inner(encoded_args: *const u8, length: usize, id: u32) -> Result<(), BridgeError> {
    use bridge_ctypes::baml_bridge::cffi::{CallFunctionArgs, call_function_args::CallTarget};

    let runtime = get_runtime()?;

    let args = if encoded_args.is_null() || length == 0 {
        CallFunctionArgs::default()
    } else {
        unsafe { CallFunctionArgs::from_c_buffer(encoded_args, length) }?
    };
    let call_id = decoded_call_id(args.call_id)?;
    let target = args.call_target.ok_or(BridgeError::MissingCallTarget)?;
    if matches!(target, CallTarget::FunctionHandle(_)) && !args.type_args.is_empty() {
        return Err(BridgeError::FunctionHandleTypeArgs);
    }
    let type_args = bridge_ctypes::proto_ty_args_to_named(&args.type_args)?;
    let kwargs = kwargs_to_bex_values(args.kwargs, &HANDLE_TABLE)?;
    let cancel = bex_project::CancellationToken::new();
    let call_ctx = function_call_context_builder(call_id)
        .with_cancel_token(cancel.clone())
        .with_type_args(type_args.type_args)
        .with_type_defs(type_args.type_defs)
        .build();
    // Install the cancellation route before returning control to the caller.
    // The guard stays alive through synchronous callback delivery, closing
    // both the pre-start and completed-before-delivery races.
    let route = register_active_call_route(call_id.0, cancel);

    get_tokio_runtime()?.spawn(async move {
        let _route = route;
        let encoded = AssertUnwindSafe(async move {
            match target {
                CallTarget::FunctionName(function_name) => {
                    baml_to_host::call_and_encode_registered(
                        runtime,
                        function_name,
                        kwargs.into(),
                        call_ctx,
                    )
                    .await
                }
                CallTarget::FunctionHandle(handle_key) => {
                    baml_to_host::call_handle_and_encode_registered(
                        runtime,
                        handle_key,
                        kwargs.into(),
                        call_ctx,
                    )
                    .await
                }
            }
        })
        .catch_unwind()
        .await;

        let bytes = match encoded {
            Ok(bytes) => bytes,
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
