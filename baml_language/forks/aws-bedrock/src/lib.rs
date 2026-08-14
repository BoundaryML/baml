//! A slim, serde-based model of the AWS Bedrock Converse API request.
//!
//! This replaces `aws-sdk-bedrockruntime` for BAML's only use case: building
//! the JSON body and URL path for a Converse request. The serde representations
//! below produce byte-compatible JSON with the Smithy-generated serializer
//! (`shape_converse_input` etc.) for the subset of the API BAML uses (text and
//! image/video/document/audio content, inference config, additional model
//! request fields).
//!
//! Response parsing is handled separately in `sys_llm` with plain serde, so it
//! is not modeled here.

use base64::Engine as _;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde::{Serialize, Serializer};

// ---------------------------------------------------------------------------
// Model path
// ---------------------------------------------------------------------------

/// Characters that must be percent-encoded in a single path label, matching
/// `aws_smithy_http::urlencode::BASE_SET` (used with `EncodingStrategy::Default`
/// — `/` IS encoded, keeping the model id in a single path segment).
const LABEL_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'/')
    .add(b':')
    .add(b',')
    .add(b'?')
    .add(b'#')
    .add(b'[')
    .add(b']')
    .add(b'{')
    .add(b'}')
    .add(b'|')
    .add(b'@')
    .add(b'!')
    .add(b'$')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b';')
    .add(b'=')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'"')
    .add(b'^')
    .add(b'`')
    .add(b'\\');

/// Build the Converse request path for a model id: `/model/{encoded}/converse`.
///
/// The model id is encoded as a single path label, so `/` becomes `%2F` and
/// `:` becomes `%3A` — matching the AWS SDK's `uri_base` for the Converse op.
pub fn converse_model_path(model_id: &str) -> String {
    let encoded = utf8_percent_encode(model_id, LABEL_SET).to_string();
    format!("/model/{encoded}/converse")
}

// ---------------------------------------------------------------------------
// Blob: base64-encoded bytes
// ---------------------------------------------------------------------------

/// Raw bytes that serialize as a standard-base64 string (matching
/// `aws_smithy_types::base64::encode`).
#[derive(Clone, Debug)]
pub struct Blob(pub Vec<u8>);

impl Blob {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Blob(bytes.into())
    }
}

impl Serialize for Blob {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(&self.0))
    }
}

// ---------------------------------------------------------------------------
// Media formats
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoFormat {
    Mp4,
    Mpeg,
    Mkv,
    Mov,
    Flv,
    Webm,
    #[serde(rename = "three_gp")]
    ThreeGp,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    Mp3,
    Wav,
    Flac,
    Ogg,
    Webm,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentFormat {
    Pdf,
}

// ---------------------------------------------------------------------------
// Media sources
// ---------------------------------------------------------------------------

/// Reference to an object in S3. Serializes as `{"uri": "s3://..."}`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct S3Location {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket_owner: Option<String>,
}

impl S3Location {
    pub fn new(uri: impl Into<String>) -> Self {
        S3Location {
            uri: uri.into(),
            bucket_owner: None,
        }
    }
}

macro_rules! media_source {
    ($name:ident) => {
        #[derive(Clone, Debug, Serialize)]
        #[serde(rename_all = "camelCase")]
        pub enum $name {
            Bytes(Blob),
            S3Location(S3Location),
        }
    };
}

media_source!(ImageSource);
media_source!(VideoSource);
media_source!(DocumentSource);

/// Audio has no S3 source in the Converse API.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AudioSource {
    Bytes(Blob),
}

// ---------------------------------------------------------------------------
// Content blocks
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct ImageBlock {
    pub format: ImageFormat,
    pub source: ImageSource,
}

#[derive(Clone, Debug, Serialize)]
pub struct VideoBlock {
    pub format: VideoFormat,
    pub source: VideoSource,
}

#[derive(Clone, Debug, Serialize)]
pub struct AudioBlock {
    pub format: AudioFormat,
    pub source: AudioSource,
}

#[derive(Clone, Debug, Serialize)]
pub struct DocumentBlock {
    pub format: DocumentFormat,
    pub name: String,
    pub source: DocumentSource,
}

/// A content block in a Converse message. Serializes externally-tagged with a
/// camelCase key, e.g. `{"text": "hi"}` or `{"image": {...}}`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentBlock {
    Text(String),
    Image(ImageBlock),
    Video(VideoBlock),
    Document(DocumentBlock),
    Audio(AudioBlock),
}

/// A block in the top-level `system` array. Only text is supported by BAML.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SystemContentBlock {
    Text(String),
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConversationRole {
    User,
    Assistant,
}

#[derive(Clone, Debug, Serialize)]
pub struct Message {
    pub role: ConversationRole,
    pub content: Vec<ContentBlock>,
}

// ---------------------------------------------------------------------------
// Inference config
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceConfiguration {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
}

impl InferenceConfiguration {
    /// True when every field is unset (so the whole object should be omitted).
    pub fn is_empty(&self) -> bool {
        self.max_tokens.is_none()
            && self.temperature.is_none()
            && self.top_p.is_none()
            && self.stop_sequences.is_none()
    }
}

// ---------------------------------------------------------------------------
// Converse request body
// ---------------------------------------------------------------------------

