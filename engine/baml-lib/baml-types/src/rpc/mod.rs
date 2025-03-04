pub mod upload_baml_src;

pub use upload_baml_src::{
    GetBamlSrcUploadStatusRequest, GetBamlSrcUploadStatusResponse, UploadBamlSrcRequest,
    UploadBamlSrcResponse,
};

use crate::tracing::events::TraceEvent;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudioTraceEventBatch {
    pub project_id: String,
    pub events: Vec<Arc<TraceEvent>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlSrcCreateUploadUrlRequest {
    pub project_id: String,
    pub baml_src_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlSrcCreateUploadUrlResponse {
    pub project_id: String,
    pub baml_src_id: String,
    pub upload_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlSrcBlob {
    pub project_id: String,
    pub baml_src_id: String,
    pub baml_src: IndexMap<String, String>,
}

// ------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum TraceEventUploadRequest {
    V1 {
        trace_event_batch: StudioTraceEventBatch,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum TraceEventUploadResponse {
    V1 { project_id: String },
}
