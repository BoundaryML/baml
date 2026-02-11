//! Reusable compile-and-run runtime for BAML programs.
//!
//! `BexFactory` wraps the compile + engine pipeline into an opaque facade
//! that any consumer (CFFI, WASM, tests, CLI) can use without reimplementing
//! the compile-and-run flow.

mod error;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use baml_base::FileId;
use baml_compiler_diagnostics::{RenderConfig, ToDiagnostic, render_diagnostic};
use baml_compiler_emit::LoweringError;
use baml_project::ProjectDatabase;
use bex_engine::BexEngine;
pub use bex_engine::EngineError;
pub use bex_external_types::{BexExternalAdt, BexExternalValue, MediaKind, Ty};
use bex_heap::BexValue;
pub use error::RuntimeError;
pub use sys_types::SysOps;

/// An opaque runtime that compiles BAML source files and executes functions.
#[derive(Clone)]
pub struct BexFactory {
    engine: Arc<BexEngine>,
}

impl BexFactory {
    /// Compile source files and create an engine.
    ///
    /// # Arguments
    /// * `root_path` - Root path for BAML files
    /// * `src_files` - Map of filename to content
    /// * `env_vars` - Environment variables
    /// * `sys_ops` - System operations provider
    pub fn new(
        root_path: &str,
        src_files: &HashMap<String, String>,
        env_vars: HashMap<String, String>,
        sys_ops: SysOps,
    ) -> Result<Self, RuntimeError> {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new(root_path));

        for (filename, content) in src_files {
            db.add_or_update_file(&PathBuf::from(filename), content);
        }

        let bytecode = baml_compiler_emit::generate_project_bytecode(&db)
            .map_err(|e| render_lowering_error(&db, &e))?;

        let engine = BexEngine::new(bytecode, env_vars, sys_ops)?;

        Ok(Self {
            engine: Arc::new(engine),
        })
    }

    /// Execute a function by name.
    ///
    /// Calls `BexEngine::call_function`, then converts the result to a fully
    /// owned `BexExternalValue` with no heap references.
    pub async fn call_function(
        &self,
        function_name: &str,
        args: Vec<BexExternalValue>,
    ) -> Result<BexExternalValue, RuntimeError> {
        let result = self.engine.call_function(function_name, args).await?;

        // Ensure the returned value is fully owned (no Handle variants).
        self.engine
            .heap()
            .with_gc_protection(|protected| {
                BexValue::ExternalValue(&result).as_owned_but_very_slow(&protected)
            })
            .map_err(RuntimeError::from)
    }

    /// Get parameter names and types for a function.
    pub fn function_params(&self, name: &str) -> Option<Vec<(&str, &Ty)>> {
        self.engine.function_params(name)
    }
}

// ---------------------------------------------------------------------------
// Error rendering helpers
// ---------------------------------------------------------------------------

/// Render a `LoweringError` using the standard diagnostics infrastructure.
fn render_lowering_error(db: &ProjectDatabase, error: &LoweringError) -> RuntimeError {
    let diagnostic = error.to_diagnostic();

    let source_files = db.get_source_files();
    let mut sources: HashMap<FileId, String> = HashMap::new();
    let mut file_paths: HashMap<FileId, PathBuf> = HashMap::new();

    for source_file in &source_files {
        let file_id = source_file.file_id(db);
        sources.insert(file_id, source_file.text(db).clone());
        file_paths.insert(file_id, source_file.path(db));
    }

    let rendered = render_diagnostic(&diagnostic, &sources, &file_paths, &RenderConfig::cli());

    RuntimeError::Compilation { message: rendered }
}
