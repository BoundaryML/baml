// HTTP API request/response types matching VSCode RPC
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct EchoRequest {
    pub message: String,
}

#[derive(Serialize)]
pub struct EchoResponse {
    pub message: String,
}

#[derive(Deserialize)]
pub struct GetPlaygroundPortRequest {}

#[derive(Serialize)]
pub struct GetPlaygroundPortResponse {
    pub port: u16,
}

#[derive(Deserialize)]
pub struct GetVSCodeSettingsRequest {}

#[derive(Serialize)]
pub struct GetVSCodeSettingsResponse {
    pub enable_playground_proxy: bool,
    pub feature_flags: Vec<String>,
}

#[derive(Deserialize)]
pub struct SetProxySettingsRequest {
    pub proxy_enabled: bool,
}

#[derive(Deserialize)]
pub struct SetFeatureFlagsRequest {
    pub feature_flags: Vec<String>,
}

#[derive(Deserialize)]
pub struct GetWebviewUriRequest {
    pub baml_src: String,
    pub path: String,
    pub contents: Option<bool>,
}

#[derive(Serialize)]
pub struct GetWebviewUriResponse {
    pub uri: String,
    pub contents: Option<String>,
    pub read_error: Option<String>,
}

#[derive(Deserialize)]
pub struct LoadAwsCredsRequest {
    pub profile: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LoadAwsCredsResponse {
    #[serde(rename_all = "camelCase")]
    Ok {
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
    },
    Error {
        name: String,
        message: String,
    },
}

#[derive(Deserialize)]
pub struct LoadGcpCredsRequest {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LoadGcpCredsResponse {
    #[serde(rename_all = "camelCase")]
    Ok {
        access_token: String,
        project_id: String,
    },
    Error {
        name: String,
        message: String,
    },
}

#[derive(Deserialize)]
pub struct InitializedRequest {}

#[derive(Serialize)]
pub struct InitializedResponse {
    pub ack: bool,
}

#[derive(Deserialize)]
pub struct OpenPlaygroundRequest {}

#[derive(Serialize)]
pub struct OpenPlaygroundResponse {
    pub success: bool,
    pub url: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub file_path: String,
    pub start_line: usize,
}

#[derive(Deserialize)]
pub struct SendLspNotificationToIdeRequest {
    pub notification: lsp_server::Notification,
}

#[derive(Serialize)]
pub struct SendLspNotificationToIdeResponse {
    pub ok: bool,
}

pub mod errors;
