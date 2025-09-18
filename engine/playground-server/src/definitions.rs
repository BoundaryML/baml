use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// Note: the name add_project should match exactly to the
// EventListener.tsx command definitions due to how serde serializes these into json
#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "method", content = "params")]
pub enum FrontendMessage {
    runtime_updated {
        root_path: String,
        files: HashMap<String, String>,
    },
    baml_settings_updated {
        settings: HashMap<String, String>,
    },
    run_test {
        function_name: String,
        test_name: String,
    },
}

#[derive(Debug, Clone)]
/// Messages sent to the webview router, see language_server/src/server.rs
pub enum WebviewRouterMessage {
    WasmIsInitialized,
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
    IdeMessage(serde_json::Value),  // Allow arbitrary JSON for ide_message
    LspMessage(lsp_server::Notification),  // Keep strict typing for lsp_message
}
