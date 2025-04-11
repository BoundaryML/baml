use serde::{Deserialize, Serialize};

use crate::rpc::ApiEndpoint;
use crate::s3::S3UploadMetadata;
use crate::trace::TraceEvent;

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTraceEventUploadUrlRequest {
    pub upload_metadata: S3UploadMetadata,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTraceEventUploadUrlResponse {
    pub upload_url: String,
}

pub struct CreateTraceEventUploadUrl;

// POST /v1/baml-trace/create-upload-url
impl ApiEndpoint for CreateTraceEventUploadUrl {
    type Request = CreateTraceEventUploadUrlRequest;
    type Response = CreateTraceEventUploadUrlResponse;

    const PATH: &'static str = "/v1/baml-trace/create-upload-url";
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTraceEventUploadRequest {
    pub trace_event_batch: Vec<TraceEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTraceEventUploadResponse {
    pub project_id: String,
}

pub struct CreateTraceEventUpload;

// POST /v1/baml-trace
impl ApiEndpoint for CreateTraceEventUpload {
    type Request = CreateTraceEventUploadRequest;
    type Response = CreateTraceEventUploadResponse;

    const PATH: &'static str = "/v1/baml-trace";
}
