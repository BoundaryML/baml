use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use baml_ids::{FunctionCallId, FunctionEventId, HttpRequestId};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

pub use super::errors::BamlError;
use crate::{type_meta, BamlMap, BamlMedia, BamlValueWithMeta, HasType};

pub type TraceTags = serde_json::Map<String, serde_json::Value>;

// THESE ARE NOT CLONEABLE!!
#[derive(Debug)]
pub struct TraceEvent<'a, T: HasType<type_meta::NonStreaming>> {
    /*
     * (call_id, function_event_id) is a unique identifier for a log event
     * The query (call_id, *) gets all logs for a function call
     */
    pub call_id: FunctionCallId,
    // a unique identifier for this particular content
    pub function_event_id: FunctionEventId,

    // The content of the log
    pub content: TraceData<'a, T>,

    // The chain of calls that lead to this log event
    // Includes call_id at the last position (function_event_id is not included)
    pub call_stack: Vec<FunctionCallId>,

    // The timestamp of the log
    pub timestamp: web_time::SystemTime,
}

impl<'a, T: HasType<type_meta::NonStreaming>> TraceEvent<'a, T> {
    fn from_existing_call(
        call_stack: Vec<FunctionCallId>,
        content: TraceData<'a, T>,
    ) -> Result<Self> {
        let Some(last_call_id) = call_stack.last() else {
            return Err(anyhow::anyhow!("Call stack is empty"));
        };
        Ok(Self {
            call_id: last_call_id.clone(),
            function_event_id: FunctionEventId::new(),
            content,
            call_stack,
            timestamp: web_time::SystemTime::now(),
        })
    }

    pub fn new_set_tags(call_stack: Vec<FunctionCallId>, tags: TraceTags) -> Self {
        Self::from_existing_call(call_stack, TraceData::SetTags(tags))
            .expect("Failed to create set tags event")
    }

    pub fn new_function_start(
        // Already has the new call_id of the function
        call_stack: Vec<FunctionCallId>,
        function_name: String,
        args: Vec<(String, BamlValueWithMeta<T>)>,
        options: EvaluationContext,
        function_type: FunctionType,
        is_stream: bool,
    ) -> Self {
        Self::from_existing_call(
            call_stack,
            TraceData::FunctionStart(FunctionStart {
                name: function_name,
                is_stream,
                args,
                options,
                function_type,
            }),
        )
        .expect("Failed to create function start event")
    }

    pub fn new_function_end(
        call_stack: Vec<FunctionCallId>,
        result: Result<BamlValueWithMeta<T>, BamlError<'a>>,
        function_type: FunctionType,
    ) -> Self {
        Self::from_existing_call(
            call_stack,
            TraceData::FunctionEnd(match result {
                Ok(value) => FunctionEnd::Success {
                    value,
                    function_type,
                },
                Err(error) => FunctionEnd::Error {
                    error,
                    function_type,
                },
            }),
        )
        .expect("Failed to create function end event")
    }

    pub fn new_llm_request(
        call_stack: Vec<FunctionCallId>,
        request: Arc<LoggedLLMRequest>,
    ) -> Self {
        Self::from_existing_call(call_stack, TraceData::LLMRequest(request))
            .expect("Failed to create LLM request event")
    }

    pub fn new_llm_response(
        call_stack: Vec<FunctionCallId>,
        response: Arc<LoggedLLMResponse>,
    ) -> Self {
        Self::from_existing_call(call_stack, TraceData::LLMResponse(response))
            .expect("Failed to create LLM response event")
    }

    pub fn new_raw_llm_request(call_stack: Vec<FunctionCallId>, request: Arc<HTTPRequest>) -> Self {
        Self::from_existing_call(call_stack, TraceData::RawLLMRequest(request))
            .expect("Failed to create raw LLM request event")
    }

    pub fn new_raw_llm_response(
        call_stack: Vec<FunctionCallId>,
        response: Arc<HTTPResponse>,
    ) -> Self {
        Self::from_existing_call(call_stack, TraceData::RawLLMResponse(response))
            .expect("Failed to create raw LLM response event")
    }

