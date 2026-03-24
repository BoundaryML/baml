#[macro_use]
pub(crate) mod notification;

#[macro_use]
mod request;

mod multi_project;

#[derive(Debug, thiserror::Error)]
pub enum LspError {
    #[error("{0}")]
    NotificationExtractError(lsp_server::ExtractError<lsp_server::Notification>),

    #[error("Notification not supported: {0}")]
    NotificationNotSupported(String),

    #[error("{0}")]
    RequestExtractError(lsp_server::ExtractError<lsp_server::Request>),

    #[error("Request not supported: {0}")]
    RequestNotSupported(String),

    #[error("Failed to serialize request result: {0}")]
    RequestSerializeError(serde_json::Error),

    #[error("{0}")]
    Runtime(#[from] crate::RuntimeError),

    #[error("Client closed")]
    ClientClosed,

    #[error("Root path not found: {}: {}", .0.as_str(), .1)]
    ProjectRootNotFound(vfs::VfsPath, String),

    #[error("Project not found: {}", .0.as_str())]
    ProjectNotFound(vfs::VfsPath),

    #[error("Unknown error code: {0}")]
    UnknownErrorCode(String),

    #[error("Invalid command arguments for command: {command}: {message}")]
    InvalidCommandArguments { command: String, message: String },

    #[error("File not found: {}", .0.as_str())]
    FileNotFound(vfs::VfsPath),

    #[error("Path is invalid: {}: {message}", path.to_string_lossy())]
    InvalidPath {
        path: std::path::PathBuf,
        message: String,
    },

    #[error("VFS path is invalid: {}: {message}", path.as_str())]
    InvalidVFSPath { path: vfs::VfsPath, message: String },

    #[error("No projects found")]
    NoProjectsFound,
}

// ---------------------------------------------------------------------------
// Playground notification types (pushed from Rust to JS)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionInfo {
    pub name: String,
    pub kind: FunctionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<LlmCapabilities>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FunctionKind {
    Llm,
    Expr,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmCapabilities {
    pub render_prompt: bool,
    pub build_request: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdate {
    pub is_bex_current: bool,
    pub functions: Vec<FunctionInfo>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PlaygroundNotification {
    #[serde(rename_all = "camelCase")]
    ListProjects { projects: Vec<String> },
    #[serde(rename_all = "camelCase")]
    UpdateProject {
        project: String,
        update: ProjectUpdate,
    },
    #[serde(rename_all = "camelCase")]
    OpenPlayground {
        project: String,
        function_name: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    ControlFlowGraphResult {
        function_name: String,
        graph: Option<serde_json::Value>,
    },
    #[serde(rename_all = "camelCase")]
    CursorContext {
        context: serde_json::Value,
    },
}

pub trait PlaygroundSender: Send + Sync {
    fn send_playground_notification(&self, notification: PlaygroundNotification);
}

// ---------------------------------------------------------------------------
// BexLsp trait
// ---------------------------------------------------------------------------
//
// Send + Sync are required so that `Arc<dyn BexLsp>` can be used as Axum app
// state (e.g. in playground_server's WsState), which must be Clone + Send + Sync.
pub trait BexLsp: Send + Sync + notification::BexLspNotification + request::BexLspRequest {
    fn get_bex_for_project(
        &self,
        project_root: &crate::fs::FsPath,
    ) -> Result<Box<dyn crate::Bex>, crate::RuntimeError>;

    fn request_playground_state(&self);

    fn ast_control_flow_graph(
        &self,
        function_name: &str,
    ) -> Option<baml_compiler2_visualization::control_flow::ControlFlowGraph>;

    /// Request the control flow graph for a function.
    ///
    /// Builds the graph and sends it back via the playground notification
    /// callback as a `PlaygroundNotification::ControlFlowGraphResult`.
    fn request_control_flow_graph(&self, function_name: &str);

    /// Get cursor context for playground navigation.
    ///
    /// Given a file path and position, returns context about what entity
    /// the cursor is on — used to navigate the playground graph.
    fn playground_cursor_context(
        &self,
        file_path: &str,
        line: u32,
        column: u32,
    ) -> baml_project::CursorContext;

    /// Compute cursor context and send it via the playground notification callback.
    ///
    /// Combines `playground_cursor_context` with notification dispatch — used by
    /// the WASM bridge which cannot access the sender directly.
    fn request_cursor_context(&self, file_path: &str, line: u32, column: u32);
}

pub use multi_project::{LspClientSenderTrait, new_lsp};
