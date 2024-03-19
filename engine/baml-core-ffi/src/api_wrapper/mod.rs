use anyhow::Result;
pub(super) mod api_interface;
pub(super) mod core_types;
mod ipc_interface;
use serde_json::{json, Value};

use crate::env_setup::{Config, LogLevel};

pub(super) use self::api_interface::{BoundaryAPI, BoundaryTestAPI};
use self::core_types::{TestCaseStatus, UpdateTestCase};

#[derive(Debug, Clone)]
pub(crate) struct APIWrapper {
  config: APIConfig,
  ipc: ipc_interface::IPCChannel,
}

#[derive(Debug, Clone)]
enum APIConfig {
  LocalOnly(PartialAPIConfig),
  Web(CompleteAPIConfig),
}

impl APIConfig {
  pub fn pretty_print(&self, payload: &core_types::LogSchema) {
    let log_level = match self {
      Self::LocalOnly(config) => config.log_level,
      Self::Web(config) => config.log_level,
    };

    if log_level {
      match payload.pretty_string() {
        Some(s) => println!("{s}"),
        None => println!("Failed to pretty print log schema {:?}", payload),
      }
    }
  }

  pub fn session_id(&self) -> &str {
    match self {
      Self::LocalOnly(config) => &config.sessions_id,
      Self::Web(config) => &config.sessions_id,
    }
  }

  pub fn stage(&self) -> &str {
    match self {
      Self::LocalOnly(config) => &config.stage,
      Self::Web(config) => &config.stage,
    }
  }

  pub fn ipc_port(&self) -> Option<u16> {
    match self {
      Self::LocalOnly(config) => config.ipc_port,
      Self::Web(config) => config.ipc_port,
    }
  }

  pub fn project_id(&self) -> Option<&str> {
    match self {
      Self::LocalOnly(config) => config.project_id.as_deref(),
      Self::Web(config) => Some(config.project_id.as_str()),
    }
  }

  pub fn host_name(&self) -> &str {
    match self {
      Self::LocalOnly(config) => &config.host_name,
      Self::Web(config) => &config.host_name,
    }
  }

  pub fn default() -> Self {
    let config = Config::from_env().unwrap();

    match (&config.api_key, &config.project_id) {
      (Some(api_key), Some(project_id)) => Self::Web(CompleteAPIConfig {
        log_level: config.log_level != LogLevel::None,
        base_url: config.base_url,
        api_key: api_key.to_string(),
        project_id: project_id.to_string(),
        stage: config.stage,
        sessions_id: config.sessions_id,
        host_name: config.host_name,
        ipc_port: config.ipc_port,
      }),
      _ => Self::LocalOnly(PartialAPIConfig {
        log_level: config.log_level != LogLevel::None,
        base_url: config.base_url,
        api_key: config.api_key,
        project_id: config.project_id,
        stage: config.stage,
        sessions_id: config.sessions_id,
        host_name: config.host_name,
        ipc_port: config.ipc_port,
      }),
    }
  }

