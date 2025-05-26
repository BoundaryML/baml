use std::sync::Arc;

use baml_ids::ProjectId;
use serde::{Deserialize, Serialize};

use crate::{
    ast::tops::{ASTId, AST},
    rpc::ApiEndpoint,
};

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct CreateBamlSrcUploadRequest {
    // pub baml_ast_id: ASTId<'a>, // TODO(seawatts): figure out if this is actually needed
    pub ast: Arc<AST>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateBamlSrcUploadResponse {
    pub project_id: ProjectId,
    pub status: String,
}

pub struct CreateBamlSrcUpload;

/// POST /v1/baml-src/upload
impl ApiEndpoint for CreateBamlSrcUpload {
    type Request<'a> = CreateBamlSrcUploadRequest;
    type Response<'a> = CreateBamlSrcUploadResponse;

    const PATH: &'static str = "/v1/baml-src/upload";
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckBamlSrcUploadStatus {
    DoesNotExist,
    Exists,
    // TODO: add partial information for better diffs
    // and to only send the diffs to the client
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CheckBamlSrcUploadRequest<'a> {
    pub baml_ast_id: ASTId<'a>,
}

pub struct CheckBamlSrcUpload;

/// POST /v1/baml-src/check-upload
impl ApiEndpoint for CheckBamlSrcUpload {
    type Request<'a> = CheckBamlSrcUploadRequest<'a>;
    type Response<'a> = CheckBamlSrcUploadStatus;

    const PATH: &'static str = "/v1/baml-src/check-upload";
}
