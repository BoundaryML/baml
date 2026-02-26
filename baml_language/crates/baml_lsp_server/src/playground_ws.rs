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
    #[serde(rename = "envVarResponse")]
    EnvVarResponse {
        id: u64,
        value: Option<String>,
        variable: Option<String>,
    },
    #[serde(rename = "requestState")]
    RequestState,
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
    CallFunctionError { id: u64, error: String },
    #[serde(rename = "envVarRequest")]
    EnvVarRequest { id: u64, variable: String },
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
    },
}
