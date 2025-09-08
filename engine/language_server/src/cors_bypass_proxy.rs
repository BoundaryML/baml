use std::collections::HashMap;

use anyhow::Result;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, Method, Request, Response, StatusCode, Uri},
    response::{IntoResponse, Response as AxumResponse},
    routing::{any, get, options},
    Router,
};
use mime_guess::from_path;
use serde::Deserialize;
use tokio::{fs, net::TcpListener};
use tower_http::cors::{Any, CorsLayer};

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub port: u16,
}

/// Custom error type for proxy operations
#[derive(Debug)]
pub struct ProxyError {
    pub message: String,
    pub status: StatusCode,
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> AxumResponse {
        let error_json = format!(
            r#"{{"code": {}, "message": "{}"}}"#,
            self.status.as_u16(),
            self.message
        );

        (
            self.status,
            [
                ("content-type", "application/json"),
                ("access-control-allow-origin", "*"),
            ],
            error_json,
        )
            .into_response()
    }
}

impl ProxyError {
    fn new(message: impl Into<String>, status: StatusCode) -> Self {
        Self {
            message: message.into(),
            status,
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(message, StatusCode::BAD_REQUEST)
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::new(message, StatusCode::NOT_FOUND)
    }

    fn internal_error(message: impl Into<String>) -> Self {
        Self::new(message, StatusCode::INTERNAL_SERVER_ERROR)
    }
}

pub fn create_proxy_router() -> Router<ProxyConfig> {
    use axum::http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        HeaderName,
    };

    // Create CORS layer that allows all origins and methods
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers([
            CONTENT_TYPE,
            AUTHORIZATION,
            HeaderName::from_static("x-api-key"),
            HeaderName::from_static("baml-original-url"),
        ])
        .max_age(std::time::Duration::from_secs(86400));

    Router::new()
        .route("/{*path}", options(handle_preflight))
        .route("/{*path}", any(handle_proxy_request))
        .layer(cors)
}

/// Handle CORS preflight requests
async fn handle_preflight() -> impl IntoResponse {
    StatusCode::OK
}

