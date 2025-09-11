use std::{borrow::Cow, collections::HashMap};

use baml_ids::{FunctionCallId, FunctionEventId};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    baml_function_call_error::BamlFunctionCallError,
    baml_value::{BamlValue, Media},
};
use crate::{
    ast::{evaluation_context::TypeBuilderValue, tops::BamlFunctionId},
    base::EpochMsTimestamp,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceEventBatch<'a> {
    pub events: Vec<BackendTraceEvent<'a>>,
}

/// This is intentionally VERY similar to TraceEvent in
/// baml-lib/baml-types/src/tracing/events.rs
/// If the convertion from baml-types to baml-rpc is not possible,
/// WE HAVE A BREAKING CHANGE.
#[derive(Debug, Serialize, Deserialize)]
pub struct BackendTraceEvent<'a> {
    /*
     * (call_id, content_event_id) is a unique identifier for a log event
     * The query (call_id, *) gets all logs for a function call
     */
    pub call_id: FunctionCallId,

    // a unique identifier for this particular content
    pub function_event_id: FunctionEventId,

    // The chain of calls that lead to this log event
    // Includes call_id at the last position (content_event_id is not included)
    pub call_stack: Vec<FunctionCallId>,

    // The timestamp of the log
    #[serde(rename = "timestamp_epoch_ms")]
    pub timestamp: EpochMsTimestamp,

    // The content of the log
    pub content: TraceData<'a>,
}

// Same as tracing/events.rs FunctionType
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum FunctionType {
    BamlLlm,
    // BamlExternal, // extern function in baml
    // Baml // a function that is defined in baml, but not a baml llm function
    Native, // python or TS function we are @tracing.
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TraceData<'a> {
    FunctionStart {
        function_display_name: String,
        args: Vec<(String, BamlValue<'a>)>,
        tags: TraceTags,
        function_type: FunctionType,
        is_stream: bool,
        /// Only sent for BAML defined functions
        baml_function_content: Option<BamlFunctionStart>,
    },
    /// Terminal Event
    FunctionEnd(FunctionEnd<'a>),

    /// Intermediate events between start and end
    Intermediate(IntermediateData<'a>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BamlFunctionStart {
    pub function_id: std::sync::Arc<BamlFunctionId>,
    pub baml_src_hash: String,
    pub eval_context: EvaluationContext,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum FunctionEnd<'a> {
    Success { result: BamlValue<'a> },
    Error { error: BamlFunctionCallError<'a> },
}

pub type TraceTags = std::collections::HashMap<String, serde_json::Value>;

#[derive(Debug, Serialize, Deserialize)]
pub struct EvaluationContext {
    pub tags: TraceTags,

    pub type_builder: Option<TypeBuilderValue>,
    // TODO(hellovai): add this
    // pub client_registry: Option<ClientRegistryValue>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RpcClientDetails {
    pub name: String,
    pub provider: String,
    pub options: IndexMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum IntermediateData<'a> {
    /// These are all resolved from the client
    LLMRequest {
        client_name: String,
        client_provider: String,
        params: HashMap<String, Cow<'a, serde_json::Value>>,
        prompt: Vec<LLMChatMessage<'a>>,
    },
    RawLLMRequest {
        http_request_id: String,
        url: String,
        method: String,
        headers: HashMap<String, String>,
        client_details: RpcClientDetails,
        body: HTTPBody<'a>,
    },
    RawLLMResponse {
        http_request_id: String,
        status: u16,
        headers: Option<HashMap<String, String>>,
        body: HTTPBody<'a>,
        client_details: RpcClientDetails,
    },
    RawLLMResponseStream {
        http_request_id: String,
        event: Event<'a>,
    },
    LLMResponse {
        client_stack: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        finish_reason: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<LLMUsage>,

        #[serde(skip_serializing_if = "Option::is_none")]
        raw_text_output: Option<Cow<'a, str>>,
    },
    SetTags(TraceTags),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HTTPBody<'a> {
    pub raw: Cow<'a, [u8]>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Event<'a> {
    pub raw: Cow<'a, str>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LLMChatMessage<'a> {
    pub role: String,
    pub content: Vec<LLMChatMessagePart<'a>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LLMChatMessagePart<'a> {
    Text(Cow<'a, str>),
    Media(Media<'a>),
    WithMeta(
        Box<LLMChatMessagePart<'a>>,
        HashMap<String, serde_json::Value>,
    ),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LLMUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
}
