//! OpenAI-specific request types.

use std::collections::HashMap;
use std::time::Duration;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use baml_llm_interface::ChatMessagePart;
use ir_stub::BamlMediaContent;

use crate::errors::BuildRequestError;
use baml_llm_interface::{RenderedChatMessage, RenderedPrompt};
use crate::render_options::RenderOptions;

/// HTTP method for requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
}

impl HttpMethod {
    pub fn as_str(&self) -> &str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
        }
    }
}

/// Header value that may be secret.
#[derive(Debug, Clone)]
pub enum HeaderValue {
    Plain(String),
    /// Will be masked unless expose_secrets=true.
    Secret(String),
}

impl HeaderValue {
    pub fn render(&self, expose_secrets: bool) -> String {
        match self {
            HeaderValue::Plain(s) => s.clone(),
            HeaderValue::Secret(s) => {
                if expose_secrets {
                    s.clone()
                } else {
                    "[REDACTED]".to_string()
                }
            }
        }
    }
}

/// OpenAI-specific request format.
/// Media may be unresolved (for render_raw_curl) or resolved (for execution).
#[derive(Debug, Clone)]
pub struct OpenAiRequest {
    /// HTTP method (always POST for OpenAI chat completions).
    pub method: HttpMethod,
    /// Full URL including endpoint.
    pub url: String,
    /// Request headers (may contain masked or real secrets).
    pub headers: HashMap<String, HeaderValue>,
    /// JSON body (may contain unresolved media URLs).
    pub body: serde_json::Value,
    /// Request timeout.
    pub timeout: Option<Duration>,
    /// Whether media has been rewritten to inline format.
    pub media_resolved: bool,
    /// Whether to stream the response.
    pub stream: bool,
}

/// OpenAI client configuration.
#[derive(Debug, Clone)]
pub struct OpenAiClientConfig {
    /// Base URL for the API.
    pub base_url: String,
    /// API key.
    pub api_key: String,
    /// Model to use.
    pub model: String,
    /// Temperature for generation.
    pub temperature: Option<f32>,
    /// Maximum tokens to generate.
    pub max_tokens: Option<u32>,
    /// Request timeout.
    pub timeout: Option<Duration>,
}

impl Default for OpenAiClientConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: "gpt-4".to_string(),
            temperature: None,
            max_tokens: None,
            timeout: Some(Duration::from_secs(60)),
        }
    }
}

/// OpenAI message format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    pub content: OpenAiContent,
}

/// OpenAI content format (can be string or array of parts).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAiContent {
    Text(String),
    Parts(Vec<OpenAiContentPart>),
}

/// OpenAI content part.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OpenAiContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: OpenAiImageUrl },
}

/// OpenAI image URL format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiImageUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl OpenAiRequest {
    /// Build from rendered prompt + client config (preserves media as-is).
    pub fn from_rendered(
        rendered: &RenderedPrompt,
        config: &OpenAiClientConfig,
        stream: bool,
    ) -> Result<Self, BuildRequestError> {
        // Convert messages to OpenAI format based on prompt type
        let messages: Vec<OpenAiMessage> = match rendered {
            RenderedPrompt::Completion { text } => {
                // Convert completion to a single user message
                vec![OpenAiMessage {
                    role: "user".to_string(),
                    content: OpenAiContent::Text(text.clone()),
                }]
            }
            RenderedPrompt::Chat { messages } => {
                messages.iter().map(|m| Self::convert_message(m)).collect()
            }
        };

        // Build request body
        let mut body = serde_json::json!({
            "model": config.model,
            "messages": messages,
        });

        if let Some(temp) = config.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(max_tokens) = config.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if stream {
            body["stream"] = serde_json::json!(true);
        }

        // Build headers
        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            HeaderValue::Plain("application/json".to_string()),
        );
        headers.insert(
            "Authorization".to_string(),
            HeaderValue::Secret(format!("Bearer {}", config.api_key)),
        );

        Ok(Self {
            method: HttpMethod::Post,
            url: format!("{}/chat/completions", config.base_url),
            headers,
            body,
            timeout: config.timeout,
            media_resolved: false,
            stream,
        })
    }

    /// Convert a rendered message to OpenAI format.
    fn convert_message(msg: &RenderedChatMessage) -> OpenAiMessage {
        let role = msg.role.clone();

        // Check if we have any media parts
        let has_media = msg.parts.iter().any(|p| matches!(p, ChatMessagePart::Media { .. }));

        let content = if has_media {
            // Use array format for mixed content
            let parts: Vec<OpenAiContentPart> = msg
                .parts
                .iter()
                .map(|p| Self::convert_part(p))
                .collect();
            OpenAiContent::Parts(parts)
        } else {
            // Use simple string format for text-only
            let text = msg
                .parts
                .iter()
                .filter_map(|p| p.as_text().cloned())
                .collect::<Vec<_>>()
                .join("");
            OpenAiContent::Text(text)
        };

        OpenAiMessage { role, content }
    }

    /// Convert a message part to OpenAI format.
    fn convert_part(part: &ChatMessagePart) -> OpenAiContentPart {
        match part {
            ChatMessagePart::Text { text } => OpenAiContentPart::Text { text: text.clone() },
            ChatMessagePart::Media { media } => {
                let url = match &media.content {
                    BamlMediaContent::Url(url_content) => url_content.url.clone(),
                    BamlMediaContent::Base64(b64) => {
                        format!("data:{};base64,{}", b64.media_type, b64.base64)
                    }
                    BamlMediaContent::File(file) => {
                        // This shouldn't happen in a well-resolved request
                        format!("file://{}", file.path)
                    }
                };
                OpenAiContentPart::ImageUrl {
                    image_url: OpenAiImageUrl { url, detail: None },
                }
            }
            ChatMessagePart::WithMeta { inner, .. } => Self::convert_part(inner),
        }
    }

    /// Check if this request has unresolved media.
    pub fn has_unresolved_media(&self) -> bool {
        !self.media_resolved
    }

    /// Convert to curl command string.
    /// Uses pre-rewrite state (shows URLs, not base64 blobs).
    pub fn to_curl(&self, options: &RenderOptions) -> String {
        let mut parts = vec![format!("curl -X {}", self.method.as_str())];

        // Add headers
        for (key, value) in &self.headers {
            let rendered_value = value.render(options.expose_secrets);
            parts.push(format!("-H '{}: {}'", key, rendered_value));
        }

        // Add body
        let body_str = serde_json::to_string_pretty(&self.body).unwrap_or_default();
        parts.push(format!("-d '{}'", body_str.replace('\'', "'\\''")));

        // Add URL
        parts.push(format!("'{}'", self.url));

        parts.join(" \\\n  ")
    }
}

