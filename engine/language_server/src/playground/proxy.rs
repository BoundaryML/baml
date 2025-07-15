use anyhow::Result;
use std::collections::HashMap;
use warp::{http, Filter, Rejection, Reply};

// API keys for model providers
const API_KEY_INJECTION_ALLOWED: &[(&str, &str, &str, &str)] = &[
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

// Dummy API keys for testing
const DUMMY_API_KEYS: &[(&str, &str)] = &[
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

// Custom response type for binary data
pub struct BinaryResponse {
    body: Vec<u8>,
    status: http::StatusCode,
    headers: HashMap<String, String>,
}

impl warp::Reply for BinaryResponse {
    fn into_response(self) -> warp::http::Response<warp::hyper::Body> {
        let mut response = warp::http::Response::builder()
            .status(self.status)
            .header("access-control-allow-origin", "*");

        for (key, value) in self.headers {
            response = response.header(key, value);
        }

        response.body(warp::hyper::Body::from(self.body)).unwrap()
    }
}

/// Creates the CORS preflight route for proxy requests
pub fn proxy_cors_route() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone
{
    warp::path("proxy")
        .and(warp::options())
        .and(warp::path::tail())
        .map(|_: warp::path::Tail| {
            warp::reply::with_status(
                warp::reply::with_header(
                    warp::reply::with_header(
                        warp::reply::with_header(
                            warp::reply::with_header(
                                warp::reply(),
                                "access-control-allow-origin", "*"
                            ),
                            "access-control-allow-methods", "GET, POST, PUT, DELETE, OPTIONS"
                        ),
                        "access-control-allow-headers", "Content-Type, Authorization, x-api-key, baml-original-url, baml-openai-api-key, baml-anthropic-api-key, baml-google-api-key, baml-openrouter-api-key, baml-llama-api-key"
                    ),
                    "access-control-max-age", "86400"
                ),
                http::StatusCode::OK
            )
        })
}

/// Creates the main proxy route for handling requests
pub fn proxy_route() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("proxy")
        .and(warp::body::bytes())
        .and(warp::method())
        .and(warp::path::tail())
        .and(warp::header::headers_cloned())
        .and_then(handle_proxy_request)
}

pub async fn handle_proxy_request(
    body: bytes::Bytes,
    method: http::Method,
    path: warp::path::Tail,
    headers: http::HeaderMap,
) -> Result<BinaryResponse, Rejection> {
    // Extract the original URL from headers
    let original_url = headers
        .get("baml-original-url")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| warp::reject::not_found())?;

    // Parse the target URL
    let mut target_url = match url::Url::parse(original_url) {
        Ok(url) => url,
        Err(_) => return Ok(create_error_response(http::StatusCode::BAD_REQUEST)),
    };

    // Handle static file requests with single dot (like .js files)
    if path.as_str().matches('.').count() == 1 && method == http::Method::GET {
        return Ok(create_empty_response());
    }

    // Build the final URL path
    let path_str = path.as_str();
    let base_path = target_url.path().trim_end_matches('/');

    let final_path = if base_path.is_empty() {
        path_str.trim_end_matches('/')
    } else if !path_str.starts_with(base_path) {
        &format!("{}/{}", base_path, path_str.trim_start_matches('/'))
    } else {
        path_str.trim_end_matches('/')
    };

    target_url.set_path(final_path);

    // Create reqwest client and request builder
    let client = reqwest::Client::new();
    let mut request_builder = match method {
        http::Method::GET => client.get(target_url.clone()),
        http::Method::POST => client.post(target_url.clone()),
        http::Method::PUT => client.put(target_url.clone()),
        http::Method::DELETE => client.delete(target_url.clone()),
        http::Method::PATCH => client.patch(target_url.clone()),
        http::Method::HEAD => client.head(target_url.clone()),
        _ => return Ok(create_error_response(http::StatusCode::METHOD_NOT_ALLOWED)),
    };

    // Add body for methods that support it
    if !body.is_empty() && method != http::Method::GET && method != http::Method::HEAD {
        request_builder = request_builder.body(body.to_vec());
    }

    // Add headers (skip internal ones)
    for (name, value) in &headers {
        let name_str = name.as_str();
        if !is_internal_header(name_str) {
            if let Ok(value_str) = value.to_str() {
                request_builder = request_builder.header(name_str, value_str);
            }
        }
    }

    // Inject API keys based on the target origin
    let origin = get_origin(&target_url);
    if let Some(api_key) = get_api_key_for_origin(&origin, &headers) {
        let (_, header_name, _, _) = API_KEY_INJECTION_ALLOWED
            .iter()
            .find(|(allowed_origin, _, _, _)| origin == *allowed_origin)
            .unwrap();

        let header_value = if *header_name == "Authorization" {
            format!("Bearer {}", api_key)
        } else {
            api_key
        };
        request_builder = request_builder.header(*header_name, header_value);
    }

    // Make the request
    let response = match request_builder.send().await {
        Ok(resp) => resp,
        Err(_) => return Ok(create_error_response(http::StatusCode::BAD_GATEWAY)),
    };

    // Extract response data
    let status = response.status();
    let response_headers: HashMap<String, String> = response
        .headers()
        .iter()
        .filter_map(|(k, v)| {
            v.to_str()
                .ok()
                .map(|v| (k.as_str().to_string(), v.to_string()))
        })
        .collect();

    let body_bytes = match response.bytes().await {
        Ok(bytes) => bytes.to_vec(),
        Err(_) => {
            return Ok(create_error_response(
                http::StatusCode::INTERNAL_SERVER_ERROR,
            ))
        }
    };

    tracing::info!(
        "[PROXY] {} {} → {} | status: {} | req_len: {} | resp_len: {}",
        method,
        path.as_str(),
        origin,
        status,
        body.len(),
        body_bytes.len()
    );

    Ok(BinaryResponse {
        body: body_bytes,
        status: http::StatusCode::from_u16(status.as_u16())
            .unwrap_or(http::StatusCode::INTERNAL_SERVER_ERROR),
        headers: response_headers,
    })
}

