use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::Value;

use anyhow::Context;
use crate::{
    api::{errors::ApiError, *},
    server::AppState,
    WebviewRouterMessage,
};

// Helper function to convert anyhow::Error to ApiError for internal operations
fn anyhow_to_internal_error(err: anyhow::Error) -> ApiError {
    ApiError::InternalError(format!("{:#}", err))
}

fn anyhow_to_bad_request(err: anyhow::Error) -> ApiError {
    ApiError::BadRequest(format!("{:#}", err))
}

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
                .map_err(|_| anyhow::anyhow!("Failed to read editor config"))
                .map_err(anyhow_to_internal_error)?;

            let response = GetVSCodeSettingsResponse {
                enable_playground_proxy: config.enable_playground_proxy,
                feature_flags: config.feature_flags.clone(),
            };
            Ok(Json(serde_json::to_value(response)?))
        }

        "SET_PROXY_SETTINGS" => {
            let request: SetProxySettingsRequest = serde_json::from_value(payload)
                .context("Failed to parse SetProxySettingsRequest")
                .map_err(anyhow_to_bad_request)?;

            let mut config = state
                .editor_config
                .write()
                .map_err(|_| anyhow::anyhow!("Failed to acquire write lock on editor config"))
                .map_err(anyhow_to_internal_error)?;
            config.enable_playground_proxy = request.proxy_enabled;

            Ok(Json(Value::Null)) // No response body for settings updates
        }

        "SET_FEATURE_FLAGS" => {
            let request: SetFeatureFlagsRequest = serde_json::from_value(payload)
                .context("Failed to parse SetFeatureFlagsRequest")
                .map_err(anyhow_to_bad_request)?;

            let mut config = state
                .editor_config
                .write()
                .map_err(|_| anyhow::anyhow!("Failed to acquire write lock on editor config"))
                .map_err(anyhow_to_internal_error)?;
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
                .context("Failed to parse GetWebviewUriRequest")
                .map_err(anyhow_to_bad_request)?;

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
                .context("Failed to parse SendLspNotificationToIdeRequest")
                .map_err(anyhow_to_bad_request)?;

            let _ = state
                .to_webview_router_tx
                .send(WebviewRouterMessage::SendLspNotificationToIde(request.notification))
                .inspect_err(|e| {
                    tracing::error!("Failed to send SEND_LSP_NOTIFICATION_TO_IDE message to language-server: {e}");
                });

            let response = SendLspNotificationToIdeResponse { ok: true };
            Ok(Json(serde_json::to_value(response)?))
        }

        "SEND_COMMAND_TO_WEBVIEW" => {
            let request: SendCommandToWebviewRequest = serde_json::from_value(payload)
                .context("Failed to parse SendCommandToWebviewRequest")
                .map_err(anyhow_to_bad_request)?;

            let _ = state
                .to_webview_router_tx
                .send(WebviewRouterMessage::SendMessageToWebview(request.0))
                .inspect_err(|e| {
                    tracing::error!("Failed to send SEND_COMMAND_TO_WEBVIEW message to language-server: {e}");
                });

            let response = SendCommandToWebviewResponse { ok: true };
            Ok(Json(serde_json::to_value(response)?))
        }

        "SEND_LSP_NOTIFICATION_TO_WEBVIEW" => {
            let request: SendLspNotificationToWebviewRequest = serde_json::from_value(payload)
                .context("Failed to parse SendLspNotificationToWebviewRequest")
                .map_err(anyhow_to_bad_request)?;

            // Convert LSP notification to WebviewCommand::LspMessage
            let webview_command = crate::definitions::WebviewCommand::LspMessage(request.notification);

            let _ = state
                .to_webview_router_tx
                .send(WebviewRouterMessage::SendMessageToWebview(webview_command))
                .inspect_err(|e| {
                    tracing::error!("Failed to send SEND_LSP_NOTIFICATION_TO_WEBVIEW message to language-server: {e}");
                });

            let response = SendLspNotificationToWebviewResponse { ok: true };
            Ok(Json(serde_json::to_value(response)?))
        }

        _ => Err(ApiError::NotFound(format!(
            "Unknown RPC command: {}",
            command
        ))),
    }
}