    pub fn new_raw_llm_response_stream(
        call_stack: Vec<FunctionCallId>,
        response: Arc<HTTPResponseStream>,
    ) -> Self {
        Self::from_existing_call(call_stack, TraceData::RawLLMResponseStream(response))
            .expect("Failed to create raw LLM response stream event")
    }
}

// DO NOT CLONE!
#[derive(Debug)]
pub enum TraceData<'a, T: HasType<type_meta::NonStreaming>> {
    // All functions, including non-LLM ones
    // All start events
    FunctionStart(FunctionStart<T>),
    // All end events
    FunctionEnd(FunctionEnd<'a, T>),

    // The rest are intermediate events that happen between start and end
    SetTags(TraceTags),

    // LLM request
    LLMRequest(Arc<LoggedLLMRequest>),
    // Raw HTTP request to the LLM
    RawLLMRequest(Arc<HTTPRequest>),

    // Do to streaming, its possible to have multiple responses for a single request
    // ----
    // Raw HTTP response from the LLM
    RawLLMResponse(Arc<HTTPResponse>),

    // Raw HTTP response stream from the LLM
    RawLLMResponseStream(Arc<HTTPResponseStream>),

    /// LLM response now a plain struct, so we don't wrap it in `Result`.
    LLMResponse(Arc<LoggedLLMResponse>),
    // ----

    // In the future, we can send more metadata, like parsing information.
}

impl<T: HasType<type_meta::NonStreaming>> TraceData<'_, T> {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::FunctionStart(_) => "FunctionStart",
            Self::FunctionEnd(_) => "FunctionEnd",
            Self::LLMRequest(_) => "LLMRequest",
            Self::RawLLMRequest(_) => "RawLLMRequest",
            Self::RawLLMResponse(_) => "RawLLMResponse",
            Self::RawLLMResponseStream(_) => "RawLLMResponseStream",
            Self::LLMResponse(_) => "LLMResponse",
            Self::SetTags(_) => "SetTags",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct EvaluationContext {
    pub tags: TraceTags,
    // TODO(hellovai): add this
    // pub type_builder: Option<TypeBuilderValue>,
    // pub client_registry: Option<ClientRegistryValue>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
#[serde(rename_all = "snake_case")]
pub enum FunctionType {
    BamlLlm,
    // BamlExternal, // extern function in baml
    // Baml // a function that is defined in baml, but not a baml llm function
    Native, // python or TS function we are @tracing.
}

#[derive(Debug)]
pub struct FunctionStart<T: HasType<type_meta::NonStreaming>> {
    pub name: String,
    pub function_type: FunctionType,
    pub is_stream: bool,
    pub args: Vec<(String, BamlValueWithMeta<T>)>,
    pub options: EvaluationContext,
}

#[derive(Debug)]
pub enum FunctionEnd<'a, T: HasType<type_meta::NonStreaming>> {
    Success {
        value: BamlValueWithMeta<T>,
        function_type: FunctionType,
    },
    Error {
        error: BamlError<'a>,
        function_type: FunctionType,
    },
}

// LLM specific events

// TODO: fix this.

// #[derive(Debug, Serialize, Deserialize)]
// pub enum LLMClientName {
//     Ref(String),
//     ShortHand(String, String),
// }

