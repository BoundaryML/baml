mod commands;
mod diagnostics;
mod notification;
mod request;
mod wasm_helpers;

use std::{collections::HashMap, io::Read};

use ::std::sync::Arc;
use baml_workspace::{BAML_SRC_DIR, BAML_TOML, find_baml_project_root_from_ancestors};
pub use wasm_helpers::BackgroundSpawner;

/// Factory that creates [`sys_ops::SysOps`] for a given project root.
type SysOpFactory =
    std::sync::Arc<dyn Fn(&vfs::VfsPath) -> std::sync::Arc<sys_ops::SysOps> + Send + Sync>;

use crate::{
    RuntimeError,
    bex_lsp::{
        LspError,
        multi_project::diagnostics::{PositionEncoding, WithDiagnostics},
    },
};

struct LiveProject {
    project: crate::project::BexProject,
    in_memory_changes:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<crate::fs::FsPath, String>>>,
    /// Tracks file paths for which we last published diagnostics, so we can
    /// send an empty publish for files that disappear (deleted) on the next
    /// full refresh.
    last_published_files:
        std::sync::Arc<std::sync::Mutex<std::collections::HashSet<crate::fs::FsPath>>>,
}

#[derive(Clone)]
struct BexMulitProject {
    projects:
        std::sync::Arc<std::sync::Mutex<HashMap<crate::fs::FsPath, std::sync::Arc<LiveProject>>>>,
    sys_op_factory: SysOpFactory,
    sender: std::sync::Arc<dyn LspClientSenderTrait + Send + Sync>,
    playground_sender: std::sync::Arc<dyn crate::bex_lsp::PlaygroundSender>,

    /// The position encoding negotiated with the LSP client.
    /// This is essential for correct character position calculation in files
    /// containing multi-byte UTF-8 characters (like 'é' or emoji).
    position_encoding: PositionEncoding,

    /// Workspace root directories provided by the LSP client during
    /// `initialize`. Used by `on_notification_initialized` to scope
    /// project discovery instead of walking the entire filesystem.
    workspace_roots: std::sync::Arc<std::sync::Mutex<Vec<vfs::VfsPath>>>,

    /// The VFS path to the project root.
    fs: crate::fs::BamlVFS,

    spawner: BackgroundSpawner,
}

pub trait LspClientSenderTrait {
    fn send_notification(&self, msg: lsp_server::Notification) -> Result<(), LspError>;
    fn send_response_impl(&self, msg: lsp_server::Response) -> Result<(), LspError>;
    fn send_response(
        &self,
        id: lsp_server::RequestId,
        msg: Result<serde_json::Value, LspError>,
    ) -> Result<(), LspError> {
        let (result, error) = match msg {
            Err(error) => (None, Some(error)),
            Ok(result) => (Some(result), None),
        };
        let response = lsp_server::Response {
            id,
            result,
            error: error.map(|e| lsp_server::ResponseError {
                code: lsp_server::ErrorCode::UnknownErrorCode as i32,
                message: e.to_string(),
                data: None,
            }),
        };
        self.send_response_impl(response)
    }
    fn make_request(&self, msg: lsp_server::Request) -> Result<(), LspError>;
}

enum ProjectRefreshMode {
    Full,
    InMemoryChangesOnly,
    Only(Vec<vfs::VfsPath>),
}

impl BexMulitProject {
    fn new(
        sys_op_factory: SysOpFactory,
        sender: std::sync::Arc<dyn LspClientSenderTrait + Send + Sync>,
        playground_sender: std::sync::Arc<dyn crate::bex_lsp::PlaygroundSender>,
        fs: crate::fs::BamlVFS,
        spawner: BackgroundSpawner,
    ) -> Self {
        Self {
            projects: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            sys_op_factory,
            sender,
            playground_sender,
            position_encoding: PositionEncoding::UTF8,
            workspace_roots: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            fs,
            spawner,
        }
    }

    fn get_path_from_uri(&self, uri: &lsp_types::Url) -> Result<vfs::VfsPath, LspError> {
        let path = wasm_helpers::to_file_path(uri).map_err(|()| {
            LspError::UnknownErrorCode("Failed to convert URI to path".to_string())
        })?;
        self.fs.get_path_from_path(&path, "get_path_from_uri")
    }

