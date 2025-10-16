use serde::{Deserialize, Serialize};

use crate::{rpc::ApiEndpoint, s3::S3UploadMetadata};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTraceEventUploadUrlRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baml_runtime: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTraceEventUploadUrlResponse {
    pub upload_url: String,
    pub upload_metadata: S3UploadMetadata,
}

pub struct CreateTraceEventUploadUrl;

// POST /v1/baml-trace/create-upload-url
impl ApiEndpoint for CreateTraceEventUploadUrl {
    type Request<'a> = CreateTraceEventUploadUrlRequest;
    type Response<'a> = CreateTraceEventUploadUrlResponse;

    const PATH: &'static str = "/v1/baml-trace/create-upload-url";
}
