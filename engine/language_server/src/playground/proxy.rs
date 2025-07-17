use anyhow::Result;
use std::sync::Arc;
use warp::{http, Filter, Rejection, Reply};

/// Custom response type for binary data with CORS headers
struct BinaryResponse {
    body: Vec<u8>,
    status: http::StatusCode,
}

impl warp::Reply for BinaryResponse {
    fn into_response(self) -> warp::http::Response<warp::hyper::Body> {
        warp::http::Response::builder()
            .status(self.status)
            .header("access-control-allow-origin", "*")
            .body(warp::hyper::Body::from(self.body))
            .unwrap()
    }
}

/// Custom error type for proxy operations
#[derive(Debug)]
struct ProxyError;

impl warp::reject::Reject for ProxyError {}

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

/// Fallback API keys for development and testing
/// TODO: Remove these in production builds
const FALLBACK_API_KEYS: &[(&str, &str)] = &[
    ("OPENAI_API_KEY", "sk-dummy-openai-key-for-testing-only"),
    (
        "ANTHROPIC_API_KEY",
        "sk-ant-dummy-anthropic-key-for-testing-only",
    ),
    ("GOOGLE_API_KEY", "dummy-google-api-key-for-testing-only"),
    (
        "OPENROUTER_API_KEY",
        "sk-dummy-openrouter-key-for-testing-only",
    ),
    ("LLAMA_API_KEY", "sk-dummy-llama-key-for-testing-only"),
];

/// Proxy server for handling CORS and API key injection
pub struct ProxyServer {
    port: u16,
}

impl ProxyServer {
    /// Create a new proxy server that will listen on the specified port
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    /// Start the proxy server and listen for requests
    pub async fn start(self) -> Result<()> {
        let addr = ([127, 0, 0, 1], self.port);

        // CORS preflight handler
        let cors_route = warp::options()
            .and(warp::path::tail())
            .map(create_cors_response);

        // Main proxy handler
        let proxy_route = warp::any()
            .and(warp::body::bytes())
            .and(warp::method())
            .and(warp::path::tail())
            .and(warp::header::headers_cloned())
            .and_then(handle_proxy_request);

        let routes = cors_route.or(proxy_route);

        tracing::info!("Proxy server listening on port {}", self.port);
        warp::serve(routes).run(addr).await;

        Ok(())
    }
}

/// Create a CORS preflight response
fn create_cors_response(_: warp::path::Tail) -> impl Reply {
    warp::http::Response::builder()
        .status(http::StatusCode::OK)
        .header("access-control-allow-origin", "*")
        .header(
            "access-control-allow-methods",
            "GET, POST, PUT, DELETE, OPTIONS",
        )
        .header(
            "access-control-allow-headers",
            "Content-Type, Authorization, x-api-key, baml-original-url, \
             baml-openai-api-key, baml-anthropic-api-key, baml-google-api-key, \
             baml-openrouter-api-key, baml-llama-api-key",
        )
        .header("access-control-max-age", "86400")
        .body(warp::hyper::Body::empty())
        .unwrap()
}

/// Main proxy request handler
async fn handle_proxy_request(
    body: bytes::Bytes,
    method: http::Method,
    path: warp::path::Tail,
    mut headers: http::HeaderMap,
) -> Result<BinaryResponse, Rejection> {
    let path_str = path.as_str();

    // Extract and validate the original URL
    let original_url = extract_original_url(&headers)?;
    clean_headers(&mut headers);

    // Parse the target URL
    let mut target_url = parse_target_url(&original_url)?;

    // Handle simple GET requests that don't need proxying
    if is_simple_get_request(&method, path_str) {
        return Ok(create_empty_response());
    }

    // Construct the final URL path
    let final_path = construct_final_path(&target_url, path_str);
    target_url.set_path(&final_path);

    // Build the request with headers and API key injection
    let request = build_proxied_request(method.clone(), &target_url, headers, body)?;

    // Execute the request and return the response
    execute_request(request, &method, path_str, &target_url).await
}

/// Extract the original URL from headers
fn extract_original_url(headers: &http::HeaderMap) -> Result<String, Rejection> {
    headers
        .get("baml-original-url")
        .and_then(|url| url.to_str().ok())
        .map(String::from)
        .ok_or_else(|| warp::reject::custom(ProxyError))
}

