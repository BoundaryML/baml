use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub type TraceTags = serde_json::Map<String, serde_json::Value>;

#[derive(Clone, Debug, Serialize, Deserialize)]
// TODO: use a prefixed UUID type for this
pub struct SpanId(pub Vec<String>);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceEventBatch {
    pub events: Vec<TraceEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TraceEvent {
    SpanStart(TraceSpanStart),
    SpanEnd(TraceSpanEnd),
    Log(TraceLog),
}

#[repr(usize)]
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum TraceLevel {
    Trace = 0o100,
    Debug = 0o200,
    Info = 0o300,
    Warn = 0o400,
    Error = 0o500,
    Fatal = 0o600,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceMetadata {
    /// human-readable callsite identifier, e.g. "ExtractResume" or "openai/gpt-4o/chat"
    pub callsite: String,
    /// verbosity level
    #[serde(with = "level_serde")]
    pub verbosity: TraceLevel,
}

// -------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceSpanStart {
    pub span_id: SpanId,
    pub meta: TraceMetadata,
    #[serde(with = "timestamp_serde")]
    pub start_time: OffsetDateTime,
    pub fields: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceSpanEnd {
    pub span_id: SpanId,
    pub meta: TraceMetadata,
    #[serde(with = "timestamp_serde")]
    pub start_time: OffsetDateTime,
    #[serde(with = "timestamp_serde")]
    pub end_time: OffsetDateTime,
    pub fields: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceLog {
    pub span_id: SpanId,
    pub log_id: String,
    pub meta: TraceMetadata,
    #[serde(with = "timestamp_serde")]
    pub start_time: OffsetDateTime,
    pub msg: String,
    pub fields: serde_json::Map<String, serde_json::Value>,
}

// Replace the timestamp_serde module with this simpler version
mod timestamp_serde {
    use serde::{Deserializer, Serializer};
    use time::OffsetDateTime;

    pub fn serialize<S>(time: &OffsetDateTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(time.unix_timestamp())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let timestamp_millis: i64 = serde::Deserialize::deserialize(deserializer)?;
        OffsetDateTime::from_unix_timestamp(timestamp_millis).map_err(serde::de::Error::custom)
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
            0o100 => Ok(TraceLevel::Trace),
            0o200 => Ok(TraceLevel::Debug),
            0o300 => Ok(TraceLevel::Info),
            0o400 => Ok(TraceLevel::Warn),
            0o500 => Ok(TraceLevel::Error),
            0o600 => Ok(TraceLevel::Fatal),
            _ => Err(serde::de::Error::custom(format!(
                "Invalid trace level: {}",
                level_num
            ))),
        }
    }
}
