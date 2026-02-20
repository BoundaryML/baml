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
pub struct ProjectUpdate {
    pub is_bex_current: bool,
    pub functions: Vec<String>,
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

#[async_trait::async_trait]
pub trait BexLsp:
    Send + Sync + notification::BexLspNotification + request::BexLspRequest + crate::bex::Bex
{
    async fn call_function_for_project(
        &self,
        project_root: &crate::fs::FsPath,
        function_name: &str,
        args: crate::BexArgs,
        call_id: crate::CallId,
        cancel: sys_types::CancellationToken,
    ) -> Result<crate::BexExternalValue, crate::RuntimeError>;

    fn request_playground_state(&self);
}

pub use multi_project::{LspClientSenderTrait, new_lsp};
