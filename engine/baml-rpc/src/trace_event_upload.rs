use serde::{Deserialize, Serialize};

use crate::rpc::ApiEndpoint;
use crate::trace::TraceEvent;

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
