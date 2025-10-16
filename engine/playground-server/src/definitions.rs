use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
/// Messages sent to the webview router, see language_server/src/server.rs
pub enum WebviewRouterMessage {
    WasmIsInitialized,
    GetLanguageServerSettings(broadcast::Sender<serde_json::Value>),
    UpdateLanguageServerSettings(serde_json::Value),
    /// WebviewRouter forwards these to the IDE using an LSP notification.
    SendLspNotificationToIde(lsp_server::Notification),
    /// WebviewRouter forwards these to the webview's EventListener using websocket_ws.rs.
    /// Command is passed directly to websocket without any processing
    SendMessageToWebview(WebviewCommand),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "source", content = "payload", rename_all = "snake_case")]
/// This is equivalent to VscodeToWebviewCommand in vscode-to-webview-rpc.ts
pub enum WebviewCommand {
    IdeMessage(serde_json::Value), // Allow arbitrary JSON for ide_message
    LspMessage(lsp_server::Notification), // Keep strict typing for lsp_message
}
