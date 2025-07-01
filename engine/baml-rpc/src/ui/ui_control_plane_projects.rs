use baml_ids::ProjectId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::rpc::ApiEndpoint;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Project {
    #[ts(type = "string")]
    pub project_id: ProjectId,
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
    type Request<'a> = ListProjectsRequest;
    type Response<'a> = ListProjectsResponse;

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
    type Request<'a> = CreateProjectRequest;
    type Response<'a> = CreateProjectResponse;

    const PATH: &'static str = "/v1/create-project";
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateProjectRequest {
    #[ts(type = "string")]
    pub project_id: ProjectId,
    #[ts(optional)]
    pub project_slug: Option<String>,
    #[ts(optional)]
    pub environments: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateProjectResponse {
    pub project: Project,
}

pub struct UpdateProject;

impl ApiEndpoint for UpdateProject {
    type Request<'a> = UpdateProjectRequest;
    type Response<'a> = UpdateProjectResponse;

    const PATH: &'static str = "/v1/update-project";
}
