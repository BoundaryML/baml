//! HTTP request types for fetch operations.
//!
//! These types mirror the `baml.HttpRequest` and `baml.HttpMethod` definitions
//! from the BAML type system.

use indexmap::IndexMap;

/// HTTP method for requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HttpMethod {
    #[default]
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

impl HttpMethod {
    /// Returns the method as an uppercase string (e.g., "GET", "POST").
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Head => "HEAD",
            HttpMethod::Options => "OPTIONS",
        }
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for HttpMethod {
    type Err = HttpMethodParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "GET" => Ok(HttpMethod::Get),
            "POST" => Ok(HttpMethod::Post),
            "PUT" => Ok(HttpMethod::Put),
            "DELETE" => Ok(HttpMethod::Delete),
            "PATCH" => Ok(HttpMethod::Patch),
            "HEAD" => Ok(HttpMethod::Head),
            "OPTIONS" => Ok(HttpMethod::Options),
            _ => Err(HttpMethodParseError(s.to_string())),
        }
    }
}

/// Error returned when parsing an invalid HTTP method string.
#[derive(Debug, Clone)]
pub struct HttpMethodParseError(pub String);

impl std::fmt::Display for HttpMethodParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid HTTP method: '{}'", self.0)
    }
}

impl std::error::Error for HttpMethodParseError {}

/// The body of an HTTP request.
#[derive(Debug, Clone, Default)]
pub enum HttpBody {
    /// No body.
    #[default]
    Empty,
    /// A string body.
    Text(String),
    /// A JSON body.
    Json(serde_json::Value),
}

/// An HTTP request configuration.
///
/// This mirrors `baml.HttpRequest` from the BAML type system and provides
/// a builder API similar to `reqwest::RequestBuilder`.
#[derive(Debug, Clone, Default)]
pub struct HttpRequest {
    /// The URL to request.
    pub url: String,

    /// The HTTP method (GET, POST, etc.).
    pub method: HttpMethod,

    /// HTTP headers.
    pub headers: IndexMap<String, String>,

    /// Query parameters (appended to URL).
    pub query_params: IndexMap<String, String>,

    /// Request body.
    pub body: HttpBody,
}

impl HttpRequest {
    /// Create a new request with the given method and URL.
    pub fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            method,
            ..Default::default()
        }
    }

    /// Create a new GET request to the given URL.
    pub fn get(url: impl Into<String>) -> Self {
        Self::new(HttpMethod::Get, url)
    }

    /// Create a new POST request to the given URL.
    pub fn post(url: impl Into<String>) -> Self {
        Self::new(HttpMethod::Post, url)
    }

    /// Create a new PUT request to the given URL.
    pub fn put(url: impl Into<String>) -> Self {
        Self::new(HttpMethod::Put, url)
    }

    /// Create a new DELETE request to the given URL.
    pub fn delete(url: impl Into<String>) -> Self {
        Self::new(HttpMethod::Delete, url)
    }

    /// Create a new PATCH request to the given URL.
    pub fn patch(url: impl Into<String>) -> Self {
        Self::new(HttpMethod::Patch, url)
    }

    // -------------------------------------------------------------------------
    // reqwest-style builder methods
    // -------------------------------------------------------------------------

    /// Add a header to the request.
    ///
    /// Similar to `reqwest::RequestBuilder::header`.
    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Set the bearer authentication token.
    ///
    /// This sets the `Authorization` header to `Bearer {token}`.
    /// Similar to `reqwest::RequestBuilder::bearer_auth`.
    pub fn bearer_auth(self, token: impl std::fmt::Display) -> Self {
        self.header("Authorization", format!("Bearer {}", token))
    }

    /// Set a string body for the request.
    ///
    /// Similar to `reqwest::RequestBuilder::body`.
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = HttpBody::Text(body.into());
        self
    }

    /// Set a JSON body for the request.
    ///
    /// This also sets the `Content-Type` header to `application/json`.
    /// Similar to `reqwest::RequestBuilder::json`.
    pub fn json(mut self, json: serde_json::Value) -> Self {
        self.body = HttpBody::Json(json);
        self.header("Content-Type", "application/json")
    }

    /// Add a query parameter.
    ///
    /// Similar to `reqwest::RequestBuilder::query`.
    pub fn query(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.query_params.insert(key.into(), value.into());
        self
    }

    // -------------------------------------------------------------------------
    // Legacy with_* methods (for compatibility)
    // -------------------------------------------------------------------------

    /// Set the HTTP method.
    pub fn with_method(mut self, method: HttpMethod) -> Self {
        self.method = method;
        self
    }

    /// Set a header (legacy alias for `header`).
    pub fn with_header(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.header(key, value)
    }

    /// Set multiple headers.
    pub fn with_headers(mut self, headers: IndexMap<String, String>) -> Self {
        self.headers = headers;
        self
    }

    /// Set a query parameter (legacy alias for `query`).
    pub fn with_query_param(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.query(key, value)
    }

    /// Set multiple query parameters.
    pub fn with_query_params(mut self, params: IndexMap<String, String>) -> Self {
        self.query_params = params;
        self
    }

    /// Set the JSON body (legacy alias for `json`).
    pub fn with_json(self, json: serde_json::Value) -> Self {
        self.json(json)
    }

    // -------------------------------------------------------------------------
    // Utility methods
    // -------------------------------------------------------------------------

    /// Build the full URL including query parameters.
    pub fn full_url(&self) -> String {
        if self.query_params.is_empty() {
            self.url.clone()
        } else {
            let query_string: String = self
                .query_params
                .iter()
                .map(|(k, v)| format!("{}={}", urlencoded(k), urlencoded(v)))
                .collect::<Vec<_>>()
                .join("&");

            if self.url.contains('?') {
                format!("{}&{}", self.url, query_string)
            } else {
                format!("{}?{}", self.url, query_string)
            }
        }
    }

    /// Check if the request has a body.
    pub fn has_body(&self) -> bool {
        !matches!(self.body, HttpBody::Empty)
    }
}

