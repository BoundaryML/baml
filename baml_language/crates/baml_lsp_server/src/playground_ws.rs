//! WebSocket message types for the playground protocol.
//!
//! Single source of truth for all messages exchanged between the Rust
//! playground server and the webview (TypeScript) over `/api/ws`.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Client -> Server (webview sends these)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum WsInMessage {
    #[serde(rename = "callFunction")]
    CallFunction {
        id: u64,
        project: String,
        name: String,
        /// Base64-encoded protobuf `HostFunctionArguments`.
        #[serde(rename = "argsProto")]
        args_proto: String,
    },
    #[serde(rename = "cancelCall")]
    CancelCall { id: u64, project: String },
    #[serde(rename = "callTestFunction")]
    CallTestFunction {
        id: u64,
        project: String,
        generation: u64,
        #[serde(rename = "testName")]
        test_name: String,
    },
    #[serde(rename = "expandTestSet")]
    ExpandTestSet {
        project: String,
        generation: u64,
        #[serde(rename = "testsetName")]
        testset_name: String,
    },
    #[serde(rename = "envVarResponse")]
    EnvVarResponse {
        id: u64,
        value: Option<String>,
        variable: Option<String>,
    },
    #[serde(rename = "inputResponse")]
    InputResponse {
        id: u64,
        value: String,
        #[serde(rename = "callId")]
        call_id: u64,
    },
    #[serde(rename = "requestState")]
    RequestState,
    #[serde(rename = "requestCollectTests")]
    RequestCollectTests { project: String },
    #[serde(rename = "requestControlFlowGraph")]
    RequestControlFlowGraph {
        project: String,
        #[serde(rename = "functionName")]
        function_name: String,
    },
    #[serde(rename = "cursorPosition")]
    CursorPosition {
        file: String,
        line: u32,
        column: u32,
    },
    /// User set/overrode an env var in the UI.
    #[serde(rename = "setEnvVar")]
    SetEnvVar { key: String, value: String },
    /// User deleted an env var override in the UI.
    #[serde(rename = "deleteEnvVar")]
    DeleteEnvVar { key: String },
}

// ---------------------------------------------------------------------------
// Server -> Client (server pushes these)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type")]
pub enum WsOutMessage {
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "playgroundNotification")]
    PlaygroundNotification { notification: serde_json::Value },
    #[serde(rename = "callFunctionResult")]
    CallFunctionResult {
        id: u64,
        /// Base64-encoded protobuf `CffiValueHolder`.
        result: String,
    },
    #[serde(rename = "callFunctionError")]
    CallFunctionError {
        id: u64,
        error: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cancelled: Option<bool>,
    },
    #[serde(rename = "envVarRequest")]
    EnvVarRequest { id: u64, variable: String },
    /// Bulk send of all process env vars on session init.
    #[serde(rename = "processEnvVars")]
    ProcessEnvVars {
        vars: std::collections::HashMap<String, String>,
    },
    /// An env var was resolved from the server's process environment.
    #[serde(rename = "envVarFromShell")]
    EnvVarFromShell { variable: String, value: String },
    /// Server asks the playground UI to prompt the user for input.
    #[serde(rename = "inputRequest")]
    InputRequest {
        id: u64,
        prompt: Option<String>,
        #[serde(rename = "callId")]
        call_id: u64,
    },
    /// Notifies all clients that an input request was already resolved.
    #[serde(rename = "inputResolved")]
    InputResolved {
        id: u64,
        #[serde(rename = "callId")]
        call_id: u64,
    },
    /// Env var names referenced in BAML source code (from compilation).
    #[serde(rename = "knownEnvVarNames")]
    KnownEnvVarNames { names: Vec<String> },
    #[serde(rename = "fetchLogNew")]
    FetchLogNew {
        #[serde(rename = "callId")]
        call_id: u64,
        id: u64,
        method: String,
        url: String,
        #[serde(rename = "requestHeaders")]
        request_headers: std::collections::HashMap<String, String>,
        #[serde(rename = "requestBody")]
        request_body: String,
    },
    #[serde(rename = "fetchLogUpdate")]
    FetchLogUpdate {
        #[serde(rename = "callId")]
        call_id: u64,
        #[serde(rename = "logId")]
        log_id: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<i64>,
        #[serde(rename = "durationMs", skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
        #[serde(rename = "responseBody", skip_serializing_if = "Option::is_none")]
        response_body: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(rename = "responseHeaders", skip_serializing_if = "Option::is_none")]
        response_headers: Option<std::collections::HashMap<String, String>>,
    },
    #[serde(rename = "controlFlowGraphResult")]
    ControlFlowGraphResult {
        #[serde(rename = "functionName")]
        function_name: String,
        graph: Option<serde_json::Value>,
    },
    #[serde(rename = "cursorContext")]
    CursorContext { context: serde_json::Value },
    /// A runtime event was emitted during execution (protobuf-encoded).
    #[serde(rename = "runtimeEvent")]
    RuntimeEvent {
        /// Base64-encoded protobuf RuntimeEvent bytes
        data: String,
        #[serde(rename = "callId")]
        call_id: u64,
    },
}
