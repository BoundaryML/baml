use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{ast::tops::AST, rpc::ApiEndpoint, s3::S3UploadMetadata};

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckBamlSrcUploadRequest {
    pub baml_src_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baml_runtime: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckBamlSrcUploadResponse {
    pub should_upload: bool,
    pub upload_url: Option<String>,
    pub upload_metadata: Option<S3UploadMetadata>,
}

pub struct CheckBamlSrcUpload;

// POST /v1/baml-src/check-upload
impl ApiEndpoint for CheckBamlSrcUpload {
    type Request<'a> = CheckBamlSrcUploadRequest;
    type Response<'a> = CheckBamlSrcUploadResponse;

    const PATH: &'static str = "/v1/baml-src/check-upload";
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BamlSrcUploadS3File {
    pub ast: Arc<AST>,
}
