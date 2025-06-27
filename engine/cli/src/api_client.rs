use anyhow::Result;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

pub struct ApiClient {
    pub base_url: String,
    pub token: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Project {
    /// Example: "my-project"
    #[allow(dead_code)]
    pub project_id: String,
    /// Example: "@boundaryml/my-project"
    pub project_fqn: String,
}

// #[derive(Debug, Deserialize, Serialize)]
// pub struct GetOrCreateProjectRequest {}

// #[derive(Debug, Deserialize)]
// pub struct GetOrCreateProjectResponse {
//     pub single_project: Option<Project>,
//     #[allow(dead_code)]
//     pub first_n_projects: Vec<Project>,
//     pub total_project_count: u64,
// }

trait ApiResponse {
    async fn into_result(self) -> Result<serde_json::Value>;
}

impl ApiResponse for reqwest::Response {
    async fn into_result(self) -> Result<serde_json::Value> {
        let status = self.status();
        if status.is_success() {
            Ok(self.json().await?)
        } else {
            let resp_body = self.text().await?;
            Err(anyhow::anyhow!("request returned {status}:\n{resp_body}"))
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CreateProjectRequest {
    /// Example: "@boundaryml/baml"
    pub project_fqn: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectResponse {
    pub project: Project,
}

impl ApiClient {
    pub async fn create_project(&self, req: CreateProjectRequest) -> Result<CreateProjectResponse> {
        let resp = baml_runtime::request::create_client()?
            .put(format!("{}/v3/projects", self.base_url))
            .bearer_auth(&self.token)
            .json(&req)
            .send()
            .await?;

        let resp_body = resp.into_result().await?;
        log::debug!("resp_body: {resp_body:#}");

        Ok(serde_json::from_value(resp_body)?)
    }
}

#[derive(Debug, Serialize)]
pub struct ListProjectsRequest {
    pub org_slug: String,
}

#[derive(Debug, Deserialize)]
pub struct ListProjectsResponse {
    pub projects: Vec<Project>,
    #[allow(dead_code)]
    pub total_project_count: u64,
}

impl ApiClient {
    pub async fn list_projects(&self, req: ListProjectsRequest) -> Result<ListProjectsResponse> {
        let resp = baml_runtime::request::create_client()?
            .get(format!("{}/v3/projects", self.base_url))
            .query(&[("org_slug", &req.org_slug)])
            .bearer_auth(&self.token)
            .send()
            .await?;

        let resp_body = resp.into_result().await?;

        log::debug!("resp_body: {resp_body:#}");

        Ok(serde_json::from_value(resp_body)?)
    }
}

pub const DEPLOYMENT_ID: &str = "prod";

#[derive(Debug, Serialize)]
pub struct CreateDeploymentRequest {
    /// Map from file path (within baml_src) to the contents of the file
    ///
    /// For example, baml_src/clients.baml becomes { "clients.baml": "<contents>" }
    pub baml_src: IndexMap<String, String>,
    pub project_fqn: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateDeploymentResponse {
    #[allow(dead_code)]
    deployment_tag: String,
}

impl ApiClient {
    pub async fn create_deployment(
        &self,
        req: CreateDeploymentRequest,
    ) -> Result<CreateDeploymentResponse> {
        let resp = baml_runtime::request::create_client()?
            .post(format!("{}/v3/functions/{DEPLOYMENT_ID}", self.base_url))
            .bearer_auth(&self.token)
            .json(&req)
            .send()
            .await?;

        let resp_body = resp.into_result().await?;

        Ok(serde_json::from_value(resp_body)?)
    }
}
