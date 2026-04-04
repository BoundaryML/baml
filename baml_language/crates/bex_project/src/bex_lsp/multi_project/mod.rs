mod commands;
mod diagnostics;
mod notification;
mod request;
mod wasm_helpers;

use std::collections::{HashMap, HashSet};

use ::std::sync::Arc;

use baml_compiler2_hir::package::PackageId;
use baml_compiler2_tir::{
    package_interface::{ExportedType, package_interface},
    ty::{PrimitiveType, Ty},
};
use baml_db::Name;

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

        let flat_diags = Self::flatten_diagnostics(&diagnostics);

        self.send_list_projects();
        self.send_update_project(project_root, &project, flat_diags);
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

        // Get the full package interface for type resolution
        let pkg_id = PackageId::new(db, Name::new("user"));
        let iface = package_interface(db, pkg_id);

        let functions = baml_project::list_functions_with_metadata(db)
            .into_iter()
            .filter(|f| !f.is_sub_function)
            .map(|f| crate::bex_lsp::FunctionInfo {
                name: f.name.clone(),
                kind: if f.is_llm {
                    crate::bex_lsp::FunctionKind::Llm
                } else {
                    crate::bex_lsp::FunctionKind::Expr
                },
                capabilities: if f.is_llm {
                    Some(crate::bex_lsp::LlmCapabilities {
                        render_prompt: true,
                        build_request: true,
                        client_name: f.client_name,
                    })
                } else {
                    None
                },
                params: resolve_function_params(&f.name, iface),
            })
            .collect();

        let tests = baml_project::list_tests_with_metadata(db)
            .into_iter()
            .map(|t| crate::bex_lsp::TestInfo {
                name: t.name,
                function_name: t.function_name,
                args_json: t.args_json,
            })
            .collect();

        crate::bex_lsp::ProjectUpdate {
            is_bex_current,
            functions,
            tests,
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
}