    fn get_or_create_project(
        &self,
        root_path: vfs::VfsPath,
    ) -> Result<std::sync::Arc<LiveProject>, LspError> {
        let mut projects = self.projects.lock().unwrap();
        if !root_path.exists().unwrap_or(false) {
            projects.remove(&crate::fs::FsPath::from_vfs(&root_path));
            return Err(LspError::ProjectNotFound(root_path));
        }

        if let Some(project) = projects.get(&crate::fs::FsPath::from_vfs(&root_path)) {
            return Ok(project.clone());
        }

        let sys_ops = (self.sys_op_factory)(&root_path);
        let project = crate::project::BexProject::new(&root_path, sys_ops);
        let project = std::sync::Arc::new(LiveProject {
            project,
            in_memory_changes: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            last_published_files: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashSet::new(),
            )),
        });
        projects.insert(crate::fs::FsPath::from_vfs(&root_path), project.clone());
        Ok(project)
    }

    fn get_bex_for_project(
        &self,
        project_root: &crate::fs::FsPath,
    ) -> Result<Arc<dyn crate::Bex>, RuntimeError> {
        let project = {
            let projects = self.projects.lock().unwrap();
            projects
                .get(project_root)
                .ok_or(RuntimeError::Compilation {
                    message: format!("Project not found: {}", project_root.as_path().display()),
                })?
                .clone()
        };
        let bex = project.project.get_bex()?;
        Ok(bex)
    }

    /// Resolve the project root using real project markers only: the closest
    /// ancestor with a `baml.toml` or a `baml_src/` directory. This is the
    /// resolver used during workspace discovery, where a lenient fallback
    /// would promote stray `.baml` files into full projects.
    fn get_marked_baml_project_root(path: &vfs::VfsPath) -> Result<vfs::VfsPath, LspError> {
        let start = Self::project_search_start(path);
        find_baml_project_root_from_ancestors(
            vfs_ancestors(start),
            Self::has_baml_toml,
            Self::has_baml_src_dir,
        )
        .ok_or_else(|| {
            LspError::ProjectRootNotFound(path.clone(), "Not a BAML project".to_string())
        })
    }

    fn get_baml_project_root(path: &vfs::VfsPath) -> Result<vfs::VfsPath, LspError> {
        if let Ok(root) = Self::get_marked_baml_project_root(path) {
            return Ok(root);
        }

        // In some special cases, .baml files are treated as their own projects
        // This is only for internal development
        let allow_standalone_baml_file = path.as_str().split('/').any(|p| p == "baml_language");

        if allow_standalone_baml_file
            && path.extension().is_some_and(|e| e.as_str() == "baml")
            && path.is_file().map_err(|e| LspError::InvalidVFSPath {
                path: path.clone(),
                message: format!("Failed to check if path is a file: {e}"),
            })?
        {
            return Ok(path.clone());
        }

        Err(LspError::ProjectRootNotFound(
            path.clone(),
            "Not a BAML project".to_string(),
        ))
    }

    fn load_project_sources(
        &self,
        project_root: &vfs::VfsPath,
    ) -> Result<HashMap<crate::fs::FsPath, String>, LspError> {
        if project_root
            .is_file()
            .map_err(|e| LspError::InvalidVFSPath {
                path: project_root.clone(),
                message: format!("Failed to check if path is a file: {e}"),
            })?
        {
            if project_root
                .extension()
                .is_some_and(|e| e.as_str() == "baml")
            {
                let mut reader =
                    project_root
                        .open_file()
                        .map_err(|e| LspError::InvalidVFSPath {
                            path: project_root.clone(),
                            message: format!("Failed to open file: {e}"),
                        })?;
                let mut bytes = Vec::new();
                reader
                    .read_to_end(&mut bytes)
                    .map_err(|e| LspError::InvalidVFSPath {
                        path: project_root.clone(),
                        message: format!("Failed to read file: {e}"),
                    })?;
                let mut files = HashMap::new();
                files.insert(
                    crate::fs::FsPath::from_vfs(project_root),
                    String::from_utf8(bytes).unwrap_or_default(),
                );
                return Ok(files);
            }
        }

        let source_root = Self::project_source_root(project_root)?;
        let glob = format!("{}/**/*.baml", source_root.as_str());
        let entries = self
            .fs
            .read_many(&glob)
            .map_err(|e| LspError::InvalidVFSPath {
                path: project_root.clone(),
                message: e.to_string(),
            })?;
        let files = entries
            .into_iter()
            .map(|(path, bytes)| {
                let content = String::from_utf8(bytes).unwrap_or_default();
                (crate::fs::FsPath::from_str(path), content)
            })
            .collect();
        Ok(files)
    }

    fn refresh_project(&self, project_root: &vfs::VfsPath, refresh_mode: ProjectRefreshMode) {
        self.refresh_project_async(project_root, refresh_mode);
    }

    fn has_baml_toml(path: &vfs::VfsPath) -> bool {
        path.join(BAML_TOML)
            .ok()
            .and_then(|path| path.is_file().ok())
            .unwrap_or(false)
    }

    fn has_baml_src_dir(path: &vfs::VfsPath) -> bool {
        path.join(BAML_SRC_DIR)
            .ok()
            .and_then(|path| path.is_dir().ok())
            .unwrap_or(false)
    }

    fn project_source_root(project_root: &vfs::VfsPath) -> Result<vfs::VfsPath, LspError> {
        let baml_src = project_root
            .join(BAML_SRC_DIR)
            .map_err(|e| LspError::InvalidVFSPath {
                path: project_root.clone(),
                message: format!("Failed to join path: {e}"),
            })?;
        if baml_src.is_dir().unwrap_or(false) {
            Ok(baml_src)
        } else {
            Ok(project_root.clone())
        }
    }

    fn project_search_start(path: &vfs::VfsPath) -> vfs::VfsPath {
        if path.filename().as_str() == BAML_TOML
            || path.extension().is_some_and(|ext| ext.as_str() == "baml")
            || path.is_file().unwrap_or(false)
        {
            path.parent()
        } else {
            path.clone()
        }
    }

    fn discover_workspace_projects(&self, workspace_roots: &[vfs::VfsPath]) -> Vec<vfs::VfsPath> {
        workspace_roots.clone_into(&mut self.workspace_roots.lock().unwrap());

        if workspace_roots.is_empty() {
            tracing::warn!(
                "No workspace roots provided during initialize — skipping project discovery"
            );
            return Vec::new();
        }

        let mut project_roots = Vec::new();
        for root in workspace_roots {
            if root.is_file().unwrap_or(false)
                && root.extension().is_some_and(|e| e.as_str() == "baml")
            {
                project_roots.push(root.clone());
                continue;
            }

            // The workspace folder itself may live inside a project
            // (e.g. the user opened `baml_src/` or a subdirectory).
            if let Ok(pr) = Self::get_marked_baml_project_root(root) {
                project_roots.push(pr);
            }

            project_roots.extend(self.collect_marked_project_roots(root));
        }

        project_roots.sort_by_key(|path| path.as_str().to_string());
        project_roots.dedup_by(|a, b| a.as_str() == b.as_str());
        let manifest_roots = project_roots
            .iter()
            .filter(|path| Self::has_baml_toml(path))
            .map(|path| path.as_str().trim_end_matches('/').to_string())
            .collect::<Vec<_>>();
        project_roots.retain(|candidate| {
            if Self::has_baml_toml(candidate) {
                return true;
            }
            let candidate = candidate.as_str().trim_end_matches('/');
            !manifest_roots.iter().any(|manifest_root| {
                candidate != manifest_root
                    && candidate
                        .strip_prefix(manifest_root)
                        .is_some_and(|suffix| suffix.starts_with('/'))
            })
        });

        tracing::info!("Discovered {} BAML project(s)", project_roots.len());

        for project_root in &project_roots {
            let Ok(_) = self.get_or_create_project(project_root.clone()) else {
                continue;
            };
            self.refresh_project(project_root, ProjectRefreshMode::Full);
        }

        project_roots
    }

    /// Directories that are never descended into during workspace discovery,
    /// even when a workspace has no `.gitignore` to prune them. Mirrors
    /// `should_skip_poll_dir` in `baml_lsp_server`.
    fn should_skip_discovery_dir(name: &str) -> bool {
        name.starts_with('.') || matches!(name, "node_modules" | "target" | "dist")
    }

    /// Recursively find directories that are project roots by real markers
    /// (`baml.toml` file or `baml_src/` child).
    ///
    /// On native this walks with the `ignore` crate (like ruff), so
    /// `.gitignore`d directories (`target/`, `node_modules/`, build output)
    /// are pruned before descending; `should_skip_discovery_dir` is a
    /// backstop for workspaces that are not git repositories.
    fn collect_marked_project_roots(&self, root: &vfs::VfsPath) -> Vec<vfs::VfsPath> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Native VfsPaths are OS paths joined onto the filesystem root, so
            // the OS-level walker applies whenever the path really exists on
            // disk. Fall back to the VFS walk otherwise (e.g. in-memory
            // filesystems in tests).
            let os_root = std::path::Path::new(root.as_str());
            if os_root.is_dir() {
                return self.collect_marked_project_roots_native(os_root);
            }
        }
        let mut found = Vec::new();
        Self::collect_marked_project_roots_vfs(root, &mut found);
        found
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn collect_marked_project_roots_native(&self, os_root: &std::path::Path) -> Vec<vfs::VfsPath> {
        let mut found = Vec::new();
        for dir in Self::scan_marked_project_roots_native(os_root) {
            match self
                .fs
                .get_path_from_path(&dir, "discover_workspace_projects")
            {
                Ok(vfs_path) => found.push(vfs_path),
                Err(e) => tracing::warn!("Skipping discovered project root: {e}"),
            }
        }
        found
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn scan_marked_project_roots_native(os_root: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut found = Vec::new();
        let walker = ignore::WalkBuilder::new(os_root)
            .standard_filters(true)
            .follow_links(false)
            .filter_entry(|entry| {
                let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
                !(is_dir
                    && entry
                        .file_name()
                        .to_str()
                        .is_some_and(Self::should_skip_discovery_dir))
            })
            .build();
        for entry in walker.filter_map(Result::ok) {
            if !entry.file_type().is_some_and(|t| t.is_dir()) {
                continue;
            }
            let dir = entry.path();
            if dir.join(BAML_TOML).is_file() || dir.join(BAML_SRC_DIR).is_dir() {
                found.push(dir.to_path_buf());
            }
        }
        found
    }

    fn collect_marked_project_roots_vfs(root: &vfs::VfsPath, found: &mut Vec<vfs::VfsPath>) {
        if Self::has_baml_toml(root) || Self::has_baml_src_dir(root) {
            found.push(root.clone());
        }
        let Ok(entries) = root.read_dir() else {
            return;
        };
        for entry in entries {
            if Self::should_skip_discovery_dir(&entry.filename()) {
                continue;
            }
            if entry.is_dir().unwrap_or(false) {
                Self::collect_marked_project_roots_vfs(&entry, found);
            }
        }
    }

    fn refresh_project_async(&self, project_root: &vfs::VfsPath, refresh_mode: ProjectRefreshMode) {
        use crate::bex_lsp::notification::BexLspNotification;
        let mode_label = match &refresh_mode {
            ProjectRefreshMode::Full => "Full",
            ProjectRefreshMode::InMemoryChangesOnly => "InMemoryChangesOnly",
            ProjectRefreshMode::Only(_) => "Only",
        };
        tracing::debug!(
            "refresh_project({}, mode={})",
            project_root.as_str(),
            mode_label
        );

        let Ok(project) = self.get_or_create_project(project_root.to_owned()) else {
            return;
        };

        let is_full_refresh = matches!(refresh_mode, ProjectRefreshMode::Full);
        match refresh_mode {
            ProjectRefreshMode::Full => {
                tracing::debug!("  loading sources from disk...");
                let mut sources = match self.load_project_sources(project_root) {
                    Ok(sources) => sources,
                    Err(e) => {
                        let _ = self.send_notification_show_message(lsp_types::ShowMessageParams {
                            typ: lsp_types::MessageType::ERROR,
                            message: format!(
                                "Failed to read project files for {project_root:?}: {e}"
                            ),
                        });
                        return;
                    }
                };
                tracing::debug!("  loaded {} source file(s)", sources.len());

                {
                    let in_memory_changes = project.in_memory_changes.lock().unwrap();
                    for (path, source) in in_memory_changes.iter() {
                        sources.insert(path.clone(), source.clone());
                    }
                }

                let project = &project.project;

                tracing::debug!("  update_all_sources...");
                project.update_all_sources(&sources);
                tracing::debug!("  update_all_sources done");
            }
            ProjectRefreshMode::InMemoryChangesOnly => {
                let in_memory_changes = project.in_memory_changes.lock().unwrap();
                let sources = in_memory_changes
                    .iter()
                    .map(|(path, source)| (path.clone(), source.clone()))
                    .collect();
                drop(in_memory_changes);

                let project = &project.project;
                project.update_some_sources(&sources);
            }
            ProjectRefreshMode::Only(paths) => {
                // TODO: make this smarter and only read that the required files, instead of reading all files
                let mut sources = match self.load_project_sources(project_root) {
                    Ok(sources) => sources,
                    Err(e) => {
                        let _ = self.send_notification_show_message(lsp_types::ShowMessageParams {
                            typ: lsp_types::MessageType::ERROR,
                            message: format!(
                                "Failed to read project files for {project_root:?}: {e}"
                            ),
                        });
                        return;
                    }
                };

                {
                    let in_memory_changes = project.in_memory_changes.lock().unwrap();
                    for (path, source) in in_memory_changes.iter() {
                        sources.insert(path.clone(), source.clone());
                    }
                }

                let sources = paths
                    .into_iter()
                    .filter_map(|path| {
                        let key = crate::fs::FsPath::from_vfs(&path);
                        sources.remove(&key).map(|source| (key, source))
                    })
                    .collect();

                let project = &project.project;
                project.update_some_sources(&sources);
            }
        }

        tracing::debug!("  computing diagnostics...");
        let diagnostics = project.project.diagnostics_by_file(self.position_encoding);
        tracing::debug!("  diagnostics computed for {} file(s)", diagnostics.len());

        // Always publish diagnostics for every file the compiler knows about
        // (including files with 0 diagnostics, to clear stale markers).
        let current_paths: std::collections::HashSet<crate::fs::FsPath> = diagnostics
            .keys()
            .map(|p| crate::fs::FsPath::from_str(p.to_string_lossy().into_owned()))
            .collect();

        for path in &current_paths {
            let file_diagnostics = diagnostics.get(path.as_path()).cloned().unwrap_or_default();
            let Ok(uri) = wasm_helpers::from_file_path(path.as_path()) else {
                continue;
            };
            let _ = self.send_notification_publish_diagnostics(
                lsp_types::PublishDiagnosticsParams::new(uri, file_diagnostics, None),
            );
        }

        // On a full refresh, also clear diagnostics for files that no longer
        // exist (deleted since the last refresh).
        if is_full_refresh {
            let mut prev = project.last_published_files.lock().unwrap();
            for deleted in prev.difference(&current_paths) {
                let Ok(uri) = wasm_helpers::from_file_path(deleted.as_path()) else {
                    continue;
                };
                let _ = self.send_notification_publish_diagnostics(
                    lsp_types::PublishDiagnosticsParams::new(uri, vec![], None),
                );
            }
            *prev = current_paths;
        }

        let flat_diags = Self::flatten_diagnostics(&diagnostics);

        self.send_list_projects();
        self.send_update_project(project_root, &project, flat_diags);

        // Auto-trigger runtime test collection when BexEngine is ready
        if project.project.is_bex_current() {
            self.request_collect_tests_impl(project_root.as_str());
        }

        tracing::debug!("refresh_project done");
    }

    fn flatten_diagnostics(
        diagnostics: &std::collections::HashMap<std::path::PathBuf, Vec<lsp_types::Diagnostic>>,
    ) -> Vec<crate::bex_lsp::ProjectDiagnostic> {
        let mut out = Vec::new();
        for (path, diags) in diagnostics {
            let filename = path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            for d in diags {
                let severity = match d.severity {
                    Some(lsp_types::DiagnosticSeverity::ERROR) => "error",
                    Some(lsp_types::DiagnosticSeverity::WARNING) => "warning",
                    _ => "info",
                };
                let line = d.range.start.line + 1;
                out.push(crate::bex_lsp::ProjectDiagnostic {
                    severity,
                    message: format!("{filename}:{line}: {}", d.message),
                });
            }
        }
        out.sort_by(|a, b| a.message.cmp(&b.message));
        out
    }

    fn build_project_update(
        project: &LiveProject,
        diagnostics: Vec<crate::bex_lsp::ProjectDiagnostic>,
    ) -> crate::bex_lsp::ProjectUpdate {
        let is_bex_current = project.project.is_bex_current();

        let db_guard = project.project.db.lock().unwrap();
        let db = db_guard.db();
        let functions = baml_project::list_functions_with_metadata(db)
            .into_iter()
            .map(|f| crate::bex_lsp::FunctionInfo {
                name: f.name,
                kind: if f.is_llm {
                    crate::bex_lsp::FunctionKind::Llm
                } else {
                    crate::bex_lsp::FunctionKind::Expr
                },
                origin: f.origin.into(),
                capabilities: if f.is_llm {
                    Some(crate::bex_lsp::LlmCapabilities {
                        render_prompt: true,
                        build_request: true,
                        client_name: f.client_name,
                    })
                } else {
                    None
                },
            })
            .collect();

        crate::bex_lsp::ProjectUpdate {
            is_bex_current,
            functions,
            diagnostics,
        }
    }

    fn send_list_projects(&self) {
        let projects = self.projects.lock().unwrap();
        let roots: Vec<String> = projects
            .keys()
            .map(|p| p.as_path().to_string_lossy().into_owned())
            .collect();
        self.playground_sender.send_playground_notification(
            crate::bex_lsp::PlaygroundNotification::ListProjects { projects: roots },
        );
    }

    fn send_update_project(
        &self,
        project_root: &vfs::VfsPath,
        project: &LiveProject,
        diagnostics: Vec<crate::bex_lsp::ProjectDiagnostic>,
    ) {
        let update = Self::build_project_update(project, diagnostics);
        self.playground_sender.send_playground_notification(
            crate::bex_lsp::PlaygroundNotification::UpdateProject {
                project: project_root.as_str().to_string(),
                update,
            },
        );
    }

    fn request_collect_tests_impl(&self, project_root_str: &str) {
        log::info!("[request_collect_tests_impl] project={project_root_str}");
        // Resolve project and get the concrete BexEngine (before trait erasure)
        let (engine, test_state) = {
            let projects = self.projects.lock().unwrap();
            let project = projects
                .iter()
                .find(|(k, _)| k.as_path().to_string_lossy() == project_root_str)
                .map(|(_, v)| v.clone());
            let Some(project) = project else {
                return;
            };
            let Ok(engine) = project.project.get_bex() else {
                return;
            };
            (engine, project.project.test_state())
        };

        // Cancel in-flight collection tasks and clear stale registry. The
        // generation tracks compiled project snapshots and is bumped only when a
        // new BexEngine/CFG snapshot is installed.
        let (generation, cancel) = {
            let mut state = test_state.lock().unwrap();
            state.cancel.cancel();
            state.cancel = sys_types::CancellationToken::new();
            state.registry = None;
            (state.generation, state.cancel.clone())
        };

        let sender = self.playground_sender.clone();
        let project = project_root_str.to_string();
        let package = "user".to_string();
        let call_id = sys_types::CallId::next();

        // Spawn async collection task
        self.spawner.spawn(async move {
            match engine
                .collect_tests(&package, call_id, cancel.clone())
                .await
            {
                Ok(registry) => {
                    // Discard stale results if the engine was swapped during collection.
                    // The guard is scoped to this block so it is dropped before the await below.
                    let should_continue = {
                        let mut state = test_state.lock().unwrap();
                        if state.generation != generation {
                            log::info!(
                                "[collect_tests] discarding stale result (gen {generation} vs current {})",
                                state.generation
                            );
                            false
                        } else {
                            // Extract Handle from BexExternalValue::Handle.
                            // Null means the project has no tests ($init_test absent).
                            let handle = match &registry {
                                bex_engine::BexExternalValue::Handle(h) => Some(h.clone()),
                                bex_engine::BexExternalValue::Null => None,
                                _ => {
                                    log::error!("[collect_tests] unexpected result type");
                                    return;
                                }
                            };
                            state.registry = handle;
                            true
                        }
                    };
                    if !should_continue {
                        return;
                    }

                    // If the project has no tests, send an empty test tree.
                    if matches!(registry, bex_engine::BexExternalValue::Null) {
                        sender.send_playground_notification(
                            crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                                project,
                                generation,
                                call_id: call_id.0,
                                data: serde_json::to_vec(&serde_json::json!([]))
                                    .unwrap_or_default(),
                                expand_error: None,
                            },
                        );
                        return;
                    }

                    // Serialize the full test tree via TestRegistry.serialize
                    let ctx = bex_engine::FunctionCallContextBuilder::new(call_id)
                        .with_cancel_token(cancel)
                        .with_profile_enabled(false)
                        .build();
                    match engine
                        .call_function(
                            "testing.TestRegistry.serialize",
                            vec![registry],
                            ctx,
                            true, // deep copy for wire
                        )
                        .await
                    {
                        Ok(serialized) => {
                            let data = serde_json::to_vec(&bex_value_to_json(&serialized))
                                .unwrap_or_default();
                            sender.send_playground_notification(
                                crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                                    project,
                                    generation,
                                    call_id: call_id.0,
                                    data,
                                    expand_error: None,
                                },
                            );
                        }
                        Err(e) => {
                            log::error!("[collect_tests] serialize failed: {e}");
                            sender.send_playground_notification(
                                crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                                    project: project.clone(),
                                    generation,
                                    call_id: call_id.0,
                                    data: serde_json::to_vec(&serde_json::json!([]))
                                        .unwrap_or_default(),
                                    expand_error: None,
                                },
                            );
                        }
                    }
                }
                Err(e) => {
                    // Collection failed — notify the frontend with an empty result so it unblocks
                    log::error!("[collect_tests] collect_tests failed: {e}");
                    sender.send_playground_notification(
                        crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                            project,
                            generation,
                            call_id: call_id.0,
                            data: serde_json::to_vec(&serde_json::json!([])).unwrap_or_default(),
                            expand_error: None,
                        },
                    );
                }
            }
        });
    }

    async fn call_test_function_impl(
        &self,
        project_root_str: &str,
        generation: u64,
        test_name: &str,
        ctx: bex_engine::FunctionCallContext,
    ) -> Result<bex_engine::BexCallResult, bex_engine::EngineError> {
        let (engine, registry_value) = {
            let projects = self.projects.lock().unwrap();
            let project = projects
                .iter()
                .find(|(k, _)| k.as_path().to_string_lossy() == project_root_str)
                .map(|(_, v)| v.clone())
                .ok_or_else(|| bex_engine::EngineError::FunctionNotFound {
                    name: format!("project not found: {project_root_str}"),
                })?;
            let engine = project.project.get_bex().map_err(|e| {
                bex_engine::EngineError::FunctionNotFound {
                    name: format!("engine not ready: {e}"),
                }
            })?;
            let test_state = project.project.test_state();
            let state = test_state.lock().unwrap();
            if state.generation != generation {
                return Err(bex_engine::EngineError::FunctionNotFound {
                    name: "stale generation".to_string(),
                });
            }
            let registry_value = match &state.registry {
                Some(handle) => bex_engine::BexExternalValue::Handle(handle.clone()),
                None => {
                    return Err(bex_engine::EngineError::FunctionNotFound {
                        name: "no test registry".to_string(),
                    });
                }
            };
            (engine, registry_value)
        };

        log::info!("[call_test_function] test_name={test_name} generation={generation}");

        let result = engine
            .call_function_with_trace(
                "testing.TestRegistry.run_test",
                vec![
                    registry_value,
                    bex_engine::BexExternalValue::String(test_name.into()),
                ],
                ctx,
                true, // deep copy TestReport for wire
            )
            .await;

        match &result {
            Ok(_) => log::info!("[call_test_function] test_name={test_name} succeeded"),
            Err(e) => log::error!("[call_test_function] test_name={test_name} failed: {e}"),
        }

        result
    }

    fn expand_test_set_impl(&self, project_root_str: &str, generation: u64, testset_name: &str) {
        let (engine, registry_value, cancel) = {
            let projects = self.projects.lock().unwrap();
            let Some(project) = projects
                .iter()
                .find(|(k, _)| k.as_path().to_string_lossy() == project_root_str)
                .map(|(_, v)| v.clone())
            else {
                return;
            };
            let Ok(engine) = project.project.get_bex() else {
                return;
            };
            let test_state = project.project.test_state();
            let state = test_state.lock().unwrap();
            if state.generation != generation {
                return;
            }
            let registry_value = match &state.registry {
                Some(handle) => bex_engine::BexExternalValue::Handle(handle.clone()),
                None => return,
            };
            let cancel = state.cancel.clone();
            (engine, registry_value, cancel)
        };

        let call_id = sys_types::CallId::next();
        let sender = self.playground_sender.clone();
        let project = project_root_str.to_string();
        let name = testset_name.to_string();

        self.spawner.spawn(async move {
            let ctx = bex_engine::FunctionCallContextBuilder::new(call_id)
                .with_cancel_token(cancel.clone())
                .with_profile_enabled(false)
                .build();

            // Expand — mutates registry.expansions in-place on the heap
            log::info!("[expand_test_set] expanding testset: {name}");
            if let Err(e) = engine
                .call_function(
                    "testing.TestRegistry.expand_set",
                    vec![
                        registry_value.clone(),
                        bex_engine::BexExternalValue::String(name.as_str().into()),
                    ],
                    ctx,
                    true,
                )
                .await
            {
                log::error!("[expand_test_set] expand failed for testset '{name}': {e}");
                // Re-serialize and send the current (pre-expansion) state so the
                // UI unblocks from the loading spinner instead of spinning forever.
                let ctx_resend =
                    bex_engine::FunctionCallContextBuilder::new(sys_types::CallId::next())
                        .with_cancel_token(cancel)
                        .with_profile_enabled(false)
                        .build();
                let data = match engine
                    .call_function(
                        "testing.TestRegistry.serialize",
                        vec![registry_value],
                        ctx_resend,
                        true,
                    )
                    .await
                {
                    Ok(serialized) => {
                        serde_json::to_vec(&bex_value_to_json(&serialized)).unwrap_or_default()
                    }
                    Err(serialize_err) => {
                        log::error!(
                            "[expand_test_set] serialize after failed expand for '{name}' also failed: {serialize_err}"
                        );
                        serde_json::to_vec(&serde_json::json!([])).unwrap_or_default()
                    }
                };
                sender.send_playground_notification(
                    crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                        project,
                        generation,
                        call_id: call_id.0,
                        data,
                        expand_error: Some(crate::bex_lsp::TestExpandError {
                            testset_name: name.clone(),
                            message: format!("{e}"),
                        }),
                    },
                );
                return;
            }
            log::info!("[expand_test_set] expanded testset '{name}' successfully");

            // Re-serialize full state
            let ctx2 = bex_engine::FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_cancel_token(cancel)
                .with_profile_enabled(false)
                .build();
            match engine
                .call_function(
                    "testing.TestRegistry.serialize",
                    vec![registry_value],
                    ctx2,
                    true,
                )
                .await
            {
                Ok(serialized) => {
                    let data =
                        serde_json::to_vec(&bex_value_to_json(&serialized)).unwrap_or_default();
                    sender.send_playground_notification(
                        crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                            project,
                            generation,
                            call_id: call_id.0,
                            data,
                            expand_error: None,
                        },
                    );
                }
                Err(e) => {
                    log::error!("[expand_test_set] serialize after expanding '{name}' failed: {e}");
                    // Send empty result so the UI unblocks
                    sender.send_playground_notification(
                        crate::bex_lsp::PlaygroundNotification::TestCollectionResult {
                            project,
                            generation,
                            call_id: call_id.0,
                            data: serde_json::to_vec(&serde_json::json!([])).unwrap_or_default(),
                            expand_error: None,
                        },
                    );
                }
            }
        });
    }
}

