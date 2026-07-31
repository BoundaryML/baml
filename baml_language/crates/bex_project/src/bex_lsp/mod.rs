#[macro_use]
pub(crate) mod notification;

#[macro_use]
mod request;

mod multi_project;
mod position_codec;
mod protocol;
pub(crate) mod request_cancellation;

use std::{path::PathBuf, sync::Arc};

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

    /// The connection-owned outbound sink cannot accept more frames right
    /// now (bounded transport backpressure; LSP `RequestFailed`, `-32803`).
    #[error("LSP outbound sink is saturated")]
    OutboundSaturated,

    /// The serialized outbound frame exceeds the transport's frame limit
    /// (LSP `RequestFailed`, `-32803`).
    #[error("LSP outbound frame exceeds the transport limit")]
    OutboundOversized,

    #[error("Root path not found: {}: {}", .0.as_str(), .1)]
    ProjectRootNotFound(vfs::VfsPath, String),

    #[error("Project not found: {}", .0.as_str())]
    ProjectNotFound(vfs::VfsPath),

    /// Legacy constructor kept for the WASM bridge; serialized as
    /// `InternalError`, never as `-32001`.
    #[error("{0}")]
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

    /// Explicit client cancellation won response ownership while the request
    /// was queued or running (LSP `RequestCanceled`, `-32800`). Observed only
    /// at safe handler boundaries — never through Salsa unwinding.
    #[error("Request canceled: {0}")]
    RequestCanceled(String),

    /// The request's view became stale: sources were mutated while it waited
    /// (LSP `ContentModified`, `-32801`).
    #[error("Content modified: {0}")]
    ContentModified(String),

    /// Syntactically valid request that cannot be served right now:
    /// same-revision busy timeout, overload, or unavailable target
    /// (LSP `RequestFailed`, `-32803`).
    #[error("{0}")]
    RequestFailed(String),

    /// Violated invariant, poisoned state, or other internal failure
    /// (LSP `InternalError`, `-32603`).
    #[error("Internal error: {0}")]
    Internal(String),

    /// Malformed params, position, or range (LSP `InvalidParams`, `-32602`).
    #[error("Invalid params: {0}")]
    InvalidParams(String),

    /// Request arrived before `initialize` completed
    /// (LSP `ServerNotInitialized`, `-32002`).
    #[error("Server not initialized: {0}")]
    ServerNotInitialized(String),
}