impl super::BexLsp for BexMulitProject {
    fn get_bex_for_project(
        &self,
        project_root: &crate::fs::FsPath,
    ) -> Result<Arc<dyn crate::Bex>, crate::RuntimeError> {
        self.get_bex_for_project(project_root)
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
            test_name: None,
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

// ---------------------------------------------------------------------------
// Parameter schema resolution helpers
// ---------------------------------------------------------------------------

/// Resolve a function's parameter names and types into presentation-layer ParamInfo.
fn resolve_function_params(
    function_name: &str,
    iface: &baml_compiler2_tir::package_interface::PackageInterface,
) -> Vec<crate::bex_lsp::ParamInfo> {
    for ns_funcs in iface.functions.values() {
        if let Some(exported_fn) = ns_funcs.get(&Name::new(function_name)) {
            return exported_fn
                .params
                .iter()
                .map(|(name, ty)| crate::bex_lsp::ParamInfo {
                    name: name.to_string(),
                    field_type: ty_to_field_type(ty, iface, &mut HashSet::new()),
                })
                .collect();
        }
    }
    Vec::new()
}

/// Convert a resolved Ty into a FieldType for the playground form.
/// `ancestors` tracks class names on the current expansion path for cycle detection.
fn ty_to_field_type(
    ty: &Ty,
    iface: &baml_compiler2_tir::package_interface::PackageInterface,
    ancestors: &mut HashSet<String>,
) -> crate::bex_lsp::FieldType {
    match ty {
        Ty::Primitive(PrimitiveType::String, _) => crate::bex_lsp::FieldType::String,
        Ty::Primitive(PrimitiveType::Int, _) => crate::bex_lsp::FieldType::Int,
        Ty::Primitive(PrimitiveType::Float, _) => crate::bex_lsp::FieldType::Float,
        Ty::Primitive(PrimitiveType::Bool, _) => crate::bex_lsp::FieldType::Bool,
        Ty::Primitive(PrimitiveType::Null, _) => crate::bex_lsp::FieldType::Null,
        Ty::Primitive(PrimitiveType::Image, _) => crate::bex_lsp::FieldType::Media {
            media_type: "image".to_string(),
        },
        Ty::Primitive(PrimitiveType::Audio, _) => crate::bex_lsp::FieldType::Media {
            media_type: "audio".to_string(),
        },
        Ty::Primitive(PrimitiveType::Video, _) => crate::bex_lsp::FieldType::Media {
            media_type: "video".to_string(),
        },
        Ty::Primitive(PrimitiveType::Pdf, _) => crate::bex_lsp::FieldType::Media {
            media_type: "pdf".to_string(),
        },

        Ty::Enum(qtn, _) => {
            let enum_name = qtn.name().to_string();
            let values = find_enum_values(iface, &enum_name);
            crate::bex_lsp::FieldType::Enum {
                name: enum_name,
                values,
            }
        }

        Ty::Class(qtn, _) => {
            let class_name = qtn.name().to_string();
            if !ancestors.insert(class_name.clone()) {
                // Cycle detected
                return crate::bex_lsp::FieldType::RecursiveRef { name: class_name };
            }
            let fields = find_class_fields(iface, &class_name)
                .into_iter()
                .map(|(name, field_ty)| crate::bex_lsp::ParamInfo {
                    name: name.to_string(),
                    field_type: ty_to_field_type(&field_ty, iface, ancestors),
                })
                .collect();
            ancestors.remove(&class_name);
            crate::bex_lsp::FieldType::Class {
                name: class_name,
                fields,
            }
        }

        Ty::List(inner, _) | Ty::EvolvingList(inner, _) => crate::bex_lsp::FieldType::List {
            item: Box::new(ty_to_field_type(inner, iface, ancestors)),
        },

        Ty::Map(key, value, _) | Ty::EvolvingMap(key, value, _) => crate::bex_lsp::FieldType::Map {
            key: Box::new(ty_to_field_type(key, iface, ancestors)),
            value: Box::new(ty_to_field_type(value, iface, ancestors)),
        },

        Ty::Optional(inner, _) => crate::bex_lsp::FieldType::Optional {
            inner: Box::new(ty_to_field_type(inner, iface, ancestors)),
        },

        Ty::Union(variants, _) => crate::bex_lsp::FieldType::Union {
            variants: variants
                .iter()
                .map(|v| ty_to_field_type(v, iface, ancestors))
                .collect(),
        },

        Ty::Literal(lit, _, _) => {
            let value = match lit {
                baml_base::Literal::String(s) => crate::bex_lsp::LiteralValue::String(s.clone()),
                baml_base::Literal::Int(i) => crate::bex_lsp::LiteralValue::Int(*i),
                baml_base::Literal::Bool(b) => crate::bex_lsp::LiteralValue::Bool(*b),
                baml_base::Literal::Float(_) => {
                    // Float literals are rare; treat as Any for now
                    return crate::bex_lsp::FieldType::Any;
                }
            };
            crate::bex_lsp::FieldType::Literal { value }
        }

        // Fallback — all other Ty variants map to Any
        _ => crate::bex_lsp::FieldType::Any,
    }
}

/// Look up enum variant names from the PackageInterface.
fn find_enum_values(
    iface: &baml_compiler2_tir::package_interface::PackageInterface,
    enum_name: &str,
) -> Vec<String> {
    for ns_types in iface.types.values() {
        if let Some(ExportedType::Enum { variants, .. }) = ns_types.get(&Name::new(enum_name)) {
            return variants.iter().map(|v| v.to_string()).collect();
        }
    }
    Vec::new()
}

/// Look up class fields from the PackageInterface.
fn find_class_fields(
    iface: &baml_compiler2_tir::package_interface::PackageInterface,
    class_name: &str,
) -> Vec<(Name, Ty)> {
    for ns_types in iface.types.values() {
        if let Some(ExportedType::Class { fields, .. }) = ns_types.get(&Name::new(class_name)) {
            return fields.clone();
        }
    }
    Vec::new()
}
