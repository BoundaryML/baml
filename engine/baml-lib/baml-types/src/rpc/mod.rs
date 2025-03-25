pub mod upload_baml_src;

pub use upload_baml_src::{
    GetBamlSrcUploadStatusRequest, GetBamlSrcUploadStatusResponse, UploadBamlSrcRequest,
    UploadBamlSrcResponse,
};

use crate::tracing;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct StudioTraceEventBatch {
    pub project_id: String,
    pub events: Vec<tracing::rpc::TraceEvent>,
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