fn is_internal_header(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "baml-original-url"
            | "origin"
            | "host"
            | "authorization"
            | "baml-openai-api-key"
            | "baml-anthropic-api-key"
            | "baml-google-api-key"
            | "baml-openrouter-api-key"
            | "baml-llama-api-key"
    )
}

fn get_origin(url: &url::Url) -> String {
    match url.origin() {
        url::Origin::Tuple(scheme, host, port) => match (scheme.as_str(), port) {
            ("http", 80) | ("https", 443) => format!("{}://{}", scheme, host),
            _ => format!("{}://{}:{}", scheme, host, port),
        },
        url::Origin::Opaque(_) => url.to_string(),
    }
}

fn get_api_key_for_origin(origin: &str, headers: &http::HeaderMap) -> Option<String> {
    for (allowed_origin, _, env_var, baml_header) in API_KEY_INJECTION_ALLOWED {
        if origin == *allowed_origin {
            // Try environment variable first
            if let Ok(api_key) = std::env::var(env_var) {
                return Some(api_key);
            }

            // Try custom header
            if let Some(header_value) = headers.get(*baml_header) {
                if let Ok(api_key) = header_value.to_str() {
                    return Some(api_key.to_string());
                }
            }

            // Fallback to dummy key for testing
            if let Some((_, dummy_key)) = DUMMY_API_KEYS.iter().find(|(key, _)| *key == *env_var) {
                return Some(dummy_key.to_string());
            }
        }
    }
    None
}

fn create_error_response(status: http::StatusCode) -> BinaryResponse {
    BinaryResponse {
        body: Vec::new(),
        status,
        headers: HashMap::new(),
    }
}

fn create_empty_response() -> BinaryResponse {
    BinaryResponse {
        body: Vec::new(),
        status: http::StatusCode::OK,
        headers: HashMap::new(),
    }
}
