#[macro_use]
pub(crate) mod notification;

#[macro_use]
mod request;

mod multi_project;

use async_trait::async_trait;

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
    pub origin: FunctionOrigin,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<LlmCapabilities>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FunctionKind {
    Llm,
    Expr,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FunctionOrigin {
    UserDefined,
    Companion,
    Internal,
    AutoDerive,
}

impl From<baml_project::FunctionOrigin> for FunctionOrigin {
    fn from(origin: baml_project::FunctionOrigin) -> Self {
        match origin {
            baml_project::FunctionOrigin::UserDefined => Self::UserDefined,
            baml_project::FunctionOrigin::Companion => Self::Companion,
            baml_project::FunctionOrigin::Internal => Self::Internal,
            baml_project::FunctionOrigin::AutoDerive => Self::AutoDerive,
        }
    }
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
pub struct ProjectDiagnostic {
    pub severity: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdate {
    pub is_bex_current: bool,
    pub functions: Vec<FunctionInfo>,
    pub diagnostics: Vec<ProjectDiagnostic>,
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
    CursorContext { context: serde_json::Value },
    #[serde(rename_all = "camelCase")]
    TestCollectionResult {
        project: String,
        generation: u64,
        call_id: u64,
        data: Vec<u8>,
        /// If a testset expansion failed, the name + error message.
        #[serde(skip_serializing_if = "Option::is_none")]
        expand_error: Option<TestExpandError>,
    },
    /// A runtime event was emitted during execution (protobuf-encoded).
    #[serde(rename_all = "camelCase")]
    RuntimeEvent {
        /// Protobuf-encoded `RuntimeEvent` bytes (decode with `RuntimeEvent.decode()`)
        data: Vec<u8>,
        call_id: u64,
    },
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestExpandError {
    pub testset_name: String,
    pub message: String,
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
#[async_trait]
pub trait BexLsp: Send + Sync + notification::BexLspNotification + request::BexLspRequest {
    fn get_bex_for_project(
        &self,
        project_root: &crate::fs::FsPath,
    ) -> Result<Arc<dyn crate::Bex>, crate::RuntimeError>;

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

    /// Collect all unique env var names referenced in BAML source across all projects.
    fn all_env_var_names(&self) -> Vec<String>;

    fn request_collect_tests(&self, project: &str);

    /// Run a specific test by name. Request-response — returns the serialized
    /// `TestReport` as proto bytes, using the same path as `call_function`.
    async fn call_test_function(
        &self,
        project: &str,
        generation: u64,
        test_name: &str,
        ctx: bex_engine::FunctionCallContext,
    ) -> Result<bex_external_types::BexExternalValue, bex_engine::EngineError>;

    /// Expand a lazy test set by name. Fire-and-forget — result comes via a
    /// `TestCollectionResult` playground notification with the full serialized tree.
    fn expand_test_set(&self, project: &str, generation: u64, testset_name: &str);

    /// Resolve a file ID to its file path.
    ///
    /// Used by the playground to navigate to source locations when clicking on
    /// log events. Returns the file path if the ID is valid, or None if not found.
    fn resolve_file_id(&self, file_id: u32) -> Option<String>;
}

use ::std::sync::Arc;
pub use multi_project::{BackgroundSpawner, LspClientSenderTrait, new_lsp};
