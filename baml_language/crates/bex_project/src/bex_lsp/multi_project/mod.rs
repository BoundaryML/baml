mod commands;
mod diagnostics;
mod notification;
mod request;
mod wasm_helpers;

use std::collections::HashMap;

/// Factory that creates [`sys_types::SysOps`] for a given project root.
type SysOpFactory =
    std::sync::Arc<dyn Fn(&vfs::VfsPath) -> std::sync::Arc<sys_types::SysOps> + Send + Sync>;

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
    event_sink: Option<std::sync::Arc<dyn bex_events::EventSink>>,
    #[allow(dead_code)] // TODO: reserved for upcoming playground integration
    playground_state: std::sync::Arc<std::sync::Mutex<PlaygroundState>>,
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

// #[derive(Clone, Debug)]
// struct LspClientSender {
//     weak_sender: std::sync::Weak<crossbeam::channel::Sender<lsp_server::Message>>,
// }

// impl LspClientSenderTrait for LspClientSender {
//     fn send_notification(&self, msg: lsp_server::Notification) -> Result<(), LspError> {
//         let Some(sender) = self.weak_sender.upgrade() else {
//             return Err(LspError::ClientClosed);
//         };
//         sender
//             .send(lsp_server::Message::Notification(msg))
//             .map_err(|_| LspError::ClientClosed)
//     }

//     #[allow(dead_code)]
//     fn make_request(&self, msg: lsp_server::Request) -> Result<(), LspError> {
//         let Some(sender) = self.weak_sender.upgrade() else {
//             return Err(LspError::ClientClosed);
//         };
//         sender
//             .send(lsp_server::Message::Request(msg))
//             .map_err(|_| LspError::ClientClosed)
//     }

//     fn send_response_impl(&self, response: lsp_server::Response) -> Result<(), LspError> {
//         let Some(sender) = self.weak_sender.upgrade() else {
//             return Err(LspError::ClientClosed);
//         };

//         sender
//             .send(lsp_server::Message::Response(response))
//             .map_err(|_| LspError::ClientClosed)
//     }
// }

#[allow(dead_code)]
enum SelectionReason {
    UserSelection,
    AutomaticSelection,
}

#[allow(dead_code)]
struct Selection<T> {
    value: Option<T>,
    reason: SelectionReason,
}

impl<T> Default for Selection<T> {
    fn default() -> Self {
        Self {
            value: None,
            reason: SelectionReason::AutomaticSelection,
        }
    }
}

#[allow(dead_code)]
impl<T> Selection<T> {
    fn set_user_selection(&mut self, value: T) {
        self.value = Some(value);
        self.reason = SelectionReason::UserSelection;
    }

    fn set_automatic_selection(&mut self, value: T) {
        self.value = Some(value);
        self.reason = SelectionReason::AutomaticSelection;
    }
}

#[allow(dead_code, clippy::struct_field_names)]
#[derive(Default)]
struct PlaygroundState {
    last_selected_project: Selection<vfs::VfsPath>,
    last_selected_function: Selection<String>,
    last_selected_test: Selection<String>,
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
        event_sink: Option<std::sync::Arc<dyn bex_events::EventSink>>,
    ) -> Self {
        Self {
            projects: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            sys_op_factory,
            event_sink,
            playground_state: std::sync::Arc::new(
                std::sync::Mutex::new(PlaygroundState::default()),
            ),
            sender,
            playground_sender,
            position_encoding: PositionEncoding::UTF8,
            workspace_roots: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            fs,
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
        let project = crate::project::BexProject::new(&root_path, sys_ops, self.event_sink.clone());
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
    ) -> Result<Box<dyn crate::Bex>, RuntimeError> {
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
        Ok(Box::new(bex))
    }

    fn get_baml_project_root(path: &vfs::VfsPath) -> Result<vfs::VfsPath, LspError> {
        // Baml project live in one of three places:
        // 1. inside a baml_src directory
        // 2. inside a folder which has a baml.toml file
        // 3. (internal development only) as standalone files inside a folder named baml_language

        let file_name = path.filename();

        match file_name.as_str() {
            "baml_src"
                if path.is_dir().map_err(|e| LspError::InvalidVFSPath {
                    path: path.clone(),
                    message: format!("Failed to check if path is a directory: {e}"),
                })? =>
            {
                return Ok(path.clone());
            }
            "baml.toml"
                if path.is_file().map_err(|e| LspError::InvalidVFSPath {
                    path: path.clone(),
                    message: format!("Failed to check if path is a file: {e}"),
                })? =>
            {
                return Ok(path.parent());
            }
            _ => {}
        }

        let mut current = path.parent();
        while !current.is_root() {
            let parent = current;
            // check if parent is baml_src directory
            if parent.filename().as_str() == "baml_src"
                && parent.is_dir().map_err(|e| LspError::InvalidVFSPath {
                    path: parent.clone(),
                    message: format!("Failed to check if path is a directory: {e}"),
                })?
            {
                return Ok(parent);
            }

            // check if parent has a baml.toml file
            let baml_toml_path =
                parent
                    .join("baml.toml")
                    .map_err(|e| LspError::InvalidVFSPath {
                        path: parent.clone(),
                        message: format!("Failed to join path: {e}"),
                    })?;
            if baml_toml_path.exists().unwrap_or(false) {
                return Ok(parent);
            }

            current = parent.parent();
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
        let glob = format!("{}/**/*.baml", project_root.as_str());
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

        self.send_list_projects();
        self.send_update_project(project_root, &project);
        tracing::debug!("refresh_project done");
    }

    fn build_project_update(project: &LiveProject) -> crate::bex_lsp::ProjectUpdate {
        let is_bex_current = project.project.is_bex_current();

        let db_guard = project.project.db.lock().unwrap();
        let db = db_guard.db();
        let functions = match db_guard.project() {
            Some(p) => baml_project::list_functions(db, p)
                .into_iter()
                .map(|f| f.name)
                .collect(),
            None => vec![],
        };

        crate::bex_lsp::ProjectUpdate {
            is_bex_current,
            functions,
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

    fn send_update_project(&self, project_root: &vfs::VfsPath, project: &LiveProject) {
        let update = Self::build_project_update(project);
        self.playground_sender.send_playground_notification(
            crate::bex_lsp::PlaygroundNotification::UpdateProject {
                project: project_root.as_str().to_string(),
                update,
            },
        );
    }
}

impl super::BexLsp for BexMulitProject {
    fn get_bex_for_project(
        &self,
        project_root: &crate::fs::FsPath,
    ) -> Result<Box<dyn crate::Bex>, crate::RuntimeError> {
        self.get_bex_for_project(project_root)
    }

    fn request_playground_state(&self) {
        self.send_list_projects();
        let projects = self.projects.lock().unwrap();
        for (fs_path, project) in projects.iter() {
            let root_str = fs_path.as_path().to_string_lossy().into_owned();
            let update = Self::build_project_update(project);
            self.playground_sender.send_playground_notification(
                crate::bex_lsp::PlaygroundNotification::UpdateProject {
                    project: root_str,
                    update,
                },
            );
        }
    }
}

pub fn new_lsp(
    sys_op_factory: SysOpFactory,
    sender: std::sync::Arc<dyn LspClientSenderTrait + Send + Sync>,
    playground_sender: std::sync::Arc<dyn crate::bex_lsp::PlaygroundSender>,
    fs: crate::fs::BamlVFS,
    event_sink: Option<std::sync::Arc<dyn bex_events::EventSink>>,
) -> impl crate::bex_lsp::BexLsp {
    BexMulitProject::new(sys_op_factory, sender, playground_sender, fs, event_sink)
}
