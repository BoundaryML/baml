use std::time::Duration;

use lsp_server::ErrorCode;
use lsp_types::{request, ExecuteCommandParams, MessageType};
use playground_server::{FrontendMessage, WebviewRouterMessage};
use tokio::time::sleep;
use webbrowser;

use crate::{
    server::{
        api::{
            traits::{RequestHandler, SyncRequestHandler},
            ResultExt,
        },
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
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        use crate::server::commands::RegisteredCommands;

        if params.command == "openPlayground" {
            // Get the actual playground port from session (determined by server after availability check)
            // Fall back to configured port if actual port not set yet

            use playground_server::{FrontendMessage, WebviewRouterMessage};

            // Construct the URL
            let url = format!("http://localhost:{}", session.playground_port);

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

            if let Some(function_name) = params
                .arguments
                .first()
                .and_then(|arg| arg.as_str().map(|s| s.to_string()))
            {
                session
                    .to_webview_router_tx
                    .send(WebviewRouterMessage::CustomNotificationToWebview(
                        FrontendMessage::select_function {
                            // TODO: this can't be correct... but it looks like it is
                            root_path: function_name.to_string(),
                            function_name,
                        },
                    ))
                    .unwrap();
            }
            return Ok(None);
        }

        tracing::info!("Executing command: {:?}", params);
        match RegisteredCommands::from_execute_command(params) {
            Err(e) => {
                return Err(crate::server::api::Error {
                    code: ErrorCode::InternalError,
                    error: e.into(),
                });
            }
            Ok(RegisteredCommands::OpenBamlPanel(args)) => {
                let tx = session.to_webview_router_tx.send(
                    WebviewRouterMessage::CustomNotificationToWebview(
                        FrontendMessage::select_function {
                            // TODO: this can't be correct... but it looks like it is
                            root_path: args.project_id,
                            function_name: args.function_name,
                        },
                    ),
                );
                if let Err(e) = tx {
                    tracing::warn!("Error forwarding OpenBamlPanel to playground: {}", e);
                }
            }
            Ok(RegisteredCommands::RunTest(args)) => {
                let tx = session.to_webview_router_tx.send(
                    WebviewRouterMessage::CustomNotificationToWebview(FrontendMessage::run_test {
                        function_name: args.function_name,
                        test_name: args.test_case_name,
                    }),
                );
                if let Err(e) = tx {
                    tracing::warn!("Error forwarding RunTest to playground: {}", e);
                }
            }
        }

        Ok(None)
    }
}
