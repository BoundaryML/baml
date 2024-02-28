use anyhow::Result;
pub(super) mod api_interface;
pub(super) mod core_types;
use serde_json::Value;

use self::api_interface::BoundaryAPI;

#[derive(Debug, Clone)]
pub(crate) enum APIWrapper {
    LocalOnly(PartialAPIConfig),
    Web(APIConfig),
}

const DEFAULT_BASE_URL: &str = "https://api.boundary.com";

impl APIWrapper {
    pub fn pretty_print(&self, payload: &core_types::LogSchema) {
        let log_level = match self {
            Self::LocalOnly(config) => config.log_level,
            Self::Web(config) => config.log_level,
        };

        if log_level {
            println!("{:?}", payload.pretty_string());
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
        let base_url = std::env::var("BASE_URL").unwrap_or(DEFAULT_BASE_URL.to_string());
        let api_key = std::env::var("BOUNDARY_API_KEY").ok();
        let project_id = std::env::var("BOUNDARY_PROJECT_ID").ok();
        let sessions_id =
            std::env::var("BOUNDARY_SESSIONS_ID").unwrap_or(uuid::Uuid::new_v4().to_string());
        let stage = std::env::var("BOUNDARY_STAGE").unwrap_or("development".to_string());
        let host_name = std::env::var("BOUNDARY_HOST_NAME").unwrap_or(
            hostname::get()
                .map(|host| host.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
        );
        let log_level = std::env::var("BOUNDARY_LOG_LEVEL")
            .ok()
            .map(|v| v != "true")
            .unwrap_or(true);

        match (&api_key, &project_id) {
            (Some(api_key), Some(project_id)) => Self::Web(APIConfig {
                log_level,
                base_url,
                api_key: api_key.to_string(),
                project_id: project_id.to_string(),
                stage,
                sessions_id,
                host_name,
            }),
            _ => Self::LocalOnly(PartialAPIConfig {
                log_level,
                base_url,
                api_key,
                project_id,
                stage,
                sessions_id,
                host_name,
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
        let api_key = api_key.or_else(|| match self {
            Self::LocalOnly(config) => config.api_key.as_deref(),
            Self::Web(config) => Some(config.api_key.as_str()),
        });
        let project_id = project_id.or_else(|| match self {
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

        match (api_key, project_id) {
            (Some(api_key), Some(project_id)) => Self::Web(APIConfig {
                log_level,
                base_url: base_url.to_string(),
                api_key: api_key.to_string(),
                project_id: project_id.to_string(),
                stage: stage.to_string(),
                sessions_id: sessions_id.to_string(),
                host_name: host_name.to_string(),
            }),
            _ => Self::LocalOnly(PartialAPIConfig {
                log_level,
                base_url: base_url.to_string(),
                api_key: api_key.map(String::from),
                project_id: project_id.map(String::from),
                stage: stage.to_string(),
                sessions_id: sessions_id.to_string(),
                host_name: host_name.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct APIConfig {
    log_level: bool,
    pub base_url: String,
    pub api_key: String,
    pub project_id: String,
    pub stage: String,
    pub sessions_id: String,
    pub host_name: String,
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
}

impl APIConfig {
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

impl BoundaryAPI for APIConfig {
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
}

impl BoundaryAPI for APIWrapper {
    async fn check_cache(
        &self,
        payload: &api_interface::CacheRequest,
    ) -> Result<Option<api_interface::CacheResponse>> {
        match self {
            Self::LocalOnly(_) => Ok(None),
            Self::Web(config) => config.check_cache(payload).await,
        }
    }

    async fn log_schema(&self, payload: &core_types::LogSchema) -> Result<()> {
        match self {
            Self::LocalOnly(_) => Ok(()),
            Self::Web(config) => config.log_schema(payload).await,
        }
    }
}
