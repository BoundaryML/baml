use axum::{
    extract::{Path, State},
    Json,
};
use lsp_server::Notification;
use serde_json::Value;

use crate::{
    api::{errors::ApiError, *},
    server::AppState,
    WebviewRouterMessage,
};

pub async fn webview_rpc_handler(
    Path(command): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    match command.as_str() {
        "GET_VSCODE_SETTINGS" => {
            let config = state
                .editor_config
                .read()
                .map_err(|_| ApiError::InternalError("Failed to read config".to_string()))?;

            let response = GetVSCodeSettingsResponse {
                enable_playground_proxy: config.enable_playground_proxy,
                feature_flags: config.feature_flags.clone(),
            };
            Ok(Json(serde_json::to_value(response)?))
        }

        "SET_PROXY_SETTINGS" => {
            let request: SetProxySettingsRequest = serde_json::from_value(payload)
                .map_err(|_| ApiError::BadRequest("Invalid request format".to_string()))?;

            let mut config = state
                .editor_config
                .write()
                .map_err(|_| ApiError::InternalError("Failed to write config".to_string()))?;
            config.enable_playground_proxy = request.proxy_enabled;

            Ok(Json(Value::Null)) // No response body for settings updates
        }

        "SET_FEATURE_FLAGS" => {
            let request: SetFeatureFlagsRequest = serde_json::from_value(payload)
                .map_err(|_| ApiError::BadRequest("Invalid feature flags request".to_string()))?;

            let mut config = state
                .editor_config
                .write()
                .map_err(|_| ApiError::InternalError("Failed to write config".to_string()))?;
            config.feature_flags = request.feature_flags;

            Ok(Json(Value::Null)) // No response body for settings updates
        }

        "GET_PLAYGROUND_PORT" => {
            let response = GetPlaygroundPortResponse {
                port: state.proxy_port,
            };
            Ok(Json(serde_json::to_value(response)?))
        }

        "GET_WEBVIEW_URI" => {
            let request: GetWebviewUriRequest = serde_json::from_value(payload)
                .map_err(|_| ApiError::BadRequest("Invalid webview URI request".to_string()))?;

            let file_access = &state.file_access;

            // Generate webview-compatible URI (for JCEF, this is just a file:// URI)
            let resolved_path = file_access.resolve_path(&request.path)?;
            let uri = format!("file://{}", resolved_path.display());

            let mut response = GetWebviewUriResponse {
                uri,
                contents: None,
                read_error: None,
            };

            if request.contents.unwrap_or(false) {
                match file_access.read_file(&request.path).await {
                    Ok(contents) => {
                        // Encode binary data as base64 for JSON transport
                        use base64::{engine::general_purpose, Engine as _};
                        response.contents = Some(general_purpose::STANDARD.encode(contents));
                    }
                    Err(e) => {
                        response.read_error = Some(format!("{:?}", e));
                    }
                }
            }

            Ok(Json(serde_json::to_value(response)?))
        }

        "LOAD_AWS_CREDS" => {
            let request: LoadAwsCredsRequest = serde_json::from_value(payload)
                .map_err(|_| ApiError::BadRequest("Invalid AWS creds request".to_string()))?;

            let response = crate::credentials::aws::load_aws_credentials(request).await;
            Ok(Json(serde_json::to_value(response)?))
        }

        "LOAD_GCP_CREDS" => {
            let request: LoadGcpCredsRequest = serde_json::from_value(payload)
                .map_err(|_| ApiError::BadRequest("Invalid GCP creds request".to_string()))?;

            let response = crate::credentials::gcp::load_gcp_credentials(request).await;
            Ok(Json(serde_json::to_value(response)?))
        }

        "INITIALIZED" => {
            if let Err(e) = state
                .to_webview_router_tx
                .send(WebviewRouterMessage::WasmIsInitialized)
            {
                tracing::error!("Failed to send INITIALIZED message to language-server: {e}");
            }
            let response = InitializedResponse { ack: true };
            Ok(Json(serde_json::to_value(response)?))
        }

        "OPEN_PLAYGROUND" => {
            // For JetBrains, this is a no-op since the playground is already open in JCEF
            let response = OpenPlaygroundResponse {
                success: true,
                url: Some("Already open in JetBrains".to_string()),
                error: None,
            };
            Ok(Json(serde_json::to_value(response)?))
        }

        "SEND_LSP_NOTIFICATION_TO_IDE" => {
            let request: SendLspNotificationToIdeRequest = serde_json::from_value(payload)
                .map_err(|_| ApiError::BadRequest("Invalid jump to file request".to_string()))?;

            let _ = state
                .to_webview_router_tx
                .send(WebviewRouterMessage::SendLspNotificationToIde(request.notification))
                .inspect_err(|e| {
                    tracing::error!("Failed to send SEND_LSP_NOTIFICATION_TO_IDE message to language-server: {e}");
                });

            let response = SendLspNotificationToIdeResponse { ok: true };
            Ok(Json(serde_json::to_value(response)?))
        }

        "SEND_LSP_NOTIFICATION_TO_WEBVIEW" => {
            let _ = state
                .to_webview_router_tx
                .send(WebviewRouterMessage::SendLspNotificationToWebview(Notification::new(
                    "textDocument/codeAction".to_string(),
                    payload,
                )))
                .inspect_err(|e| {
                    tracing::error!("Failed to send SEND_LSP_NOTIFICATION_TO_IDE message to language-server: {e}");
                });

            let response = SendLspNotificationToIdeResponse { ok: true };
            Ok(Json(serde_json::to_value(response)?))
        }

        "ECHO" => {
            let request: EchoRequest = serde_json::from_value(payload)
                .map_err(|_| ApiError::BadRequest("Invalid echo request".to_string()))?;

            let response = EchoResponse {
                message: request.message,
            };
            Ok(Json(serde_json::to_value(response)?))
        }

        _ => Err(ApiError::NotFound(format!(
            "Unknown RPC command: {}",
            command
        ))),
    }
}
