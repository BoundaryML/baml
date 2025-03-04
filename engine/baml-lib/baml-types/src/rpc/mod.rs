pub mod upload_baml_src;
use std::sync::Arc;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::tracing::events::TraceEvent;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudioTraceEventBatch {
    pub project_id: String,
    pub events: Vec<Arc<TraceEvent>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum BamlSrcCreateUploadUrlRequest {
    V1 {
        project_id: String,
        fingerprint: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum BamlSrcCreateUploadUrlResponse {
    V1 {
        project_id: String,
        fingerprint: String,
        upload_url: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum BamlSrcBlob {
    V1 {
        project_id: String,
        fingerprint: String,
        baml_src: IndexMap<String, String>,
    },
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