/// Convert a `BexExternalValue` to a `serde_json::Value` for serialization.
///
/// Only handles the primitive/structural variants that appear in test reports.
/// Handles, ADTs, and function refs are serialized as null.
fn bex_value_to_json(v: &bex_engine::BexExternalValue) -> serde_json::Value {
    match v {
        bex_engine::BexExternalValue::Null => serde_json::Value::Null,
        bex_engine::BexExternalValue::Int(i) => serde_json::json!(i),
        // Bigints can exceed JSON number precision; emit as a decimal string.
        bex_engine::BexExternalValue::Bigint(b) => serde_json::json!(b.to_string()),
        bex_engine::BexExternalValue::Float(f) => serde_json::json!(f),
        bex_engine::BexExternalValue::Bool(b) => serde_json::json!(b),
        bex_engine::BexExternalValue::String(s) => serde_json::json!(s.as_str()),
        bex_engine::BexExternalValue::Array { items, .. } => {
            serde_json::Value::Array(items.iter().map(bex_value_to_json).collect())
        }
        bex_engine::BexExternalValue::Map { entries, .. } => {
            let mut map = serde_json::Map::new();
            for (k, v) in entries {
                map.insert(k.clone(), bex_value_to_json(v));
            }
            serde_json::Value::Object(map)
        }
        bex_engine::BexExternalValue::Instance {
            class_name, fields, ..
        } => {
            let mut map = serde_json::Map::new();
            map.insert("$type".to_string(), serde_json::json!(class_name));
            for (k, v) in fields {
                map.insert(k.clone(), bex_value_to_json(v));
            }
            serde_json::Value::Object(map)
        }
        bex_engine::BexExternalValue::Variant {
            enum_name,
            variant_name,
        } => {
            serde_json::json!({ "$enum": enum_name, "value": variant_name })
        }
        bex_engine::BexExternalValue::Union { value, .. } => bex_value_to_json(value),
        _ => serde_json::Value::Null,
    }
}

