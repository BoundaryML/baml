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

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError, Weak},
};

use bex_project::Bex;
#[cfg(target_arch = "wasm32")]
use vfs::error::VfsErrorKind;

#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
struct WasmSourceRootFs;

#[cfg(target_arch = "wasm32")]
impl vfs::FileSystem for WasmSourceRootFs {
    fn read_dir(&self, path: &str) -> vfs::VfsResult<Box<dyn Iterator<Item = String> + Send>> {
        if path.is_empty() {
            Ok(Box::new(std::iter::empty()))
        } else {
            Err(VfsErrorKind::FileNotFound.into())
        }
    }

    fn create_dir(&self, _path: &str) -> vfs::VfsResult<()> {
        Err(VfsErrorKind::NotSupported.into())
    }

    fn open_file(&self, _path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndRead + Send>> {
        Err(VfsErrorKind::FileNotFound.into())
    }

    fn create_file(&self, _path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        Err(VfsErrorKind::NotSupported.into())
    }

    fn append_file(&self, _path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        Err(VfsErrorKind::NotSupported.into())
    }

    fn metadata(&self, path: &str) -> vfs::VfsResult<vfs::VfsMetadata> {
        if path.is_empty() {
            Ok(vfs::VfsMetadata {
                file_type: vfs::VfsFileType::Directory,
                len: 0,
                created: None,
                modified: None,
                accessed: None,
            })
        } else {
            Err(VfsErrorKind::FileNotFound.into())
        }
    }

    fn exists(&self, path: &str) -> vfs::VfsResult<bool> {
        Ok(path.is_empty())
    }

    fn remove_file(&self, _path: &str) -> vfs::VfsResult<()> {
        Err(VfsErrorKind::NotSupported.into())
    }

    fn remove_dir(&self, _path: &str) -> vfs::VfsResult<()> {
        Err(VfsErrorKind::NotSupported.into())
    }
}

#[cfg(target_arch = "wasm32")]
fn source_vfs_root() -> vfs::VfsPath {
    vfs::VfsPath::new(WasmSourceRootFs)
}

#[cfg(not(target_arch = "wasm32"))]
fn source_vfs_root() -> vfs::VfsPath {
    vfs::VfsPath::new(vfs::MemoryFS::new())
}

struct ActiveCallRoute {
    runtime: Weak<dyn Bex>,
}

static ACTIVE_CALL_RUNTIMES: LazyLock<Mutex<HashMap<u64, Arc<ActiveCallRoute>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn active_call_runtimes() -> MutexGuard<'static, HashMap<u64, Arc<ActiveCallRoute>>> {
    ACTIVE_CALL_RUNTIMES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

pub(crate) struct ActiveCallRouteGuard {
    call_id: u64,
    route: Arc<ActiveCallRoute>,
}

impl Drop for ActiveCallRouteGuard {
    fn drop(&mut self) {
        let mut routes = active_call_runtimes();
        if routes
            .get(&self.call_id)
            .is_some_and(|current| Arc::ptr_eq(current, &self.route))
        {
            routes.remove(&self.call_id);
        }
    }
}

pub(crate) fn register_active_call_runtime(
    call_id: u64,
    runtime: &Arc<dyn Bex>,
) -> ActiveCallRouteGuard {
    let route = Arc::new(ActiveCallRoute {
        runtime: Arc::downgrade(runtime),
    });
    active_call_runtimes().insert(call_id, Arc::clone(&route));
    ActiveCallRouteGuard { call_id, route }
}

pub mod baml_to_host;
pub mod buffer;
pub mod error;
pub mod handle;

pub use baml_to_host::{
    call_and_encode, call_handle_and_encode, error_to_outbound, result_to_outbound,
    unhandled_spawn_error_to_outbound,
};
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
    initialize_sdk_profiler_defaults();
    let runtime: Arc<dyn Bex> = bex_project::new_from_bytecode(bytecode, sys_ops)?;
    install_unhandled_spawn_error_handler(&runtime);
    platform::replace_runtime(runtime.clone())?;
    Ok(runtime)
}

