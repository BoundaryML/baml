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

/// Configuration for API key injection per provider
/// Format: (origin_url, header_name, env_var_name, baml_header_name)
const API_PROVIDERS: &[(&str, &str, &str, &str)] = &[
    (
        "https://api.openai.com",
        "Authorization",
        "OPENAI_API_KEY",
        "baml-openai-api-key",
    ),
    (
        "https://api.anthropic.com",
        "x-api-key",
        "ANTHROPIC_API_KEY",
        "baml-anthropic-api-key",
    ),
    (
        "https://generativelanguage.googleapis.com",
        "x-goog-api-key",
        "GOOGLE_API_KEY",
        "baml-google-api-key",
    ),
    (
        "https://openrouter.ai",
        "Authorization",
        "OPENROUTER_API_KEY",
        "baml-openrouter-api-key",
    ),
    (
        "https://api.llmapi.com",
        "Authorization",
        "LLAMA_API_KEY",
        "baml-llama-api-key",
    ),
];

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
            HeaderName::from_static("baml-openai-api-key"),
            HeaderName::from_static("baml-anthropic-api-key"),
            HeaderName::from_static("baml-google-api-key"),
            HeaderName::from_static("baml-openrouter-api-key"),
            HeaderName::from_static("baml-llama-api-key"),
        ])
        .max_age(std::time::Duration::from_secs(86400));

    Router::new()
        .route("/static/{*path}", get(serve_static_file))
        .route("/{*path}", options(handle_preflight))
        .route("/{*path}", any(handle_proxy_request))
        .layer(cors)
}

/// Handle CORS preflight requests
async fn handle_preflight() -> impl IntoResponse {
    StatusCode::OK
}

/// Serve static files from the current working directory
async fn serve_static_file(Path(path): Path<String>) -> Result<AxumResponse, ProxyError> {
    let file_path = path.strip_prefix("static/").unwrap_or(&path);
    let current_dir = std::env::current_dir()
        .map_err(|e| ProxyError::internal_error(format!("Failed to get current dir: {e}")))?;

    // Try multiple potential base directories
    let potential_paths = vec![
        current_dir.join(file_path),
        current_dir.join("baml_src").join(file_path),
        current_dir.join("../baml_src").join(file_path),
    ];

    let absolute_path = potential_paths
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or_else(|| current_dir.join(file_path));

    match fs::read(&absolute_path).await {
        Ok(contents) => {
            let mime_type = from_path(file_path).first_or_octet_stream();
            let content_type = mime_type.as_ref().to_string();

            Ok((
                StatusCode::OK,
                [
                    ("content-type", content_type.as_str()),
                    ("access-control-allow-origin", "*"),
                ],
                contents,
            )
                .into_response())
        }
        Err(err) => {
            tracing::warn!("Failed to read static file {}: {}", file_path, err);

            match err.kind() {
                std::io::ErrorKind::NotFound => Err(ProxyError::not_found(format!(
                    "File not found: {file_path}"
                ))),
                std::io::ErrorKind::PermissionDenied => Err(ProxyError::new(
                    format!("Permission denied: {file_path}"),
                    StatusCode::FORBIDDEN,
                )),
                _ => Err(ProxyError::internal_error(format!(
                    "Error reading file: {file_path}"
                ))),
            }
        }
    }
}

