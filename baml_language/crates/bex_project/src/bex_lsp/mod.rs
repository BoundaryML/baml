#[macro_use]
pub(crate) mod notification;

#[macro_use]
mod request;

mod multi_project;
mod protocol;

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;

#[derive(Debug, thiserror::Error)]
pub enum LspError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Invalid JSON-RPC request: {0}")]
    InvalidRequest(String),

    #[error("Server not initialized: {0}")]
    ServerNotInitialized(String),

    #[error("Method not found: {0}")]
    MethodNotFound(String),

    #[error("Invalid params: {0}")]
    InvalidParams(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Request canceled: {0}")]
    RequestCanceled(String),

    #[error("Content modified: {0}")]
    ContentModified(String),

    #[error("Request failed: {0}")]
    RequestFailed(String),

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

    #[error("LSP outbound sink is saturated")]
    OutboundSaturated,

    #[error("LSP outbound frame exceeds the transport limit")]
    OutboundOversized,

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

impl From<baml_project::position::PositionCodecError> for LspError {
    fn from(error: baml_project::position::PositionCodecError) -> Self {
        Self::InvalidParams(error.to_string())
    }
}

impl LspError {
    /// The one JSON-RPC error-code mapping boundary used by every transport.
    /// `-32001` is deliberately never emitted.
    #[must_use]
    pub fn json_rpc_code(&self) -> i32 {
        match self {
            Self::ParseError(_) => -32700,
            Self::InvalidRequest(_) => -32600,
            Self::ServerNotInitialized(_) => -32002,
            Self::MethodNotFound(_)
            | Self::NotificationNotSupported(_)
            | Self::RequestNotSupported(_) => -32601,
            Self::InvalidParams(_)
            | Self::NotificationExtractError(_)
            | Self::RequestExtractError(_)
            | Self::InvalidCommandArguments { .. }
            | Self::InvalidPath { .. }
            | Self::InvalidVFSPath { .. } => -32602,
            Self::InternalError(_) | Self::RequestSerializeError(_) => -32603,
            Self::RequestCanceled(_) => -32800,
            Self::ContentModified(_) => -32801,
            Self::RequestFailed(_)
            | Self::Runtime(_)
            | Self::ClientClosed
            | Self::OutboundSaturated
            | Self::OutboundOversized
            | Self::ProjectRootNotFound(..)
            | Self::ProjectNotFound(_)
            | Self::UnknownErrorCode(_)
            | Self::FileNotFound(_)
            | Self::NoProjectsFound => -32803,
        }
    }
}

// ---------------------------------------------------------------------------
// Playground notification types (pushed from Rust to JS)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FunctionInfo {
    pub name: String,
    pub kind: FunctionKind,
    pub origin: FunctionOrigin,
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

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FunctionKind {
    Llm,
    Expr,
}

#[derive(Debug, Clone, Copy, serde::Serialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LlmCapabilities {
    pub render_prompt: bool,
    pub build_request: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDiagnostic {
    pub severity: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdate {
    pub source_revision: u64,
    pub project_incarnation: u64,
    pub runtime: ProjectRuntimeStatus,
    pub is_bex_current: bool,
    pub functions: Vec<FunctionInfo>,
    /// Shared type table for `FunctionInfo.params` refs: every named type
    /// referenced from any function's schema, defined exactly once and keyed
    /// by canonical dotted FQN. `None` (omitted on the wire) means the binary
    /// predates the args form; `Some` may be an empty map. Same `None` ≠
    /// `Some(empty)` discipline as `params`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<std::collections::BTreeMap<String, baml_project::TypeSchema>>,
    pub diagnostics: Vec<ProjectDiagnostic>,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCatalogEntry {
    pub project: String,
    pub incarnation: u64,
    pub source_revision: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PlaygroundNotification {
    #[serde(rename_all = "camelCase")]
    ListProjects {
        session_epoch: u64,
        projects: Vec<String>,
        entries: Vec<ProjectCatalogEntry>,
    },
    #[serde(rename_all = "camelCase")]
    UpdateProject {
        session_epoch: u64,
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
        session_epoch: u64,
        project: String,
        project_incarnation: u64,
        source_revision: u64,
        generation: u64,
        derived_epoch: u64,
        function_name: String,
        graph: Option<serde_json::Value>,
    },
    #[serde(rename_all = "camelCase")]
    CursorContext { context: serde_json::Value },
    #[serde(rename_all = "camelCase")]
    TestCollectionResult {
        session_epoch: u64,
        project: String,
        project_incarnation: u64,
        source_revision: u64,
        generation: u64,
        collection_epoch: u64,
        call_id: u64,
        data: Vec<u8>,
        /// If a testset expansion failed, the name + error message.
        #[serde(skip_serializing_if = "Option::is_none")]
        expand_error: Option<TestExpandError>,
        /// Collection/serialization failure. Consumers retain the previous
        /// tree; an empty `data` payload is never interpreted as zero tests.
        #[serde(skip_serializing_if = "Option::is_none")]
        collection_error: Option<String>,
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

#[derive(Clone)]
pub struct PreparedFunctionRun {
    pub source_revision: u64,
    pub generation: u64,
    pub engine: Arc<dyn crate::Bex>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreparedTestRun {
    pub source_revision: u64,
    pub generation: u64,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRuntimeStatus {
    pub state: String,
    pub requested_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
    pub has_last_known_good: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

pub trait PlaygroundSender: Send + Sync {
    fn send_playground_notification(&self, notification: PlaygroundNotification);

    /// Port included in the editor-only `baml/openPlayground` LSP
    /// notification. Browser/WASM senders handle the action themselves.
    fn lsp_playground_port(&self) -> Option<u16> {
        None
    }

    /// Whether constructing and serializing project runtime detail can reach
    /// at least one consumer. Catalog notifications remain ungated.
    fn has_runtime_subscribers(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// BexLsp trait
// ---------------------------------------------------------------------------
//
// Send + Sync are required so that `Arc<dyn BexLsp>` can be used as Axum app
// state (e.g. in playground_server's WsState), which must be Clone + Send + Sync.
#[async_trait]
pub trait BexLsp: Send + Sync + notification::BexLspNotification + request::BexLspRequest {
    /// Create a connection-scoped LSP dispatcher that shares project state but
    /// starts with fresh, uninitialized session capabilities and writes only
    /// to that connection's revocable outbound sink.
    fn new_lsp_session(
        &self,
        sender: Arc<dyn LspClientSenderTrait + Send + Sync>,
    ) -> Arc<dyn BexLsp>;

    fn get_bex_for_project(
        &self,
        project_root: &crate::fs::FsPath,
    ) -> Result<Arc<dyn crate::Bex>, crate::RuntimeError>;

    fn request_playground_state(&self);

    /// Invalidate engine construction state after an environment/configuration
    /// input changes. Demanded projects start one new build flight; cold
    /// projects remain stale until demand arrives.
    fn runtime_inputs_changed(&self);

    /// Register a browser playground endpoint and allocate its unique epoch.
    fn begin_playground_session(&self) -> u64;

    /// Create a lightweight dispatcher clone whose project-derived work is
    /// stamped and routed only to the registered endpoint epoch.
    fn bind_playground_session(&self, session_epoch: u64) -> Arc<dyn BexLsp>;

    /// Revoke one endpoint without invalidating other browser sessions.
    fn end_playground_session(&self, session_epoch: u64);

    fn playground_session_is_current(&self, session_epoch: u64) -> bool;

    /// Acquire selected-project runtime demand and join the current-revision
    /// build flight. Catalog/editor diagnostics do not require this lease.
    async fn ensure_project_runtime(
        &self,
        project: &str,
        incarnation: Option<u64>,
    ) -> Result<ProjectRuntimeStatus, LspError>;

    async fn retry_project_runtime(
        &self,
        project: &str,
        incarnation: Option<u64>,
    ) -> Result<ProjectRuntimeStatus, LspError>;

    fn release_project_runtime(&self, project: &str, incarnation: Option<u64>);

    /// Current catalog identity for a project root. Runtime-demand messages
    /// use this to reject a lease for a removed/re-added project instance.
    fn project_incarnation(&self, project: &str) -> Option<u64>;

    /// Build current source if necessary, atomically pin engine/CFG identity,
    /// and register the run before returning it to the transport.
    async fn prepare_function_run(
        &self,
        project: &str,
        call_id: sys_types::CallId,
        function_name: &str,
    ) -> Result<PreparedFunctionRun, LspError>;

    /// Build current source and atomically pin the matching test registry
    /// before a `RunStore` start is exposed to the client.
    async fn prepare_test_run(
        &self,
        project: &str,
        call_id: sys_types::CallId,
        generation: u64,
    ) -> Result<PreparedTestRun, LspError>;

    fn finish_project_run(&self, project: &str, call_id: sys_types::CallId);

    fn cancel_project_run(
        &self,
        project: &str,
        call_id: sys_types::CallId,
    ) -> Result<(), crate::RuntimeError>;

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
        call_id: Option<sys_types::CallId>,
    ) -> Option<std::sync::Arc<baml_compiler2_visualization::control_flow::ControlFlowGraph>>;

    /// Request the control flow graph for a function.
    ///
    /// Builds the graph and sends it back via the playground notification
    /// callback as a `PlaygroundNotification::ControlFlowGraphResult`.
    fn request_control_flow_graph(&self, project: &str, function_name: &str);

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
        call_id: sys_types::CallId,
        ctx: bex_engine::FunctionCallContext,
    ) -> Result<bex_external_types::BexExternalValue, bex_engine::EngineError>;

    /// Run a specific test by name and surface the BEX entry trace identity.
    async fn call_test_function_with_trace(
        &self,
        project: &str,
        generation: u64,
        test_name: &str,
        call_id: sys_types::CallId,
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
mod notification_identity_tests {
    use super::PlaygroundNotification;

    #[test]
    fn project_derived_notifications_serialize_complete_identity() {
        let cfg = serde_json::to_value(PlaygroundNotification::ControlFlowGraphResult {
            session_epoch: 23,
            project: "/workspace/project".to_string(),
            project_incarnation: 7,
            source_revision: 11,
            generation: 13,
            derived_epoch: 17,
            function_name: "Workflow".to_string(),
            graph: None,
        })
        .expect("CFG notification should serialize");
        assert_eq!(cfg["projectIncarnation"], 7);
        assert_eq!(cfg["sessionEpoch"], 23);
        assert_eq!(cfg["sourceRevision"], 11);
        assert_eq!(cfg["generation"], 13);
        assert_eq!(cfg["derivedEpoch"], 17);

        let tests = serde_json::to_value(PlaygroundNotification::TestCollectionResult {
            session_epoch: 23,
            project: "/workspace/project".to_string(),
            project_incarnation: 7,
            source_revision: 11,
            generation: 13,
            collection_epoch: 17,
            call_id: 19,
            data: vec![],
            expand_error: None,
            collection_error: None,
        })
        .expect("test collection notification should serialize");
        assert_eq!(tests["projectIncarnation"], 7);
        assert_eq!(tests["sourceRevision"], 11);
        assert_eq!(tests["generation"], 13);
        assert_eq!(tests["collectionEpoch"], 17);
    }
}