/// Initialize the global runtime from an in-memory BAML source map.
///
/// The candidate runtime is fully compiled before the process-global slot is
/// replaced, so a path or compilation failure leaves the previous runtime
/// usable. The memory filesystem supplies a platform-neutral project root;
/// source contents continue to enter through `bex_project::new`'s source map.
pub fn initialize_runtime_from_files_with_sys_ops(
    root_path: &str,
    src_files: HashMap<String, String>,
    sys_ops: sys_ops::SysOps,
) -> Result<Arc<dyn Bex>, BridgeError> {
    initialize_sdk_profiler_defaults();
    // `vfs::MemoryFS` timestamps its root with `SystemTime::now()`, which is
    // unavailable on wasm32-unknown-unknown. WASM therefore uses an equivalent
    // timestamp-free root marker; source contents are supplied separately.
    let vfs_root = source_vfs_root();
    let project_root =
        vfs_root
            .join(root_path)
            .map_err(|error| bex_project::RuntimeError::InvalidArgument {
                name: format!("root_path: {error}"),
            })?;

    let mut files = HashMap::with_capacity(src_files.len());
    for (name, contents) in src_files {
        if name.is_empty() {
            return Err(bex_project::RuntimeError::InvalidArgument {
                name: "source filename must not be empty".to_string(),
            }
            .into());
        }
        let validated_path = project_root.join(&name).map_err(|error| {
            bex_project::RuntimeError::InvalidArgument {
                name: format!("source filename {name:?}: {error}"),
            }
        })?;
        let root = project_root.as_str();
        let inside_root = if root.is_empty() {
            validated_path.as_str().starts_with('/')
        } else {
            validated_path
                .as_str()
                .strip_prefix(root)
                .is_some_and(|suffix| suffix.starts_with('/'))
        };
        if !inside_root {
            return Err(bex_project::RuntimeError::InvalidArgument {
                name: format!("source filename {name:?} resolves outside the project root"),
            }
            .into());
        }
        files.insert(bex_project::FsPath::from_str(name), contents);
    }

    let runtime: Arc<dyn Bex> = bex_project::new(project_root, sys_ops, files)?;
    install_unhandled_spawn_error_handler(&runtime);
    platform::replace_runtime(runtime.clone())?;
    Ok(runtime)
}

#[cfg(not(target_arch = "wasm32"))]
fn install_unhandled_spawn_error_handler(runtime: &Arc<dyn Bex>) {
    runtime.set_unhandled_spawn_error_handler(Some(Arc::new(|error| {
        let cancelled = error.cancelled;
        platform::dispatch_unhandled_spawn_error(
            unhandled_spawn_error_to_outbound(error),
            cancelled,
        );
    })));
}

#[cfg(target_arch = "wasm32")]
fn install_unhandled_spawn_error_handler(_: &Arc<dyn Bex>) {}

pub async fn shutdown_runtime() -> Result<(), BridgeError> {
    if let Some(runtime) = platform::take_runtime()? {
        runtime.shutdown().await;
    }
    Ok(())
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
    let project_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    bex_project::FunctionCallContextBuilder::new(call_id)
        .with_boundary_storage(bex_project::BoundaryStorageContext::new(
            "sdk",
            project_root,
        ))
        .with_default_history_capture(false)
}

#[cfg(not(target_arch = "wasm32"))]
fn initialize_sdk_profiler_defaults() {
    // Safe, first-initializer-wins configuration. If an embedding process
    // already initialized profiling, its immutable policy remains authoritative.
    let _ = bex_events::prof::ProfConfig::try_init_with_overflow_policy(
        bex_events::prof::RingOverflowPolicy::Shed,
    );
}

#[cfg(target_arch = "wasm32")]
fn initialize_sdk_profiler_defaults() {}

/// Cancel an in-flight function call by ID.
///
/// Returns true on success, false if the runtime is not initialized.
pub fn cancel_function_call_by_id(id: u64) -> bool {
    if id == 0 {
        return false;
    }
    let originating_runtime = active_call_runtimes()
        .get(&id)
        .and_then(|route| route.runtime.upgrade());
    originating_runtime
        .map(Ok)
        .unwrap_or_else(get_runtime)
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