fn relative_source_path(project_root: &vfs::VfsPath, path: &crate::fs::FsPath) -> String {
    let root_path = std::path::Path::new(project_root.as_str());
    let path = path.as_path();
    if path == root_path {
        return path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
    }
    path.strip_prefix(root_path)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn vfs_ancestors(start: vfs::VfsPath) -> impl Iterator<Item = vfs::VfsPath> {
    let mut ancestors = Vec::new();
    let mut current = start;
    loop {
        ancestors.push(current.clone());
        if current.is_root() {
            break;
        }
        let parent = current.parent();
        if parent.as_str() == current.as_str() {
            break;
        }
        current = parent;
    }
    ancestors.into_iter()
}

fn resolve_source_path_for_project(
    project_root: &vfs::VfsPath,
    path: &str,
) -> Result<vfs::VfsPath, LspError> {
    let raw = std::path::Path::new(path);
    if raw.is_absolute() {
        return Err(LspError::InvalidVFSPath {
            path: project_root.clone(),
            message: format!("Expected a project-relative source path, got {path}"),
        });
    }

    if raw
        .components()
        .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(LspError::InvalidVFSPath {
            path: project_root.clone(),
            message: format!("Unsafe relative source path: {path}"),
        });
    }

    if project_root.is_file().unwrap_or(false) {
        return Ok(project_root.clone());
    }

    project_root
        .join(path)
        .map_err(|e| LspError::InvalidVFSPath {
            path: project_root.clone(),
            message: format!("Failed to join path: {e}"),
        })
}

