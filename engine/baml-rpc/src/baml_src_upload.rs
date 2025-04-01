use serde::{Deserialize, Serialize};

use crate::ast::{BamlFunctionDefinition, BamlTypeDefinition};
use crate::ast_node_id::AstNodeId;
use crate::rpc::ApiEndpoint;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BamlSrcUploadStatus {
    DoesNotExist,
    Exists,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBamlSrcUploadStatusRequest {
    pub project_id: String,
    pub baml_src_id: AstNodeId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBamlSrcUploadStatusResponse {
    pub project_id: String,
    pub baml_src_id: AstNodeId,
    pub status: BamlSrcUploadStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBamlSrcUploadRequest {
    pub project_id: String,
    pub baml_src_id: AstNodeId,
    pub function_definitions: Vec<BamlFunctionDefinition>,
    pub type_definitions: Vec<BamlTypeDefinition>,
}

impl CreateBamlSrcUploadRequest {
    pub fn to_get_baml_src_upload_status_request(&self) -> GetBamlSrcUploadStatusRequest {
        GetBamlSrcUploadStatusRequest {
            project_id: self.project_id.clone(),
            baml_src_id: self.baml_src_id.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBamlSrcUploadResponse {
    pub project_id: String,
    pub baml_src_id: AstNodeId,
}

pub struct CreateBamlSrcUpload;

/// POST /v1/baml-src/upload
impl ApiEndpoint for CreateBamlSrcUpload {
    type Request = CreateBamlSrcUploadRequest;
    type Response = CreateBamlSrcUploadResponse;

    const PATH: &'static str = "/v1/baml-src/upload";
}