/// Main proxy request handler
async fn handle_proxy_request(
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> Result<AxumResponse, ProxyError> {
    let path_str = uri.path();

    // Handle static file serving
    if path_str.starts_with("/static/") && method == Method::GET {
        return serve_static_file(Path(path_str.to_string())).await;
    }

    // Extract and validate the original URL
    let original_url = extract_original_url(&headers)?;
    let mut clean_headers = clean_headers(&headers);

    // Parse the target URL
    let mut target_url = parse_target_url(&original_url)?;

    // Handle simple GET requests that don't need proxying
    if is_simple_get_request(&method, path_str) {
        return Ok(create_empty_response().into_response());
    }

    // Construct the final URL path
    let final_path = construct_final_path(&target_url, path_str);
    target_url.set_path(&final_path);

    // Read the body
    let body_bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .map_err(|e| ProxyError::bad_request(format!("Failed to read body: {e}")))?;

    // Inject API keys for supported providers
    inject_api_key(&mut clean_headers, &target_url, &headers);

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
    let headers_to_remove = ["baml-original-url", "origin", "authorization", "host"];

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

/// Check if this is a simple GET request that doesn't need proxying
fn is_simple_get_request(method: &Method, path: &str) -> bool {
    path.matches('.').count() == 1 && method == Method::GET
}

/// Create an empty successful response
fn create_empty_response() -> impl IntoResponse {
    (StatusCode::OK, [("access-control-allow-origin", "*")], "")
}

/// Construct the final path for the target URL
fn construct_final_path(url: &url::Url, path_str: &str) -> String {
    let base_path = url.path().trim_end_matches('/');

    let final_path = if base_path.is_empty() {
        path_str.trim_end_matches('/').to_string()
    } else if !path_str.starts_with(base_path) {
        if path_str.starts_with('/') {
            format!("{base_path}{path_str}")
        } else {
            format!("{base_path}/{path_str}")
        }
    } else {
        path_str.to_string()
    };

    final_path.trim_end_matches('/').to_string()
}

/// Inject appropriate API key based on the target URL
fn inject_api_key(headers: &mut HeaderMap, target_url: &url::Url, original_headers: &HeaderMap) {
    let origin = get_origin_string(target_url);

    for (allowed_origin, header_name, env_var, baml_header) in API_PROVIDERS {
        if origin == *allowed_origin {
            if let Some(api_key) = get_api_key(env_var, baml_header, original_headers) {
                let header_value = format_api_key_header(header_name, &api_key);
                if let Ok(header_val) = header_value.parse() {
                    headers.insert(*header_name, header_val);
                }
            }
            break;
        }
    }
}

/// Convert URL origin to string format
fn get_origin_string(url: &url::Url) -> String {
    match url.origin() {
        url::Origin::Tuple(scheme, host, port) => match (scheme.as_str(), port) {
            ("http", 80) | ("https", 443) => format!("{scheme}://{host}"),
            _ => format!("{scheme}://{host}:{port}"),
        },
        url::Origin::Opaque(_) => String::new(),
    }
}

/// Get API key from environment or headers
fn get_api_key(env_var: &str, baml_header: &str, headers: &HeaderMap) -> Option<String> {
    // Try environment variable first
    std::env::var(env_var)
        .ok()
        // Then try custom header
        .or_else(|| {
            headers
                .get(baml_header)
                .and_then(|v| v.to_str().ok())
                .map(String::from)
        })
}

/// Format API key header value based on header type
fn format_api_key_header(header_name: &str, api_key: &str) -> String {
    if header_name == "Authorization" {
        format!("Bearer {api_key}")
    } else {
        api_key.to_string()
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
    fn test_get_origin_string() {
        let url = url::Url::parse("https://api.example.com/v1/chat").unwrap();
        assert_eq!(get_origin_string(&url), "https://api.example.com");

        let url = url::Url::parse("http://localhost:8080/api").unwrap();
        assert_eq!(get_origin_string(&url), "http://localhost:8080");
    }

    #[test]
    fn test_format_api_key_header() {
        assert_eq!(
            format_api_key_header("Authorization", "sk-123"),
            "Bearer sk-123"
        );
        assert_eq!(format_api_key_header("x-api-key", "key123"), "key123");
    }

    #[test]
    fn test_proxy_server_creation() {
        let server = ProxyServer::new();
        assert!(matches!(server, ProxyServer {}));

        let default_server = ProxyServer::default();
        assert!(matches!(default_server, ProxyServer {}));
    }
}