// ============================================================================
// Response Parsing
// ============================================================================

/// Raw OpenAI chat completion response.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiChatCompletionResponse {
    pub id: String,
    pub object: String,
    #[serde(default)]
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAiChoice>,
    #[serde(default)]
    pub usage: Option<OpenAiUsage>,
}

/// A choice in the OpenAI response.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiChoice {
    pub index: u32,
    pub message: OpenAiResponseMessage,
    pub finish_reason: Option<String>,
}

/// Message in the OpenAI response.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiResponseMessage {
    pub role: String,
    pub content: Option<String>,
}

/// Usage statistics from OpenAI.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// OpenAI error response.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiErrorResponse {
    pub error: OpenAiError,
}

/// OpenAI error details.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiError {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: Option<String>,
    pub param: Option<String>,
    pub code: Option<String>,
}

/// Parse an OpenAI response body into our unified format.
pub fn parse_openai_response(
    body: &str,
    status_code: u16,
    client_name: &str,
    prompt: RenderedPrompt,
    start_time: std::time::SystemTime,
    latency: Duration,
) -> crate::llm_response::LLMResponse {
    use crate::llm_response::*;

    // Handle non-2xx status codes
    if status_code < 200 || status_code >= 300 {
        // Try to parse error response
        let message = if let Ok(err_resp) = serde_json::from_str::<OpenAiErrorResponse>(body) {
            err_resp.error.message
        } else {
            body.to_string()
        };

        return LLMResponse::LLMFailure(LLMErrorResponse {
            client: client_name.to_string(),
            model: None,
            prompt,
            request_options: IndexMap::new(),
            start_time,
            latency,
            message,
            code: ErrorCode::from_status(status_code),
        });
    }

    // Try to parse successful response
    match serde_json::from_str::<OpenAiChatCompletionResponse>(body) {
        Ok(response) => {
            let content = response
                .choices
                .first()
                .and_then(|c| c.message.content.clone())
                .unwrap_or_default();

            let finish_reason = response
                .choices
                .first()
                .and_then(|c| c.finish_reason.clone());

            let is_complete = finish_reason.as_deref() == Some("stop");

            let metadata = LLMResponseMetadata {
                baml_is_complete: is_complete,
                finish_reason,
                prompt_tokens: response.usage.as_ref().map(|u| u.prompt_tokens),
                output_tokens: response.usage.as_ref().map(|u| u.completion_tokens),
                total_tokens: response.usage.as_ref().map(|u| u.total_tokens),
                cached_input_tokens: None,
            };

            LLMResponse::Success(LLMCompleteResponse {
                client: client_name.to_string(),
                model: response.model,
                prompt,
                request_options: IndexMap::new(),
                content,
                start_time,
                latency,
                metadata,
            })
        }
        Err(e) => LLMResponse::InternalFailure(format!("Failed to parse OpenAI response: {}", e)),
    }
}

// ============================================================================
// SSE Streaming Types
// ============================================================================

/// A single SSE event from OpenAI streaming.
#[derive(Debug, Clone)]
pub enum SseEvent {
    /// Data chunk with content.
    Data(String),
    /// Stream completed.
    Done,
    /// Error occurred.
    Error(String),
}

/// OpenAI streaming chunk response.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiStreamChunk {
    pub id: String,
    pub object: String,
    #[serde(default)]
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAiStreamChoice>,
}

