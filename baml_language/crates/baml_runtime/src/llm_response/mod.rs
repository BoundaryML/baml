//! LLM response types - unified format for all providers.

use std::time::{Duration, SystemTime};

use crate::prompt::RenderedPrompt;
use crate::types::BamlMap;

/// All possible outcomes from an LLM call.
#[derive(Debug, Clone)]
pub enum LLMResponse {
    /// HTTP 2xx, got content.
    Success(LLMCompleteResponse),
    /// HTTP non-2xx from provider.
    LLMFailure(LLMErrorResponse),
    /// Validation failed before HTTP call.
    UserFailure(String),
    /// Internal error before HTTP call.
    InternalFailure(String),
    /// User cancelled the operation.
    Cancelled(String),
}

impl LLMResponse {
    /// Check if this is a successful response.
    pub fn is_success(&self) -> bool {
        matches!(self, LLMResponse::Success(_))
    }

    /// Get the content if this is a successful response.
    pub fn content(&self) -> Option<&str> {
        match self {
            LLMResponse::Success(resp) => Some(&resp.content),
            _ => None,
        }
    }

    /// Get the error message if this is a failure.
    pub fn error_message(&self) -> Option<&str> {
        match self {
            LLMResponse::LLMFailure(e) => Some(&e.message),
            LLMResponse::UserFailure(s) => Some(s),
            LLMResponse::InternalFailure(s) => Some(s),
            LLMResponse::Cancelled(s) => Some(s),
            LLMResponse::Success(_) => None,
        }
    }
}

/// Successful LLM response.
#[derive(Debug, Clone)]
pub struct LLMCompleteResponse {
    /// Client name that was used.
    pub client: String,
    /// Model that was used.
    pub model: String,
    /// The prompt that was sent.
    pub prompt: RenderedPrompt,
    /// Request options that were used.
    pub request_options: BamlMap<String, serde_json::Value>,
    /// The response content.
    pub content: String,
    /// When the request started.
    pub start_time: SystemTime,
    /// How long the request took.
    pub latency: Duration,
    /// Additional metadata.
    pub metadata: LLMResponseMetadata,
}

/// Metadata about an LLM response.
#[derive(Debug, Clone, Default)]
pub struct LLMResponseMetadata {
    /// Whether the response is complete (not truncated).
    pub baml_is_complete: bool,
    /// Reason the model stopped generating.
    pub finish_reason: Option<String>,
    /// Number of prompt tokens.
    pub prompt_tokens: Option<u64>,
    /// Number of output tokens.
    pub output_tokens: Option<u64>,
    /// Total tokens used.
    pub total_tokens: Option<u64>,
    /// Cached input tokens (if caching was used).
    pub cached_input_tokens: Option<u64>,
}

/// Failed LLM response (HTTP error from provider).
#[derive(Debug, Clone)]
pub struct LLMErrorResponse {
    /// Client name that was used.
    pub client: String,
    /// Model that was attempted (if known).
    pub model: Option<String>,
    /// The prompt that was sent.
    pub prompt: RenderedPrompt,
    /// Request options that were used.
    pub request_options: BamlMap<String, serde_json::Value>,
    /// When the request started.
    pub start_time: SystemTime,
    /// How long before the error occurred.
    pub latency: Duration,
    /// Error message.
    pub message: String,
    /// Error code category.
    pub code: ErrorCode,
}

/// Categorized error codes from LLM providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// 401 - Invalid authentication.
    InvalidAuthentication,
    /// 403 - Feature not supported.
    NotSupported,
    /// 429 - Rate limited.
    RateLimited,
    /// 500 - Server error.
    ServerError,
    /// 503 - Service unavailable.
    ServiceUnavailable,
    /// Request timed out.
    Timeout,
    /// Other HTTP status code.
    Other(u16),
}

impl ErrorCode {
    /// Create from HTTP status code.
    pub fn from_status(status: u16) -> Self {
        match status {
            401 => ErrorCode::InvalidAuthentication,
            403 => ErrorCode::NotSupported,
            429 => ErrorCode::RateLimited,
            500 => ErrorCode::ServerError,
            503 => ErrorCode::ServiceUnavailable,
            other => ErrorCode::Other(other),
        }
    }

    /// Check if this error should trigger a retry.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ErrorCode::RateLimited | ErrorCode::ServerError | ErrorCode::ServiceUnavailable
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_response_success() {
        let response = LLMResponse::Success(LLMCompleteResponse {
            client: "openai".to_string(),
            model: "gpt-4".to_string(),
            prompt: RenderedPrompt::simple("test"),
            request_options: BamlMap::new(),
            content: "Hello!".to_string(),
            start_time: SystemTime::now(),
            latency: Duration::from_millis(100),
            metadata: LLMResponseMetadata::default(),
        });

        assert!(response.is_success());
        assert_eq!(response.content(), Some("Hello!"));
        assert!(response.error_message().is_none());
    }

    #[test]
    fn test_error_code_from_status() {
        assert_eq!(ErrorCode::from_status(401), ErrorCode::InvalidAuthentication);
        assert_eq!(ErrorCode::from_status(429), ErrorCode::RateLimited);
        assert_eq!(ErrorCode::from_status(500), ErrorCode::ServerError);
        assert_eq!(ErrorCode::from_status(404), ErrorCode::Other(404));
    }

    #[test]
    fn test_error_code_retryable() {
        assert!(ErrorCode::RateLimited.is_retryable());
        assert!(ErrorCode::ServerError.is_retryable());
        assert!(!ErrorCode::InvalidAuthentication.is_retryable());
        assert!(!ErrorCode::Other(404).is_retryable());
    }
}
