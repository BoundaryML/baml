//! Global Bex runtime management.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use bex_project::Bex;
use once_cell::sync::OnceCell;
use sys_native::SysOpsExt;
use tokio::runtime::Runtime;

use crate::error::BridgeError;

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

    let event_sink = std::env::var("BAML_TRACE_FILE")
        .ok()
        .map(|trace_file| bex_events_native::start(trace_file.into()));

    let rt = bex_project::new(vfs_path, bex_project::SysOps::native(), files, event_sink)?;

    let mut guard = RUNTIME_INSTANCE
        .write()
        .map_err(|_| BridgeError::LockPoisoned)?;
    *guard = Some(rt.clone());

    Ok(rt)
}

/// Flush the current runtime's event sink. Called by `bridge_python::flush_events()`.
pub fn flush_event_sink() {
    if let Ok(rt) = get_runtime()
        && let Some(sink) = rt.event_sink()
    {
        sink.flush();
    }
}

/// Get the current runtime's event sink (for passing to HostSpanManager).
pub fn get_event_sink() -> Option<Arc<dyn bex_events::EventSink>> {
    get_runtime().ok().and_then(|rt| rt.event_sink())
}
