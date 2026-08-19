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
    CANCELLED_PANIC_CLASS, CaptureDefaults, EngineError, FunctionCallContext,
    FunctionCallContextBuilder, InboundUnionAmbiguityPolicy, UnhandledSpawnError,
    UnhandledSpawnErrorHandler, is_cancelled_engine_error, register_inbound_union_ambiguity_policy,
    value_capture::{
        CaptureKind, EncodedTraceValue, TraceCaptureConfig, TraceCaptureProducer,
        TraceDrainFailure, TraceDrainFailureReason, TraceDrainReport, TraceLogMetadata,
    },
};
pub use bex_external_types::{
    BexExternalAdt, BexExternalValue, DynWitnessDef, Handle, HostReleaseFn, HostReturnTypeError,
    HostValueArc, HostValueKind, MediaKind, PortableClassDef, PortableClassFieldDef,
    PortableEnumDef, PortableEnumVariantDef, PortableMetadata, PortableTypeDef, RuntimeTy, TyAttr,
    host_release_dispatch, runtime_ty_structurally_equal, selected_arm_equal,
    try_convert_rust_data, validate_host_return,
};
use indexmap::IndexMap;
pub use sys_ops::SysOps;
pub use sys_types::{CallId, CancellationToken};
use thiserror::Error;

mod bex;
mod fs;
mod precompiled_stdlib;
mod precompiled_stdlib_config;
mod runtime_compile;

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

/// Compile a BAML project from in-memory sources and initialize a runtime.
///
/// `files` are the project's `.baml` sources keyed by the path the host
/// spelled (relative to `root_path` or absolute); the embedded stdlib is
/// compiled from source alongside them. Compile errors surface as
/// [`RuntimeError::Compilation`] listing every diagnostic.
///
/// Keep pass-by-value so the returned `Arc<impl Bex>` does not capture caller
/// locals; taking `&VfsPath` / `&HashMap` would require returning a value that
/// references them.
#[allow(clippy::needless_pass_by_value)]
pub fn new(
    root_path: vfs::VfsPath,
    sys_ops: SysOps,
    files: std::collections::HashMap<crate::fs::FsPath, String>,
) -> Result<Arc<impl Bex>, RuntimeError> {
    let mut db = baml_db::ProjectDatabase::new();
    db.ensure_stdlib_sources();
    let root = db
        .add_source_root(baml_db::SourceRootSpec {
            path: std::path::PathBuf::from(root_path.as_str()),
            package: baml_base::Name::new(baml_type::RESERVED_USER_PACKAGE),
            kind: baml_base::SourceRootKind::Workspace,
        })
        .unwrap_or_else(|e| unreachable!("fresh database accepts one workspace root: {e}"));
    db.add_or_update_files_in(
        root,
        files
            .iter()
            .map(|(path, text)| (path.as_path(), text.as_str())),
    );

    let diagnostics = baml_db::collect_diagnostics(&db);
    let errors: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.severity == baml_compiler_diagnostics::Severity::Error)
        .map(|d| {
            let location = d
                .primary_span()
                .and_then(|span| db.file_id_to_path(span.file_id))
                .map(|path| format!("{}: ", path.display()))
                .unwrap_or_default();
            format!("{location}{}", d.message)
        })
        .collect();
    if !errors.is_empty() {
        return Err(RuntimeError::Compilation {
            message: format!("{} compile error(s):\n{}", errors.len(), errors.join("\n")),
        });
    }
    let program = db
        .get_bytecode_unchecked()
        .map_err(|e| RuntimeError::Compilation {
            message: e.to_string(),
        })?;

    let engine = bex_engine::BexEngine::new_with_deferred_profiling_and_runtime_compiler(
        program,
        Arc::new(sys_ops),
        Vec::new(),
        Some(runtime_compiler()),
    )?;
    engine.set_unhandled_spawn_error_handler(Some(Arc::new(|error| {
        let cancelled = error.cancelled;
        let error = error.into_engine_error();
        if cancelled {
            log::warn!("cancelled spawned task failed: {error}");
        } else {
            log::error!("unhandled spawned task failed: {error}");
        }
    })));
    Ok(Arc::new(engine))
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

pub use fs::FsPath;
