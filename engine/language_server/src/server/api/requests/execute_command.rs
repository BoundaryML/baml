use std::time::Duration;

use lsp_server::{ErrorCode, Notification};
use lsp_types::{request, ExecuteCommandParams, MessageType};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use playground_server::WebviewRouterMessage;
use serde_json::json;
use tokio::time::sleep;
use webbrowser;

use crate::{
    server::{
        api::{
            requests::code_action::OPEN_IN_BROWSER_COMMAND,
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

        if params.command == OPEN_IN_BROWSER_COMMAND {
            // Get the actual playground port from session (determined by server after availability check)
            // Fall back to configured port if actual port not set yet

            // Extract function name from arguments if provided
            // Arguments come as: ["FunctionName"] (single string in array)
            let function_name: Option<String> = params
                .arguments
                .first()
                .and_then(|val| val.as_str())
                .map(|s| s.to_string());

            // Construct the URL with optional function parameter
            let url = match &function_name {
                Some(name) => format!(
                    "http://localhost:{}?function={}",
                    session.playground_port,
                    utf8_percent_encode(name, NON_ALPHANUMERIC)
                ),
                None => format!("http://localhost:{}", session.playground_port),
            };

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

            let _ = session
                .to_webview_router_tx
                .send(WebviewRouterMessage::SendMessageToWebview(
                    playground_server::WebviewCommand::LspMessage(Notification::new(
                        "workspace/executeCommand".to_string(),
                        json!(params),
                    )),
                ))
                .inspect_err(|e| {
                    tracing::error!(
                        "Failed to send SEND_MESSAGE_TO_WEBVIEW message to webview: {e}"
                    );
                });
            return Ok(None);
        }

        Ok(None)
    }
}