/// Main proxy request handler
async fn handle_proxy_request(
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> Result<AxumResponse, ProxyError> {
    tracing::debug!(
        "handle_proxy_request: {:?}: {:?}: {:?}",
        method,
        uri,
        headers
    );
    let path_str = uri.path();

    // Extract and validate the original URL
    let original_url = extract_original_url(&headers)?;
    let clean_headers = clean_headers(&headers);

    // Parse the target URL
    let mut target_url = parse_target_url(&original_url)?;

    // Handle image requests that should return empty content (matching Express behavior)
    if is_image_request(&method, path_str) {
        return Ok(create_empty_response().into_response());
    }

    // Construct the final URL path
    let final_path = construct_final_path(&target_url, path_str);
    target_url.set_path(&final_path);

    // Read the body
    let body_bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .map_err(|e| ProxyError::bad_request(format!("Failed to read body: {e}")))?;

    // Execute the request and return the response
    execute_request(method, &target_url, clean_headers, body_bytes.to_vec()).await
}

/// Extract the original URL from headers
fn extract_original_url(headers: &HeaderMap) -> Result<String, ProxyError> {
    headers
        .get("baml-original-url")
        .and_then(|url| url.to_str().ok())
        .map(String::from)
        .ok_or_else(|| ProxyError::bad_request("Missing baml-original-url header"))
}

/// Remove headers that shouldn't be forwarded
fn clean_headers(headers: &HeaderMap) -> HeaderMap {
    let mut clean_headers = headers.clone();
    let headers_to_remove = ["baml-original-url", "origin", "host"];

    for header_name in &headers_to_remove {
        clean_headers.remove(*header_name);
    }

    clean_headers
}

/// Parse and validate the target URL
fn parse_target_url(url_str: &str) -> Result<url::Url, ProxyError> {
    let clean_url = url_str.trim_end_matches('/');
    url::Url::parse(clean_url).map_err(|e| ProxyError::bad_request(format!("Invalid URL: {e}")))
}

/// Check if this is an image request that should return empty content
fn is_image_request(method: &Method, path: &str) -> bool {
    if method != Method::GET {
        return false;
    }

    // Match the Express regex: /\.(png|jpe?g|gif|bmp|webp|svg)$/i
    let path_lower = path.to_lowercase();
    path_lower.ends_with(".png")
        || path_lower.ends_with(".jpg")
        || path_lower.ends_with(".jpeg")
        || path_lower.ends_with(".gif")
        || path_lower.ends_with(".bmp")
        || path_lower.ends_with(".webp")
        || path_lower.ends_with(".svg")
}

/// Create an empty successful response
fn create_empty_response() -> impl IntoResponse {
    (StatusCode::OK, [("access-control-allow-origin", "*")], "")
}

/// Construct the final path for the target URL (matching Express behavior)
fn construct_final_path(url: &url::Url, path_str: &str) -> String {
    let base_path = url.path().trim_end_matches('/');

    if base_path.is_empty() {
        path_str.to_string()
    } else if !path_str.starts_with(base_path) {
        // Match Express logic: basePath + (req.url.startsWith('/') ? '' : '/') + req.url
        if path_str.starts_with('/') {
            format!("{base_path}{path_str}")
        } else {
            format!("{base_path}/{path_str}")
        }
    } else {
        path_str.to_string()
    }
}

/// Execute the HTTP request and handle the response
async fn execute_request(
    method: Method,
    target_url: &url::Url,
    headers: HeaderMap,
    body: Vec<u8>,
) -> Result<AxumResponse, ProxyError> {
    let client = reqwest::Client::new();

    // Convert axum method to reqwest method
    let reqwest_method = match method {
        Method::GET => reqwest::Method::GET,
        Method::POST => reqwest::Method::POST,
        Method::PUT => reqwest::Method::PUT,
        Method::DELETE => reqwest::Method::DELETE,
        Method::PATCH => reqwest::Method::PATCH,
        Method::HEAD => reqwest::Method::HEAD,
        Method::OPTIONS => reqwest::Method::OPTIONS,
        _ => reqwest::Method::GET, // fallback
    };

    // Build reqwest request
    let mut reqwest_builder = client.request(reqwest_method, target_url.as_str());

    // Add headers
    for (name, value) in headers.iter() {
        if let Ok(value_str) = value.to_str() {
            reqwest_builder = reqwest_builder.header(name.as_str(), value_str);
        }
    }

    // Add body
    reqwest_builder = reqwest_builder.body(body);

    let response = reqwest_builder
        .send()
        .await
        .map_err(|e| ProxyError::internal_error(format!("Request failed: {e}")))?;

    let status = response.status();
    let response_headers = response.headers().clone();
    let body_bytes = response
        .bytes()
        .await
        .map_err(|e| ProxyError::internal_error(format!("Failed to read response: {e}")))?;

    tracing::debug!(
        "[PROXY] {} {} → {:?} | status: {}",
        method,
        target_url.path(),
        target_url.origin(),
        status
    );

    // Convert reqwest status to axum status
    let axum_status =
        StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    // Build response headers, ensuring CORS headers are present
    let mut response_header_map = HeaderMap::new();
    response_header_map.insert("access-control-allow-origin", "*".parse().unwrap());

    // Copy response headers from the upstream response
    for (name, value) in response_headers.iter() {
        response_header_map.insert(name, value.clone());
    }

    Ok((axum_status, response_header_map, body_bytes.to_vec()).into_response())
}

/// Standalone proxy server
#[derive(Debug)]
pub struct ProxyServer {}

impl ProxyServer {
    /// Create a new proxy server instance
    pub fn new() -> Self {
        Self {}
    }

    pub async fn run(self, listener: TcpListener) -> Result<(), Box<dyn std::error::Error + Send>> {
        let config = ProxyConfig {
            port: listener
                .local_addr()
                .expect("Listener should always have a port")
                .port(),
        }; // Port is determined by the listener

        let app = create_proxy_router().with_state(config);

        tracing::info!(
            "Starting Proxy server on {}",
            listener.local_addr().unwrap()
        );
        axum::serve(listener, app)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)
    }
}

impl Default for ProxyServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_construct_final_path() {
        let url = url::Url::parse("https://api.example.com/v1").unwrap();
        assert_eq!(
            construct_final_path(&url, "/chat/completions"),
            "/v1/chat/completions"
        );

        let url = url::Url::parse("https://api.example.com").unwrap();
        assert_eq!(
            construct_final_path(&url, "/chat/completions"),
            "/chat/completions"
        );
    }

    #[test]
    fn test_is_image_request() {
        use axum::http::Method;

        // Test image extensions
        assert!(is_image_request(&Method::GET, "/path/image.png"));
        assert!(is_image_request(&Method::GET, "/path/image.jpg"));
        assert!(is_image_request(&Method::GET, "/path/image.jpeg"));
        assert!(is_image_request(&Method::GET, "/path/image.gif"));
        assert!(is_image_request(&Method::GET, "/path/image.bmp"));
        assert!(is_image_request(&Method::GET, "/path/image.webp"));
        assert!(is_image_request(&Method::GET, "/path/image.svg"));

        // Test case insensitive
        assert!(is_image_request(&Method::GET, "/path/IMAGE.PNG"));

        // Test non-GET methods
        assert!(!is_image_request(&Method::POST, "/path/image.png"));

        // Test non-image paths
        assert!(!is_image_request(&Method::GET, "/api/endpoint"));
        assert!(!is_image_request(&Method::GET, "/pdf/2305.08675"));
    }

    #[test]
    fn test_proxy_server_creation() {
        let server = ProxyServer::new();
        assert!(matches!(server, ProxyServer {}));

        let default_server = ProxyServer::default();
        assert!(matches!(default_server, ProxyServer {}));
    }
}