fn ensure_source_belongs_to_project(
    project_root: &vfs::VfsPath,
    source_path: &vfs::VfsPath,
) -> Result<(), LspError> {
    let expected_root;
    if project_root.is_file().unwrap_or(false) {
        if source_path.as_str() == project_root.as_str() {
            return Ok(());
        }
        expected_root = project_root.as_str().to_string();
    } else {
        let source_root = BexMulitProject::project_source_root(project_root)?;
        expected_root = source_root.as_str().to_string();
        let root = source_root.as_str().trim_end_matches('/');
        let source = source_path.as_str();
        if source == root
            || source
                .strip_prefix(root)
                .is_some_and(|suffix| suffix.starts_with('/'))
        {
            return Ok(());
        }
    }

    Err(LspError::InvalidVFSPath {
        path: source_path.clone(),
        message: format!("Source file is outside project source root {expected_root}"),
    })
}

#[async_trait::async_trait]
impl super::BexLsp for BexMulitProject {
    fn get_bex_for_project(
        &self,
        project_root: &crate::fs::FsPath,
    ) -> Result<Arc<dyn crate::Bex>, crate::RuntimeError> {
        self.get_bex_for_project(project_root)
    }

    fn all_env_var_names(&self) -> Vec<String> {
        let projects = self.projects.lock().unwrap();
        let mut names = std::collections::BTreeSet::new();
        for project in projects.values() {
            let db_guard = project.project.db.lock().unwrap();
            let db = db_guard.db();
            for name in baml_lsp2_actions::all_env_var_names(db) {
                names.insert(name);
            }
        }
        names.into_iter().collect()
    }

