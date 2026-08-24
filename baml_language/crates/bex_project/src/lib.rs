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
pub use bex::{Bex, BexCallTraceResult};
pub use bex_engine::{
    CANCELLED_PANIC_CLASS, EngineError, FunctionCallContext, FunctionCallContextBuilder,
    InboundUnionAmbiguityPolicy, UnhandledSpawnError, UnhandledSpawnErrorHandler,
    is_cancelled_engine_error,
    logger::{TraceLogDrainReport, TraceLogMetadata, TraceLogger},
    register_inbound_union_ambiguity_policy,
};
pub use bex_external_types::{
    BexExternalAdt, BexExternalValue, DynWitnessDef, Handle, HostReleaseFn, HostReturnTypeError,
    HostValueArc, HostValueKind, MediaKind, PortableClassDef, PortableClassFieldDef,
    PortableEnumDef, PortableEnumVariantDef, PortableMetadata, PortableTypeDef, RuntimeTy, TyAttr,
    TypeDefRef, host_release_dispatch, runtime_ty_structurally_equal, selected_arm_equal,
    try_convert_rust_data, validate_host_return,
};
use indexmap::IndexMap;
pub use sys_ops::SysOps;
pub use sys_types::{CallId, CancellationToken};
use thiserror::Error;

mod bex;
mod bex_lsp;
mod fs;
mod precompiled_stdlib;
mod precompiled_stdlib_config;
mod project;
mod runtime_compile;
mod seed;

pub fn runtime_compiler() -> Arc<dyn bex_engine::RuntimeCompiler> {
    runtime_compile::runtime_compiler()
}

pub struct BexArgs {
    /// Required values keyed by their type-level names and kept in declared order.
    pub required: IndexMap<String, BexExternalValue>,
    /// Supplied optional values keyed by their type-level parameter names.
    pub optional: IndexMap<String, BexExternalValue>,
}

impl From<HashMap<&str, BexExternalValue>> for BexArgs {
    fn from(m: HashMap<&str, BexExternalValue>) -> Self {
        Self {
            required: m.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            optional: IndexMap::new(),
        }
    }
}

impl From<HashMap<String, BexExternalValue>> for BexArgs {
    fn from(m: HashMap<String, BexExternalValue>) -> Self {
        Self {
            required: m.into_iter().collect(),
            optional: IndexMap::new(),
        }
    }
}

impl From<IndexMap<String, BexExternalValue>> for BexArgs {
    fn from(required: IndexMap<String, BexExternalValue>) -> Self {
        Self {
            required,
            optional: IndexMap::new(),
        }
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
) -> Result<Arc<impl Bex>, RuntimeError> {
    let project = project::BexProject::new(&root_path, Arc::new(sys_ops));
    project.update_all_sources(&files);
    let engine = project.take()?;
    Ok(engine)
}

/// Initialize a runtime from a serialized BAML program — the borsh-encoded
/// `bex_vm_types::Program` that `baml pack` embeds — rather than from source
/// files. Mirrors [`new`] but skips compilation, decoding the program and
/// instantiating the engine directly.
///
/// This is the blessed seam for running pre-packed bytecode: bridge crates call
/// it instead of reaching into `bex_engine` / `bex_vm_types` themselves.
#[allow(clippy::needless_pass_by_value)]
pub fn new_from_bytecode(bytecode: &[u8], sys_ops: SysOps) -> Result<Arc<dyn Bex>, RuntimeError> {
    let program: bex_vm_types::Program =
        borsh::from_slice(bytecode).map_err(|e| RuntimeError::Compilation {
            message: format!("Failed to deserialize BAML bytecode: {e}"),
        })?;
    let engine = bex_engine::BexEngine::new_with_runtime_compiler(
        program,
        Arc::new(sys_ops),
        Vec::new(),
        runtime_compiler(),
    )?;
    Ok(Arc::new(engine))
}

// Schema types re-exported for `bridge_wasm`, which depends on `bex_project`
// but not `baml_project` and needs to name them in its `From` impl.
pub use baml_project::{FieldSchema, FieldSchemaField, ParamSchema, TypeSchema};
pub use bex_lsp::{
    BackgroundSpawner, BexLsp, FunctionInfo, FunctionKind, FunctionOrigin, LlmCapabilities,
    LspClientSenderTrait, LspError, PlaygroundNotification, PlaygroundSender, PlaygroundSourceFile,
    PreparedRun, ProjectDiagnostic, ProjectUpdate, TestExpandError, new_lsp,
};
pub use fs::{BamlVFS, BulkReadFileSystem, DefaultBulkReadFileSystem, FsPath};
