use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// Note: the name add_project should match exactly to the
// EventListener.tsx command definitions due to how serde serializes these into json
#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "command", content = "content")]
pub enum FrontendMessage {
    add_project {
        root_path: String,
        files: HashMap<String, String>,
    },
    samtest_update_project {
        root_path: String,
        files: HashMap<String, String>,
    },
    remove_project {
        root_path: String,
    },
    select_function {
        root_path: String,
        function_name: String,
    },
    baml_settings_updated {
        settings: HashMap<String, String>,
    },
    run_test {
        function_name: String,
        test_name: String,
    },
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
#[serde(untagged)]
// TODO(sam): this is an unnecessary wrapper layer
pub enum WebviewNotification {
    PlaygroundMessage(FrontendMessage),
}
