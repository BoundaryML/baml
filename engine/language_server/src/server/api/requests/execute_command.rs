use std::time::Duration;

use lsp_server::ErrorCode;
use lsp_types::{request, ExecuteCommandParams, MessageType};
use tokio::time::sleep;
#[cfg(feature = "playground-server")]
use webbrowser;

// use crate::server::api::DocumentKey;
use crate::server::api::ResultExt;
use crate::{
    server::{
        api::traits::{RequestHandler, SyncRequestHandler},
        client::{Notifier, Requester},
        Result,
    },
    Session,
};

pub struct ExecuteCommand;

impl RequestHandler for ExecuteCommand {
    type RequestType = request::ExecuteCommand;
}

impl SyncRequestHandler for ExecuteCommand {
    #[cfg(feature = "playground-server")]
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        if params.command == "openPlayground" {
            // Get the actual playground port from session (determined by server after availability check)
            // Fall back to configured port if actual port not set yet
            let port = session
                .get_session_playground_port()
                .unwrap_or_else(|| session.baml_settings.playground_port.unwrap_or(3030));

            // Construct the URL
            let url = format!("http://localhost:{port}");

            // Open the browser
            if let Err(e) = webbrowser::open(&url) {
                notifier
                    .notify::<lsp_types::notification::ShowMessage>(lsp_types::ShowMessageParams {
                        typ: MessageType::WARNING,
                        message: format!("Failed to open browser: {e}"),
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
        } else if params.command == "baml.changeFunction" {
            // Logic for getting the function can be improved
            if let Some(state) = &session.playground_state {
                if let Some(function_name) = params
                    .arguments
                    .first()
                    .and_then(|arg| arg.get("functionName"))
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                {
                    tracing::info!("Broadcasting function change for: {}", function_name);
                    let state = state.clone();
                    if let Some(runtime) = &session.playground_runtime {
                        runtime.spawn(async move {
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
        } else if params.command == "baml.runTest" {
            // Logic for running a test
            if let Some(state) = &session.playground_state {
                if let Some(args) = params.arguments.first().and_then(|arg| arg.as_object()) {
                    if let (Some(test_case_name), Some(function_name), Some(project_id)) = (
                        args.get("testCaseName").and_then(|v| v.as_str()),
                        args.get("functionName").and_then(|v| v.as_str()),
                        args.get("projectId").and_then(|v| v.as_str()),
                    ) {
                        tracing::info!(
                            "Broadcasting test run for: {} in function: {}",
                            test_case_name,
                            function_name
                        );

                        // First, set the selected function
                        // TODO: test run should handle this in the future
                        let state_clone = state.clone();
                        let func_name = function_name.to_string();
                        let project_path = project_id.to_string();
                        if let Some(runtime) = &session.playground_runtime {
                            runtime.spawn(async move {
                                let _ = crate::playground::broadcast_function_change(
                                    &state_clone,
                                    &project_path,
                                    func_name,
                                )
                                .await;
                            });
                        }

                        // Then, broadcast the test run
                        let state_clone = state.clone();
                        let test_name = test_case_name.to_string();
                        if let Some(runtime) = &session.playground_runtime {
                            runtime.spawn(async move {
                                // TODO: temoporary fix to wait for function change to process
                                sleep(Duration::from_millis(1200)).await;
                                let _ =
                                    crate::playground::broadcast_test_run(&state_clone, test_name)
                                        .await;
                            });
                        }
                    }
                }
            }
        } else {
            return Err(crate::server::api::Error {
                code: ErrorCode::InternalError,
                error: anyhow::anyhow!("Unknown command: {}", params.command),
            });
        }
        Ok(None)
    }
    #[cfg(not(feature = "playground-server"))]
    fn run(
        _session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        // If the playground-server feature is not enabled, return an error for playground commands
        if params.command == "openPlayground"
            || params.command == "baml.changeFunction"
            || params.command == "baml.runTest"
        {
            return Err(crate::server::api::Error {
                code: ErrorCode::InternalError,
                error: anyhow::anyhow!("Playground server is not enabled in this build."),
            });
        }
        Err(crate::server::api::Error {
            code: ErrorCode::InternalError,
            error: anyhow::anyhow!("Unknown command: {}", params.command),
        })
    }
}
