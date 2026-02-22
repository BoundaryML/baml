use bex_engine::BexEngine;
use sys_types::SysOps;

use crate::RuntimeError;

pub(crate) struct BexProject {
    pub(crate) db: std::sync::Arc<std::sync::Mutex<baml_project::ProjectDatabase>>,
    sys_ops: std::sync::Arc<SysOps>,
    event_sink: Option<std::sync::Arc<dyn bex_events::EventSink>>,
    current_bex: std::sync::RwLock<(bool, Option<std::sync::Arc<BexEngine>>)>,
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
    }

    fn update_bex(&self) -> Result<(), RuntimeError> {
        self.set_bex_outdated();
        let bytecode = self.get_bytecode()?;
        let runtime = BexEngine::new(bytecode, self.sys_ops.clone(), self.event_sink.clone())
            .map_err(RuntimeError::Engine)?;
        self.set_current_bex(runtime);
        Ok(())
    }
}
