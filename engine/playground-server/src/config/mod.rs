use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditorConfig {
    pub enable_playground_proxy: bool,
    pub feature_flags: Vec<String>,
    pub generate_code_on_save: String,
    pub restart_ts_server_on_save: bool,
    pub file_watcher: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            enable_playground_proxy: true,
            feature_flags: vec!["beta".to_string()],
            generate_code_on_save: "always".to_string(),
            restart_ts_server_on_save: false,
            file_watcher: false,
        }
    }
}

pub type SharedConfig = Arc<RwLock<EditorConfig>>;