#[derive(Debug, Serialize, Deserialize)]
pub struct LLMChatMessage {
    pub role: String,
    pub content: Vec<LLMChatMessagePart>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum LLMChatMessagePart {
    Text(String),
    Media(BamlMedia),
    WithMeta(Box<LLMChatMessagePart>, HashMap<String, serde_json::Value>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoggedLLMRequest {
    pub request_id: HttpRequestId,
    pub client_name: String,
    pub client_provider: String,
    pub params: BamlMap<String, serde_json::Value>,
    pub prompt: Vec<LLMChatMessage>,
}

#[derive(Clone)]
pub struct HTTPBody {
    raw: Vec<u8>,
}

impl std::fmt::Debug for HTTPBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let preview = if self.raw.len() <= 100 {
            // If small enough, show as UTF-8 text if possible
            match std::str::from_utf8(&self.raw) {
                Ok(text) => format!("\"{}\"", text.escape_debug()),
                Err(_) => format!("{:?}", self.raw),
            }
        } else {
            // For larger bodies, show length and preview
            match std::str::from_utf8(&self.raw[..100.min(self.raw.len())]) {
                Ok(text) => format!("\"{}...\" ({} bytes)", text.escape_debug(), self.raw.len()),
                Err(_) => format!("[{} bytes]", self.raw.len()),
            }
        };

        f.debug_struct("HTTPBody").field("raw", &preview).finish()
    }
}

impl HTTPBody {
    pub fn new(body: Vec<u8>) -> Self {
        Self { raw: body }
    }

    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    pub fn text(&self) -> anyhow::Result<&str> {
        std::str::from_utf8(&self.raw).map_err(|e| anyhow::anyhow!("HTTP body is not UTF-8: {}", e))
    }

    pub fn json(&self) -> anyhow::Result<serde_json::Value> {
        serde_json::from_str(self.text()?)
            .map_err(|e| anyhow::anyhow!("HTTP body is not JSON: {}", e))
    }

    /// Returns the HTTP body as a [`serde_json::Value`].
    ///
    /// If the body is not UTF-8 or JSON, it is returned as an array of bytes.
    /// Used as input for [`serde_json::to_string_pretty`].
    pub fn as_serde_value(&self) -> serde_json::Value {
        self.json()
            .or_else(|_e| self.text().map(|s| serde_json::Value::String(s.into())))
            .unwrap_or_else(|_e| {
                serde_json::Value::Array(
                    self.raw()
                        .iter()
                        .map(|byte| serde_json::Value::from(*byte))
                        .collect(),
                )
            })
    }
}

// Custom serialization: always serialize as text; if invalid UTF-8, serialize as base64
impl serde::Serialize for HTTPBody {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize as text to avoid exploding arrays of bytes; use lossy UTF-8 if needed
        let s = String::from_utf8_lossy(&self.raw);
        serializer.serialize_str(&s)
    }
}

impl<'de> serde::Deserialize<'de> for HTTPBody {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(HTTPBody::new(s.into_bytes()))
    }
}

pub fn redact_headers(headers: HashMap<String, String>) -> HashMap<String, String> {
    headers
        .into_iter()
        .map(|(key, value)| {
            let key_lower = key.to_lowercase();
            let sensitive_keywords = [
                "authorization",
                "cookie",
                "set-cookie",
                "key",
                "secret",
                "token",
                "credential",
                "session",
                "auth",
            ];

            // tokens is usually for input and output tokens
            if key_lower.contains("ratelimit") || key_lower.contains("tokens") {
                (key, value)
            } else if sensitive_keywords
                .iter()
                .any(|&keyword| key_lower.contains(keyword))
            {
                (key, "REDACTED".to_string())
            } else {
                (key, value)
            }
        })
        .collect()
}

fn serialize_redacted_headers<S>(
    headers: &HashMap<String, String>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let redacted = redact_headers(headers.clone());
    redacted.serialize(serializer)
}

fn serialize_redacted_optional_headers<S>(
    headers: &Option<HashMap<String, String>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match headers {
        Some(h) => {
            let redacted = redact_headers(h.clone());
            Some(redacted).serialize(serializer)
        }
        None => None::<HashMap<String, String>>.serialize(serializer),
    }
}

fn deserialize_headers<'de, D>(deserializer: D) -> Result<HashMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // When deserializing, we get the redacted version, but we treat it as the "original"
    // since we can't recover the original values from redacted data
    HashMap::<String, String>::deserialize(deserializer)
}

