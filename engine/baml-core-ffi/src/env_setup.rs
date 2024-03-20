use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, PartialEq, Eq)]
pub enum LogLevel {
  Verbose,
  Info,
  None,
}

struct LogLevelVisitor;

impl<'de> serde::de::Visitor<'de> for LogLevelVisitor {
  type Value = LogLevel;

  fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
    formatter.write_str("a string, a boolean, or an integer")
  }

  fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
  where
    E: serde::de::Error,
  {
    match value.to_lowercase().as_str() {
      "all" => Ok(LogLevel::Verbose),
      "llm" => Ok(LogLevel::Info),
      "none" => Ok(LogLevel::None),
      _ => Err(serde::de::Error::custom(
        "Invalid log level: use 'ALL', 'LLM', or 'NONE'",
      )),
    }
  }

  fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
  where
    E: serde::de::Error,
  {
    if value {
      Ok(LogLevel::Info)
    } else {
      Ok(LogLevel::None)
    }
  }

  fn visit_i32<E>(self, value: i32) -> Result<Self::Value, E>
  where
    E: serde::de::Error,
  {
    if value > 1 {
      Ok(LogLevel::Verbose)
    } else if value > 0 {
      Ok(LogLevel::Info)
    } else {
      Ok(LogLevel::None)
    }
  }
}

impl<'de> Deserialize<'de> for LogLevel {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    deserializer.deserialize_any(LogLevelVisitor)
  }
}

#[derive(Deserialize, Debug)]
pub struct Config {
  #[serde(default = "default_base_url")]
  pub base_url: String,
  pub api_key: Option<String>,
  pub project_id: Option<String>,
  #[serde(default = "default_sessions_id")]
  pub sessions_id: String,
  #[serde(default = "default_stage")]
  pub stage: String,
  #[serde(default = "default_host_name")]
  pub host_name: String,
  #[serde(default = "default_log_level")]
  pub log_level: LogLevel,
  pub ipc_port: Option<u16>,
}

fn default_base_url() -> String {
  "https://app.boundaryml.com".to_string()
}

fn default_sessions_id() -> String {
  uuid::Uuid::new_v4().to_string()
}

fn default_stage() -> String {
  "development".to_string()
}

fn default_host_name() -> String {
  hostname::get()
    .map(|h| h.to_string_lossy().to_string())
    .unwrap_or("unknown".to_string())
}

fn default_log_level() -> LogLevel {
  LogLevel::None
}

impl Config {
  pub fn from_env() -> Result<Self> {
    let config = envy::prefixed("BOUNDARY_").from_env::<Config>();

    match config {
      Ok(config) => Ok(config),
      Err(err) => Err(anyhow::anyhow!(
        "Failed to parse config from environment: {}",
        err
      )),
    }
  }
}
