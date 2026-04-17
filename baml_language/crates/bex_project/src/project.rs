use bex_engine::BexEngine;
use bex_external_types::Handle;
use sys_ops::SysOps;

use crate::RuntimeError;

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
    event_sink: Option<std::sync::Arc<dyn bex_events::EventSink>>,
    current_bex: std::sync::RwLock<(bool, Option<std::sync::Arc<BexEngine>>)>,
    /// `(generation, cancel_token, registry)` — generation is bumped on every engine swap;
    /// stale async tasks compare their captured generation before storing results.
    /// The cancel token is cancelled and replaced on engine swap so in-flight tasks abort.
    /// The registry (a `Handle` to a live `testing.TestRegistry` on the heap) is cleared on engine swap.
    test_state: std::sync::Arc<std::sync::Mutex<TestState>>,
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

    pub(crate) fn new(
        root_path: &vfs::VfsPath,
        sys_ops: std::sync::Arc<SysOps>,
        event_sink: Option<std::sync::Arc<dyn bex_events::EventSink>>,
    ) -> Self {
        let mut db = baml_project::ProjectDatabase::new();
        db.set_project_root(crate::fs::FsPath::from_vfs(root_path).as_path());
        Self {
            db: std::sync::Arc::new(std::sync::Mutex::new(db)),
            sys_ops,
            event_sink,
            current_bex: std::sync::RwLock::new((false, None)),
            test_state: std::sync::Arc::new(std::sync::Mutex::new(TestState::new())),
        }
    }

    pub(crate) fn event_sink(&self) -> Option<std::sync::Arc<dyn bex_events::EventSink>> {
        self.event_sink.clone()
    }

    #[allow(dead_code)]
    pub(crate) fn update_single_source(&self, path: &vfs::VfsPath, source: &str) {
        let mut db = self.db.lock().unwrap();
        db.add_or_update_file(crate::fs::FsPath::from_vfs(path).as_path(), source);
        drop(db);

        let _ = self.update_bex();
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

    #[allow(dead_code)]
    pub(crate) fn remove_source(&self, path: &vfs::VfsPath) -> Result<(), RuntimeError> {
        let mut db = self.db.lock().unwrap();
        db.remove_file(crate::fs::FsPath::from_vfs(path).as_path());
        drop(db);

        self.update_bex()
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
        let runtime = match BexEngine::new(
            bytecode,
            self.sys_ops.clone(),
            self.event_sink.clone(),
            Vec::new(),
        ) {
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
