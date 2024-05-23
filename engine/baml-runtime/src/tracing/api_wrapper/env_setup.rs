use std::collections::HashMap;

use anyhow::Result;
use serde::Deserialize;

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
    #[cfg(target_arch = "wasm32")]
    return "<browser>".to_string();

    #[cfg(not(target_arch = "wasm32"))]
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or("unknown".to_string())
}

impl Config {
    pub fn from_env_vars<T: AsRef<str>>(env_vars: impl Iterator<Item = (T, T)>) -> Result<Self> {
        let config: Result<Config, envy::Error> = envy::prefixed("BOUNDARY_")
            .from_iter(env_vars.map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string())));

        match config {
            Ok(config) => Ok(config),
            Err(err) => Err(anyhow::anyhow!(
                "Failed to parse config from environment variables: {}",
                err
            )),
        }
    }
}
