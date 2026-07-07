use std::collections::{HashMap, VecDeque};

use bex_engine::BexEngine;
use bex_external_types::Handle;
use sys_ops::SysOps;

use crate::RuntimeError;

/// Prepared control-flow graphs pinned for playground runs, keyed by
/// `(project generation, function name)`.
///
/// A run captures the project generation at launch and resolves its graph
/// overlay against that generation later, possibly after several recompiles.
/// Graphs are built lazily for the function a run actually executes (see
/// [`BexProject::control_flow_graph_for_generation`]) instead of eagerly for
/// every function on every compile — fully-inlined graphs grow with
/// call-site fan-out, so eager per-compile snapshots dominated LSP memory.
const RETAINED_RUN_CFGS: usize = 64;

#[derive(Default)]
struct RunCfgCache {
    order: VecDeque<(u64, String)>,
    graphs: HashMap<
        (u64, String),
        std::sync::Arc<baml_compiler2_visualization::control_flow::ControlFlowGraph>,
    >,
}

impl RunCfgCache {
    fn insert(
        &mut self,
        generation: u64,
        function_name: &str,
        graph: std::sync::Arc<baml_compiler2_visualization::control_flow::ControlFlowGraph>,
    ) {
        let key = (generation, function_name.to_string());
        if self.graphs.insert(key.clone(), graph).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > RETAINED_RUN_CFGS {
            if let Some(evicted) = self.order.pop_front() {
                self.graphs.remove(&evicted);
            }
        }
    }

    fn graph(
        &self,
        generation: u64,
        function_name: &str,
    ) -> Option<std::sync::Arc<baml_compiler2_visualization::control_flow::ControlFlowGraph>> {
        self.graphs
            .get(&(generation, function_name.to_string()))
            .cloned()
    }
}

pub(crate) struct TestState {
    pub generation: u64,
    pub cancel: sys_types::CancellationToken,
    pub registry: Option<Handle>,
}

impl TestState {
    fn new() -> Self {
        Self {
            generation: 0,
            cancel: sys_types::CancellationToken::new(),
            registry: None,
        }
    }
}

pub(crate) struct BexProject {
    pub(crate) db: std::sync::Arc<std::sync::Mutex<baml_project::ProjectDatabase>>,
    sys_ops: std::sync::Arc<SysOps>,
    current_bex: std::sync::RwLock<(bool, Option<std::sync::Arc<BexEngine>>)>,
    /// `(generation, cancel_token, registry)` — generation is bumped on every engine swap;
    /// stale async tasks compare their captured generation before storing results.
    /// The cancel token is cancelled and replaced on engine swap so in-flight tasks abort.
    /// The registry (a `Handle` to a live `testing.TestRegistry` on the heap) is cleared on engine swap.
    test_state: std::sync::Arc<std::sync::Mutex<TestState>>,
    run_cfgs: std::sync::Mutex<RunCfgCache>,
}