impl LspError {
    /// The one LSP error-code mapping. Every request error is serialized
    /// through this table; `-32001 UnknownErrorCode` is never emitted.
    #[must_use]
    pub fn to_response_error(&self) -> lsp_server::ResponseError {
        use lsp_server::ErrorCode;
        let code = match self {
            // Malformed params / positions / ranges.
            LspError::NotificationExtractError(_)
            | LspError::RequestExtractError(_)
            | LspError::InvalidCommandArguments { .. }
            | LspError::InvalidParams(_) => ErrorCode::InvalidParams,

            // Unsupported method.
            LspError::NotificationNotSupported(_) | LspError::RequestNotSupported(_) => {
                ErrorCode::MethodNotFound
            }

            // Request before initialize completed.
            LspError::ServerNotInitialized(_) => ErrorCode::ServerNotInitialized,

            // Explicit cancellation claimed the response.
            LspError::RequestCanceled(_) => ErrorCode::RequestCanceled,

            // The request's view became stale under an applied source change.
            LspError::ContentModified(_) => ErrorCode::ContentModified,

            // Valid request that cannot be served: unknown project/file,
            // same-revision busy timeout, overload, saturated/oversized
            // outbound transport.
            LspError::ProjectRootNotFound(..)
            | LspError::ProjectNotFound(_)
            | LspError::FileNotFound(_)
            | LspError::InvalidPath { .. }
            | LspError::InvalidVFSPath { .. }
            | LspError::NoProjectsFound
            | LspError::OutboundSaturated
            | LspError::OutboundOversized
            | LspError::RequestFailed(_) => ErrorCode::RequestFailed,

            // Internal failures: poison, violated invariants, serialization,
            // runtime errors, and the legacy catch-all constructor.
            LspError::RequestSerializeError(_)
            | LspError::Runtime(_)
            | LspError::ClientClosed
            | LspError::UnknownErrorCode(_)
            | LspError::Internal(_) => ErrorCode::InternalError,
        };
        lsp_server::ResponseError {
            code: code as i32,
            message: self.to_string(),
            data: None,
        }
    }
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
    pub signature: String,
    pub source_position: FunctionSourcePosition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<LlmCapabilities>,
    /// Parameter schemas for the playground args form; named types inside are
    /// refs into `ProjectUpdate.types`. `None` (omitted on the wire) means no
    /// schema is available and the UI degrades to raw JSON; `Some(vec![])`
    /// means the function takes no arguments. Do not collapse the empty vec —
    /// the UI relies on the distinction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Vec<baml_project::ParamSchema>>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionSourcePosition {
    pub file: String,
    pub line: u32,
    pub column: u32,
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
pub struct TestInfo {
    pub name: String,
    pub function_name: String,
    pub args_json: String,
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
    /// Generation of the installed engine that backs this project update.
    pub generation: u64,
    pub functions: Vec<FunctionInfo>,
    /// Statically declared legacy test cases that can seed function previews.
    pub tests: Vec<TestInfo>,
    /// Shared type table for `FunctionInfo.params` refs: every named type
    /// referenced from any function's schema, defined exactly once and keyed
    /// by canonical dotted FQN. `None` (omitted on the wire) means the binary
    /// predates the args form; `Some` may be an empty map. Same `None` ≠
    /// `Some(empty)` discipline as `params`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<std::collections::BTreeMap<String, baml_project::TypeSchema>>,
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
        #[serde(skip_serializing_if = "Option::is_none")]
        function_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        test_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        testset_name: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    ControlFlowGraphResult {
        function_name: String,
        graph: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<u32>,
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
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestExpandError {
    pub testset_name: String,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaygroundSourceFile {
    pub path: String,
    pub relative_path: String,
    pub content: String,
}

pub trait PlaygroundSender: Send + Sync {
    /// Serialize and enqueue the notification synchronously without waiting
    /// for a consumer or performing blocking I/O. Project snapshots keep
    /// revision-fencing guards through this call, so implementations must
    /// return after a bounded in-process handoff.
    fn send_playground_notification(&self, notification: PlaygroundNotification);
}

/// A coherent run-launch snapshot: engine and generation captured in one
/// source→runtime transaction, with the overlay control-flow graph already
/// pinned for that generation. Holding `engine` keeps the launched-on engine
/// alive for the run's duration, so cancel can always target it.
pub struct PreparedRun {
    pub generation: u64,
    pub engine: Arc<dyn crate::Bex>,
}

// ---------------------------------------------------------------------------
// BexLsp trait
// ---------------------------------------------------------------------------
//
// Send + Sync are required so that `Arc<dyn BexLsp>` can be used as Axum app
// state (e.g. in playground_server's WsState), which must be Clone + Send + Sync.
#[async_trait]
pub trait BexLsp: Send + Sync + notification::BexLspNotification + request::BexLspRequest {
    /// Create a connection-scoped LSP dispatcher: it shares the
    /// process-owned project registry but owns a fresh position-encoding
    /// negotiation and fresh initialize workspace roots, and writes only
    /// through `sender` — the connection's revocable outbound sink. Browser
    /// takeover revokes that sink, so a retained clone of the old
    /// session can no longer leak output into the replacement.
    fn new_lsp_session(
        &self,
        sender: Arc<dyn LspClientSenderTrait + Send + Sync>,
    ) -> Arc<dyn BexLsp>;

    fn get_bex_for_project(
        &self,
        project_root: &crate::fs::FsPath,
    ) -> Result<Arc<dyn crate::Bex>, crate::RuntimeError>;

    /// Capture a coherent run-launch snapshot: validates that the
    /// installed engine matches current sources, and pins the overlay
    /// control-flow graph for `overlay_function` under the same transaction.
    ///
    /// Errors are typed for the playground boundary: `ContentModified` when a
    /// rebuild is needed first, `RequestFailed` when the project is busy.
    fn prepare_function_run(
        &self,
        project_root: &str,
        overlay_function: Option<&str>,
    ) -> Result<PreparedRun, LspError>;

    /// The engine a run launched on, looked up by its pinned generation
    /// for cancel targeting. `None` once that generation has been replaced
    /// and released.
    fn engine_for_generation(
        &self,
        project_root: &str,
        generation: u64,
    ) -> Option<Arc<dyn crate::Bex>>;

    fn request_playground_state(&self);

    /// Seed workspace roots when the LSP is launched without an editor client
    /// that can provide `initialize.workspaceFolders`.
    fn initialize_workspace_roots(&self, roots: Vec<PathBuf>) -> Result<Vec<String>, LspError>;

    fn ast_control_flow_graph(
        &self,
        function_name: &str,
    ) -> Option<baml_compiler2_visualization::control_flow::ControlFlowGraph>;

    fn project_generation(&self, project_root: &str) -> Option<u64>;

    /// Prepared control-flow graph for a function as of a project generation.
    ///
    /// Built lazily and cached per `(generation, function)`: a miss can only
    /// be built while `generation` is still current. Playground run launches
    /// call this right after capturing their generation to pin the graph for
    /// the run's later overlay resolutions.
    fn control_flow_graph_for_generation(
        &self,
        project_root: &str,
        generation: u64,
        function_name: &str,
    ) -> Option<std::sync::Arc<baml_compiler2_visualization::control_flow::ControlFlowGraph>>;

    /// Request the control flow graph for a function.
    ///
    /// Builds the graph and sends it back via the playground notification
    /// callback as a `PlaygroundNotification::ControlFlowGraphResult`.
    fn request_control_flow_graph(&self, function_name: &str, request_id: Option<u32>);

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

    /// Source files currently visible to the playground for a project.
    fn playground_source_files(&self, project: &str)
    -> Result<Vec<PlaygroundSourceFile>, LspError>;

    /// Apply an in-memory source edit from the browser playground.
    fn playground_update_source_file(
        &self,
        project: &str,
        path: &str,
        content: String,
    ) -> Result<(), LspError>;

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

    /// Run a specific test by name and surface the BEX entry trace identity.
    async fn call_test_function_with_trace(
        &self,
        project: &str,
        generation: u64,
        test_name: &str,
        ctx: bex_engine::FunctionCallContext,
    ) -> Result<bex_engine::BexCallResult, bex_engine::EngineError>;

    /// Expand a lazy test set by name. Fire-and-forget — result comes via a
    /// `TestCollectionResult` playground notification with the full serialized tree.
    fn expand_test_set(&self, project: &str, generation: u64, testset_name: &str);

    /// Resolve a file ID to its file path.
    ///
    /// Used by the playground to navigate to source locations when clicking on
    /// log events. Returns the file path if the ID is valid, or None if not found.
    fn resolve_file_id(&self, file_id: u32) -> Option<String>;
}

pub use multi_project::{BackgroundSpawner, LspClientSenderTrait, new_lsp};

#[cfg(test)]
mod tests {
    use super::*;

    /// The retired catch-all code (pre-0.14.2 `UnknownErrorCode`). No error
    /// may serialize to it.
    const LEGACY_UNKNOWN: i32 = -32001;

    fn code(e: &LspError) -> i32 {
        e.to_response_error().code
    }

    #[test]
    fn typed_errors_map_to_spec_codes() {
        use lsp_server::ErrorCode;

        assert_eq!(
            code(&LspError::RequestCanceled("client canceled".into())),
            ErrorCode::RequestCanceled as i32,
        );
        assert_eq!(
            code(&LspError::ContentModified("edit raced".into())),
            ErrorCode::ContentModified as i32,
        );
        assert_eq!(
            code(&LspError::RequestFailed("busy".into())),
            ErrorCode::RequestFailed as i32,
        );
        assert_eq!(
            code(&LspError::Internal("poisoned".into())),
            ErrorCode::InternalError as i32,
        );
        assert_eq!(
            code(&LspError::InvalidParams("bad position".into())),
            ErrorCode::InvalidParams as i32,
        );
        assert_eq!(
            code(&LspError::ServerNotInitialized("early".into())),
            ErrorCode::ServerNotInitialized as i32,
        );
        assert_eq!(
            code(&LspError::RequestNotSupported("x/y".into())),
            ErrorCode::MethodNotFound as i32,
        );
        assert_eq!(
            code(&LspError::NotificationNotSupported("x/y".into())),
            ErrorCode::MethodNotFound as i32,
        );
    }

    #[test]
    fn resource_lookup_failures_are_request_failed() {
        use lsp_server::ErrorCode;
        let root = vfs::VfsPath::new(vfs::MemoryFS::new());
        for e in [
            LspError::ProjectRootNotFound(root.clone(), "not a project".into()),
            LspError::ProjectNotFound(root.clone()),
            LspError::FileNotFound(root),
            LspError::NoProjectsFound,
            LspError::OutboundSaturated,
            LspError::OutboundOversized,
        ] {
            assert_eq!(code(&e), ErrorCode::RequestFailed as i32, "{e}");
        }
    }

    #[test]
    fn legacy_unknown_code_is_never_emitted() {
        let root = vfs::VfsPath::new(vfs::MemoryFS::new());
        let all = [
            LspError::NotificationNotSupported("m".into()),
            LspError::RequestNotSupported("m".into()),
            LspError::RequestSerializeError(serde_json::from_str::<()>("x").unwrap_err()),
            LspError::Runtime(crate::RuntimeError::Other("x".into())),
            LspError::ClientClosed,
            LspError::ProjectRootNotFound(root.clone(), "x".into()),
            LspError::ProjectNotFound(root.clone()),
            // The legacy constructor itself now serializes as InternalError.
            LspError::UnknownErrorCode("legacy".into()),
            LspError::InvalidCommandArguments {
                command: "c".into(),
                message: "m".into(),
            },
            LspError::FileNotFound(root),
            LspError::InvalidPath {
                path: std::path::PathBuf::from("/x"),
                message: "m".into(),
            },
            LspError::NoProjectsFound,
            LspError::RequestCanceled("m".into()),
            LspError::ContentModified("m".into()),
            LspError::RequestFailed("m".into()),
            LspError::Internal("m".into()),
            LspError::InvalidParams("m".into()),
            LspError::ServerNotInitialized("m".into()),
            LspError::OutboundSaturated,
            LspError::OutboundOversized,
        ];
        for e in &all {
            assert_ne!(code(e), LEGACY_UNKNOWN, "{e} must not emit -32001");
        }
        assert_eq!(
            code(&LspError::UnknownErrorCode("legacy".into())),
            lsp_server::ErrorCode::InternalError as i32,
        );
    }
}