  pub(crate) fn copy_from(
    &self,
    base_url: Option<&str>,
    api_key: Option<&str>,
    project_id: Option<&str>,
    sessions_id: Option<&str>,
    stage: Option<&str>,
    host_name: Option<&str>,
    debug_level: Option<bool>,
  ) -> Self {
    let base_url = base_url.unwrap_or(match self {
      Self::LocalOnly(config) => config.base_url.as_str(),
      Self::Web(config) => config.base_url.as_str(),
    });
    let api_key = api_key.or(match self {
      Self::LocalOnly(config) => config.api_key.as_deref(),
      Self::Web(config) => Some(config.api_key.as_str()),
    });
    let project_id = project_id.or(match self {
      Self::LocalOnly(config) => config.project_id.as_deref(),
      Self::Web(config) => Some(config.project_id.as_str()),
    });
    let sessions_id = sessions_id.unwrap_or_else(|| match self {
      Self::LocalOnly(config) => &config.sessions_id,
      Self::Web(config) => &config.sessions_id,
    });
    let stage = stage.unwrap_or_else(|| match self {
      Self::LocalOnly(config) => &config.stage,
      Self::Web(config) => &config.stage,
    });
    let host_name = host_name.unwrap_or_else(|| match self {
      Self::LocalOnly(config) => &config.host_name,
      Self::Web(config) => &config.host_name,
    });

    let log_level = debug_level.unwrap_or(match self {
      Self::LocalOnly(config) => config.log_level,
      Self::Web(config) => config.log_level,
    });

    let ipc_port = match self {
      Self::LocalOnly(config) => config.ipc_port,
      Self::Web(config) => config.ipc_port,
    };

    match (api_key, project_id) {
      (Some(api_key), Some(project_id)) => Self::Web(CompleteAPIConfig {
        log_level,
        base_url: base_url.to_string(),
        api_key: api_key.to_string(),
        project_id: project_id.to_string(),
        stage: stage.to_string(),
        sessions_id: sessions_id.to_string(),
        host_name: host_name.to_string(),
        ipc_port,
      }),
      _ => Self::LocalOnly(PartialAPIConfig {
        log_level,
        base_url: base_url.to_string(),
        api_key: api_key.map(String::from),
        project_id: project_id.map(String::from),
        stage: stage.to_string(),
        sessions_id: sessions_id.to_string(),
        host_name: host_name.to_string(),
        ipc_port,
      }),
    }
  }
}

#[derive(Debug, Clone)]
pub(super) struct CompleteAPIConfig {
  log_level: bool,
  pub base_url: String,
  pub api_key: String,
  pub project_id: String,
  pub stage: String,
  pub sessions_id: String,
  pub host_name: String,
  pub ipc_port: Option<u16>,
}

#[derive(Debug, Clone)]
pub(super) struct PartialAPIConfig {
  log_level: bool,
  base_url: String,
  api_key: Option<String>,
  project_id: Option<String>,
  stage: String,
  sessions_id: String,
  host_name: String,
  ipc_port: Option<u16>,
}

impl CompleteAPIConfig {
  pub(self) async fn post(&self, path: &str, body: &Value) -> Result<Value> {
    let client = reqwest::Client::new();
    let url = format!("{}/{}", self.base_url, path);
    let response = client
      .post(&url)
      .header("Authorization ", format!("Bearer {}", self.api_key))
      .header("Content-Type", "application/json")
      .json(body)
      .send()
      .await?;
    let parsed = response.json().await?;
    Ok(parsed)
  }
}

impl BoundaryAPI for CompleteAPIConfig {
  async fn check_cache(
    &self,
    payload: &api_interface::CacheRequest,
  ) -> Result<Option<api_interface::CacheResponse>> {
    let body = serde_json::to_value(payload)?;
    let response = self.post("cache", &body).await?;
    Ok(Some(serde_json::from_value(response)?))
  }

  async fn log_schema(&self, payload: &core_types::LogSchema) -> Result<()> {
    let body = serde_json::to_value(payload)?;
    self.post("log/v2", &body).await?;
    Ok(())
  }

  async fn create_session(&self) -> Result<api_interface::CreateSessionResponse> {
    let body = json!({
      "project_id": self.project_id,
      "session_id": self.sessions_id,
    });

    let response = self.post("tests/create-cycle", &body).await?;
    Ok(serde_json::from_value(response)?)
  }

  async fn finish_session(&self) -> Result<()> {
    Ok(())
  }
}

impl BoundaryAPI for APIWrapper {
  async fn check_cache(
    &self,
    payload: &api_interface::CacheRequest,
  ) -> Result<Option<api_interface::CacheResponse>> {
    match &self.config {
      APIConfig::LocalOnly(_) => Ok(None),
      APIConfig::Web(config) => config.check_cache(payload).await,
    }
  }

  async fn log_schema(&self, payload: &core_types::LogSchema) -> Result<()> {
    let _ = self.ipc.send(ipc_interface::IPCMessage::Log(payload)).await;

    match &self.config {
      APIConfig::LocalOnly(_) => Ok(()),
      APIConfig::Web(config) => config.log_schema(payload).await,
    }
  }

