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
}

#[derive(Debug, Clone)]
/// Messages sent to the webview router, see language_server/src/server.rs
pub enum WebviewRouterMessage {
    WasmIsInitialized,
    CustomNotificationToWebview(FrontendMessage),
    /// WebviewRouter forwards these to the IDE using an LSP notification.
    SendLspNotificationToIde(lsp_server::Notification),
    /// WebviewRouter forwards these to the webview's EventListener using websocket_ws.rs.
    SendLspNotificationToWebview(lsp_server::Notification),
}

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "source", content = "payload", rename_all = "snake_case")]
/// This is equivalent to VscodeToWebviewCommand in vscode-to-webview-rpc.ts
pub enum WebviewNotification {
    IdeMessage(FrontendMessage),
    LspMessage {
        method: String,
        params: serde_json::Value,
    },
}
