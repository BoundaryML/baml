use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;
use warp::{Filter, Rejection, Reply};

// Custom response type for binary data
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

// Custom error type for proxy
#[derive(Debug)]
struct ProxyError;

impl warp::reject::Reject for ProxyError {}

// API keys for model providers - these should be injected into requests
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

// Temporary dummy API keys for testing (remove in production)
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

pub struct ProxyServer {
    port: u16,
}

impl ProxyServer {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub async fn start(self) -> Result<()> {
        let addr = ([127, 0, 0, 1], self.port);

        // Handle OPTIONS requests (preflight CORS requests)
        let cors_route = warp::options()
            .and(warp::path::tail())
            .map(|_| {
                warp::http::Response::builder()
                    .status(http::StatusCode::OK)
                    .header("access-control-allow-origin", "*")
                    .header("access-control-allow-methods", "GET, POST, PUT, DELETE, OPTIONS")
                    .header("access-control-allow-headers", "Content-Type, Authorization, x-api-key, baml-original-url, baml-openai-api-key, baml-anthropic-api-key, baml-google-api-key, baml-openrouter-api-key, baml-llama-api-key")
                    .header("access-control-max-age", "86400")
                    .body(warp::hyper::Body::empty())
                    .unwrap()
            });

        // Proxy all requests - use a catch-all route that matches any path
        let proxy_route = warp::any()
            .and(warp::body::bytes())
            .and(warp::method())
            .and(warp::path::tail()) // Use tail() to capture any path
            .and(warp::header::headers_cloned())
            .and_then(handle_proxy_request);

        // Combine CORS and proxy routes
        let routes = cors_route.or(proxy_route);

        tracing::info!("Proxy server listening on port {}", self.port);
        warp::serve(routes).run(addr).await;

        Ok(())
    }
}

async fn handle_proxy_request(
    body: bytes::Bytes,
    method: http::Method,
    path: warp::path::Tail,
    mut headers: http::HeaderMap,
) -> Result<BinaryResponse, Rejection> {
    let path_str = path.as_str();
    let original_url = match headers.get("baml-original-url") {
        Some(url) => match url.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => {
                return Ok(BinaryResponse {
                    body: Vec::new(),
                    status: http::StatusCode::BAD_REQUEST,
                });
            }
        },
        None => {
            return Ok(BinaryResponse {
                body: Vec::new(),
                status: http::StatusCode::BAD_REQUEST,
            });
        }
    };

    headers.remove("baml-original-url");
    headers.remove("origin");
    headers.remove("authorization");
    headers.remove("host");

    let clean_original_url = if original_url.ends_with('/') {
        &original_url[..original_url.len() - 1]
    } else {
        &original_url
    };

    let url = match url::Url::parse(clean_original_url) {
        Ok(url) => url,
        Err(_) => {
            return Ok(BinaryResponse {
                body: Vec::new(),
                status: http::StatusCode::BAD_REQUEST,
            });
        }
    };

    if path_str.matches('.').count() == 1 && method == http::Method::GET {
        return Ok(BinaryResponse {
            body: Vec::new(),
            status: http::StatusCode::OK,
        });
    }

    let mut target_url = url.clone();

    let base_path = if url.path().ends_with('/') {
        url.path().trim_end_matches('/').to_string()
    } else {
        url.path().to_string()
    };

    let final_path = if base_path.is_empty() {
        if let Some(stripped) = path_str.strip_suffix('/') {
            stripped.to_string()
        } else {
            path_str.to_string()
        }
    } else if let Some(stripped) = path_str.strip_prefix('/') {
        if base_path.ends_with('/') {
            format!("{base_path}{stripped}")
        } else {
            format!("{base_path}/{stripped}")
        }
    } else if base_path.ends_with('/') {
        format!("{base_path}{path_str}")
    } else {
        format!("{base_path}/{path_str}")
    };

    let clean_final_path = if final_path.ends_with('/') {
        &final_path[..final_path.len() - 1]
    } else {
        &final_path
    };

    target_url.set_path(clean_final_path);

    // Create the request to the target
    let mut request_builder = http::Request::builder()
        .method(method.clone())
        .uri(target_url.to_string());

    // Add headers
    for (name, value) in headers.iter() {
        request_builder = request_builder.header(name.as_str(), value);
    }

    // Inject API keys for allowed origins (same as VSCode extension)
    let origin_str = match url.origin() {
        url::Origin::Tuple(scheme, host, port) => match (scheme.as_str(), port) {
            ("http", 80) | ("https", 443) => format!("{scheme}://{host}"),
            _ => format!("{scheme}://{host}:{port}"),
        },
        url::Origin::Opaque(_) => {
            return Ok(BinaryResponse {
                body: Vec::new(),
                status: http::StatusCode::BAD_REQUEST,
            });
        }
    };

    // API key injection logic
    for (allowed_origin, header_name, env_var, baml_header) in API_KEY_INJECTION_ALLOWED {
        if origin_str == *allowed_origin {
            let api_key = std::env::var(env_var)
                .ok()
                .or_else(|| {
                    headers
                        .get(*baml_header)
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string())
                })
                .or_else(|| {
                    DUMMY_API_KEYS
                        .iter()
                        .find(|(key, _)| *key == *env_var)
                        .map(|(_, v)| v.to_string())
                });
            if let Some(api_key) = api_key {
                let header_value = if *header_name == "Authorization" {
                    format!("Bearer {api_key}")
                } else {
                    api_key
                };
                request_builder = request_builder.header(*header_name, header_value);
            }
        }
    }

    let request = match request_builder.body(body.to_vec()) {
        Ok(req) => req,
        Err(_) => {
            return Ok(BinaryResponse {
                body: Vec::new(),
                status: http::StatusCode::INTERNAL_SERVER_ERROR,
            });
        }
    };

    let client = reqwest::Client::new();
    let response = match client
        .execute(
            request
                .try_into()
                .map_err(|_| warp::reject::custom(ProxyError))?,
        )
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            return Ok(BinaryResponse {
                body: Vec::new(),
                status: http::StatusCode::BAD_GATEWAY,
            });
        }
    };

    let status = response.status();
    let body_bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => {
            return Ok(BinaryResponse {
                body: Vec::new(),
                status: http::StatusCode::INTERNAL_SERVER_ERROR,
            });
        }
    };

    tracing::info!(
        "[PROXY] {} {} → {:?} | headers: {:?} | req_body_len: {} | resp_status: {} | resp_body_len: {}",
        method,
        path_str,
        url.origin(),
        headers,
        body.len(),
        status,
        body_bytes.len()
    );

    Ok(BinaryResponse {
        body: body_bytes.to_vec(),
        status,
    })
}
