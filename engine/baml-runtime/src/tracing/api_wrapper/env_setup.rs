use anyhow::Result;
use serde::Deserialize;

use crate::RuntimeContext;

#[derive(Deserialize, Debug)]
pub struct Config {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    pub secret: Option<String>,
    pub project_id: Option<String>,
    #[serde(default = "default_sessions_id")]
    pub sessions_id: String,
    #[serde(default = "default_stage")]
    pub stage: String,
    #[serde(default = "default_host_name")]
    pub host_name: String,
}

fn default_base_url() -> String {
    "https://app.boundaryml.com/api".to_string()
}

fn default_sessions_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn default_stage() -> String {
    "development".to_string()
}

fn default_host_name() -> String {
    #[cfg(feature = "wasm")]
    {
        return "<browser>".to_string();
    }
    #[cfg(not(feature = "wasm"))]
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or("unknown".to_string())
}

impl Config {
    pub fn from_ctx(ctx: &RuntimeContext) -> Result<Self> {
        let config: Result<Config, envy::Error> = envy::prefixed("BOUNDARY_")
            .from_iter(ctx.env.iter().map(|(k, v)| (k.to_string(), v.to_string())));

        match config {
            Ok(config) => Ok(config),
            Err(err) => Err(anyhow::anyhow!(
                "Failed to parse config from environment variables: {}",
                err
            )),
        }
    }
}
