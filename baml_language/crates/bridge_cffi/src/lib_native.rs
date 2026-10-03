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

use crate::{BridgeError, Buffer, PreparedRuntime, baml_to_host, error_to_outbound};

#[path = "api.rs"]
pub mod api;
#[path = "ffi/mod.rs"]
mod ffi;
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
        __testonly_seed_function_ref, __testonly_seed_generic_media, __testonly_seed_heap_handle,
        BamlCffiStatus, baml_handle_clone, baml_handle_release, baml_media_base64, baml_media_file,
        baml_media_from_base64, baml_media_from_file, baml_media_from_url, baml_media_mime_type,
        baml_media_url,
    },
    host_value::{
        HostDispatchFn, complete_host_call, register_host_cancel_callback,
        register_host_dispatch_callback, register_host_dispatch_v2, register_host_release_callback,
    },
    runtime::{
        BamlBridgeInfoV1, create_baml_runtime, destroy_baml_runtime, invoke_runtime_cli,
        register_bridge_ffi, shutdown_runtime as shutdown_runtime_ffi,
        stage_runtime_from_blob as stage_runtime_from_blob_ffi,
        stage_runtime_from_blob_with_metadata, version,
    },
    unhandled_spawn::register_unhandled_spawn_error_callback,
};

/// A program as a host handed it to `stage_runtime*`: not yet validated,
/// decoded, or compiled.
enum StagedProgram {
    Bytecode {
        bytecode: Vec<u8>,
        embedded_baml_toml: Option<String>,
    },
    Files {
        root_path: String,
        src_files: HashMap<String, String>,
    },
}

impl StagedProgram {
    /// Validate and compile the program, build its engine, and run its package
    /// initializers. Building the engine is where telemetry is configured from
    /// the environment.
    fn initialize(self) -> Result<Arc<dyn Bex>, BridgeError> {
        let prepared = match self {
            Self::Bytecode {
                bytecode,
                embedded_baml_toml,
            } => crate::prepare_runtime_from_blob(
                &bytecode,
                embedded_baml_toml.as_deref(),
                bex_project::SysOps::native(),
            )?,
            Self::Files {
                root_path,
                src_files,
            } => {
                let physical_fs = vfs::PhysicalFS::new("/");
                let vfs_root = vfs::VfsPath::new(physical_fs);
                let vfs_path = vfs_root
                    .join(&root_path)
                    .map_err(|e| bex_project::RuntimeError::Other(e.to_string()))?;
                let files = src_files
                    .into_iter()
                    .map(|(k, v)| (bex_project::FsPath::from_str(k), v))
                    .collect();
                PreparedRuntime {
                    runtime: bex_project::prepare(vfs_path, bex_project::SysOps::native(), files)?,
                    error_context: None,
                }
            }
        };
        prepared.build()
    }
}

/// The process-global runtime slot. Staging stores the program; the first
/// [`get_or_init_runtime`] (in practice, the first BAML call) initializes it.
enum RuntimeSlot {
    Staged(StagedProgram),
    Ready(Arc<dyn Bex>),
    /// Initialization failed. Every later use reports the same error.
    Failed(String),
}

/// Global Bex runtime. Uses RwLock to allow replacing the runtime.
static RUNTIME_INSTANCE: RwLock<Option<RuntimeSlot>> = RwLock::new(None);

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

pub(crate) fn get_or_init_runtime() -> Result<Arc<dyn Bex>, BridgeError> {
    if let Some(RuntimeSlot::Ready(runtime)) = &*RUNTIME_INSTANCE
        .read()
        .map_err(|_| BridgeError::LockPoisoned)?
    {
        return Ok(runtime.clone());
    }

    // Initialize under the write lock, so concurrent first uses share one engine.
    let mut slot = RUNTIME_INSTANCE
        .write()
        .map_err(|_| BridgeError::LockPoisoned)?;
    let initialized = match slot.take().ok_or(BridgeError::NotInitialized)? {
        RuntimeSlot::Staged(program) => program.initialize().map_err(|error| error.to_string()),
        RuntimeSlot::Ready(runtime) => Ok(runtime),
        RuntimeSlot::Failed(message) => Err(message),
    };
    match initialized {
        Ok(runtime) => {
            *slot = Some(RuntimeSlot::Ready(runtime.clone()));
            Ok(runtime)
        }
        Err(message) => {
            *slot = Some(RuntimeSlot::Failed(message.clone()));
            Err(BridgeError::Startup(message))
        }
    }
}