/// A choice in an OpenAI streaming chunk.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiStreamChoice {
    pub index: u32,
    pub delta: OpenAiDelta,
    pub finish_reason: Option<String>,
}

/// Delta content in streaming response.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiDelta {
    pub role: Option<String>,
    pub content: Option<String>,
}

/// Parse an SSE line from OpenAI.
pub fn parse_sse_line(line: &str) -> Option<SseEvent> {
    let line = line.trim();

    if line.is_empty() {
        return None;
    }

    if let Some(data) = line.strip_prefix("data: ") {
        if data == "[DONE]" {
            return Some(SseEvent::Done);
        }

        match serde_json::from_str::<OpenAiStreamChunk>(data) {
            Ok(chunk) => {
                // Extract content from the delta
                let content = chunk
                    .choices
                    .first()
                    .and_then(|c| c.delta.content.clone())
                    .unwrap_or_default();

                if content.is_empty() {
                    // Check for finish reason
                    if chunk.choices.first().and_then(|c| c.finish_reason.as_ref()).is_some() {
                        return Some(SseEvent::Done);
                    }
                    return None;
                }

                Some(SseEvent::Data(content))
            }
            Err(e) => Some(SseEvent::Error(format!("Failed to parse SSE chunk: {}", e))),
        }
    } else {
        // Ignore other SSE fields (event:, id:, retry:, etc.)
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_request_from_rendered() {
        let rendered = RenderedPrompt::Chat {
            messages: vec![
                RenderedChatMessage {
                    role: "system".to_string(),
                    allow_duplicate_role: false,
                    parts: vec![ChatMessagePart::Text { text: "You are helpful.".to_string() }],
                },
                RenderedChatMessage {
                    role: "user".to_string(),
                    allow_duplicate_role: false,
                    parts: vec![ChatMessagePart::Text { text: "Hello!".to_string() }],
                },
            ],
        };

        let config = OpenAiClientConfig {
            api_key: "sk-test-key".to_string(),
            model: "gpt-4".to_string(),
            ..Default::default()
        };

        let request = OpenAiRequest::from_rendered(&rendered, &config, false).unwrap();

        assert_eq!(request.method, HttpMethod::Post);
        assert!(request.url.contains("chat/completions"));
        assert!(!request.media_resolved);
    }

    #[test]
    fn test_curl_masks_secrets() {
        let rendered = RenderedPrompt::Completion { text: "Test".to_string() };
        let config = OpenAiClientConfig {
            api_key: "sk-secret-key".to_string(),
            ..Default::default()
        };

        let request = OpenAiRequest::from_rendered(&rendered, &config, false).unwrap();
        let curl = request.to_curl(&RenderOptions::default());

        assert!(curl.contains("[REDACTED]"));
        assert!(!curl.contains("sk-secret-key"));
    }

    #[test]
    fn test_curl_exposes_secrets() {
        let rendered = RenderedPrompt::Completion { text: "Test".to_string() };
        let config = OpenAiClientConfig {
            api_key: "sk-secret-key".to_string(),
            ..Default::default()
        };

        let request = OpenAiRequest::from_rendered(&rendered, &config, false).unwrap();
        let curl = request.to_curl(&RenderOptions::for_execution());

        assert!(curl.contains("sk-secret-key"));
        assert!(!curl.contains("[REDACTED]"));
    }

    #[test]
    fn test_parse_openai_response_success() {
        let body = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help you?"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 8,
                "total_tokens": 18
            }
        }"#;

        let response = parse_openai_response(
            body,
            200,
            "openai",
            RenderedPrompt::Completion { text: "test".to_string() },
            std::time::SystemTime::now(),
            Duration::from_millis(100),
        );

        assert!(response.is_success());
        assert_eq!(response.content(), Some("Hello! How can I help you?"));
    }

    #[test]
    fn test_parse_openai_response_error() {
        let body = r#"{
            "error": {
                "message": "Rate limit exceeded",
                "type": "rate_limit_error",
                "code": "rate_limit_exceeded"
            }
        }"#;

        let response = parse_openai_response(
            body,
            429,
            "openai",
            RenderedPrompt::Completion { text: "test".to_string() },
            std::time::SystemTime::now(),
            Duration::from_millis(100),
        );

        assert!(!response.is_success());
        assert_eq!(response.error_message(), Some("Rate limit exceeded"));
    }

    #[test]
    fn test_parse_sse_line_data() {
        let line = r#"data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1677652288,"model":"gpt-4","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let event = parse_sse_line(line);

        assert!(matches!(event, Some(SseEvent::Data(s)) if s == "Hello"));
    }

    #[test]
    fn test_parse_sse_line_done() {
        let line = "data: [DONE]";
        let event = parse_sse_line(line);

        assert!(matches!(event, Some(SseEvent::Done)));
    }

    #[test]
    fn test_parse_sse_line_empty() {
        assert!(parse_sse_line("").is_none());
        assert!(parse_sse_line("   ").is_none());
    }
}
