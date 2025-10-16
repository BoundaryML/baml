use serde::{Deserialize, Serialize};

use crate::{rpc::ApiEndpoint, s3::S3UploadMetadata};

/// Request to get upload URLs for blob batch
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateBlobBatchUploadUrlRequest {
    pub blob_metadata: Vec<BlobMetadataItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baml_runtime: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlobMetadataItem {
    pub blob_hash: String,
    pub function_call_id: String,
    pub media_type: Option<String>,
    pub size_bytes: usize,
}

/// Response with upload URL and existing blobs
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateBlobBatchUploadUrlResponse {
    pub s3_presigned_url: String,
    pub exclude_blobs: Vec<String>, // Hashes of blobs that already exist
    pub upload_metadata: S3UploadMetadata,
}

pub struct CreateBlobBatchUploadUrl;

// POST /v1/blobs/batch-upload-url
impl ApiEndpoint for CreateBlobBatchUploadUrl {
    type Request<'a> = CreateBlobBatchUploadUrlRequest;
    type Response<'a> = CreateBlobBatchUploadUrlResponse;

    const PATH: &'static str = "/v1/blobs/batch-upload-url";
}

/// The structure that gets uploaded to S3
#[derive(Debug, Serialize, Deserialize)]
pub struct BlobBatchUploadS3File {
    pub blobs: Vec<BlobUploadItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlobUploadItem {
    pub function_call_id: String,
    pub blob_hash: String,
    pub base64_payload: String, // Base64 string of the blob
    pub media_type: Option<String>,
}
