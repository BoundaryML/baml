//! The Bedrock `ConverseStream` event model: every `ConverseStreamOutput`
//! event and every exception the stream can carry, as serde types.
//!
//! The JSON types in [`types`] (re-exported here) are generated from
//! `aws-sdk-bedrockruntime` by `scripts/generate_converse_stream_types.py`,
//! with the SDK's field names and wire keys. Like the SDK they are lenient:
//! a missing required field takes its default, and an enum value or union
//! member this model does not know is kept (`Unknown`) rather than failing,
//! so a newer Bedrock never breaks an older client.
//!
//! [`decode_message`] turns one event stream [`Message`] into a
//! [`ConverseStreamOutput`], or the [`ConverseStreamError`] it carries.

use std::fmt;

use base64::Engine as _;

use crate::frame::Message;

/// A string-valued enum that keeps values it does not know.
macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        $name:ident { $($(#[$vmeta:meta])* $variant:ident = $value:literal,)* }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum $name {
            $($(#[$vmeta])* $variant,)*
            /// A value this model does not know.
            Unknown(String),
        }

        impl $name {
            /// The wire value.
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $value,)*
                    Self::Unknown(value) => value,
                }
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                match value {
                    $($value => Self::$variant,)*
                    other => Self::Unknown(other.to_owned()),
                }
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::Unknown(String::new())
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                Ok(Self::from(value.as_str()))
            }
        }
    };
}

/// A Smithy union: a JSON object with exactly one member. A member this model
/// does not know is kept, with its raw value.
macro_rules! union {
    (
        $(#[$meta:meta])*
        $name:ident { $($(#[$vmeta:meta])* $variant:ident($ty:ty) = $key:literal,)* }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq)]
        #[non_exhaustive]
        pub enum $name {
            $($(#[$vmeta])* $variant($ty),)*
            /// A member this model does not know: its key and raw value.
            Unknown(String, serde_json::Value),
        }

        impl Default for $name {
            fn default() -> Self {
                Self::Unknown(String::new(), serde_json::Value::Null)
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeMap as _;
                let mut map = serializer.serialize_map(Some(1))?;
                match self {
                    $(Self::$variant(value) => map.serialize_entry($key, value)?,)*
                    Self::Unknown(key, value) => map.serialize_entry(key, value)?,
                }
                map.end()
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                use serde::de::Error as _;
                let members = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
                // Smithy serializers omit unset members, but a null one is
                // still "unset".
                let (key, value) = members
                    .into_iter()
                    .find(|(_, value)| !value.is_null())
                    .ok_or_else(|| D::Error::custom(concat!(stringify!($name), " union has no member")))?;
                Ok(match key.as_str() {
                    $($key => Self::$variant(serde_json::from_value(value).map_err(D::Error::custom)?),)*
                    _ => Self::Unknown(key, value),
                })
            }
        }
    };
}

mod types;

pub use types::*;

/// An open-content JSON document (`additionalModelResponseFields`, …).
pub type Document = serde_json::Value;

/// Binary data, base64 on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Blob(pub Vec<u8>);

impl serde::Serialize for Blob {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(&self.0))
    }
}

impl<'de> serde::Deserialize<'de> for Blob {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        let encoded = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map(Blob)
            .map_err(D::Error::custom)
    }
}

/// One `ConverseStream` event, keyed on the wire by the `:event-type` header.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ConverseStreamOutput {
    /// The messages output content block delta.
    ContentBlockDelta(ContentBlockDeltaEvent),
    /// Start information for a content block.
    ContentBlockStart(ContentBlockStartEvent),
    /// Stop information for a content block.
    ContentBlockStop(ContentBlockStopEvent),
    /// Message start information.
    MessageStart(MessageStartEvent),
    /// Message stop information.
    MessageStop(MessageStopEvent),
    /// Metadata for the converse output stream.
    Metadata(ConverseStreamMetadataEvent),
    /// An event this model does not know: its `:event-type` and raw payload.
    Unknown {
        event_type: String,
        payload: Vec<u8>,
    },
}

impl ConverseStreamOutput {
    /// The `:event-type` this event is sent as.
    pub fn event_type(&self) -> &str {
        match self {
            Self::ContentBlockDelta(_) => "contentBlockDelta",
            Self::ContentBlockStart(_) => "contentBlockStart",
            Self::ContentBlockStop(_) => "contentBlockStop",
            Self::MessageStart(_) => "messageStart",
            Self::MessageStop(_) => "messageStop",
            Self::Metadata(_) => "metadata",
            Self::Unknown { event_type, .. } => event_type,
        }
    }
}

/// An exception frame (`:message-type` `exception`), keyed by the
/// `:exception-type` header.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ConverseStreamException {
    InternalServer(InternalServerException),
    ModelStreamError(ModelStreamErrorException),
    Validation(ValidationException),
    Throttling(ThrottlingException),
    ServiceUnavailable(ServiceUnavailableException),
    /// An exception this model does not know: its type and the payload's
    /// `message`, when it has one.
    Unknown {
        exception_type: String,
        message: Option<String>,
    },
}

impl ConverseStreamException {
    /// The `:exception-type` this exception is sent as.
    pub fn exception_type(&self) -> &str {
        match self {
            Self::InternalServer(_) => "internalServerException",
            Self::ModelStreamError(_) => "modelStreamErrorException",
            Self::Validation(_) => "validationException",
            Self::Throttling(_) => "throttlingException",
            Self::ServiceUnavailable(_) => "serviceUnavailableException",
            Self::Unknown { exception_type, .. } => exception_type,
        }
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            Self::InternalServer(e) => e.message.as_deref(),
            Self::ModelStreamError(e) => e.message.as_deref(),
            Self::Validation(e) => e.message.as_deref(),
            Self::Throttling(e) => e.message.as_deref(),
            Self::ServiceUnavailable(e) => e.message.as_deref(),
            Self::Unknown { message, .. } => message.as_deref(),
        }
    }
}