    fn playground_source_files(
        &self,
        project: &str,
    ) -> Result<Vec<crate::bex_lsp::PlaygroundSourceFile>, LspError> {
        let project_root = self
            .fs
            .get_path_from_path(std::path::Path::new(project), "playground source files")?;
        let project_handle = self.get_or_create_project(project_root.clone())?;
        let mut sources = self.load_project_sources(&project_root)?;
        {
            let in_memory_changes = project_handle.in_memory_changes.lock().unwrap();
            for (path, source) in in_memory_changes.iter() {
                sources.insert(path.clone(), source.clone());
            }
        }

        let mut files = sources
            .into_iter()
            .map(|(path, content)| {
                let relative_path = relative_source_path(&project_root, &path);
                crate::bex_lsp::PlaygroundSourceFile {
                    path: path.as_path().to_string_lossy().into_owned(),
                    relative_path,
                    content,
                }
            })
            .collect::<Vec<_>>();
        files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        Ok(files)
    }

    fn playground_update_source_file(
        &self,
        project: &str,
        path: &str,
        content: String,
    ) -> Result<(), LspError> {
        let project_root = self.fs.get_path_from_path(
            std::path::Path::new(project),
            "playground update source file",
        )?;
        let raw_path = std::path::Path::new(path);
        let source_path = if raw_path.is_absolute() {
            self.fs
                .get_path_from_path(raw_path, "playground update source file path")?
        } else {
            resolve_source_path_for_project(&project_root, path)?
        };
        if source_path.extension().is_none_or(|e| e.as_str() != "baml") {
            return Err(LspError::InvalidVFSPath {
                path: source_path,
                message: "Only .baml files can be edited from the playground".to_string(),
            });
        }
        ensure_source_belongs_to_project(&project_root, &source_path)?;

        let project_handle = self.get_or_create_project(project_root.clone())?;
        let mut in_memory_changes = project_handle.in_memory_changes.lock().unwrap();
        in_memory_changes.insert(crate::fs::FsPath::from_vfs(&source_path), content);
        drop(in_memory_changes);

        self.refresh_project(&project_root, ProjectRefreshMode::InMemoryChangesOnly);
        Ok(())
    }

