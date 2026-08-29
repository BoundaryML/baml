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
    #[serde(rename = "startRun")]
    StartRun {
        #[serde(rename = "requestId")]
        request_id: u64,
        project: String,
        #[serde(rename = "functionName")]
        function_name: String,
        /// Base64-encoded length-delimited `InboundMapEntry` argument bytes.
        #[serde(rename = "argsBytes")]
        args_bytes: String,
    },
    #[serde(rename = "startPreviewRun")]
    StartPreviewRun {
        #[serde(rename = "requestId")]
        request_id: u64,
        project: String,
        #[serde(rename = "parentFunctionName")]
        parent_function_name: String,
        helper: String,
        #[serde(rename = "functionName")]
        function_name: String,
        /// Base64-encoded length-delimited `InboundMapEntry` argument bytes.
        #[serde(rename = "argsBytes")]
        args_bytes: String,
    },
    #[serde(rename = "cancelRun")]
    CancelRun {
        #[serde(rename = "requestId")]
        request_id: u64,
        #[serde(rename = "boundaryId")]
        boundary_id: String,
    },
    #[serde(rename = "respondToInput")]
    RespondToInput {
        #[serde(rename = "requestId")]
        request_id: u64,
        #[serde(rename = "boundaryId")]
        boundary_id: String,
        #[serde(rename = "inputRequestId")]
        input_request_id: String,
        value: String,
    },
    #[serde(rename = "respondToEnv")]
    RespondToEnv {
        #[serde(rename = "requestId")]
        request_id: u64,
        #[serde(rename = "boundaryId")]
        boundary_id: String,
        #[serde(rename = "envRequestId")]
        env_request_id: String,
        value: Option<String>,
    },
    #[serde(rename = "listRuns")]
    ListRuns {
        #[serde(rename = "requestId")]
        request_id: u64,
        filter: Option<RunListFilter>,
    },
    #[serde(rename = "listHistory")]
    ListHistory {
        #[serde(rename = "requestId")]
        request_id: u64,
        filter: Option<RunListFilter>,
    },
    /// Read one captured value's media bytes by content id. Values travel
    /// with media as a descriptor; this fetches the bytes on demand.
    #[serde(rename = "readTelemetryMedia")]
    ReadTelemetryMedia {
        #[serde(rename = "requestId")]
        request_id: u64,
        project: String,
        cid: String,
    },
    /// List executions in the project's `profiles-v1` store. Structure and
    /// timing live there, not in the run store.
    #[serde(rename = "listExecutions")]
    ListExecutions {
        #[serde(rename = "requestId")]
        request_id: u64,
        project: String,
    },
    /// Read one execution's threads, calling contexts, retained spans, and
    /// errors.
    #[serde(rename = "openExecution")]
    OpenExecution {
        #[serde(rename = "requestId")]
        request_id: u64,
        project: String,
        #[serde(rename = "executionId")]
        execution_id: String,
    },
    #[serde(rename = "openHistory")]
    OpenHistory {
        #[serde(rename = "requestId")]
        request_id: u64,
        #[serde(rename = "boundaryId")]
        boundary_id: String,
    },
    #[serde(rename = "snapshot")]
    Snapshot {
        #[serde(rename = "requestId")]
        request_id: u64,
        #[serde(rename = "boundaryId")]
        boundary_id: String,
    },
    #[serde(rename = "readValue")]
    ReadValue {
        #[serde(rename = "requestId")]
        request_id: u64,
        #[serde(rename = "boundaryId")]
        boundary_id: String,
        #[serde(rename = "valueRef")]
        value_ref: WsValueRef,
    },
    #[serde(rename = "subscribe")]
    Subscribe {
        #[serde(rename = "requestId")]
        request_id: u64,
        #[serde(rename = "subscriptionId")]
        subscription_id: String,
        #[serde(rename = "boundaryId")]
        boundary_id: String,
        #[serde(rename = "afterCursor")]
        after_cursor: Option<u64>,
    },
    #[serde(rename = "unsubscribe")]
    Unsubscribe {
        #[serde(rename = "requestId")]
        request_id: u64,
        #[serde(rename = "subscriptionId")]
        subscription_id: String,
    },
    #[serde(rename = "startTestRun")]
    StartTestRun {
        #[serde(rename = "requestId")]
        request_id: u64,
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
        #[serde(rename = "requestId")]
        request_id: Option<u32>,
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

#[derive(Debug, Deserialize)]
pub struct WsValueRef {
    pub id: String,
    pub codec: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RunListFilter {
    #[serde(rename = "projectId")]
    pub project_id: Option<String>,
    #[serde(rename = "projectGeneration")]
    pub project_generation: Option<u64>,
    pub kinds: Option<Vec<RunListKind>>,
    #[serde(rename = "callTreeContainsFunction")]
    pub call_tree_contains_function: Option<String>,
    pub visibility: Option<RunListVisibility>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunListKind {
    Function,
    Test,
    Preview,
    Companion,
    Internal,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunListVisibility {
    HistoryOnly,
    IncludeHidden,
    AllForDebug,
}

// ---------------------------------------------------------------------------
// Server -> Client (server pushes these)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type")]
pub enum WsOutMessage {
    #[serde(rename = "hello")]
    Hello {
        #[serde(rename = "toolchainVersion")]
        toolchain_version: String,
        #[serde(rename = "playgroundProtocol")]
        playground_protocol: u32,
        #[serde(rename = "minClientPlaygroundProtocol")]
        min_client_playground_protocol: u32,
        capabilities: Vec<String>,
    },
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "playgroundNotification")]
    PlaygroundNotification { notification: serde_json::Value },
    #[serde(rename = "runStarted")]
    RunStarted {
        #[serde(rename = "requestId", skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
        run: serde_json::Value,
    },
    #[serde(rename = "runPatch")]
    RunPatch { patch: serde_json::Value },
    #[serde(rename = "commandAck")]
    CommandAck {
        #[serde(rename = "requestId")]
        request_id: u64,
        outcome: String,
    },
    #[serde(rename = "commandError")]
    CommandError {
        #[serde(rename = "requestId")]
        request_id: u64,
        code: String,
        message: String,
    },
    #[serde(rename = "runList")]
    RunList {
        #[serde(rename = "requestId")]
        request_id: u64,
        runs: Vec<serde_json::Value>,
    },
    #[serde(rename = "historyList")]
    HistoryList {
        #[serde(rename = "requestId")]
        request_id: u64,
        runs: Vec<serde_json::Value>,
    },
    #[serde(rename = "executionList")]
    ExecutionList {
        #[serde(rename = "requestId")]
        request_id: u64,
        executions: Vec<serde_json::Value>,
        /// Set when the project has no profile store yet. The client renders
        /// an empty state, not an error: nothing has run here.
        #[serde(rename = "storeMissing", skip_serializing_if = "std::ops::Not::not")]
        store_missing: bool,
    },
    #[serde(rename = "telemetryMedia")]
    TelemetryMedia {
        #[serde(rename = "requestId")]
        request_id: u64,
        cid: String,
        media: serde_json::Value,
    },
    #[serde(rename = "executionTelemetry")]
    ExecutionTelemetry {
        #[serde(rename = "requestId")]
        request_id: u64,
        #[serde(rename = "executionId")]
        execution_id: String,
        telemetry: serde_json::Value,
    },
    #[serde(rename = "runSnapshot")]
    RunSnapshot {
        #[serde(rename = "requestId", skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
        #[serde(rename = "boundaryId")]
        boundary_id: String,
        snapshot: serde_json::Value,
    },
    #[serde(rename = "valueBody")]
    ValueBody {
        #[serde(rename = "requestId")]
        request_id: u64,
        #[serde(rename = "boundaryId")]
        boundary_id: String,
        #[serde(rename = "valueRefId")]
        value_ref_id: String,
        codec: String,
        availability: String,
        #[serde(rename = "bodyBase64", skip_serializing_if = "Option::is_none")]
        body_base64: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        diagnostic: Option<String>,
    },
    #[serde(rename = "runCursorExpired")]
    RunCursorExpired {
        #[serde(rename = "requestId", skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
        #[serde(rename = "subscriptionId", skip_serializing_if = "Option::is_none")]
        subscription_id: Option<String>,
        #[serde(rename = "boundaryId")]
        boundary_id: String,
        reason: String,
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
        #[serde(rename = "requestId", skip_serializing_if = "Option::is_none")]
        request_id: Option<u32>,
    },
    #[serde(rename = "cursorContext")]
    CursorContext { context: serde_json::Value },
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn run_command_frames_use_run_vocabulary() {
        let cancel = serde_json::from_value::<WsInMessage>(json!({
            "type": "cancelRun",
            "requestId": 7,
            "boundaryId": "baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ"
        }))
        .unwrap();
        assert!(matches!(
            cancel,
            WsInMessage::CancelRun { request_id: 7, .. }
        ));

        let subscribe = serde_json::from_value::<WsInMessage>(json!({
            "type": "subscribe",
            "requestId": 8,
            "subscriptionId": "sub-1",
            "boundaryId": "baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ",
            "afterCursor": 3
        }))
        .unwrap();
        assert!(matches!(
            subscribe,
            WsInMessage::Subscribe {
                request_id: 8,
                after_cursor: Some(3),
                ..
            }
        ));

        let start = serde_json::from_value::<WsInMessage>(json!({
            "type": "startRun",
            "requestId": 9,
            "project": "/tmp/project",
            "functionName": "Extract",
            "argsBytes": ""
        }))
        .unwrap();
        assert!(matches!(
            start,
            WsInMessage::StartRun {
                request_id: 9,
                ref function_name,
                ..
            } if function_name == "Extract"
        ));

        let preview = serde_json::from_value::<WsInMessage>(json!({
            "type": "startPreviewRun",
            "requestId": 14,
            "project": "/tmp/project",
            "parentFunctionName": "Extract",
            "helper": "render_prompt",
            "functionName": "Extract@render_prompt",
            "argsBytes": ""
        }))
        .unwrap();
        assert!(matches!(
            preview,
            WsInMessage::StartPreviewRun {
                request_id: 14,
                ref parent_function_name,
                ref helper,
                ref function_name,
                ..
            } if parent_function_name == "Extract"
                && helper == "render_prompt"
                && function_name == "Extract@render_prompt"
        ));

        let test = serde_json::from_value::<WsInMessage>(json!({
            "type": "startTestRun",
            "requestId": 10,
            "project": "/tmp/project",
            "generation": 3,
            "testName": "suite/test"
        }))
        .unwrap();
        assert!(matches!(
            test,
            WsInMessage::StartTestRun {
                request_id: 10,
                generation: 3,
                ref test_name,
                ..
            } if test_name == "suite/test"
        ));

        let input = serde_json::from_value::<WsInMessage>(json!({
            "type": "respondToInput",
            "requestId": 11,
            "boundaryId": "baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ",
            "inputRequestId": "4",
            "value": "hello"
        }))
        .unwrap();
        assert!(matches!(
            input,
            WsInMessage::RespondToInput {
                request_id: 11,
                ref input_request_id,
                ref value,
                ..
            } if input_request_id == "4" && value == "hello"
        ));

        let env = serde_json::from_value::<WsInMessage>(json!({
            "type": "respondToEnv",
            "requestId": 12,
            "boundaryId": "baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ",
            "envRequestId": "5",
            "value": "secret"
        }))
        .unwrap();
        assert!(matches!(
            env,
            WsInMessage::RespondToEnv {
                request_id: 12,
                ref env_request_id,
                value: Some(ref value),
                ..
            } if env_request_id == "5" && value == "secret"
        ));

        let list = serde_json::from_value::<WsInMessage>(json!({
            "type": "listRuns",
            "requestId": 13,
            "filter": {
                "projectId": "/tmp/project",
                "projectGeneration": 4,
                "kinds": ["function"],
                "callTreeContainsFunction": "Extract",
                "visibility": "historyOnly"
            }
        }))
        .unwrap();
        assert!(matches!(
            list,
            WsInMessage::ListRuns {
                request_id: 13,
                filter: Some(RunListFilter {
                    project_id: Some(ref project_id),
                    project_generation: Some(4),
                    call_tree_contains_function: Some(ref function_name),
                    visibility: Some(RunListVisibility::HistoryOnly),
                    ..
                }),
            } if project_id == "/tmp/project" && function_name == "Extract"
        ));

        let read_value = serde_json::from_value::<WsInMessage>(json!({
            "type": "readValue",
            "requestId": 15,
            "boundaryId": "baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ",
            "valueRef": {
                "id": "value_1",
                "codec": "bamlOutboundValue"
            }
        }))
        .unwrap();
        assert!(matches!(
            read_value,
            WsInMessage::ReadValue {
                request_id: 15,
                ref boundary_id,
                value_ref: WsValueRef {
                    ref id,
                    codec: Some(ref codec),
                },
            } if boundary_id == "baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ"
                && id == "value_1"
                && codec == "bamlOutboundValue"
        ));
    }

    #[test]
    fn run_recovery_frames_echo_request_identity() {
        let msg = WsOutMessage::RunCursorExpired {
            request_id: Some(8),
            subscription_id: Some("sub-1".to_string()),
            boundary_id: "baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ".to_string(),
            reason: "compacted".to_string(),
        };
        let wire = serde_json::to_value(msg).unwrap();
        assert_eq!(wire["type"], "runCursorExpired");
        assert_eq!(wire["requestId"], 8);
        assert_eq!(wire["subscriptionId"], "sub-1");
        assert_eq!(wire["boundaryId"], "baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ");
        assert_eq!(wire["reason"], "compacted");
    }

    #[test]
    fn value_body_frame_uses_value_ref_identity() {
        let msg = WsOutMessage::ValueBody {
            request_id: 15,
            boundary_id: "baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ".to_string(),
            value_ref_id: "value_1".to_string(),
            codec: "bamlOutboundValue".to_string(),
            availability: "available".to_string(),
            body_base64: Some("AQID".to_string()),
            diagnostic: None,
        };
        let wire = serde_json::to_value(msg).unwrap();
        assert_eq!(wire["type"], "valueBody");
        assert_eq!(wire["requestId"], 15);
        assert_eq!(wire["boundaryId"], "baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ");
        assert_eq!(wire["valueRefId"], "value_1");
        assert_eq!(wire["codec"], "bamlOutboundValue");
        assert_eq!(wire["availability"], "available");
        assert_eq!(wire["bodyBase64"], "AQID");
        assert!(wire.get("diagnostic").is_none());
    }
}
