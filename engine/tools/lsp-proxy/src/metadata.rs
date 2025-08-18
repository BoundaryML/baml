use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub lsp_command: Vec<String>,
    pub recorded_at: SystemTime,
    pub proxy_version: String,
}

impl SessionMetadata {
    pub fn new(lsp_command: Vec<String>) -> Self {
        Self {
            lsp_command,
            recorded_at: SystemTime::now(),
            proxy_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
    
    pub fn to_json_line(&self) -> anyhow::Result<String> {
        let json = serde_json::to_string(self)?;
        Ok(format!("METADATA:{}\n", json))
    }
    
    pub fn from_json_line(line: &str) -> anyhow::Result<Option<Self>> {
        if let Some(json_str) = line.strip_prefix("METADATA:") {
            let metadata: SessionMetadata = serde_json::from_str(json_str)?;
            Ok(Some(metadata))
        } else {
            Ok(None)
        }
    }
}