fn deserialize_optional_headers<'de, D>(
    deserializer: D,
) -> Result<Option<HashMap<String, String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // When deserializing, we get the redacted version, but we treat it as the "original"
    // since we can't recover the original values from redacted data
    Option::<HashMap<String, String>>::deserialize(deserializer)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HTTPRequest {
    // since LLM requests could be made in parallel, we need to match the response to the request
    pub id: HttpRequestId,
    pub url: String,
    pub method: String,
    #[serde(serialize_with = "serialize_redacted_headers")]
    #[serde(deserialize_with = "deserialize_headers")]
    headers: HashMap<String, String>,
    pub body: HTTPBody,
    pub client_details: std::sync::Arc<ClientDetails>,
}

impl HTTPRequest {
    pub fn new(
        id: HttpRequestId,
        url: String,
        method: String,
        headers: HashMap<String, String>,
        body: HTTPBody,
        client_details: ClientDetails,
    ) -> Self {
        Self {
            id,
            url,
            method,
            headers,
            body,
            client_details: std::sync::Arc::new(client_details),
        }
    }

    pub fn id(&self) -> &HttpRequestId {
        &self.id
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }

    pub fn body(&self) -> &HTTPBody {
        &self.body
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientDetails {
    /// e.g. for `client<llm> MyOpenaiClient` this is "MyOpenaiClient"
    pub name: String,
    /// e.g. for `client<llm> MyOpenaiClient` this is "openai"
    pub provider: String,
    /// e.g. for `client<llm> MyOpenaiClient` this is the options passed to the client
    pub options: IndexMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HTTPResponse {
    // since LLM requests could be made in parallel, we need to match the response to the request
    pub request_id: HttpRequestId,
    pub status: u16,
    #[serde(serialize_with = "serialize_redacted_optional_headers")]
    #[serde(deserialize_with = "deserialize_optional_headers")]
    headers: Option<HashMap<String, String>>,
    pub body: Arc<HTTPBody>,

    pub client_details: Arc<ClientDetails>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HTTPResponseStream {
    pub request_id: HttpRequestId,
    pub event: Arc<SSEEvent>,
}

impl HTTPResponse {
    pub fn new(
        request_id: HttpRequestId,
        status: u16,
        headers: Option<HashMap<String, String>>,
        body: HTTPBody,
        client_details: ClientDetails,
    ) -> Self {
        Self {
            request_id,
            status,
            headers,
            body: Arc::new(body),
            client_details: Arc::new(client_details),
        }
    }

    pub fn id(&self) -> &HttpRequestId {
        &self.request_id
    }

    pub fn headers(&self) -> Option<&HashMap<String, String>> {
        self.headers.as_ref()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SSEEvent {
    pub timestamp_utc_ms: i64,
    pub event: String,
    pub data: String,
    pub id: String,
}

impl HTTPResponseStream {
    pub fn new(request_id: HttpRequestId, event: SSEEvent) -> Self {
        Self {
            request_id,
            event: Arc::new(event),
        }
    }
}

impl SSEEvent {
    pub fn new(event: String, data: String, id: String) -> Self {
        Self {
            event,
            data,
            id,
            timestamp_utc_ms: web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoggedLLMResponse {
    /// Since LLM requests could be made in parallel, we need to match the response to the request.
    pub request_id: HttpRequestId,

    // List of the client stack used by the LLM function to get the response, e.g. if a roundrobin
    // client "MyRoundrobin" wraps a fallback client "MyFallback" wraps an openai client "MyOpenai"
    // then the client stack would be ["MyRoundrobin", "MyFallback", "MyOpenai"]
    pub client_stack: Vec<String>,

    /// If available, fully qualified model name. None in failure cases or unknown state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// If available, a textual finish reason from the LLM. None in errors or unknown state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,

    /// If available, usage information from the LLM. None if usage data is unavailable or in error states.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<LLMUsage>,

    /// If available, the accumulated text output after retrieving chunks from LLM. None in error states.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_text_output: Option<String>,

    /// If an error occurred, store the message here. None if the request was successful.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

impl LoggedLLMResponse {
    pub fn new_success(
        request_id: HttpRequestId,
        model: String,
        finish_reason: Option<String>,
        usage: LLMUsage,
        raw_text_output: String,
        client_stack: Vec<String>,
    ) -> Self {
        Self {
            request_id,
            client_stack,
            model: Some(model),
            finish_reason,
            usage: Some(usage),
            raw_text_output: Some(raw_text_output),
            error_message: None,
        }
    }

    pub fn new_failure(
        request_id: HttpRequestId,
        error_message: String,
        model: Option<String>,
        finish_reason: Option<String>,
        client_stack: Vec<String>,
    ) -> Self {
        Self {
            request_id,
            client_stack,
            model,
            finish_reason,
            usage: None,
            raw_text_output: None,
            error_message: Some(error_message),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LLMUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
}

#[cfg(test)]
mod tests {
    use baml_ids::HttpRequestId;

    use super::*;

    #[test]
    fn test_headers_redaction_in_serialization() {
        let mut headers = HashMap::new();
        headers.insert(
            "authorization".to_string(),
            "Bearer secret-token".to_string(),
        );
        headers.insert("content-type".to_string(), "application/json".to_string());
        headers.insert("x-api-key".to_string(), "secret-key".to_string());

        let request = HTTPRequest::new(
            HttpRequestId::new(),
            "https://api.example.com".to_string(),
            "POST".to_string(),
            headers.clone(),
            HTTPBody::new(b"test body".to_vec()),
            ClientDetails {
                name: "test-client".to_string(),
                provider: "test-provider".to_string(),
                options: IndexMap::new(),
            },
        );

        // Test that .headers() returns original headers
        let actual_headers = request.headers();
        assert_eq!(
            actual_headers.get("authorization"),
            Some(&"Bearer secret-token".to_string())
        );
        assert_eq!(
            actual_headers.get("content-type"),
            Some(&"application/json".to_string())
        );
        assert_eq!(
            actual_headers.get("x-api-key"),
            Some(&"secret-key".to_string())
        );

        // Test that serialization redacts sensitive headers
        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("REDACTED"));
        assert!(!serialized.contains("secret-token"));
        assert!(!serialized.contains("secret-key"));
        assert!(serialized.contains("application/json")); // non-sensitive header should remain

        // Test deserialization works
        let deserialized: HTTPRequest = serde_json::from_str(&serialized).unwrap();

        // After deserialization, we can't recover original values
        assert_eq!(
            deserialized.headers().get("authorization"),
            Some(&"REDACTED".to_string())
        );
        assert_eq!(
            deserialized.headers().get("x-api-key"),
            Some(&"REDACTED".to_string())
        );
        assert_eq!(
            deserialized.headers().get("content-type"),
            Some(&"application/json".to_string())
        );
    }

    #[test]
    fn test_response_headers_redaction_in_serialization() {
        let mut headers = HashMap::new();
        headers.insert("set-cookie".to_string(), "session=abc123".to_string());
        headers.insert("content-length".to_string(), "100".to_string());

        let response = HTTPResponse::new(
            HttpRequestId::new(),
            200,
            Some(headers.clone()),
            HTTPBody::new(b"response body".to_vec()),
            ClientDetails {
                name: "test-client".to_string(),
                provider: "test-provider".to_string(),
                options: IndexMap::new(),
            },
        );

        // Test that .headers() returns original headers
        let actual_headers = response.headers().unwrap();
        assert_eq!(
            actual_headers.get("set-cookie"),
            Some(&"session=abc123".to_string())
        );
        assert_eq!(
            actual_headers.get("content-length"),
            Some(&"100".to_string())
        );

        // Test that serialization redacts sensitive headers
        let serialized = serde_json::to_string(&response).unwrap();
        assert!(serialized.contains("REDACTED"));
        assert!(!serialized.contains("session=abc123"));
        assert!(serialized.contains("100")); // non-sensitive header should remain
    }
}
