//! Global Bex runtime management.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use bex_factory::Bex;
use once_cell::sync::OnceCell;
use sys_native::SysOpsExt;
use tokio::runtime::Runtime;

use crate::error::BridgeError;

/// Global Bex runtime. Uses RwLock to allow replacing the runtime.
static RUNTIME_INSTANCE: RwLock<Option<Arc<dyn Bex>>> = RwLock::new(None);

/// Global Tokio runtime for async execution.
static TOKIO_RUNTIME: OnceCell<Arc<Runtime>> = OnceCell::new();

/// Initialize the global Tokio runtime.
pub fn get_tokio_runtime() -> &'static Arc<Runtime> {
    TOKIO_RUNTIME.get_or_init(|| Arc::new(Runtime::new().expect("Failed to create Tokio runtime")))
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
) -> Result<(), BridgeError> {
    let rt = bex_factory::new(root_path, &src_files, bex_factory::SysOps::native())?;

    let mut guard = RUNTIME_INSTANCE
        .write()
        .map_err(|_| BridgeError::LockPoisoned)?;
    *guard = Some(rt);

    Ok(())
}
