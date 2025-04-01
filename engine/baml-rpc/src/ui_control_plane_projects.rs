use crate::rpc::ApiEndpoint;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Project {
    pub project_id: String,
    pub short_name: String,
    pub environments: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListProjectsRequest {
    org_slug: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListProjectsResponse {
    projects: Vec<Project>,
    total_project_count: i64,
}

struct ListProjects;

impl ApiEndpoint for ListProjects {
    type Request = ListProjectsRequest;
    type Response = ListProjectsResponse;

    const PATH: &'static str = "/v1/list-projects";
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreateProjectRequest {
    short_name: String,
    org_id: String,
    environments: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreateProjectResponse {
    project: Project,
}

struct CreateProject;

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
    pub short_name: String,
    pub environments: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateProjectResponse {
    pub project: Project,
}

struct UpdateProject;

impl ApiEndpoint for UpdateProject {
    type Request = UpdateProjectRequest;
    type Response = UpdateProjectResponse;

    const PATH: &'static str = "/v1/update-project";
}
