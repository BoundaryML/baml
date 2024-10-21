use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub struct ApiClient {
    pub base_url: String,
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct Project {
    #[serde(rename = "project_uuid")]
    pub dbid: String,
    // This field is returned by the API, but does not yet have concrete semantics, so we shouldn't use it yet.
    // pub name: String,
    pub auto_created: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GetOrCreateProjectRequest {}

#[derive(Debug, Deserialize)]
pub struct GetOrCreateProjectResponse {
    pub single_project: Option<Project>,
    #[allow(dead_code)]
    pub first_n_projects: Vec<Project>,
    pub total_project_count: u64,
}

impl ApiClient {
    pub async fn get_or_create_project(
        &self,
        req: GetOrCreateProjectRequest,
    ) -> Result<GetOrCreateProjectResponse> {
        let resp = baml_runtime::request::create_client()?
            .post(format!("{}/v3/projects", self.base_url))
            .bearer_auth(&self.token)
            .json(&req)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(resp
                .error_for_status()
                .context("Failed to get or create project")
                .unwrap_err());
        }

        let resp_body: serde_json::Value = resp.json().await?;
        log::debug!("resp_body: {:#}", resp_body);

        Ok(serde_json::from_value(resp_body)?)
    }
}
