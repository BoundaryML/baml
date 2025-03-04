use baml_types::rpc::{
    GetBamlSrcUploadStatusRequest, GetBamlSrcUploadStatusResponse, TraceEventUploadRequest,
    TraceEventUploadResponse, UploadBamlSrcRequest, UploadBamlSrcResponse,
};
use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

// ------------------------------------------------------------
// API Endpoints
// ------------------------------------------------------------
use serde::Deserialize;


pub struct GetBamlSrcUploadStatus;

/// POST /v1/baml-src/get-upload-status
impl ApiEndpoint for GetBamlSrcUploadStatus {
    type Request = GetBamlSrcUploadStatusRequest;
    type Response = GetBamlSrcUploadStatusResponse;

    fn path(&self) -> String {
        format!("v1/baml-src/get-upload-status")
    }
}

pub struct UploadBamlSrc;

/// POST /v1/baml-src/upload
impl ApiEndpoint for UploadBamlSrc {
    type Request = UploadBamlSrcRequest;
    type Response = UploadBamlSrcResponse;

    fn path(&self) -> String {
        format!("v1/baml-src/upload")
    }
}

pub struct UploadTraceEvent;

/// POST /v1/baml-trace
impl ApiEndpoint for UploadTraceEvent {
    type Request = TraceEventUploadRequest;
    type Response = TraceEventUploadResponse;

    fn path(&self) -> String {
        format!("v1/baml-trace")
    }
}

// ------------------------------------------------------------
// API Client
// ------------------------------------------------------------
#[derive(Error, Debug)]
pub enum ApiError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {0}")]
    Api(String),
}

#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    client: Client,
    project_id: String,
    api_key: Option<String>,
}

impl std::fmt::Debug for ApiClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ApiClient(base_url={}, project_id={}, api_key=<exists={}>)",
            self.base_url,
            self.project_id,
            self.api_key.is_some()
        )
    }
}

/// Trait for GET endpoints (no request body).
pub trait GetEndpoint {
    type Response: DeserializeOwned;
    /// Returns the endpoint path (e.g., "users/42").
    fn path(&self) -> String;
}

/// Trait for POST endpoints that have an associated request body and response.
pub trait ApiEndpoint {
    type Request: Serialize;
    type Response: DeserializeOwned;
    /// Returns the endpoint path (e.g., "users").
    fn path(&self) -> String;
}

impl ApiClient {
    /// Create a new API client with a base URL and an optional API key.
    pub fn new(base_url: &str, project_id: &str, api_key: Option<String>) -> Self {
        Self {
            base_url: base_url.to_owned(),
            project_id: project_id.to_owned(),
            api_key,
            client: Client::new(),
        }
    }

    fn add_headers(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let request = request.header("x-boundary-project-id", self.project_id.clone());
        if let Some(ref key) = self.api_key {
            request.header("x-boundary-api-key", key)
        } else {
            request
        }
    }
    /// Generic GET request for endpoints without a request body.
    pub async fn get<E: GetEndpoint>(&self, endpoint: E) -> Result<E::Response, ApiError> {
        let url = format!("{}/{}", self.base_url, endpoint.path());
        let request = self.client.get(&url);
        let request = self.add_headers(request);
        let response = request.send().await?.json::<E::Response>().await?;
        Ok(response)
    }

    /// Generic POST request where the request body and response are tied by type.
    pub async fn post<E: ApiEndpoint>(
        &self,
        endpoint: E,
        body: &E::Request,
    ) -> Result<E::Response, ApiError> {
        let url = format!("{}/{}", self.base_url, endpoint.path());
        println!("request url: {} -> {:?}", url, serde_json::to_string(body).unwrap());
        let request = self.client.post(&url).json(body);
        let request = self.add_headers(request);
        let response = request.send().await.map_err(|e| ApiError::Http(e))?;
        let response = response.json::<E::Response>().await?;
        Ok(response)
    }
}