/// Remove headers that shouldn't be forwarded
fn clean_headers(headers: &mut http::HeaderMap) {
    let headers_to_remove = ["baml-original-url", "origin", "authorization", "host"];
    for header_name in &headers_to_remove {
        headers.remove(*header_name);
    }
}

/// Parse and validate the target URL
fn parse_target_url(url_str: &str) -> Result<url::Url, Rejection> {
    let clean_url = url_str.trim_end_matches('/');
    url::Url::parse(clean_url).map_err(|_| warp::reject::custom(ProxyError))
}

/// Check if this is a simple GET request that doesn't need proxying
fn is_simple_get_request(method: &http::Method, path: &str) -> bool {
    path.matches('.').count() == 1 && method == http::Method::GET
}

/// Create an empty successful response
fn create_empty_response() -> BinaryResponse {
    BinaryResponse {
        body: Vec::new(),
        status: http::StatusCode::OK,
    }
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

/// Build the HTTP request with proper headers and API key injection
fn build_proxied_request(
    method: http::Method,
    target_url: &url::Url,
    headers: http::HeaderMap,
    body: bytes::Bytes,
) -> Result<http::Request<Vec<u8>>, Rejection> {
    let mut request_builder = http::Request::builder()
        .method(method)
        .uri(target_url.to_string());

    // Add existing headers
    for (name, value) in headers.iter() {
        request_builder = request_builder.header(name.as_str(), value);
    }

    // Inject API keys for supported providers
    inject_api_key(&mut request_builder, target_url, &headers);

    request_builder
        .body(body.to_vec())
        .map_err(|_| warp::reject::custom(ProxyError))
}

/// Inject appropriate API key based on the target URL
fn inject_api_key(
    request_builder: &mut http::request::Builder,
    target_url: &url::Url,
    headers: &http::HeaderMap,
) {
    let origin = get_origin_string(target_url);

    for (allowed_origin, header_name, env_var, baml_header) in API_PROVIDERS {
        if origin == *allowed_origin {
            if let Some(api_key) = get_api_key(env_var, baml_header, headers) {
                let header_value = format_api_key_header(header_name, &api_key);
                let new_builder = std::mem::replace(request_builder, http::Request::builder());
                *request_builder = new_builder.header(*header_name, header_value);
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

/// Get API key from environment, headers, or fallback
fn get_api_key(env_var: &str, baml_header: &str, headers: &http::HeaderMap) -> Option<String> {
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
        // Finally try fallback keys
        .or_else(|| {
            FALLBACK_API_KEYS
                .iter()
                .find(|(key, _)| *key == env_var)
                .map(|(_, value)| value.to_string())
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
    request: http::Request<Vec<u8>>,
    method: &http::Method,
    path_str: &str,
    target_url: &url::Url,
) -> Result<BinaryResponse, Rejection> {
    let client = reqwest::Client::new();

    // Build reqwest request manually
    let mut reqwest_builder = client.request(
        reqwest::Method::from_bytes(method.as_str().as_bytes())
            .map_err(|_| warp::reject::custom(ProxyError))?,
        target_url.as_str(),
    );

    // Add headers
    for (name, value) in request.headers() {
        if let Ok(value_str) = value.to_str() {
            reqwest_builder = reqwest_builder.header(name.as_str(), value_str);
        }
    }

    // Add body
    reqwest_builder = reqwest_builder.body(request.into_body());

    let response = reqwest_builder
        .send()
        .await
        .map_err(|_| warp::reject::custom(ProxyError))?;

    let status = response.status();
    let body_bytes = response
        .bytes()
        .await
        .map_err(|_| warp::reject::custom(ProxyError))?;

    tracing::info!(
        "[PROXY] {} {} → {:?} | status: {} | body_len: {}",
        method,
        path_str,
        target_url.origin(),
        status,
        body_bytes.len()
    );

    Ok(BinaryResponse {
        body: body_bytes.to_vec(),
        status: http::StatusCode::from_u16(status.as_u16())
            .map_err(|_| warp::reject::custom(ProxyError))?,
    })
}