/// Stage BAML source files as the global runtime, replacing any previous one.
/// Nothing is compiled until the first use of the runtime.
///
/// # Arguments
/// * `root_path` - Root path for BAML files
/// * `src_files` - Map of filename to content
pub fn stage_runtime(
    root_path: &str,
    src_files: HashMap<String, String>,
) -> Result<(), BridgeError> {
    replace_slot(StagedProgram::Files {
        root_path: root_path.to_owned(),
        src_files,
    })
}

/// Stage serialized BAML bytecode, and the generated `baml.toml` when there is
/// one, as the global runtime, replacing any previous one. Nothing is
/// validated or decoded until the first use of the runtime.
pub fn stage_runtime_from_blob(
    bytecode: &[u8],
    embedded_baml_toml: Option<&str>,
) -> Result<(), BridgeError> {
    replace_slot(StagedProgram::Bytecode {
        bytecode: bytecode.to_vec(),
        embedded_baml_toml: embedded_baml_toml.map(str::to_owned),
    })
}

fn replace_slot(program: StagedProgram) -> Result<(), BridgeError> {
    let mut guard = RUNTIME_INSTANCE
        .write()
        .map_err(|_| BridgeError::LockPoisoned)?;
    let previous = guard.replace(RuntimeSlot::Staged(program));
    drop(guard);
    if let Some(RuntimeSlot::Ready(previous)) = previous {
        get_tokio_runtime()?.spawn(previous.shutdown(None));
    }
    Ok(())
}

pub(crate) fn take_runtime() -> Result<Option<Arc<dyn Bex>>, BridgeError> {
    let slot = RUNTIME_INSTANCE
        .write()
        .map_err(|_| BridgeError::LockPoisoned)?
        .take();
    Ok(match slot {
        Some(RuntimeSlot::Ready(runtime)) => Some(runtime),
        _ => None,
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
    if let Err(e) = call_function_inner(encoded_args, length, id) {
        send_outbound_result_to_callback(id, &error_to_outbound(e));
    }
}

fn call_function_inner(encoded_args: *const u8, length: usize, id: u32) -> Result<(), BridgeError> {
    let bytes: &[u8] = if encoded_args.is_null() || length == 0 {
        &[]
    } else {
        // SAFETY: the caller promises `encoded_args` is valid for `length` bytes.
        unsafe { std::slice::from_raw_parts(encoded_args, length) }
    };
    // Pin the target before yielding to the executor: the SDK may release the
    // callable's key as soon as this returns.
    let request = crate::decode_invocation_request(bytes)?;

    get_tokio_runtime()?.spawn(async move {
        let encoded = AssertUnwindSafe(crate::execute_invocation(request))
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

/// Reports the fixed native invocation contract. Wire messages are unversioned.
#[unsafe(no_mangle)]
pub extern "C" fn invocation_protocol_version() -> u32 {
    1
}

/// Read the original runtime clock. The output is untouched on failure.
/// # Safety
/// `out_now` must point to writable u64 storage unless null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn invocation_clock_ns(call_id: u64, out_now: *mut u64) -> BamlCffiStatus {
    if out_now.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    match crate::invocation_clock_by_id(call_id) {
        Ok(now) => {
            unsafe {
                out_now.write(now);
            }
            BamlCffiStatus::Ok
        }
        Err(_) => BamlCffiStatus::InvalidHandle,
    }
}

/// Project a generated trace capability without executing a BAML call.
/// # Safety
/// Both outputs must point to writable storage. They remain untouched on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_selection_ffi(
    call_id: u64,
    key: u64,
    out_selection: *mut Buffer,
    out_reservation: *mut u64,
) -> BamlCffiStatus {
    if out_selection.is_null() || out_reservation.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    match std::panic::catch_unwind(|| crate::control_projection::trace_selection(call_id, key)) {
        Ok(Ok((wire, owner))) => {
            unsafe {
                out_selection.write(Buffer::from(wire));
                out_reservation.write(owner.unwrap_or(0));
            }
            BamlCffiStatus::Ok
        }
        Ok(Err(_)) => BamlCffiStatus::TypeMismatch,
        Err(_) => BamlCffiStatus::InternalError,
    }
}

/// Inspect a retained callback context without executing a BAML call.
/// # Safety
/// The output must point to writable storage and remains untouched on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn invocation_context_ffi(
    key: u64,
    out_context: *mut Buffer,
) -> BamlCffiStatus {
    if out_context.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    match std::panic::catch_unwind(|| crate::control_projection::invocation_context(key)) {
        Ok(Ok(wire)) => {
            unsafe {
                out_context.write(Buffer::from(wire));
            }
            BamlCffiStatus::Ok
        }
        Ok(Err(_)) => BamlCffiStatus::InvalidHandle,
        Err(_) => BamlCffiStatus::InternalError,
    }
}
