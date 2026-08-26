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
// The engine type itself, and the compiled program it is built from, for
// hosts that manage engine lifecycles (the LSP server's and the browser's
// playground runtimes): the blessed seam stays this crate rather than a
// direct `bex_engine`/`bex_vm_types` dependency.
pub use bex_engine::BexCallResult;
pub use bex_engine::{
    BexEngine, CANCELLED_PANIC_CLASS, EngineError, FunctionCallContext, FunctionCallContextBuilder,
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
pub use bex_vm_types::Program;
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
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == baml_compiler_diagnostics::Severity::Error)
    {
        let source_files = baml_compiler2_hir::compiler2_all_files(&db);
        let sources = source_files
            .iter()
            .map(|file| (file.file_id(&db), file.text(&db).clone()))
            .collect();
        let file_paths = source_files
            .iter()
            .map(|file| (file.file_id(&db), file.path(&db)))
            .collect();
        let message = baml_compiler_diagnostics::render::render_diagnostics(
            &diagnostics,
            &sources,
            &file_paths,
            &baml_compiler_diagnostics::render::RenderConfig::agent(),
        );
        return Err(RuntimeError::Compilation { message });
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
    // Deferred construction exists so the handler above lands before any
    // profiling event fires; the engine is live from here, so activate now —
    // without this, `BAML_PROFILE` runs record nothing and the drop-time
    // unhandled-spawn drain warning is never armed.
    engine.activate_profiling();
    Ok(Arc::new(engine))
}

/// Initialize a runtime from a versioned BAML program artifact rather than
/// from source files. Mirrors [`new`] but skips compilation, validating and
/// decoding the program before instantiating the engine directly.
///
/// This is the blessed seam for running pre-packed bytecode: bridge crates call
/// it instead of reaching into `bex_engine` / `bex_vm_types` themselves.
#[allow(clippy::needless_pass_by_value)]
pub fn new_from_bytecode(bytecode: &[u8], sys_ops: SysOps) -> Result<Arc<dyn Bex>, RuntimeError> {
    let program: bex_vm_types::Program =
        baml_artifact::decode(baml_artifact::ArtifactKind::Program, bytecode).map_err(|e| {
            RuntimeError::Compilation {
                message: format!("Failed to deserialize BAML bytecode: {e}"),
            }
        })?;
    let engine = bex_engine::BexEngine::new_with_runtime_compiler(
        program,
        Arc::new(sys_ops),
        Vec::new(),
        runtime_compiler(),
    )?;
    Ok(Arc::new(engine))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod bytecode_artifact_tests {
    use sys_native::SysOpsExt as _;

    use super::*;

    #[test]
    fn synchronous_build_returns_rendered_source_diagnostics() {
        let root = vfs::VfsPath::new(vfs::MemoryFS::new());
        let files = HashMap::from([(
            FsPath::from_str("/p/a.baml".to_string()),
            "function main() -> int { nope( }".to_string(),
        )]);

        let error = match new(root, sys_ops::SysOps::native(), files) {
            Ok(_) => panic!("invalid source unexpectedly compiled"),
            Err(error) => error.to_string(),
        };

        assert!(error.contains("a.baml:1:"), "{error}");
        assert!(error.contains("error[E"), "{error}");
        assert!(error.contains("unexpected token"), "{error}");
        assert!(!error.contains("Bex is outdated"), "{error}");
    }

    #[test]
    fn rejects_format_skew_before_decoding_the_program() {
        let bytecode = baml_artifact::encode_with_format_for_test(
            baml_artifact::FORMAT_VERSION + 1,
            baml_artifact::ArtifactKind::Program,
            &7_u32,
        )
        .unwrap();

        let Err(error) = new_from_bytecode(&bytecode, sys_ops::SysOps::native()) else {
            panic!("format-skewed bytecode must fail");
        };
        assert_eq!(
            error.to_string(),
            format!(
                "Failed to deserialize BAML bytecode: generated bytecode: toolchain {} / format {}; this runtime: {} / format {} — regenerate baml_sdk and rebuild the bridge from the same commit",
                baml_artifact::BUILD_FINGERPRINT,
                baml_artifact::FORMAT_VERSION + 1,
                baml_artifact::BUILD_FINGERPRINT,
                baml_artifact::FORMAT_VERSION,
            )
        );
    }
}

pub use fs::FsPath;