/// The JSON body of a Converse request. The model id and credentials are NOT
/// part of the body — they go in the URL path and signed headers respectively.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConverseRequest {
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<Vec<SystemContentBlock>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_config: Option<InferenceConfiguration>,
    /// Free-form additional fields merged into the request, modeled directly as
    /// JSON (the Smithy `Document` type is just untyped JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_model_request_fields: Option<serde_json::Value>,
}

impl ConverseRequest {
    /// Serialize the request body to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn body(req: &ConverseRequest) -> serde_json::Value {
        serde_json::from_str(&req.to_json().unwrap()).unwrap()
    }

    #[test]
    fn model_path_encodes_colon() {
        assert_eq!(
            converse_model_path("anthropic.claude-3-haiku-20240307-v1:0"),
            "/model/anthropic.claude-3-haiku-20240307-v1%3A0/converse"
        );
    }

    #[test]
    fn model_path_encodes_arn_slashes() {
        let p = converse_model_path(
            "arn:aws:bedrock:us-west-2:123456789012:foundation-model/anthropic.claude-3-sonnet",
        );
        assert!(p.contains("%2F"));
        assert!(p.contains("%3A"));
        assert!(p.starts_with("/model/"));
        assert!(p.ends_with("/converse"));
    }

    #[test]
    fn system_and_user_text() {
        let req = ConverseRequest {
            messages: vec![Message {
                role: ConversationRole::User,
                content: vec![ContentBlock::Text("Hello".into())],
            }],
            system: Some(vec![SystemContentBlock::Text("You are helpful.".into())]),
            ..Default::default()
        };
        assert_eq!(
            body(&req),
            json!({
                "system": [{"text": "You are helpful."}],
                "messages": [{"role": "user", "content": [{"text": "Hello"}]}]
            })
        );
    }

    #[test]
    fn inference_config_shape() {
        let req = ConverseRequest {
            messages: vec![Message {
                role: ConversationRole::User,
                content: vec![ContentBlock::Text("Hi".into())],
            }],
            inference_config: Some(InferenceConfiguration {
                max_tokens: Some(500),
                temperature: Some(0.5),
                top_p: Some(0.75),
                stop_sequences: None,
            }),
            ..Default::default()
        };
        assert_eq!(
            body(&req),
            json!({
                "messages": [{"role": "user", "content": [{"text": "Hi"}]}],
                "inferenceConfig": {"maxTokens": 500, "temperature": 0.5, "topP": 0.75}
            })
        );
    }

    #[test]
    fn image_base64_block() {
        let req = ConverseRequest {
            messages: vec![Message {
                role: ConversationRole::User,
                content: vec![ContentBlock::Image(ImageBlock {
                    format: ImageFormat::Png,
                    source: ImageSource::Bytes(Blob::new(b"Hello".to_vec())),
                })],
            }],
            ..Default::default()
        };
        assert_eq!(
            body(&req),
            json!({
                "messages": [{
                    "role": "user",
                    "content": [{"image": {"format": "png", "source": {"bytes": "SGVsbG8="}}}]
                }]
            })
        );
    }

    #[test]
    fn image_s3_block() {
        let req = ConverseRequest {
            messages: vec![Message {
                role: ConversationRole::User,
                content: vec![ContentBlock::Image(ImageBlock {
                    format: ImageFormat::Jpeg,
                    source: ImageSource::S3Location(S3Location::new("s3://my-bucket/photo.jpg")),
                })],
            }],
            ..Default::default()
        };
        assert_eq!(
            body(&req),
            json!({
                "messages": [{
                    "role": "user",
                    "content": [{"image": {
                        "format": "jpeg",
                        "source": {"s3Location": {"uri": "s3://my-bucket/photo.jpg"}}
                    }}]
                }]
            })
        );
    }

    #[test]
    fn pdf_document_block() {
        let req = ConverseRequest {
            messages: vec![Message {
                role: ConversationRole::User,
                content: vec![ContentBlock::Document(DocumentBlock {
                    format: DocumentFormat::Pdf,
                    name: "document".into(),
                    source: DocumentSource::Bytes(Blob::new(b"Hello".to_vec())),
                })],
            }],
            ..Default::default()
        };
        assert_eq!(
            body(&req),
            json!({
                "messages": [{
                    "role": "user",
                    "content": [{"document": {
                        "format": "pdf", "name": "document", "source": {"bytes": "SGVsbG8="}
                    }}]
                }]
            })
        );
    }

    #[test]
    fn audio_block() {
        let req = ConverseRequest {
            messages: vec![Message {
                role: ConversationRole::User,
                content: vec![ContentBlock::Audio(AudioBlock {
                    format: AudioFormat::Mp3,
                    source: AudioSource::Bytes(Blob::new(b"Hello".to_vec())),
                })],
            }],
            ..Default::default()
        };
        assert_eq!(
            body(&req),
            json!({
                "messages": [{
                    "role": "user",
                    "content": [{"audio": {"format": "mp3", "source": {"bytes": "SGVsbG8="}}}]
                }]
            })
        );
    }

    #[test]
    fn video_three_gp_format() {
        assert_eq!(
            serde_json::to_value(VideoFormat::ThreeGp).unwrap(),
            json!("three_gp")
        );
    }

    #[test]
    fn no_model_field_in_body() {
        let req = ConverseRequest {
            messages: vec![Message {
                role: ConversationRole::User,
                content: vec![ContentBlock::Text("Hello".into())],
            }],
            ..Default::default()
        };
        let b = body(&req);
        assert!(b.get("model").is_none());
        assert!(b.get("modelId").is_none());
    }
}
