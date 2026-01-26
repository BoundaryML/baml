use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum BamlNotification {
    #[serde(rename = "baml/playground_port")]
    PlaygroundPort { port: u16 },

    #[serde(rename = "runtime_updated")]
    RuntimeUpdated {
        root_path: String,
        files: HashMap<String, String>,
    },
}

impl BamlNotification {
    pub fn to_lsp_notification(&self) -> lsp_server::Notification {
        let mut to_json = json!(self);
        let method = to_json["method"].as_str().unwrap().to_string();
        let params = to_json["params"].take();

        lsp_server::Notification::new(method, params)
    }

    pub fn to_lsp_message(&self) -> lsp_server::Message {
        lsp_server::Message::Notification(self.to_lsp_notification())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeUpdated {
    pub root_path: String,
    pub files: HashMap<String, String>,
}

impl lsp_types::notification::Notification for RuntimeUpdated {
    type Params = Self;
    const METHOD: &'static str = "runtime_updated";
}
