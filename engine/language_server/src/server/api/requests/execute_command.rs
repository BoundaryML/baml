use crate::server::api::traits::{RequestHandler, SyncRequestHandler};
// use crate::server::api::DocumentKey;
use crate::server::api::ResultExt;
use crate::server::client::Requester;
use crate::server::{client::Notifier, Result};
use crate::Session;
use lsp_server::ErrorCode;
use lsp_types::{request, ExecuteCommandParams, MessageType};
use std::time::Duration;
use tokio::time::sleep;
use webbrowser;

pub struct ExecuteCommand;

impl RequestHandler for ExecuteCommand {
    type RequestType = request::ExecuteCommand;
}

impl SyncRequestHandler for ExecuteCommand {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        if params.command == "openPlayground" {
            // Get the playground port from session settings
            let port = session.baml_settings.playground_port.unwrap_or(3030);

            // Construct the URL
            let url = format!("http://localhost:{}", port);

            // Open the browser
            if let Err(e) = webbrowser::open(&url) {
                notifier
                    .notify::<lsp_types::notification::ShowMessage>(lsp_types::ShowMessageParams {
                        typ: MessageType::WARNING,
                        message: format!("Failed to open browser: {}", e),
                    })
                    .internal_error()?;
                return Err(crate::server::api::Error {
                    code: ErrorCode::InternalError,
                    error: anyhow::anyhow!("Failed to open browser: {}", e),
                });
            }

            // If we have a function name from the code action, broadcast it
            if let Some(state) = &session.playground_state {
                if let Some(function_name) = params
                    .arguments
                    .first()
                    .and_then(|arg| arg.as_str().map(|s| s.to_string()))
                {
                    tracing::info!("Broadcasting function change for: {}", function_name);
                    let state = state.clone();
                    if let Some(runtime) = &session.playground_runtime {
                        runtime.spawn(async move {
                            // Wait a bit for the server to be ready
                            sleep(Duration::from_millis(500)).await;
                            let _ = crate::playground::broadcast_function_change(
                                &state,
                                &function_name.to_string(),
                                function_name,
                            )
                            .await;
                        });
                    }
                }
            }
        } else {
            return Err(crate::server::api::Error {
                code: ErrorCode::MethodNotFound,
                error: anyhow::anyhow!("Unknown command: {}", params.command),
            });
        }
        Ok(None)
    }
}