/// Why a message is not a `ConverseStreamOutput` event.
#[derive(Debug)]
#[non_exhaustive]
pub enum ConverseStreamError {
    /// The stream carried a modeled (or unknown) exception.
    Exception(ConverseStreamException),
    /// An unmodeled error frame (`:message-type` `error`).
    Error { code: String, message: String },
    /// A required header is missing or not a string.
    MissingHeader(&'static str),
    /// A `:message-type` other than `event`, `exception` or `error`.
    UnknownMessageType(String),
    /// A known event or exception whose JSON payload does not decode.
    Payload {
        event_type: String,
        source: serde_json::Error,
    },
}

impl fmt::Display for ConverseStreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exception(exception) => write!(
                f,
                "{}: {}",
                exception.exception_type(),
                exception.message().unwrap_or("<no message>")
            ),
            Self::Error { code, message } => write!(f, "{code}: {message}"),
            Self::MissingHeader(name) => write!(f, "event stream message has no {name} header"),
            Self::UnknownMessageType(kind) => write!(f, "unrecognized :message-type {kind:?}"),
            Self::Payload { event_type, source } => {
                write!(f, "malformed {event_type} payload: {source}")
            }
        }
    }
}

impl std::error::Error for ConverseStreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Payload { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn payload<T: serde::de::DeserializeOwned>(
    event_type: &str,
    bytes: &[u8],
) -> Result<T, ConverseStreamError> {
    serde_json::from_slice(bytes).map_err(|source| ConverseStreamError::Payload {
        event_type: event_type.to_owned(),
        source,
    })
}

/// Decode one message of a `ConverseStream` response.
///
/// A `:message-type` absent from the message is treated as `event`, as the
/// SDK's unmarshaller does.
pub fn decode_message(message: &Message) -> Result<ConverseStreamOutput, ConverseStreamError> {
    let message_type = message.string_header(":message-type").unwrap_or("event");
    match message_type {
        "event" => {
            let event_type = message
                .string_header(":event-type")
                .ok_or(ConverseStreamError::MissingHeader(":event-type"))?;
            let bytes = &message.payload;
            Ok(match event_type {
                "contentBlockDelta" => {
                    ConverseStreamOutput::ContentBlockDelta(payload(event_type, bytes)?)
                }
                "contentBlockStart" => {
                    ConverseStreamOutput::ContentBlockStart(payload(event_type, bytes)?)
                }
                "contentBlockStop" => {
                    ConverseStreamOutput::ContentBlockStop(payload(event_type, bytes)?)
                }
                "messageStart" => ConverseStreamOutput::MessageStart(payload(event_type, bytes)?),
                "messageStop" => ConverseStreamOutput::MessageStop(payload(event_type, bytes)?),
                "metadata" => ConverseStreamOutput::Metadata(payload(event_type, bytes)?),
                other => ConverseStreamOutput::Unknown {
                    event_type: other.to_owned(),
                    payload: bytes.clone(),
                },
            })
        }
        "exception" => {
            let exception_type = message
                .string_header(":exception-type")
                .ok_or(ConverseStreamError::MissingHeader(":exception-type"))?;
            let bytes = &message.payload;
            let exception = match exception_type {
                "internalServerException" => {
                    ConverseStreamException::InternalServer(payload(exception_type, bytes)?)
                }
                "modelStreamErrorException" => {
                    ConverseStreamException::ModelStreamError(payload(exception_type, bytes)?)
                }
                "validationException" => {
                    ConverseStreamException::Validation(payload(exception_type, bytes)?)
                }
                "throttlingException" => {
                    ConverseStreamException::Throttling(payload(exception_type, bytes)?)
                }
                "serviceUnavailableException" => {
                    ConverseStreamException::ServiceUnavailable(payload(exception_type, bytes)?)
                }
                other => ConverseStreamException::Unknown {
                    exception_type: other.to_owned(),
                    message: serde_json::from_slice::<serde_json::Value>(bytes)
                        .ok()
                        .and_then(|v| v.get("message")?.as_str().map(str::to_owned)),
                },
            };
            Err(ConverseStreamError::Exception(exception))
        }
        "error" => Err(ConverseStreamError::Error {
            code: message
                .string_header(":error-code")
                .unwrap_or_default()
                .to_owned(),
            message: message
                .string_header(":error-message")
                .unwrap_or_default()
                .to_owned(),
        }),
        other => Err(ConverseStreamError::UnknownMessageType(other.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Header, HeaderValue};

    fn event(event_type: &str, payload: &str) -> Message {
        Message {
            headers: vec![
                Header::new(":event-type", HeaderValue::String(event_type.into())),
                Header::new(
                    ":content-type",
                    HeaderValue::String("application/json".into()),
                ),
                Header::new(":message-type", HeaderValue::String("event".into())),
            ],
            payload: payload.as_bytes().to_vec(),
        }
    }

    fn exception(exception_type: &str, payload: &str) -> Message {
        Message {
            headers: vec![
                Header::new(
                    ":exception-type",
                    HeaderValue::String(exception_type.into()),
                ),
                Header::new(":message-type", HeaderValue::String("exception".into())),
            ],
            payload: payload.as_bytes().to_vec(),
        }
    }

    #[test]
    fn decodes_a_full_stream() {
        let stream = [
            event("messageStart", r#"{"role":"assistant","p":"abc"}"#),
            event(
                "contentBlockStart",
                r#"{"contentBlockIndex":1,"start":{"toolUse":{"toolUseId":"t1","name":"weather"}}}"#,
            ),
            event(
                "contentBlockDelta",
                r#"{"contentBlockIndex":0,"delta":{"text":"Hi"}}"#,
            ),
            event(
                "contentBlockDelta",
                r#"{"contentBlockIndex":1,"delta":{"toolUse":{"input":"{\"city\":"}}}"#,
            ),
            event(
                "contentBlockDelta",
                r#"{"contentBlockIndex":2,"delta":{"reasoningContent":{"redactedContent":"AAEC"}}}"#,
            ),
            event("contentBlockStop", r#"{"contentBlockIndex":0}"#),
            event(
                "messageStop",
                r#"{"stopReason":"end_turn","additionalModelResponseFields":{"x":[1]}}"#,
            ),
            event(
                "metadata",
                r#"{"usage":{"inputTokens":10,"outputTokens":2,"totalTokens":12,"cacheReadInputTokens":4},"metrics":{"latencyMs":321}}"#,
            ),
        ];
        let events: Vec<_> = stream.iter().map(|m| decode_message(m).unwrap()).collect();

        let ConverseStreamOutput::MessageStart(start) = &events[0] else {
            panic!()
        };
        assert_eq!(start.role, ConversationRole::Assistant);
        let ConverseStreamOutput::ContentBlockStart(block) = &events[1] else {
            panic!()
        };
        let Some(ContentBlockStart::ToolUse(tool)) = &block.start else {
            panic!()
        };
        assert_eq!(
            (tool.tool_use_id.as_str(), tool.name.as_str()),
            ("t1", "weather")
        );
        let ConverseStreamOutput::ContentBlockDelta(delta) = &events[2] else {
            panic!()
        };
        assert_eq!(delta.delta, Some(ContentBlockDelta::Text("Hi".into())));
        let ConverseStreamOutput::ContentBlockDelta(delta) = &events[3] else {
            panic!()
        };
        let Some(ContentBlockDelta::ToolUse(input)) = &delta.delta else {
            panic!()
        };
        assert_eq!(input.input, "{\"city\":");
        let ConverseStreamOutput::ContentBlockDelta(delta) = &events[4] else {
            panic!()
        };
        assert_eq!(
            delta.delta,
            Some(ContentBlockDelta::ReasoningContent(
                ReasoningContentBlockDelta::RedactedContent(Blob(vec![0, 1, 2]))
            ))
        );
        let ConverseStreamOutput::MessageStop(stop) = &events[6] else {
            panic!()
        };
        assert_eq!(stop.stop_reason, StopReason::EndTurn);
        assert_eq!(
            stop.additional_model_response_fields,
            Some(serde_json::json!({"x": [1]}))
        );
        let ConverseStreamOutput::Metadata(metadata) = &events[7] else {
            panic!()
        };
        let usage = metadata.usage.as_ref().unwrap();
        assert_eq!((usage.input_tokens, usage.output_tokens), (10, 2));
        assert_eq!(usage.cache_read_input_tokens, Some(4));
        assert_eq!(metadata.metrics.as_ref().unwrap().latency_ms, 321);
        assert_eq!(events[7].event_type(), "metadata");
    }

    #[test]
    fn keeps_what_it_does_not_know() {
        let unknown = decode_message(&event("somethingNew", r#"{"a":1}"#)).unwrap();
        assert_eq!(unknown.event_type(), "somethingNew");

        let stop = decode_message(&event("messageStop", r#"{"stopReason":"brand_new"}"#)).unwrap();
        let ConverseStreamOutput::MessageStop(stop) = stop else {
            panic!()
        };
        assert_eq!(stop.stop_reason, StopReason::Unknown("brand_new".into()));
        assert_eq!(stop.stop_reason.as_str(), "brand_new");

        let delta = decode_message(&event(
            "contentBlockDelta",
            r#"{"contentBlockIndex":0,"delta":{"image":{"x":1}}}"#,
        ))
        .unwrap();
        let ConverseStreamOutput::ContentBlockDelta(delta) = delta else {
            panic!()
        };
        assert_eq!(
            delta.delta,
            Some(ContentBlockDelta::Unknown(
                "image".into(),
                serde_json::json!({"x": 1})
            ))
        );
    }

    #[test]
    fn round_trips_through_serde() {
        let delta = ContentBlockDeltaEvent {
            delta: Some(ContentBlockDelta::Text("Hi".into())),
            content_block_index: 3,
        };
        let json = serde_json::to_string(&delta).unwrap();
        assert_eq!(json, r#"{"delta":{"text":"Hi"},"contentBlockIndex":3}"#);
        assert_eq!(
            serde_json::from_str::<ContentBlockDeltaEvent>(&json).unwrap(),
            delta
        );
    }

    #[test]
    fn surfaces_exceptions_and_errors() {
        let err = decode_message(&exception(
            "modelStreamErrorException",
            r#"{"message":"boom","originalStatusCode":424,"originalMessage":"upstream"}"#,
        ))
        .unwrap_err();
        let ConverseStreamError::Exception(ConverseStreamException::ModelStreamError(e)) = &err
        else {
            panic!("{err:?}")
        };
        assert_eq!(e.original_status_code, Some(424));
        assert_eq!(err.to_string(), "modelStreamErrorException: boom");

        let err =
            decode_message(&exception("somethingNewException", r#"{"message":"x"}"#)).unwrap_err();
        assert_eq!(err.to_string(), "somethingNewException: x");

        let err = decode_message(&Message {
            headers: vec![
                Header::new(":message-type", HeaderValue::String("error".into())),
                Header::new(":error-code", HeaderValue::String("InternalFailure".into())),
                Header::new(":error-message", HeaderValue::String("bad".into())),
            ],
            payload: Vec::new(),
        })
        .unwrap_err();
        assert_eq!(err.to_string(), "InternalFailure: bad");
    }

    #[test]
    fn rejects_a_malformed_known_payload() {
        let err = decode_message(&event("contentBlockDelta", "not json")).unwrap_err();
        assert!(
            matches!(err, ConverseStreamError::Payload { .. }),
            "{err:?}"
        );
    }
}
