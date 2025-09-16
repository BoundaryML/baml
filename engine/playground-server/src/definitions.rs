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
    execute_command {
        command: String,
        arguments: Vec<serde_json::Value>,
    },
    #[serde(untagged)]
    lsp_message {
        method: String,
        params: serde_json::Value,
    },
}

#[derive(Debug, Clone)]
/// for lang-server internal comms, before sending out to the playground
pub enum WebviewRouterMessage {
    WasmIsInitialized,
    CustomNotificationToWebview(FrontendMessage),
    SendLspNotificationToIde(lsp_server::Notification),
    SendLspNotificationToWebview(lsp_server::Notification),
}

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "source", content = "payload", rename_all = "snake_case")]
pub enum WebviewNotification {
    LspMessage(FrontendMessage),
}
