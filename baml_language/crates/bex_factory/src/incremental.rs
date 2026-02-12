//! Incremental Bex runtime: holds the project DB and can update source, swap engine, and return diagnostics.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use baml_project::{ProjectDatabase, list_functions};
use bex_engine::BexEngine;
use bex_external_types::BexExternalValue;

use crate::{Bex, BexArgs, RuntimeError, SysOps, render_lowering_error};

/// Result of `add_source` / `set_source`: whether the engine was updated and any diagnostics.
#[derive(Debug, Clone)]
pub struct AddSourceResult {
    /// True if the project compiled and the engine was swapped.
    pub engine_updated: bool,
    /// Rendered diagnostic message(s). Empty on success.
    pub diagnostics: String,
}

/// Trait for the incremental runtime API (DB, `add_source`, `set_source`, `function_names`, `engine_is_current`, `call_function`).
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

    /// Add or update a source file. Recompiles and swaps the engine on success; returns diagnostics on failure.
    fn add_source(&mut self, path: &str, content: &str) -> AddSourceResult;

    /// Names of all functions in the current project (from DB, no full compile).
    fn function_names(&self) -> Vec<String>;

    /// True iff the last `add_source`/`set_source` compiled successfully.
    fn engine_is_current(&self) -> bool;
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
}

fn make_engine(db: &ProjectDatabase, sys_ops: SysOps) -> Result<Arc<BexEngine>, RuntimeError> {
    let bytecode = baml_compiler_emit::generate_project_bytecode(db)
        .map_err(|e| render_lowering_error(db, &e))?;

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
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new(root_path));

        for (filename, content) in src_files {
            db.add_or_update_file(Path::new(filename), content);
        }

        let engine = make_engine(&db, sys_ops.clone()).ok();

        Self {
            engine_is_current: engine.is_some(),
            root_path: PathBuf::from(root_path),
            engine,
            sys_ops,
            db,
        }
    }

    /// Add or update a source file. Recompiles and swaps the engine on success; returns diagnostics on failure.
    pub(crate) fn add_source(&mut self, path: &str, content: &str) -> AddSourceResult {
        let full_path = self.root_path.join(path);
        self.db.add_or_update_file(&full_path, content);

        let engine = make_engine(&self.db, self.sys_ops.clone());

        match engine {
            Ok(engine) => {
                self.engine = Some(engine);
                self.engine_is_current = true;
                AddSourceResult {
                    engine_updated: true,
                    diagnostics: String::new(),
                }
            }
            Err(e) => {
                self.engine_is_current = false;
                AddSourceResult {
                    engine_updated: false,
                    diagnostics: format!("Engine error: {e}"),
                }
            }
        }
    }

    /// Names of all functions in the current project (from DB, no full compile).
    pub(crate) fn function_names(&self) -> Vec<String> {
        let Some(project) = self.db.get_project() else {
            return vec![];
        };
        list_functions(&self.db, project)
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    /// True iff the last `add_source`/`set_source` compiled successfully.
    pub(crate) fn engine_is_current(&self) -> bool {
        self.engine_is_current
    }

    /// Call a BAML function (delegates to current engine).
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

    async fn call_function(
        &self,
        function_name: &str,
        args: BexArgs,
    ) -> Result<BexExternalValue, RuntimeError> {
        BexIncrementalRuntime::call_function(self, function_name, args).await
    }
}