    fn initialize_workspace_roots(
        &self,
        roots: Vec<std::path::PathBuf>,
    ) -> Result<Vec<String>, LspError> {
        let roots = roots
            .into_iter()
            .map(|root| self.fs.get_path_from_path(&root, "lsp --workspace"))
            .collect::<Result<Vec<_>, _>>()?;
        let projects = self.discover_workspace_projects(&roots);
        Ok(projects
            .into_iter()
            .map(|project| project.as_str().to_string())
            .collect())
    }

    fn request_playground_state(&self) {
        self.send_list_projects();
        let projects = self.projects.lock().unwrap();
        for (fs_path, project) in projects.iter() {
            let root_str = fs_path.as_path().to_string_lossy().into_owned();
            let diags_by_file = project.project.diagnostics_by_file(self.position_encoding);
            let flat_diags = Self::flatten_diagnostics(&diags_by_file);
            let update = Self::build_project_update(project, flat_diags);
            self.playground_sender.send_playground_notification(
                crate::bex_lsp::PlaygroundNotification::UpdateProject {
                    project: root_str,
                    update,
                },
            );
        }
    }

    fn ast_control_flow_graph(
        &self,
        function_name: &str,
    ) -> Option<baml_compiler2_visualization::control_flow::ControlFlowGraph> {
        let projects = self.projects.lock().ok()?;
        for project in projects.values() {
            let db = project.project.db.lock().ok()?;
            if let Some(graph) = db.ast_control_flow_graph(function_name) {
                return Some(graph);
            }
        }
        None
    }

    fn project_generation(&self, project_root: &str) -> Option<u64> {
        let projects = self.projects.lock().ok()?;
        projects
            .iter()
            .find(|(path, _)| path.as_path().to_string_lossy() == project_root)
            .map(|(_, project)| project.project.current_generation())
    }

    fn control_flow_graph_for_generation(
        &self,
        project_root: &str,
        generation: u64,
        function_name: &str,
    ) -> Option<std::sync::Arc<baml_compiler2_visualization::control_flow::ControlFlowGraph>> {
        // Clone the project handle out of the registry lock: building a
        // missing graph takes the project's database lock, which must not be
        // held while the registry lock is.
        let project = {
            let projects = self.projects.lock().ok()?;
            projects
                .iter()
                .find(|(path, _)| path.as_path().to_string_lossy() == project_root)
                .map(|(_, project)| project.clone())?
        };
        project
            .project
            .control_flow_graph_for_generation(generation, function_name)
    }

    fn request_control_flow_graph(&self, function_name: &str) {
        let graph = self.ast_control_flow_graph(function_name);
        let graph = graph.map(|g| {
            baml_compiler2_visualization::control_flow::prepare_control_flow_graph_for_visualization(&g)
        });
        let graph_json = graph.as_ref().and_then(|g| serde_json::to_value(g).ok());
        self.playground_sender.send_playground_notification(
            crate::bex_lsp::PlaygroundNotification::ControlFlowGraphResult {
                function_name: function_name.to_string(),
                graph: graph_json,
            },
        );
    }

    fn playground_cursor_context(
        &self,
        file_path: &str,
        line: u32,
        column: u32,
    ) -> baml_project::CursorContext {
        let empty = baml_project::CursorContext {
            function_name: None,
            is_workflow: false,
            workflow_memberships: vec![],
            source_expr_id: None,
            source_expr_candidates: vec![],
            source_expr_function_name: None,
            test_name: None,
            cursor_offset: None,
        };

        let Ok(projects) = self.projects.lock() else {
            return empty;
        };

        for project in projects.values() {
            let Ok(db) = project.project.db.lock() else {
                continue;
            };

            // Convert line/column to byte offset using the source file text.
            // The file_path from Monaco may be relative — find matching file.
            let Some(source_file) = db.find_source_file(file_path) else {
                continue;
            };

            let text: &str = source_file.text(&*db);
            let position = lsp_types::Position {
                line,
                character: column,
            };
            let byte_offset = u32::try_from(baml_project::position::lsp_position_to_offset(
                text, &position,
            ))
            .unwrap_or(0);

            return db.playground_cursor_context(file_path, byte_offset);
        }

        empty
    }

