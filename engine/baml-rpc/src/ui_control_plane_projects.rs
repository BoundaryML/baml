use crate::rpc::ApiEndpoint;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Project {
    pub project_id: String,
    pub project_slug: String,
    pub org_id: String,
    pub environments: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListProjectsRequest {
    pub org_id: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListProjectsResponse {
    pub projects: Vec<Project>,
    pub total_project_count: i64,
}

pub struct ListProjects;

impl ApiEndpoint for ListProjects {
    type Request = ListProjectsRequest;
    type Response = ListProjectsResponse;

    const PATH: &'static str = "/v1/list-projects";
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreateProjectRequest {
    pub project_slug: String,
    pub org_id: String,
    pub environments: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreateProjectResponse {
    pub project: Project,
}

pub struct CreateProject;

impl ApiEndpoint for CreateProject {
    type Request = CreateProjectRequest;
    type Response = CreateProjectResponse;

    const PATH: &'static str = "/v1/create-project";
}

// TODO: fill in partial fields
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateProjectRequest {
    pub project_id: String,
    pub project_slug: Option<String>,
    pub environments: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateProjectResponse {
    pub project: Project,
}

pub struct UpdateProject;

impl ApiEndpoint for UpdateProject {
    type Request = UpdateProjectRequest;
    type Response = UpdateProjectResponse;

    const PATH: &'static str = "/v1/update-project";
}
