use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ProjectId;

/// Request to fetch blob content
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GetBlobRequest {
    /// The 64-character BLAKE3 hash of the blob
    pub blob_hash: String,
    /// The project ID that owns this blob
    #[ts(type = "string")]
    pub project_id: ProjectId,
    /// Response format: 'raw' returns binary, 'json' returns base64-encoded content
    #[serde(default)]
    pub format: BlobFormat,
}

/// Response format for blob content
#[derive(Debug, Serialize, Deserialize, TS, Default)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum BlobFormat {
    /// Return raw binary content with appropriate Content-Type header
    #[default]
    Raw,
    /// Return JSON with base64-encoded content
    Json,
}

/// JSON response format for blob content
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GetBlobResponse {
    /// The blob hash
    pub blob_hash: String,
    /// The content type of the decoded blob (e.g., "image/png")
    pub content_type: String,
    /// Base64-encoded content (exact same as used in LLM request)
    pub base64_content: String,
}
