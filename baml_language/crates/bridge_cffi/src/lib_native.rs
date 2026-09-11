//! Native runtime and C ABI implementation for `bridge_cffi`.

use std::{collections::HashMap, panic::AssertUnwindSafe, sync::Arc};

use bex_project::Bex;
use bridge_ctypes::{DecodeFromBuffer, HANDLE_TABLE, kwargs_to_bex_values};
use futures::future::FutureExt;
use once_cell::sync::OnceCell;
use sys_native::SysOpsExt;
use tokio::runtime::Runtime;

use crate::{
    BridgeError, baml_to_host, call_and_encode, call_handle_and_encode, error_to_outbound,
    function_call_context_builder,
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
        BamlBridgeInfoV1, create_baml_runtime, create_runtime_ffi, destroy_baml_runtime,
        initialize_runtime_from_bytecode as initialize_runtime_from_bytecode_ffi,
        initialize_runtime_from_bytecode_with_metadata, invoke_runtime_cli, program_key_ffi,
        register_bridge_ffi, register_program_ffi, shutdown_runtime as shutdown_runtime_ffi,
        unregister_runtime_ffi, version,
    },
    unhandled_spawn::register_unhandled_spawn_error_callback,
};

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

/// Create an independent dynamic runtime from BAML source files.
///
/// # Arguments
/// * `root_path` - Root path for BAML files
/// * `src_files` - Map of filename to content
pub fn initialize_runtime(
    root_path: &str,
    src_files: HashMap<String, String>,
) -> Result<Arc<dyn Bex>, BridgeError> {
    let rt = build_source_runtime(root_path, src_files)?;
    let rt = crate::runtime_owner::own_dynamic(rt);
    crate::runtime_registry::insert_dynamic(rt.clone())?;
    Ok(rt)
}

fn build_source_runtime(
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
    Ok(rt)
}

pub fn register_runtime_from_sources(
    key: u64,
    root_path: &str,
    files: HashMap<String, String>,
) -> Result<Arc<dyn Bex>, BridgeError> {
    let canonical = baml_program_identity::canonical_sources(
        files
            .iter()
            .map(|(path, source)| (path.as_str(), source.as_str())),
    )
    .map_err(BridgeError::Startup)?;
    crate::runtime_registry::register_generated(key, canonical, || {
        build_source_runtime(root_path, files)
    })
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
    dispatch_call(None, encoded_args, length, id);
}

fn call_function_inner(
    runtime_key: Option<u64>,
    encoded_args: *const u8,
    length: usize,
    id: u32,
) -> Result<(), BridgeError> {
    use bridge_ctypes::baml_bridge::cffi::{CallFunctionArgs, call_function_args::CallTarget};

    if length > isize::MAX as usize || (encoded_args.is_null() && length != 0) {
        return Err(BridgeError::Internal("invalid call buffer".into()));
    }
    let args = if encoded_args.is_null() || length == 0 {
        CallFunctionArgs::default()
    } else {
        unsafe { CallFunctionArgs::from_c_buffer(encoded_args, length) }?
    };
    let reservation = crate::FunctionCallReservation::new(args.call_id);
    let runtime = crate::runtime_for_call(runtime_key, &args)?;
    let call_id = decoded_call_id(args.call_id)?;
    let target = args.call_target.ok_or(BridgeError::MissingCallTarget)?;
    if matches!(target, CallTarget::FunctionHandle(_)) && !args.type_args.is_empty() {
        return Err(BridgeError::FunctionHandleTypeArgs);
    }
    let type_args = bridge_ctypes::proto_ty_args_to_named(&args.type_args)?;
    let kwargs = kwargs_to_bex_values(args.kwargs, &HANDLE_TABLE)?;
    let call_ctx = function_call_context_builder(call_id)
        .with_type_args(type_args.type_args)
        .with_type_defs(type_args.type_defs);

    get_tokio_runtime()?.spawn(async move {
        let _reservation = reservation;
        let encoded = AssertUnwindSafe(async move {
            match target {
                CallTarget::FunctionName(function_name) => {
                    call_and_encode(runtime, function_name, kwargs.into(), call_ctx.build()).await
                }
                CallTarget::FunctionHandle(handle_key) => {
                    call_handle_and_encode(runtime, handle_key, kwargs.into(), call_ctx.build())
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

/// Enqueue a call against its originating registration.
pub extern "C" fn call_function_for_runtime(
    key: u64,
    encoded_args: *const u8,
    length: usize,
    id: u32,
) {
    dispatch_call(Some(key), encoded_args, length, id);
}

fn dispatch_call(key: Option<u64>, bytes: *const u8, length: usize, id: u32) {
    let outcome = std::panic::catch_unwind(|| call_function_inner(key, bytes, length, id));
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => send_outbound_result_to_callback(id, &error_to_outbound(error)),
        Err(panic) => {
            send_outbound_result_to_callback(id, &baml_to_host::panic_to_outbound(panic.as_ref()))
        }
    }
}
