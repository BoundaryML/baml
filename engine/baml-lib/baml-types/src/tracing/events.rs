use anyhow::Result;
use std::{collections::HashMap, sync::Arc};

use crate::{BamlMap, BamlMedia, BamlValueWithMeta, HasFieldType};
use baml_ids::{FunctionCallId, FunctionEventId, HttpRequestId};
use serde::{Deserialize, Serialize};

pub use super::errors::BamlError;

pub type TraceTags = serde_json::Map<String, serde_json::Value>;

// THESE ARE NOT CLONEABLE!!
#[derive(Debug)]
pub struct TraceEvent<'a, T: HasFieldType> {
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

impl<'a, T: HasFieldType> TraceEvent<'a, T> {
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
        is_baml_function: bool,
    ) -> Self {
        Self::from_existing_call(
            call_stack,
            TraceData::FunctionStart(FunctionStart {
                name: function_name,
                args,
                options,
                is_baml_function,
            }),
        )
        .expect("Failed to create function start event")
    }

    pub fn new_function_end(
        call_stack: Vec<FunctionCallId>,
        result: Result<BamlValueWithMeta<T>, BamlError<'a>>,
    ) -> Self {
        Self::from_existing_call(
            call_stack,
            TraceData::FunctionEnd(match result {
                Ok(value) => FunctionEnd::Success(value),
                Err(e) => FunctionEnd::Error(e),
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
}

#[derive(Debug)]
pub enum TraceData<'a, T: HasFieldType> {
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
    /// LLM response now a plain struct, so we don't wrap it in `Result`.
    LLMResponse(Arc<LoggedLLMResponse>),
    // ----

    // In the future, we can send more metadata, like parsing information.
}

impl<'a, T: HasFieldType> TraceData<'a, T> {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::FunctionStart(_) => "FunctionStart",
            Self::FunctionEnd(_) => "FunctionEnd",
            Self::LLMRequest(_) => "LLMRequest",
            Self::RawLLMRequest(_) => "RawLLMRequest",
            Self::RawLLMResponse(_) => "RawLLMResponse",
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

#[derive(Debug)]
pub struct FunctionStart<T: HasFieldType> {
    pub name: String,
    pub is_baml_function: bool,
    pub args: Vec<(String, BamlValueWithMeta<T>)>,
    pub options: EvaluationContext,
}

#[derive(Debug)]
pub enum FunctionEnd<'a, T: HasFieldType> {
    Success(BamlValueWithMeta<T>),
    Error(BamlError<'a>),
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

#[derive(Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct HTTPRequest {
    // since LLM requests could be made in parallel, we need to match the response to the request
    pub id: HttpRequestId,
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub body: HTTPBody,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HTTPResponse {
    // since LLM requests could be made in parallel, we need to match the response to the request
    pub request_id: HttpRequestId,
    pub status: u16,
    pub headers: Option<HashMap<String, String>>,
    pub body: HTTPBody,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoggedLLMResponse {
    /// Since LLM requests could be made in parallel, we need to match the response to the request.
    pub request_id: HttpRequestId,

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
    ) -> Self {
        Self {
            request_id,
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
    ) -> Self {
        Self {
            request_id,
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
}