  async fn create_session(&self) -> Result<api_interface::CreateSessionResponse> {
    let result = match &self.config {
      APIConfig::LocalOnly(config) => Ok(api_interface::CreateSessionResponse {
        session_id: config.sessions_id.clone(),
        dashboard_url: None,
      }),
      APIConfig::Web(config) => config.create_session().await,
    };

    match &result {
      Ok(response) => {
        if let Some(dashboard_url) = &response.dashboard_url {
          let _ = self
            .ipc
            .send(ipc_interface::IPCMessage::TestRunMeta(
              &ipc_interface::TestRunMeta {
                dashboard_url: dashboard_url.clone(),
              },
            ))
            .await;
        }
      }
      _ => {}
    }

    result
  }

  async fn finish_session(&self) -> Result<()> {
    match &self.config {
      APIConfig::LocalOnly(_) => Ok(()),
      APIConfig::Web(config) => config.finish_session().await,
    }
  }
}

impl BoundaryTestAPI for APIWrapper {
  async fn register_test_cases<T: IntoIterator<Item = (String, String)>>(
    &self,
    payload: T,
  ) -> Result<()> {
    // TODO: We should probably batch these requests
    let queries = payload.into_iter().map(|(suite_name, test_name)| {
      json!({
        "project_id": self.config.project_id(),
        "test_cycle_id": self.config.session_id(),
        "test_dataset_name": suite_name,
        // Deprecated (exists legacy api reason)
        "test_case_definition_name": "test",
        "test_case_args": [{"name": test_name}],
      })
    });

    match &self.config {
      APIConfig::LocalOnly(_) => Ok(()),
      APIConfig::Web(config) => {
        for query in queries {
          config.post("tests/create-case", &query).await?;
        }
        Ok(())
      }
    }
  }

  async fn update_test_case_batch(
    &self,
    payload: &Vec<api_interface::UpdateTestCaseRequest>,
  ) -> Result<()> {
    let res = payload
      .iter()
      .map(|p| self.update_test_case(&p.test_suite, &p.test_case, p.status, None));

    // Await all the requests
    for r in res {
      r.await?;
    }

    Ok(())
  }

  async fn update_test_case(
    &self,
    test_suite: &str,
    test_case: &str,
    status: TestCaseStatus,
    error_data: Option<Value>,
  ) -> Result<()> {
    let body = UpdateTestCase {
      project_id: self.config.project_id().map(String::from),
      test_cycle_id: self.config.session_id().to_string(),
      test_dataset_name: test_suite.to_string(),
      // Deprecated (exists legacy api reason)
      test_case_definition_name: "test".to_string(),
      test_case_arg_name: test_case.to_string(),
      status,
      error_data,
    };

    let _ = self
      .ipc
      .send(ipc_interface::IPCMessage::UpdateTestCase(&body))
      .await;

    match &self.config {
      APIConfig::LocalOnly(_) => Ok(()),
      APIConfig::Web(config) => {
        config.post("tests/update", &json!(body)).await?;
        Ok(())
      }
    }
  }
}

impl APIWrapper {
  pub fn default() -> Self {
    let config = APIConfig::default();
    let ipc_addr = config.ipc_port().map(|port| format!("127.0.0.1:{}", port));
    let ipc = ipc_interface::IPCChannel::new(ipc_addr).unwrap();
    Self { config, ipc }
  }

  pub fn is_test_mode(&self) -> bool {
    match &self.config {
      APIConfig::LocalOnly(_) => true,
      APIConfig::Web(_) => false,
    }
  }

  pub fn pretty_print(&self, payload: &core_types::LogSchema) {
    self.config.pretty_print(payload);
  }

  pub fn project_id(&self) -> Option<&str> {
    self.config.project_id()
  }

  pub fn session_id(&self) -> &str {
    self.config.session_id()
  }

  pub fn stage(&self) -> &str {
    self.config.stage()
  }

  pub fn host_name(&self) -> &str {
    self.config.host_name()
  }
}
