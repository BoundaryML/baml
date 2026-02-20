//! Reusable compile-and-run runtime for BAML programs.
//!
//! Three traits define the API:
//! - **`Bex`**: core run API (`call_function`). Implemented by `Arc<BexEngine>`.
//! - **`BexRuntime`**: holds DB, `update_source`, `function_names`, `engine_is_current`, `diagnostics`.
//! - **`BexWithLsp`**: LSP capabilities on top of `BexRuntime` (requires `lsp` feature).
//!
//! Two public constructors:
//! - [`new`] — compile source files and return `Arc<dyn Bex>`.
use std::{collections::HashMap, sync::Arc};

pub use bex::Bex;
pub use bex_external_types::{BexExternalValue, Ty};
pub use sys_types::{CallId, CancellationToken, SysOps};
use thiserror::Error;

mod bex;
mod bex_lsp;
mod fs;
mod project;

pub struct BexArgs(pub HashMap<String, BexExternalValue>);

impl From<HashMap<&str, BexExternalValue>> for BexArgs {
    fn from(m: HashMap<&str, BexExternalValue>) -> Self {
        BexArgs(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
}

impl From<HashMap<String, BexExternalValue>> for BexArgs {
    fn from(m: HashMap<String, BexExternalValue>) -> Self {
        BexArgs(m)
    }
}

/// Errors that can occur during runtime operations.
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("{0}")]
    Other(String),

    #[error("Invalid argument: {name}")]
    InvalidArgument { name: String },

    #[error("{message}")]
    Compilation { message: String },

    #[error("{0}")]
    Engine(#[from] bex_engine::EngineError),

    #[error("Failed to convert result to owned value: {0}")]
    Access(#[from] bex_heap::AccessError),
}

pub fn new(
    root_path: &vfs::VfsPath,
    sys_ops: SysOps,
    files: &std::collections::HashMap<crate::fs::FsPath, String>,
) -> Result<Arc<dyn Bex>, RuntimeError> {
    let project = project::BexProject::new(root_path, Arc::new(sys_ops));
    project.update_all_sources(files);
    let engine = project.take()?;
    Ok(engine as Arc<dyn Bex>)
}

pub use bex_lsp::{
    BexLsp, LspClientSenderTrait, LspError, PlaygroundNotification, PlaygroundSender,
    ProjectUpdate, new_lsp,
};
pub use fs::{BamlVFS, BulkReadFileSystem, DefaultBulkReadFileSystem, FsPath};
