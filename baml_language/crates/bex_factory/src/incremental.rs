//! Incremental Bex runtime: holds the project DB and can update source, swap engine, and return diagnostics.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use baml_compiler_diagnostics::{RenderConfig, Severity, render_diagnostic};
use baml_project::{ProjectDatabase, list_functions};
use bex_engine::BexEngine;
use bex_external_types::BexExternalValue;

use crate::{Bex, BexArgs, RuntimeError, SysOps, render_lowering_error};

/// Result of `add_source`: whether the engine was updated.
#[derive(Debug, Clone)]
pub struct AddSourceResult {
    /// True if the project compiled and the engine was swapped.
    pub engine_updated: bool,
}

/// A single rendered diagnostic with its severity.
#[derive(Debug, Clone)]
pub struct RenderedDiagnostic {
    pub severity: Severity,
    pub message: String,
}

/// Trait for the incremental runtime API.
///
/// Implemented by the incremental runtime. Use [`crate::new_incremental`] to get a `Box<dyn BexIncremental>`.
#[async_trait(?Send)]
pub trait BexIncremental {
    /// Call a BAML function.
    async fn call_function(
        &self,
        function_name: &str,
        args: BexArgs,
    ) -> Result<BexExternalValue, RuntimeError>;

    /// Add or update a source file. Recompiles and swaps the engine on success.
    ///
    /// After calling this, use [`diagnostics`](Self::diagnostics) to get all errors/warnings.
    fn add_source(&mut self, path: &str, content: &str) -> AddSourceResult;

    /// Names of all functions in the current project (from DB, no full compile).
    fn function_names(&self) -> Vec<String>;

    /// True iff the last `add_source`/`set_source` compiled successfully.
    fn engine_is_current(&self) -> bool;

    /// All diagnostics (errors, warnings, info) for the current project state.
    ///
    /// Uses the Salsa DB's `check()` method — the single source of truth for diagnostics.
    /// Rendered without ANSI colors (suitable for WASM / browser display).
    fn diagnostics(&self) -> Vec<RenderedDiagnostic>;
}

/// Incremental runtime: holds the DB, implements [`BexIncremental`].
pub(crate) struct BexIncrementalRuntime {
    db: ProjectDatabase,
    root_path: PathBuf,
    sys_ops: SysOps,
    /// Current engine, if the last compile succeeded.
    engine: Option<Arc<BexEngine>>,
    /// True iff the last `add_source`/`set_source` compiled successfully (engine matches current DB).
    engine_is_current: bool,
    /// Error message from the last failed engine build (lowering/codegen), if any.
    /// Cleared on successful builds. Included in `diagnostics()` output.
    last_engine_error: Option<String>,
}

fn make_engine(db: &ProjectDatabase, sys_ops: SysOps) -> Result<Arc<BexEngine>, RuntimeError> {
    let bytecode = baml_compiler_emit::generate_project_bytecode(db)
        .map_err(|e| render_lowering_error(db, &e, &RenderConfig::test()))?;

    BexEngine::new(bytecode, sys_ops)
        .map_err(std::convert::Into::into)
        .map(Arc::new)
}

impl BexIncrementalRuntime {
    pub(crate) fn new(
        root_path: &str,
        src_files: &HashMap<String, String>,
        sys_ops: SysOps,
    ) -> Self {
        let root = PathBuf::from(root_path);
        let mut db = ProjectDatabase::new();
        db.set_project_root(&root);

        for (filename, content) in src_files {
            let full_path = root.join(filename);
            db.add_or_update_file(&full_path, content);
        }

        let engine = make_engine(&db, sys_ops.clone()).ok();

        Self {
            engine_is_current: engine.is_some(),
            last_engine_error: None,
            root_path: root,
            engine,
            sys_ops,
            db,
        }
    }

    pub(crate) fn add_source(&mut self, path: &str, content: &str) -> AddSourceResult {
        let full_path = self.root_path.join(path);
        self.db.add_or_update_file(&full_path, content);

        let engine = make_engine(&self.db, self.sys_ops.clone());

        match engine {
            Ok(engine) => {
                self.engine = Some(engine);
                self.engine_is_current = true;
                self.last_engine_error = None;
                AddSourceResult {
                    engine_updated: true,
                }
            }
            Err(e) => {
                self.engine_is_current = false;
                self.last_engine_error = Some(e.to_string());
                AddSourceResult {
                    engine_updated: false,
                }
            }
        }
    }

    pub(crate) fn function_names(&self) -> Vec<String> {
        let Some(project) = self.db.get_project() else {
            return vec![];
        };
        list_functions(&self.db, project)
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    pub(crate) fn engine_is_current(&self) -> bool {
        self.engine_is_current
    }

    pub(crate) fn diagnostics(&self) -> Vec<RenderedDiagnostic> {
        let check_result = self.db.check();
        let config = RenderConfig::test(); // Ariadne, no color, with error codes

        let mut diags: Vec<RenderedDiagnostic> = check_result
            .diagnostics
            .iter()
            .map(|d| {
                let message =
                    render_diagnostic(d, &check_result.sources, &check_result.file_paths, &config);
                RenderedDiagnostic {
                    severity: d.severity,
                    message,
                }
            })
            .collect();

        // Include the last engine build error (lowering/codegen) if the engine is stale.
        if let Some(ref err) = self.last_engine_error {
            diags.push(RenderedDiagnostic {
                severity: Severity::Error,
                message: err.clone(),
            });
        }

        diags
    }

    pub(crate) async fn call_function(
        &self,
        function_name: &str,
        args: BexArgs,
    ) -> Result<BexExternalValue, RuntimeError> {
        let engine = self
            .engine
            .as_ref()
            .ok_or_else(|| RuntimeError::Compilation {
                message: "No engine: compile failed or no source yet. Fix errors and try again."
                    .to_string(),
            })?;
        Bex::call_function(engine, function_name, args).await
    }
}

#[async_trait(?Send)]
impl BexIncremental for BexIncrementalRuntime {
    fn add_source(&mut self, path: &str, content: &str) -> AddSourceResult {
        BexIncrementalRuntime::add_source(self, path, content)
    }

    fn function_names(&self) -> Vec<String> {
        BexIncrementalRuntime::function_names(self)
    }

    fn engine_is_current(&self) -> bool {
        BexIncrementalRuntime::engine_is_current(self)
    }

    fn diagnostics(&self) -> Vec<RenderedDiagnostic> {
        BexIncrementalRuntime::diagnostics(self)
    }

    async fn call_function(
        &self,
        function_name: &str,
        args: BexArgs,
    ) -> Result<BexExternalValue, RuntimeError> {
        BexIncrementalRuntime::call_function(self, function_name, args).await
    }
}
