use crate::base::EpochMsTimestamp;
use crate::define_id::{HttpRequestId, SpanId, TraceEventId};
use anyhow::Result;
use serde::{Deserialize, Serialize, Serializer};

pub type TraceTags = serde_json::Map<String, serde_json::Value>;

#[repr(usize)]
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum TraceLevel {
    Trace = 100,
    Debug = 200,
    Info = 300,
    Warn = 400,
    Error = 500,
    Fatal = 600,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceEventBatch {
    pub events: Vec<TraceEvent>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceEvent {
    /*
     * (span_id, content_span_id) is a unique identifier for a log event
     * The query (span_id, *) gets all logs for a function call
     */
    pub span_id: SpanId,
    // a unique identifier for this particular content
    pub event_id: TraceEventId,

    // The content of the log
    pub content: TraceData,

    // The chain of spans that lead to this log event
    // Includes span_id at the last position (content_span_id is not included)
    pub span_chain: Vec<SpanId>,

    // The timestamp of the log
    #[serde(rename = "timestamp_epoch_ms")]
    pub timestamp: EpochMsTimestamp,

    /// human-readable callsite identifier, e.g. "ExtractResume" or "openai/gpt-4o/chat"
    pub callsite: String,

    /// verbosity level
    #[serde(with = "level_serde")]
    pub verbosity: TraceLevel,

    pub tags: TraceTags,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum TraceData {
    FunctionStart(FunctionStart),
    FunctionEnd(FunctionEnd),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BamlOptions {
    pub type_builder: Option<serde_json::Value>,
    pub client_registry: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionStart {
    pub function_id: SpanId,
    pub function_display_name: String,
    pub args: Vec<(String, serde_json::Value)>,
    pub options: (),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionEnd {
    pub function_id: SpanId,
    pub function_display_name: String,
    #[serde(deserialize_with = "deserialize_ok", serialize_with = "serialize_ok")]
    pub result: Result<serde_json::Value, anyhow::Error>,
    // Everything below is duplicated from the start event
    // to deal with the case where the log is dropped.
    // P2: as we can for now assume logs are not dropped,

    // pub name: String,
    // pub start_timestamp: web_time::Instant,
    // pub start_args: Vec<BamlValue>,
}

// LLM specific events

// TODO: fix this.
pub type Prompt = serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
pub struct LoggedLLMRequest {
    pub request_id: HttpRequestId,
    pub client_name: String,
    pub client_provider: String,
    pub params: serde_json::Value,
    pub prompt: Prompt,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HTTPRequest {
    // since LLM requests could be made in parallel, we need to match the response to the request
    pub request_id: HttpRequestId,
    pub url: String,
    pub method: String,
    pub headers: serde_json::Value,
    pub body: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HTTPResponse {
    // since LLM requests could be made in parallel, we need to match the response to the request
    pub request_id: HttpRequestId,
    pub status: u16,
    pub headers: serde_json::Value,
    pub body: serde_json::Value,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct LLMUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

/// -------------------------------------------------------------------------
///
/// Helper deserializer for our Result types.
///
/// This assumes that the incoming JSON always represents the Ok variant.
/// (If you need to support error variants, you will have to expand this logic.)
///
use serde::Deserializer;
fn deserialize_ok<'de, D, T>(deserializer: D) -> Result<Result<T, anyhow::Error>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Ok)
}

fn serialize_ok<S, T>(value: &Result<T, anyhow::Error>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize,
{
    match value {
        Ok(v) => v.serialize(serializer),
        Err(err) => Err(serde::ser::Error::custom(format!("Error: {}", err))),
    }
}

// Add this helper module for tracing::Level serialization
mod level_serde {
    use super::TraceLevel;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(level: &TraceLevel, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(*level as u32)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<TraceLevel, D::Error>
    where
        D: Deserializer<'de>,
    {
        let level_num: u32 = serde::Deserialize::deserialize(deserializer)?;
        match level_num {
            100 => Ok(TraceLevel::Trace),
            200 => Ok(TraceLevel::Debug),
            300 => Ok(TraceLevel::Info),
            400 => Ok(TraceLevel::Warn),
            500 => Ok(TraceLevel::Error),
            600 => Ok(TraceLevel::Fatal),
            _ => Err(serde::de::Error::custom(format!(
                "Invalid trace level: {}",
                level_num
            ))),
        }
    }
}
