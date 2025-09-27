use anyhow::Context;
use axum::{
    extract::{Path, State},
    Json,
};
use mime_guess;
use serde_json::Value;

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

// Helper function to get MIME type from file path
fn get_mime_type(path: &std::path::Path) -> Option<String> {
    // Get MIME type from file extension
    mime_guess::from_path(path)
        .first()
        .map(|mime| mime.to_string())
}

pub async fn webview_rpc_handler(
    Path(command): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    match command.as_str() {
        "GET_VSCODE_SETTINGS" => {
            let (tx, mut rx) = tokio::sync::broadcast::channel(1);
            let _ = state
                .to_webview_router_tx
                .send(WebviewRouterMessage::GetLanguageServerSettings(tx))
                .inspect_err(|e| {
                    tracing::error!(
                        "Failed to send GET_LANGUAGE_SERVER_SETTINGS message to WebviewRouter: {e}"
                    );
                });
            let response = rx
                .recv()
                .await
                .map_err(|e| anyhow_to_internal_error(e.into()))?;

            Ok(Json(response))
        }

        "UPDATE_SETTINGS" => {
            let request: UpdateSettingsRequest = serde_json::from_value(payload)
                .context("Failed to parse UpdateSettingsRequest")
                .map_err(anyhow_to_bad_request)?;

            tracing::info!("UPDATE_SETTINGS: {:#?}", request);

            let _ = state
                .to_webview_router_tx
                .send(WebviewRouterMessage::UpdateLanguageServerSettings(request.settings))
                .inspect_err(|e| {
                    tracing::error!("Failed to send UPDATE_LANGUAGE_SERVER_SETTINGS message to WebviewRouter: {e}");
                });

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

            tracing::debug!("GET_WEBVIEW_URI request for path: {}", request.path);

            // Use the path directly from the request
            let path = std::path::Path::new(&request.path);

            // Check if we can determine the MIME type
            let uri = if let Some(mime_type) = get_mime_type(path) {
                tracing::debug!("Returning data URL for {}: {}", mime_type, request.path);

                // For files with detectable MIME types, read the file and create a data URL
                match tokio::fs::read(&request.path).await {
                    Ok(contents) => {
                        use base64::{engine::general_purpose, Engine as _};
                        let encoded = general_purpose::STANDARD.encode(&contents);
                        let data_url = format!("data:{};base64,{}", mime_type, encoded);

                        // Log if the data URL is large
                        if data_url.len() > 1_000_000 {
                            // 1MB
                            tracing::warn!(
                                "Large data URL generated for {}: {} bytes",
                                request.path,
                                data_url.len()
                            );
                        }

                        data_url
                    }
                    Err(e) => {
                        // Fall back to file:// URI if we can't read the file
                        tracing::warn!("Failed to read file for data URL: {}", e);
                        format!("file://{}", request.path)
                    }
                }
            } else {
                // For files without detectable MIME types, use file:// URI as before
                tracing::debug!("Returning file URI for unknown type: {}", request.path);
                format!("file://{}", request.path)
            };

            let mut response = GetWebviewUriResponse {
                uri,
                contents: None,
                read_error: None,
            };

            // Still support the optional contents field for backward compatibility
            if request.contents.unwrap_or(false) && get_mime_type(path).is_none() {
                match tokio::fs::read(&request.path).await {
                    Ok(contents) => {
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
                    tracing::error!(
                        "Failed to send SEND_COMMAND_TO_WEBVIEW message to language-server: {e}"
                    );
                });

            let response = SendCommandToWebviewResponse { ok: true };
            Ok(Json(serde_json::to_value(response)?))
        }

        _ => Err(ApiError::NotFound(format!(
            "Unknown RPC command: {}",
            command
        ))),
    }
}
