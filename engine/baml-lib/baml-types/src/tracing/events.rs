use std::fmt;
use std::sync::Arc;

use crate::BamlValue;
use anyhow::Result;
use serde::{Deserialize, Serialize, Serializer};

// TODO: use a prefixed UUID type for this
type SpanId = String;

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct FunctionId(pub SpanId);

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct ContentId(pub SpanId);

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct HttpRequestId(pub SpanId);

impl fmt::Display for HttpRequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub type TraceTags = serde_json::Map<String, serde_json::Value>;

// THESE ARE NOT CLONEABLE!!
#[derive(Debug, Serialize, Deserialize)]
pub struct TraceEvent {
    /*
     * (span_id, content_span_id) is a unique identifier for a log event
     * The query (span_id, *) gets all logs for a function call
     */
    pub span_id: FunctionId,
    // a unique identifier for this particular content
    pub event_id: ContentId,

    // The content of the log
    pub content: TraceData,

    // The chain of spans that lead to this log event
    // Includes span_id at the last position (content_span_id is not included)
    pub span_chain: Vec<FunctionId>,

    // The timestamp of the log
    // idk what this does yet #[serde(with = "timestamp_serde")]
    #[serde(with = "timestamp_serde")]
    pub timestamp: web_time::SystemTime,

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
    LogMessage {
        msg: String,
    },
    // All functions, including non-LLM ones
    // All start events
    FunctionStart(FunctionStart),
    // All end events
    FunctionEnd(FunctionEnd),

    // The rest are intermediate events that happen between start and end

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

    // We don't want to store the parsed LLM response in the log event
    // as we have it in FunctionEnd
    #[serde(deserialize_with = "deserialize_ok", serialize_with = "serialize_ok")]
    Parsed(Result<()>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BamlOptions {
    pub type_builder: Option<serde_json::Value>,
    pub client_registry: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionStart {
    pub name: String,
    pub args: Vec<BamlValue>,
    pub options: BamlOptions,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionEnd {
    #[serde(deserialize_with = "deserialize_ok", serialize_with = "serialize_ok")]
    pub result: Result<BamlValue, anyhow::Error>,
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

// #[derive(Debug, Serialize, Deserialize)]
// pub enum LLMClientName {
//     Ref(String),
//     ShortHand(String, String),
// }

#[derive(Debug, Serialize, Deserialize)]
pub struct LoggedLLMRequest {
    pub request_id: HttpRequestId,
    pub client_name: String,
    pub client_provider: String,
    pub params: serde_json::Value,
    pub prompt: Prompt,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HTTPBody {
    raw: Vec<u8>,
}

// TODO: Cache parsed JSON and UTF-8 text in order to avoid parsing the bytes
// on every access (not trivial because we'd need &mut self or interior
// mutability).
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
    pub headers: serde_json::Value,
    pub body: HTTPBody,
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

mod timestamp_serde {
    use serde::{Deserializer, Serializer};
    use web_time::{Duration, SystemTime};

    pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Convert to duration since Unix epoch, then to i64 milliseconds
        let dur = time
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(serde::ser::Error::custom)?;
        let millis = dur.as_millis() as i64;
        serializer.serialize_i64(millis)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Read the i64 milliseconds, convert back to SystemTime
        let millis: i64 = serde::Deserialize::deserialize(deserializer)?;
        Ok(SystemTime::UNIX_EPOCH + Duration::from_millis(millis as u64))
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

// unused yet
// use like this:
//  #[serde(with = "level_serde")]
//  pub verbosity: TraceLevel,
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

impl Into<TraceLevel> for tracing_core::Level {
    fn into(self) -> TraceLevel {
        match self {
            tracing_core::Level::TRACE => TraceLevel::Trace,
            tracing_core::Level::DEBUG => TraceLevel::Debug,
            tracing_core::Level::INFO => TraceLevel::Info,
            tracing_core::Level::WARN => TraceLevel::Warn,
            tracing_core::Level::ERROR => TraceLevel::Error,
        }
    }
}
