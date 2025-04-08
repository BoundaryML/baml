use std::sync::Arc;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::tracing::events::TraceEvent;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceEventBatch {
    pub events: Vec<Arc<TraceEvent>>,
}

// ------------------------------------------------------------------------------------------------

// TODO: version handling should be non-exhaustive for all of these
// clients need to say "i can only handle v1 responses"

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BamlSrcUploadStatus {
    Pending,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum GetBamlSrcUploadStatusRequest {
    V1 {
        project_id: String,
        fingerprint: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum GetBamlSrcUploadStatusResponse {
    V1 {
        project_id: String,
        fingerprint: String,
        status: BamlSrcUploadStatus,
    },
}

// ------------------------------------------------------------------------------------------------

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
        project_id: String,
        trace_event_batch: TraceEventBatch,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum TraceEventUploadResponse {
    V1 { project_id: String },
}