    fn request_cursor_context(&self, file_path: &str, line: u32, column: u32) {
        let ctx = self.playground_cursor_context(file_path, line, column);
        let ctx_json = serde_json::to_value(&ctx).unwrap_or(serde_json::Value::Null);
        self.playground_sender.send_playground_notification(
            crate::bex_lsp::PlaygroundNotification::CursorContext { context: ctx_json },
        );
    }

    fn request_collect_tests(&self, project: &str) {
        self.request_collect_tests_impl(project);
    }

    async fn call_test_function(
        &self,
        project: &str,
        generation: u64,
        test_name: &str,
        ctx: bex_engine::FunctionCallContext,
    ) -> Result<bex_engine::BexExternalValue, bex_engine::EngineError> {
        self.call_test_function_impl(project, generation, test_name, ctx)
            .await
            .and_then(|result| result.value)
    }

    async fn call_test_function_with_trace(
        &self,
        project: &str,
        generation: u64,
        test_name: &str,
        ctx: bex_engine::FunctionCallContext,
    ) -> Result<bex_engine::BexCallResult, bex_engine::EngineError> {
        self.call_test_function_impl(project, generation, test_name, ctx)
            .await
    }

    fn expand_test_set(&self, project: &str, generation: u64, testset_name: &str) {
        self.expand_test_set_impl(project, generation, testset_name);
    }

    fn resolve_file_id(&self, file_id: u32) -> Option<String> {
        let projects = self.projects.lock().unwrap();
        for project in projects.values() {
            let db = project.project.db.lock().unwrap();
            if let Some(path) = db.file_id_to_path(baml_base::FileId::new(file_id)) {
                return Some(path.to_string_lossy().to_string());
            }
        }
        None
    }
}

pub fn new_lsp(
    sys_op_factory: SysOpFactory,
    sender: std::sync::Arc<dyn LspClientSenderTrait + Send + Sync>,
    playground_sender: std::sync::Arc<dyn crate::bex_lsp::PlaygroundSender>,
    fs: crate::fs::BamlVFS,
    spawner: BackgroundSpawner,
) -> impl crate::bex_lsp::BexLsp {
    BexMulitProject::new(sys_op_factory, sender, playground_sender, fs, spawner)
}

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;

    struct TempWorkspace {
        root: std::path::PathBuf,
    }

    impl TempWorkspace {
        fn new(tag: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("bex_discovery_{}_{}", tag, std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn file(&self, rel: &str, contents: &str) {
            let path = self.root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }

        fn dir(&self, rel: &str) {
            std::fs::create_dir_all(self.root.join(rel)).unwrap();
        }

        fn vfs_path(&self, rel: &str) -> vfs::VfsPath {
            let abs = self.root.join(rel);
            vfs::VfsPath::new(vfs::PhysicalFS::new("/"))
                .join(abs.to_string_lossy())
                .unwrap()
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn standalone_baml_file_is_not_promoted_by_strict_resolver() {
        let ws = TempWorkspace::new("standalone_baml_language");
        // Path contains a `baml_language` segment, triggering the lenient
        // internal-dev fallback.
        ws.file("baml_language/case.baml", "// standalone");
        let file = ws.vfs_path("baml_language/case.baml");

        let lenient = BexMulitProject::get_baml_project_root(&file).unwrap();
        assert_eq!(lenient.as_str(), file.as_str());

        let strict = BexMulitProject::get_marked_baml_project_root(&file);
        assert!(matches!(strict, Err(LspError::ProjectRootNotFound(..))));
    }

    #[test]
    fn strict_resolver_finds_marked_project_root() {
        let ws = TempWorkspace::new("marked_root");
        ws.file("proj/baml_src/main.baml", "// main");
        let file = ws.vfs_path("proj/baml_src/main.baml");

        let root = BexMulitProject::get_marked_baml_project_root(&file).unwrap();
        assert_eq!(root.as_str(), ws.vfs_path("proj").as_str());
    }

    #[test]
    fn native_scan_skips_generated_and_hidden_dirs() {
        let ws = TempWorkspace::new("scan_skips");
        ws.file("proj/baml_src/main.baml", "// main");
        ws.dir("target/junk/baml_src");
        ws.dir("node_modules/pkg/baml_src");
        ws.dir(".hidden/baml_src");

        let found = BexMulitProject::scan_marked_project_roots_native(&ws.root);
        assert_eq!(
            found,
            vec![ws.root.join("proj")],
            "only the real project should be discovered"
        );
    }

    #[test]
    fn native_scan_respects_gitignore() {
        let ws = TempWorkspace::new("scan_gitignore");
        // A `.git` dir marks the workspace as a git repo for the `ignore` crate.
        ws.dir(".git");
        ws.file(".gitignore", "generated/\n");
        ws.dir("generated/baml_src");
        ws.file("app/baml_src/main.baml", "// main");

        let found = BexMulitProject::scan_marked_project_roots_native(&ws.root);
        assert_eq!(
            found,
            vec![ws.root.join("app")],
            "gitignored directories must not be discovered"
        );
    }

    #[test]
    fn vfs_walk_skips_generated_dirs_and_finds_markers() {
        let root = vfs::VfsPath::new(vfs::MemoryFS::new());
        for dir in [
            "proj/baml_src",
            "manifest_proj",
            "node_modules/pkg/baml_src",
            "target/junk/baml_src",
        ] {
            root.join(dir).unwrap().create_dir_all().unwrap();
        }
        root.join("manifest_proj/baml.toml")
            .unwrap()
            .create_file()
            .unwrap();

        let mut found = Vec::new();
        BexMulitProject::collect_marked_project_roots_vfs(&root, &mut found);
        let mut names: Vec<_> = found.iter().map(vfs::VfsPath::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, vec!["/manifest_proj", "/proj"]);
    }
}