impl BexProject {
    pub(crate) fn try_lock_db(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, baml_project::ProjectDatabase>, crate::LspError> {
        self.db.try_lock().map_err(|_| {
            crate::LspError::UnknownErrorCode(
                "Database mutex is locked (possibly from a prior panic)".to_string(),
            )
        })
    }

    pub(crate) fn new(root_path: &vfs::VfsPath, sys_ops: std::sync::Arc<SysOps>) -> Self {
        let mut db = baml_project::ProjectDatabase::new();
        db.set_project_root(crate::fs::FsPath::from_vfs(root_path).as_path());
        Self {
            db: std::sync::Arc::new(std::sync::Mutex::new(db)),
            sys_ops,
            current_bex: std::sync::RwLock::new((false, None)),
            test_state: std::sync::Arc::new(std::sync::Mutex::new(TestState::new())),
            run_cfgs: std::sync::Mutex::new(RunCfgCache::default()),
        }
    }

    /// Update all sources in the project (removes any sources that are not in the new sources)
    pub(crate) fn update_all_sources(
        &self,
        sources: &std::collections::HashMap<crate::fs::FsPath, String>,
    ) {
        let mut db = self.db.lock().unwrap();
        let mut existing_paths: std::collections::HashSet<_> =
            db.non_builtin_file_paths().collect();
        for (path, source) in sources {
            db.add_or_update_file(path.as_path(), source);
            existing_paths.remove(path.as_path());
        }
        for path in existing_paths {
            db.remove_file(&path);
        }
        drop(db);

        // We don't care about the result here.
        // If someone cares, they should get the diagnostics from the diagnostics_by_file method.
        let _ = self.update_bex();
    }

    /// Update some sources in the project (but doesn't remove any sources)
    pub(crate) fn update_some_sources(
        &self,
        sources: &std::collections::HashMap<crate::fs::FsPath, String>,
    ) {
        let mut db = self.db.lock().unwrap();
        for (path, source) in sources {
            db.add_or_update_file(path.as_path(), source);
        }
        drop(db);

        let _ = self.update_bex();
    }

    pub(crate) fn take(self) -> Result<std::sync::Arc<BexEngine>, RuntimeError> {
        let current_bex = self.current_bex.into_inner().unwrap();
        if current_bex.0 {
            #[allow(clippy::redundant_clone)]
            current_bex.1.clone().ok_or(RuntimeError::Compilation {
                message: "No bex".to_string(),
            })
        } else {
            Err(RuntimeError::Compilation {
                message: "Bex is outdated".to_string(),
            })
        }
    }

    /// Returns the shared test state (generation, cancel token, registry).
    /// Callers capture `generation` and clone `cancel` before starting async work.
    pub(crate) fn test_state(&self) -> std::sync::Arc<std::sync::Mutex<TestState>> {
        self.test_state.clone()
    }

    pub(crate) fn is_bex_current(&self) -> bool {
        let current_bex = self.current_bex.read().unwrap();
        current_bex.0
    }

    pub(crate) fn current_generation(&self) -> u64 {
        self.test_state.lock().unwrap().generation
    }

    /// Return the prepared control-flow graph for `function_name` as of the
    /// given project generation.
    ///
    /// Graphs are built on demand and cached: a hit serves any generation
    /// still cached; a miss can only be built while `generation` is still
    /// current (the database has moved on otherwise). Playground run launches
    /// call this immediately after capturing their generation, which pins the
    /// graph for later overlay resolutions of that run.
    pub(crate) fn control_flow_graph_for_generation(
        &self,
        generation: u64,
        function_name: &str,
    ) -> Option<std::sync::Arc<baml_compiler2_visualization::control_flow::ControlFlowGraph>> {
        if let Some(graph) = self
            .run_cfgs
            .lock()
            .unwrap()
            .graph(generation, function_name)
        {
            return Some(graph);
        }
        if generation != self.current_generation() {
            return None;
        }
        let graph = {
            let db = self.db.lock().unwrap();
            let graph = db.ast_control_flow_graph(function_name)?;
            std::sync::Arc::new(
                baml_compiler2_visualization::control_flow::prepare_control_flow_graph_for_visualization(
                    &graph,
                ),
            )
        };
        let mut run_cfgs = self.run_cfgs.lock().unwrap();
        // The generation may have moved while building; only cache a graph
        // that really reflects the requested generation.
        if generation != self.current_generation() {
            return None;
        }
        run_cfgs.insert(generation, function_name, graph.clone());
        Some(graph)
    }

    pub(crate) fn get_bex(&self) -> Result<std::sync::Arc<BexEngine>, RuntimeError> {
        let current_bex = self.current_bex.read().unwrap();
        current_bex.1.clone().ok_or(RuntimeError::Compilation {
            message: "No bex has been created yet".to_string(),
        })
    }

    fn get_bytecode(&self) -> Result<bex_vm_types::Program, RuntimeError> {
        let db = self.db.try_lock().map_err(|_| RuntimeError::Compilation {
            message: "Database mutex is locked (possibly from a prior panic)".to_string(),
        })?;
        db.get_bytecode().map_err(|e| RuntimeError::Compilation {
            message: e.to_string(),
        })
    }

    fn set_bex_outdated(&self) {
        let mut current_bex = self.current_bex.write().unwrap();
        current_bex.0 = false;
    }

    fn set_current_bex(&self, bex: BexEngine) {
        let mut current_bex = self.current_bex.write().unwrap();
        current_bex.1 = Some(std::sync::Arc::new(bex));
        current_bex.0 = true;
        // Cancel in-flight tasks, bump generation, and clear stale test registry
        let mut state = self.test_state.lock().unwrap();
        state.cancel.cancel();
        state.generation += 1;
        state.cancel = sys_types::CancellationToken::new();
        state.registry = None;
    }

    fn update_bex(&self) -> Result<(), RuntimeError> {
        self.set_bex_outdated();

        // Skip bytecode generation if there are any diagnostic errors.
        {
            let db = self.db.lock().unwrap();
            let diagnostics = baml_project::collect_compiler2_diagnostics(&db);
            let has_errors = diagnostics
                .iter()
                .any(|d| d.severity == baml_compiler_diagnostics::Severity::Error);
            if has_errors {
                log::info!("update_bex: skipping — diagnostic errors present");
                return Err(RuntimeError::Compilation {
                    message: "Cannot generate bytecode: diagnostic errors present".to_string(),
                });
            }
        }

        let bytecode = match self.get_bytecode() {
            Ok(bc) => bc,
            Err(e) => {
                log::warn!("update_bex: get_bytecode failed: {e}");
                return Err(RuntimeError::Compilation {
                    message: format!("get_bytecode failed: {e}"),
                });
            }
        };
        let runtime = match BexEngine::new(bytecode, self.sys_ops.clone(), Vec::new()) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("update_bex: BexEngine::new failed: {e}");
                return Err(RuntimeError::Engine(e));
            }
        };
        self.set_current_bex(runtime);
        log::info!("update_bex: success");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_cfg_cache_evicts_oldest_entries() {
        let mut cache = RunCfgCache::default();
        for generation in 1..=(RETAINED_RUN_CFGS as u64 + 1) {
            cache.insert(
                generation,
                "Workflow",
                std::sync::Arc::new(
                    baml_compiler2_visualization::control_flow::ControlFlowGraph::default(),
                ),
            );
        }

        assert!(cache.graph(1, "Workflow").is_none());
        assert!(
            cache
                .graph(RETAINED_RUN_CFGS as u64 + 1, "Workflow")
                .is_some()
        );
    }

    #[test]
    fn run_cfg_cache_reinsert_does_not_duplicate_order() {
        let mut cache = RunCfgCache::default();
        let graph = || {
            std::sync::Arc::new(
                baml_compiler2_visualization::control_flow::ControlFlowGraph::default(),
            )
        };
        for _ in 0..(RETAINED_RUN_CFGS * 2) {
            cache.insert(7, "Workflow", graph());
        }
        cache.insert(8, "Workflow", graph());
        assert!(cache.graph(7, "Workflow").is_some());
        assert!(cache.graph(8, "Workflow").is_some());
        assert_eq!(cache.order.len(), 2);
    }
}