/// Simple URL encoding for query parameters.
fn urlencoded(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push('+'),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_method_as_str() {
        assert_eq!(HttpMethod::Get.as_str(), "GET");
        assert_eq!(HttpMethod::Post.as_str(), "POST");
        assert_eq!(HttpMethod::Put.as_str(), "PUT");
        assert_eq!(HttpMethod::Delete.as_str(), "DELETE");
        assert_eq!(HttpMethod::Patch.as_str(), "PATCH");
    }

    #[test]
    fn test_http_method_from_str() {
        assert_eq!("get".parse::<HttpMethod>().unwrap(), HttpMethod::Get);
        assert_eq!("POST".parse::<HttpMethod>().unwrap(), HttpMethod::Post);
        assert_eq!("Put".parse::<HttpMethod>().unwrap(), HttpMethod::Put);
        assert!("INVALID".parse::<HttpMethod>().is_err());
    }

    #[test]
    fn test_http_request_builder() {
        let req = HttpRequest::get("https://example.com")
            .with_header("Authorization", "Bearer token")
            .with_query_param("page", "1");

        assert_eq!(req.url, "https://example.com");
        assert_eq!(req.method, HttpMethod::Get);
        assert_eq!(
            req.headers.get("Authorization"),
            Some(&"Bearer token".to_string())
        );
        assert_eq!(
            req.query_params.get("page"),
            Some(&"1".to_string())
        );
    }

    #[test]
    fn test_full_url_no_params() {
        let req = HttpRequest::get("https://example.com/api");
        assert_eq!(req.full_url(), "https://example.com/api");
    }

    #[test]
    fn test_full_url_with_params() {
        let req = HttpRequest::get("https://example.com/api")
            .with_query_param("foo", "bar")
            .with_query_param("baz", "qux");

        let url = req.full_url();
        assert!(url.starts_with("https://example.com/api?"));
        assert!(url.contains("foo=bar"));
        assert!(url.contains("baz=qux"));
    }

    #[test]
    fn test_full_url_with_existing_query() {
        let req =
            HttpRequest::get("https://example.com/api?existing=param").with_query_param("new", "value");

        let url = req.full_url();
        assert!(url.starts_with("https://example.com/api?existing=param&"));
        assert!(url.contains("new=value"));
    }

    #[test]
    fn test_post_with_json() {
        let req = HttpRequest::post("https://api.example.com/data")
            .json(serde_json::json!({"key": "value"}));

        assert_eq!(req.method, HttpMethod::Post);
        match &req.body {
            HttpBody::Json(v) => assert_eq!(v, &serde_json::json!({"key": "value"})),
            _ => panic!("expected JSON body"),
        }
        // json() should also set Content-Type header
        assert_eq!(req.headers.get("Content-Type"), Some(&"application/json".to_string()));
    }

    #[test]
    fn test_bearer_auth() {
        let req = HttpRequest::get("https://api.example.com")
            .bearer_auth("my-token");

        assert_eq!(
            req.headers.get("Authorization"),
            Some(&"Bearer my-token".to_string())
        );
    }

    #[test]
    fn test_body() {
        let req = HttpRequest::post("https://api.example.com")
            .body("raw body content");

        match &req.body {
            HttpBody::Text(s) => assert_eq!(s, "raw body content"),
            _ => panic!("expected text body"),
        }
    }

    #[test]
    fn test_has_body() {
        let empty = HttpRequest::get("https://example.com");
        assert!(!empty.has_body());

        let with_json = HttpRequest::post("https://example.com")
            .json(serde_json::json!({}));
        assert!(with_json.has_body());

        let with_text = HttpRequest::post("https://example.com")
            .body("hello");
        assert!(with_text.has_body());
    }
}
