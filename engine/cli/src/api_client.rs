use anyhow::Result;
use serde::{Deserialize, Serialize};

pub struct ApiClient {
    pub base_url: String,
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct Project {
    pub project_uuid: String,
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
        let resp = reqwest::Client::new()
            .post(format!("{}/v3/projects", self.base_url))
            .bearer_auth(&self.token)
            .json(&req)
            .send()
            .await?;

        let resp_body: serde_json::Value = resp.json().await?;
        log::debug!("resp_body: {:#}", resp_body);

        Ok(serde_json::from_value(resp_body)?)
    }
}
