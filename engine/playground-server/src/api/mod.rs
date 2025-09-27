// HTTP API request/response types matching VSCode RPC
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPlaygroundPortRequest {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPlaygroundPortResponse {
    pub port: u16,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetVSCodeSettingsRequest {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetVSCodeSettingsResponse {
    pub enable_playground_proxy: bool,
    pub feature_flags: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetWebviewUriRequest {
    pub baml_src: String,
    pub path: String,
    pub contents: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetWebviewUriResponse {
    pub uri: String,
    pub contents: Option<String>,
    pub read_error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct InitializedRequest {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializedResponse {
    pub ack: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenPlaygroundRequest {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenPlaygroundResponse {
    pub success: bool,
    pub url: Option<String>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendLspNotificationToIdeRequest {
    pub notification: lsp_server::Notification,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendLspNotificationToIdeResponse {
    pub ok: bool,
}

#[derive(Deserialize)]
pub struct SendCommandToWebviewRequest(pub crate::definitions::WebviewCommand);

#[derive(Serialize)]
pub struct SendCommandToWebviewResponse {
    pub ok: bool,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequest {
    pub settings: serde_json::Value,
}

pub mod errors;
