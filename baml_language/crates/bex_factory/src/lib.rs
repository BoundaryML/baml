//! Reusable compile-and-run runtime for BAML programs.
//!
//! Two traits define the API:
//! - **`Bex`**: core run API (`call_function`). Implemented by `Arc<BexEngine>`.
//! - **`BexIncremental`**: holds DB, `add_source`/`set_source`, `function_names`, `engine_is_current`, plus `call_function`/`function_params`.
//!
//! Two public constructors:
//! - [`new`] — compile source files and return `Arc<dyn Bex>`.
//! - [`new_incremental`] — return a `Box<dyn BexIncremental>` (holds DB, `env`/`sys_ops` once).

mod error;
#[cfg(feature = "incremental")]
mod incremental;

use std::{collections::HashMap, path::Path, sync::Arc};

use async_trait::async_trait;
use baml_compiler_diagnostics::{RenderConfig, ToDiagnostic, render_diagnostic};
use baml_compiler_emit::LoweringError;
use baml_project::ProjectDatabase;
pub use bex_engine::{BexEngine, CancellationToken, EngineError};
pub use bex_external_types::{BexExternalAdt, BexExternalValue, MediaKind, Ty};
use bex_heap::BexValue;
pub use bex_heap::builtin_types;
pub use bex_resource_types::{ResourceHandle, ResourceRegistryRef, ResourceType};
pub use error::RuntimeError;
#[cfg(feature = "incremental")]
use incremental::BexIncrementalRuntime;
#[cfg(feature = "incremental")]
pub use incremental::{AddSourceResult, BexIncremental};
pub use sys_types::SysOps;

// ---------------------------------------------------------------------------
// Bex trait
// ---------------------------------------------------------------------------

pub struct BexArgs(HashMap<String, BexExternalValue>);

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

/// Core runtime API: call functions and introspect parameters.
///
/// Implemented for `Arc<BexEngine>` (Send, for use from `bridge_cffi`/tokio).
/// The incremental runtime has equivalent methods for WASM/single-thread use.
#[async_trait]
pub trait Bex: Send + Sync {
    /// Execute a function by name. Returns a fully owned value (no Handle variants).
    ///
    /// The `cancel` token allows the caller to cancel the function mid-execution.
    /// When cancelled, the engine returns `EngineError::Cancelled` and aborts
    /// all in-flight async operations (HTTP requests, sleeps, etc.).
    async fn call_function(
        &self,
        function_name: &str,
        args: BexArgs,
        cancel: CancellationToken,
    ) -> Result<BexExternalValue, RuntimeError>;
}

#[async_trait]
impl Bex for BexEngine {
    async fn call_function(
        &self,
        function_name: &str,
        BexArgs(mut args): BexArgs,
        cancel: CancellationToken,
    ) -> Result<BexExternalValue, RuntimeError> {
        // guarantee function ordering.
        let params = self
            .function_params(function_name)
            .map_err(RuntimeError::from)?;

        // let ordered args:
        let ordered_args = params
            .into_iter()
            .map(|(name, _)| {
                args.remove(name)
                    .ok_or_else(|| RuntimeError::InvalidArgument {
                        name: name.to_string(),
                    })
            })
            .collect::<Result<_, _>>()?;
        if !args.is_empty() {
            let extra_args = args.keys().cloned().collect::<Vec<_>>().join(", ");
            return Err(RuntimeError::InvalidArgument {
                name: format!("extra arguments: {extra_args}"),
            });
        }

        let result =
            BexEngine::call_function(self, function_name, ordered_args, None, &[], cancel).await?;

        // For now call_function guarantees that the result is owned, but we should change this in the future
        // once we allow devs to control if functions return owned values or not.
        let owned_result = self
            .heap()
            .with_gc_protection(|p| BexValue::from(&result).as_owned_but_very_slow(&p))?;

        Ok(owned_result)
    }
}

#[async_trait]
impl Bex for Arc<BexEngine> {
    async fn call_function(
        &self,
        function_name: &str,
        args: BexArgs,
        cancel: CancellationToken,
    ) -> Result<BexExternalValue, RuntimeError> {
        Bex::call_function(self.as_ref(), function_name, args, cancel).await
    }
}

// ---------------------------------------------------------------------------
// Public constructors
// ---------------------------------------------------------------------------

/// Compile source files and create a concrete `BexEngine`.
///
/// Use this when you need direct access to engine methods like `function_params`
/// or `call_function` with tracing parameters.
///
/// # Arguments
/// * `root_path` - Root path for BAML files
/// * `src_files` - Map of filename to content
/// * `sys_ops` - System operations provider
pub fn new_engine(
    root_path: &str,
    src_files: &HashMap<String, String>,
    sys_ops: SysOps,
) -> Result<Arc<BexEngine>, RuntimeError> {
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new(root_path));

    for (filename, content) in src_files {
        db.add_or_update_file(&std::path::PathBuf::from(filename), content);
    }

    let bytecode = baml_compiler_emit::generate_project_bytecode(&db)
        .map_err(|e| render_lowering_error(&db, &e))?;

    let engine = BexEngine::new(bytecode, sys_ops)?;

    Ok(Arc::new(engine))
}

/// Compile source files and create a Bex runtime. Returns [`Arc<dyn Bex>`] for use from any consumer.
///
/// # Arguments
/// * `root_path` - Root path for BAML files
/// * `src_files` - Map of filename to content
/// * `sys_ops` - System operations provider
pub fn new(
    root_path: &str,
    src_files: &HashMap<String, String>,
    sys_ops: SysOps,
) -> Result<Arc<dyn Bex>, RuntimeError> {
    Ok(new_engine(root_path, src_files, sys_ops)?)
}

/// Create an incremental runtime that holds the project DB.
///
/// `sys_ops` is stored once. Use `add_source`/`set_source` to update the DB and swap the engine;
/// on compile failure, diagnostics are returned and the engine is left unchanged.
///
/// Requires the `incremental` feature.
#[cfg(feature = "incremental")]
pub fn new_incremental(
    root_path: &str,
    src_files: &HashMap<String, String>,
    sys_ops: SysOps,
) -> Box<dyn BexIncremental> {
    Box::new(BexIncrementalRuntime::new(root_path, src_files, sys_ops))
}

// ---------------------------------------------------------------------------
// Error rendering helpers
// ---------------------------------------------------------------------------

pub(crate) fn render_lowering_error(db: &ProjectDatabase, error: &LoweringError) -> RuntimeError {
    let diagnostic = error.to_diagnostic();

    let source_files = db.get_source_files();
    let mut sources = HashMap::new();
    let mut file_paths = HashMap::new();

    for source_file in &source_files {
        let file_id = source_file.file_id(db);
        sources.insert(file_id, source_file.text(db).clone());
        file_paths.insert(file_id, source_file.path(db));
    }

    let rendered = render_diagnostic(&diagnostic, &sources, &file_paths, &RenderConfig::cli());

    RuntimeError::Compilation { message: rendered }
}
