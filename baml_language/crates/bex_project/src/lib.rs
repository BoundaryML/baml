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

pub use baml_builtins2::{MediaContent, MediaValue, PromptAst, PromptAstSimple};
pub use bex::Bex;
pub use bex_engine::{
    CANCELLED_PANIC_CLASS, EngineError, FunctionCallContextBuilder, is_cancelled_engine_error,
};
pub use bex_events::EventSink;
pub use bex_external_types::{
    BexExternalAdt, BexExternalValue, Handle, MediaKind, Ty, TyAttr, try_convert_rust_data,
};
pub use sys_ops::SysOps;
pub use sys_types::{CallId, CancellationToken};
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

/// True iff `err` wraps an engine cancellation panic.
///
/// Centralizes the cancellation-classification logic that bridges and the
/// LSP server need to distinguish cancellation from other runtime errors.
pub fn is_cancelled_runtime_error(err: &RuntimeError) -> bool {
    matches!(err, RuntimeError::Engine(e) if is_cancelled_engine_error(e))
}

/// Keep pass-by-value so the returned `Arc<impl Bex>` does not capture caller locals;
/// taking `&VfsPath` / `&HashMap` would require returning a value that references them.
#[allow(clippy::needless_pass_by_value)]
pub fn new(
    root_path: vfs::VfsPath,
    sys_ops: SysOps,
    files: std::collections::HashMap<crate::fs::FsPath, String>,
    event_sink: Option<std::sync::Arc<dyn EventSink>>,
) -> Result<Arc<impl Bex>, RuntimeError> {
    let project = project::BexProject::new(&root_path, Arc::new(sys_ops), event_sink);
    project.update_all_sources(&files);
    let engine = project.take()?;
    Ok(engine)
}

pub use bex_lsp::{
    BackgroundSpawner, BexLsp, FunctionInfo, FunctionKind, FunctionOrigin, LlmCapabilities,
    LspClientSenderTrait, LspError, PlaygroundNotification, PlaygroundSender, ProjectDiagnostic,
    ProjectUpdate, TestExpandError, new_lsp,
};
pub use fs::{BamlVFS, BulkReadFileSystem, DefaultBulkReadFileSystem, FsPath};
