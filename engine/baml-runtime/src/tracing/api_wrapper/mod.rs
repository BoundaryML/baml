mod env_setup;

use anyhow::Result;
pub(super) mod api_interface;
pub(crate) mod core_types;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};

use crate::request::create_tracing_client;

pub(super) use self::api_interface::{BoundaryAPI, BoundaryTestAPI};
use self::core_types::{TestCaseStatus, UpdateTestCase};

#[derive(Debug, Clone)]
pub struct APIWrapper {
    pub(super) config: APIConfig,
}

#[derive(Debug, Clone)]
pub(super) enum APIConfig {
    LocalOnly(PartialAPIConfig),
    Web(CompleteAPIConfig),
}

impl APIConfig {
    pub fn session_id(&self) -> &str {
        match self {
            Self::LocalOnly(config) => &config.sessions_id,
            Self::Web(config) => &config.sessions_id,
        }
    }

    pub fn secret(&self) -> Option<&str> {
        match self {
            Self::LocalOnly(config) => config.api_key.as_deref(),
            Self::Web(config) => Some(config.api_key.as_str()),
        }
    }

    pub fn stage(&self) -> &str {
        match self {
            Self::LocalOnly(config) => &config.stage,
            Self::Web(config) => &config.stage,
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

    pub fn log_redaction_enabled(&self) -> bool {
        match self {
            Self::LocalOnly(config) => config.log_redaction_enabled,
            Self::Web(config) => config.log_redaction_enabled,
        }
    }

    pub fn log_redaction_placeholder(&self) -> &str {
        match self {
            Self::LocalOnly(config) => &config.log_redaction_placeholder,
            Self::Web(config) => &config.log_redaction_placeholder,
        }
    }

    #[allow(dead_code)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn copy_from(
        &self,
        base_url: Option<&str>,
        api_key: Option<&str>,
        project_id: Option<&str>,
        sessions_id: Option<&str>,
        stage: Option<&str>,
        host_name: Option<&str>,
        log_redaction_enabled: Option<bool>,
        log_redaction_placeholder: Option<String>,
        _debug_level: Option<bool>,
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
        let log_redaction_enabled = log_redaction_enabled.unwrap_or_else(|| match self {
            Self::LocalOnly(config) => config.log_redaction_enabled,
            Self::Web(config) => config.log_redaction_enabled,
        });
        let log_redaction_placeholder = log_redaction_placeholder.unwrap_or_else(|| match self {
            Self::LocalOnly(config) => config.log_redaction_placeholder.clone(),
            Self::Web(config) => config.log_redaction_placeholder.clone(),
        });

        match (api_key, project_id) {
            (Some(api_key), Some(project_id)) => Self::Web(CompleteAPIConfig {
                base_url: base_url.to_string(),
                api_key: api_key.to_string(),
                project_id: project_id.to_string(),
                stage: stage.to_string(),
                sessions_id: sessions_id.to_string(),
                host_name: host_name.to_string(),
                client: create_tracing_client().unwrap(),
                log_redaction_enabled,
                log_redaction_placeholder,
            }),
            _ => Self::LocalOnly(PartialAPIConfig {
                base_url: base_url.to_string(),
                api_key: api_key.map(String::from),
                project_id: project_id.map(String::from),
                stage: stage.to_string(),
                sessions_id: sessions_id.to_string(),
                host_name: host_name.to_string(),
                log_redaction_enabled,
                log_redaction_placeholder,
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct CompleteAPIConfig {
    pub base_url: String,
    pub api_key: String,
    pub project_id: String,
    pub stage: String,
    pub sessions_id: String,
    pub host_name: String,
    pub log_redaction_enabled: bool,
    pub log_redaction_placeholder: String,

    client: reqwest::Client,
}

#[derive(Debug, Clone)]
pub(super) struct PartialAPIConfig {
    #[allow(dead_code)]
    base_url: String,
    #[allow(dead_code)]
    api_key: Option<String>,
    project_id: Option<String>,
    stage: String,
    sessions_id: String,
    host_name: String,
    log_redaction_enabled: bool,
    log_redaction_placeholder: String,
}

impl CompleteAPIConfig {
    pub(self) async fn post<'a, T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        let url = format!("{}/{}", self.base_url, path);

        let req = self
            .client
            .post(&url)
            .json(body)
            .bearer_auth(&self.api_key)
            .build()?;

        let Ok(res) = self.client.execute(req).await else {
            return Err(anyhow::anyhow!("Failed to fetch: {url}"));
        };
        let status = res.status();
        match res.json::<T>().await {
            Ok(v) => Ok(v),
            Err(e) => Err(anyhow::anyhow!(
                "Failed to parse response: {url}. Status: {status}\n{:?}",
                e
            )),
        }
    }
}

#[derive(Deserialize)]
struct LogResponse {
    #[allow(dead_code)]
    status: Option<String>,
}

impl BoundaryAPI for CompleteAPIConfig {
    async fn check_cache(
        &self,
        _payload: &api_interface::CacheRequest,
    ) -> Result<Option<api_interface::CacheResponse>> {
        // TODO: @hellovai Implement this
        Ok(None)
        // let body = serde_json::to_value(payload)?;
        // let response = self.post("cache", &body).await?;
        // Ok(Some(serde_json::from_value(response)?))
    }

    async fn log_schema(&self, payload: &core_types::LogSchema) -> Result<()> {
        let body = serde_json::to_value(payload)?;
        self.post::<LogResponse>("log/v2", &body).await?;
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
              "test_name": "test",
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
        payload: &[api_interface::UpdateTestCaseRequest],
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
    pub fn from_env_vars<T: AsRef<str>>(value: impl Iterator<Item = (T, T)>) -> Result<Self> {
        let config = env_setup::Config::from_env_vars(value).unwrap();
        if config.log_redaction_enabled {
            log::info!("Redaction enabled: {}", config.log_redaction_enabled);
        }
        Ok(match (&config.secret, &config.project_id) {
            (Some(api_key), Some(project_id)) => Self {
                config: APIConfig::Web(CompleteAPIConfig {
                    base_url: config.base_url,
                    api_key: api_key.to_string(),
                    project_id: project_id.to_string(),
                    stage: config.stage,
                    sessions_id: config.sessions_id,
                    host_name: config.host_name,
                    client: create_tracing_client()?,
                    log_redaction_enabled: config.log_redaction_enabled,
                    log_redaction_placeholder: config.log_redaction_placeholder,
                }),
            },
            _ => Self {
                config: APIConfig::LocalOnly(PartialAPIConfig {
                    base_url: config.base_url,
                    api_key: config.secret,
                    project_id: config.project_id,
                    stage: config.stage,
                    sessions_id: config.sessions_id,
                    host_name: config.host_name,
                    log_redaction_enabled: config.log_redaction_enabled,
                    log_redaction_placeholder: config.log_redaction_placeholder,
                }),
            },
        })
    }

    pub fn enabled(&self) -> bool {
        self.config.project_id().is_some() && self.config.secret().is_some()
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
