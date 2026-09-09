//! Native runtime and C ABI implementation for `bridge_cffi`.

use std::{
    collections::HashMap,
    panic::AssertUnwindSafe,
    sync::{Arc, RwLock},
};

use bex_project::Bex;
use futures::future::FutureExt;
use once_cell::sync::OnceCell;
use sys_native::SysOpsExt;
use tokio::runtime::Runtime;

use crate::{BridgeError, baml_to_host, error_to_outbound};

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
    unhandled_spawn::{
        OwnedUnhandledSpawnError, register_owned_unhandled_spawn_error_callback,
        register_unhandled_spawn_error_callback,
    },
};

/// Global Bex runtime. Uses RwLock to allow replacing the runtime.
static RUNTIME_INSTANCE: RwLock<Option<(Arc<dyn Bex>, bridge_ctypes::TransferSession<'static>)>> =
    RwLock::new(None);

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
    get_runtime_with_transfers().map(|(runtime, _)| runtime)
}

pub(crate) fn get_runtime_with_transfers()
-> Result<(Arc<dyn Bex>, bridge_ctypes::TransferSession<'static>), BridgeError> {
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
    replace_runtime(rt.clone())?;
    Ok(rt)
}

pub(crate) fn replace_runtime(rt: Arc<dyn Bex>) -> Result<(), BridgeError> {
    let transfers = bridge_ctypes::TransferSession::new(&bridge_ctypes::HANDLE_TABLE);
    crate::RuntimeIssuer::install(&rt, transfers.clone())?;
    crate::install_unhandled_spawn_error_handler(&rt, transfers.clone());
    let mut guard = RUNTIME_INSTANCE
        .write()
        .map_err(|_| BridgeError::LockPoisoned)?;
    let previous = guard.replace((rt, transfers));
    drop(guard);
    if let Some((previous, transfers)) = previous {
        transfers.close();
        get_tokio_runtime()?.spawn(previous.shutdown());
    }
    Ok(())
}

pub(crate) fn take_runtime() -> Result<Option<Arc<dyn Bex>>, BridgeError> {
    let previous = RUNTIME_INSTANCE
        .write()
        .map_err(|_| BridgeError::LockPoisoned)?
        .take();
    Ok(previous.map(|(runtime, transfers)| {
        transfers.close();
        runtime
    }))
}

pub(crate) fn dispatch_unhandled_spawn_error(error: OwnedUnhandledSpawnError) {
    ffi::unhandled_spawn::dispatch(error);
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
    let runtime = get_runtime()?;
    let bytes = if encoded_args.is_null() || length == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(encoded_args, length) }
    };
    let prepared = crate::prepare_call(bytes)?;

    get_tokio_runtime()?.spawn(async move {
        let encoded = AssertUnwindSafe(crate::invoke_prepared(runtime, prepared))
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
