//! AWS Bedrock Converse API HTTP request builder.
//!
//! Builds raw HTTP requests for the Bedrock Converse API with `SigV4` signing.
//! Text-only — no media support yet.
//!
//! Body serialization is delegated to the `aws-sdk-bedrockruntime` crate: we
//! build typed SDK structs, then intercept the serialized HTTP body via
//! `customize().map_request()` before any network I/O occurs.
//!
//! Credentials are resolved in order:
//! 1. Explicit `access_key_id` / `secret_access_key` / `session_token` in client options
//! 2. AWS default provider chain (`aws_config`) — env vars, profiles, IMDS, etc.
//!
//! Region is resolved in order:
//! 1. Explicit `region` in client options
//! 2. AWS default provider chain

use aws_credential_types::Credentials;
use aws_sdk_bedrockruntime as bedrock;
use baml_base::MediaKind;
use baml_builtins::{PromptAst, PromptAstSimple};
use bedrock::types::{
    ContentBlock, ConversationRole, DocumentBlock, DocumentFormat, DocumentSource, ImageBlock,
    ImageFormat, ImageSource, InferenceConfiguration, Message, SystemContentBlock, VideoBlock,
    VideoFormat, VideoSource,
};
use indexmap::IndexMap;

use super::{
    BuildRequestCallbacks, BuildRequestError, LlmPrimitiveClient, LlmRequestBuilder,
    RawHttpRequest, get_string_option, mime_type_as_ok,
};

/// Builder for the AWS Bedrock Converse provider.
pub(crate) struct BedrockBuilder;

/// Provider-specific option keys consumed by the builder (not forwarded to body).
const BEDROCK_SKIP_KEYS: &[&str] = &[
    "region",
    "endpoint_url",
    "profile",
    "access_key_id",
    "secret_access_key",
    "session_token",
    // inference config fields handled specially
    "max_tokens",
    "temperature",
    "top_p",
    "stop_sequences",
];

/// Build the Bedrock request URL.
///
/// If `endpoint_url` is set, uses it as the base (for local mocking / custom
/// proxies like `LocalStack`). Otherwise constructs the standard AWS URL from
/// the `region` option.
fn build_bedrock_url(
    client: &LlmPrimitiveClient,
    model: &str,
    endpoint: &str,
) -> Result<String, BuildRequestError> {
    if let Some(base) = get_string_option(client, "endpoint_url") {
        let base = base.trim_end_matches('/');
        Ok(format!("{base}/model/{model}/{endpoint}"))
    } else {
        let region = get_string_option(client, "region")
            .ok_or_else(|| BuildRequestError::MissingOption("region".into()))?;
        Ok(format!(
            "https://bedrock-runtime.{region}.amazonaws.com/model/{model}/{endpoint}"
        ))
    }
}

impl BedrockBuilder {
    /// Build the request URL from region + model options.
    #[cfg(test)]
    #[allow(clippy::unused_self)]
    fn build_url(
        &self,
        client: &LlmPrimitiveClient,
        stream: bool,
    ) -> Result<String, BuildRequestError> {
        let model = get_string_option(client, "model")
            .ok_or_else(|| BuildRequestError::MissingOption("model".into()))?;
        let endpoint = if stream {
            "converse-stream"
        } else {
            "converse"
        };
        build_bedrock_url(client, &model, endpoint)
    }
}

impl LlmRequestBuilder for BedrockBuilder {
    /// Builds an unsigned Bedrock Converse API request.
    ///
    /// Resolves the AWS region (from options or the default provider chain) for
    /// URL construction. Credential resolution and `SigV4` signing are handled
    /// by [`crate::auth_request::BedrockAuth`] as a post-build step.
    async fn build_request(
        &self,
        client: &LlmPrimitiveClient,
        prompt: bex_vm_types::PromptAst,
        stream: bool,
        _callbacks: &BuildRequestCallbacks<'_>,
    ) -> Result<RawHttpRequest, BuildRequestError> {
        let model = get_string_option(client, "model")
            .ok_or_else(|| BuildRequestError::MissingOption("model".into()))?;
        let endpoint = if stream {
            "converse-stream"
        } else {
            "converse"
        };
        let url = build_bedrock_url(client, &model, endpoint)?;

        // Convert BAML prompt to SDK types.
        let (system_blocks, messages) = prompt_to_sdk_types(prompt, &client.default_role)?;
        let inference_config = build_sdk_inference_config(client);
        let additional_fields = collect_additional_fields(client);

        // Serialize body via the SDK's own serialization pipeline.
        let body = serialize_body_via_sdk(
            &model,
            system_blocks,
            messages,
            inference_config,
            additional_fields,
        )
        .await?;

        let mut headers = IndexMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        headers.insert("accept".to_string(), "application/json".to_string());

        // Forward custom headers from client.options["headers"].
        if let Some(bex_external_types::BexExternalValue::Map { entries, .. }) =
            client.options.get("headers")
        {
            for (key, value) in entries {
                if let bex_external_types::BexExternalValue::String(v) = value {
                    headers.insert(key.clone(), v.clone());
                }
            }
        }

        Ok(RawHttpRequest {
            method: "POST".to_string(),
            url,
            headers,
            body,
        })
    }
}

// ---------------------------------------------------------------------------
// SDK body serialization via map_request interception
// ---------------------------------------------------------------------------

/// Sentinel error returned by the `map_request` interceptor to abort the SDK
/// pipeline after capturing the serialized body.
#[derive(Debug)]
struct DryRunError;

impl std::fmt::Display for DryRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "dry-run: body captured, request aborted")
    }
}

impl std::error::Error for DryRunError {}

/// Build a throwaway SDK client config for serialization only.
///
/// On native, a minimal config with dummy credentials suffices. On WASM,
/// the SDK orchestrator also needs sleep and time-source implementations
/// (even though we abort before any real I/O) to avoid hanging.
fn dry_run_sdk_config() -> bedrock::Config {
    #[allow(unused_mut)] // mut needed on wasm32
    let mut builder = bedrock::Config::builder()
        .behavior_version(bedrock::config::BehaviorVersion::latest())
        .region(bedrock::config::Region::new("us-east-1"))
        .credentials_provider(Credentials::new("AKID", "SECRET", None, None, "dry-run"))
        .retry_config(aws_smithy_types::retry::RetryConfig::disabled());

    #[cfg(target_arch = "wasm32")]
    {
        builder = builder
            .sleep_impl(crate::wasm::BrowserSleep)
            .time_source(crate::wasm::BrowserTime);
    }

    builder.build()
}

/// Serialize the Converse API request body using the SDK's own Smithy serializer.
///
/// Constructs a throwaway SDK client with dummy credentials, builds the Converse
/// request with typed SDK models, then intercepts the serialized HTTP body via
/// `customize().map_request()` before any signing or network I/O occurs.
async fn serialize_body_via_sdk(
    model: &str,
    system_blocks: Vec<SystemContentBlock>,
    messages: Vec<Message>,
    inference_config: Option<InferenceConfiguration>,
    additional_fields: Option<aws_smithy_types::Document>,
) -> Result<String, BuildRequestError> {
    let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let captured_clone = captured.clone();

    let sdk_client = bedrock::Client::from_conf(dry_run_sdk_config());

    let mut fluent = sdk_client.converse().model_id(model);
    fluent = fluent.set_messages(Some(messages));
    if !system_blocks.is_empty() {
        fluent = fluent.set_system(Some(system_blocks));
    }
    if let Some(cfg) = inference_config {
        fluent = fluent.inference_config(cfg);
    }
    if let Some(doc) = additional_fields {
        fluent = fluent.additional_model_request_fields(doc);
    }

    let _ = fluent
        .customize()
        .map_request(move |req| {
            if let Some(bytes) = req.body().bytes() {
                *captured_clone.lock().unwrap() = String::from_utf8_lossy(bytes).into_owned();
            }
            Err::<_, DryRunError>(DryRunError)
        })
        .send()
        .await;

    let body = captured.lock().unwrap().clone();
    if body.is_empty() {
        return Err(BuildRequestError::BodySerialization(
            "SDK serialization produced no body (dry-run interception failed)".into(),
        ));
    }
    Ok(body)
}

// ---------------------------------------------------------------------------
// BAML → SDK type conversions
// ---------------------------------------------------------------------------

/// Convert a BAML `PromptAst` into SDK `SystemContentBlock`s and `Message`s.
fn prompt_to_sdk_types(
    prompt: bex_vm_types::PromptAst,
    default_role: &str,
) -> Result<(Vec<SystemContentBlock>, Vec<Message>), BuildRequestError> {
    let mut system_blocks = Vec::new();
    let mut messages = Vec::new();

    let items = match prompt.as_ref() {
        PromptAst::Vec(v) => v.clone(),
        _ => vec![prompt],
    };

    for item in &items {
        match item.as_ref() {
            PromptAst::Message {
                role,
                content,
                metadata: _,
            } if role == "system" => {
                system_blocks.extend(content_to_system_blocks(content)?);
            }
            PromptAst::Message {
                role,
                content,
                metadata: _,
            } => {
                let conv_role = parse_conversation_role(role)?;
                let blocks = content_to_content_blocks(content)?;
                messages.push(
                    Message::builder()
                        .role(conv_role)
                        .set_content(Some(blocks))
                        .build()
                        .map_err(|e| {
                            BuildRequestError::BodySerialization(format!(
                                "failed to build message: {e}"
                            ))
                        })?,
                );
            }
            PromptAst::Simple(content) => {
                let conv_role = parse_conversation_role(default_role)?;
                let blocks = content_to_content_blocks(content)?;
                messages.push(
                    Message::builder()
                        .role(conv_role)
                        .set_content(Some(blocks))
                        .build()
                        .map_err(|e| {
                            BuildRequestError::BodySerialization(format!(
                                "failed to build message: {e}"
                            ))
                        })?,
                );
            }
            PromptAst::Vec(_) => unreachable!(),
        }
    }

    Ok((system_blocks, messages))
}

fn parse_conversation_role(role: &str) -> Result<ConversationRole, BuildRequestError> {
    match role {
        "user" => Ok(ConversationRole::User),
        "assistant" => Ok(ConversationRole::Assistant),
        other => Err(BuildRequestError::BodySerialization(format!(
            "unsupported conversation role for Bedrock: {other}"
        ))),
    }
}

fn content_to_content_blocks(
    content: &PromptAstSimple,
) -> Result<Vec<ContentBlock>, BuildRequestError> {
    match content {
        PromptAstSimple::String(s) => Ok(vec![ContentBlock::Text(s.clone())]),
        PromptAstSimple::Media(media) => media.read_content(|c| media_to_content_block(media, c)),
        PromptAstSimple::Multiple(items) => {
            let mut blocks = Vec::new();
            for item in items {
                blocks.extend(content_to_content_blocks(item)?);
            }
            Ok(blocks)
        }
    }
}

fn content_to_system_blocks(
    content: &PromptAstSimple,
) -> Result<Vec<SystemContentBlock>, BuildRequestError> {
    match content {
        PromptAstSimple::String(s) => Ok(vec![SystemContentBlock::Text(s.clone())]),
        PromptAstSimple::Media(_) => Err(BuildRequestError::UnsupportedMedia(
            "Bedrock system messages do not support media content".into(),
        )),
        PromptAstSimple::Multiple(items) => {
            let mut blocks = Vec::new();
            for item in items {
                blocks.extend(content_to_system_blocks(item)?);
            }
            Ok(blocks)
        }
    }
}

/// Resolved media: either an S3 URI or raw bytes ready for a `Blob`.
enum ResolvedMedia {
    S3Uri(String),
    Bytes(Vec<u8>),
}

/// Resolve media content into either an S3 URI or raw bytes.
///
/// HACK: The SDK's `Blob` type stores raw bytes and the Smithy serializer
/// re-encodes them as base64. We already have base64 data, so this is a
/// wasteful decode then re-encode round-trip. If this becomes a perf issue,
/// we can upstream a change to allow base64 to be passed directly.
fn resolve_media_source(
    content: &baml_builtins::MediaContent,
    kind_label: &str,
) -> Result<ResolvedMedia, BuildRequestError> {
    use base64::Engine;
    match content {
        baml_builtins::MediaContent::Url {
            url,
            base64_data: None,
        } => {
            if url.starts_with("s3://") {
                Ok(ResolvedMedia::S3Uri(url.clone()))
            } else {
                Err(BuildRequestError::UnsupportedMedia(format!(
                    "Bedrock requires s3:// URIs for {kind_label} URLs, got: {url}"
                )))
            }
        }
        baml_builtins::MediaContent::Base64 { base64_data, .. }
        | baml_builtins::MediaContent::Url {
            base64_data: Some(base64_data),
            ..
        }
        | baml_builtins::MediaContent::File {
            base64_data: Some(base64_data),
            ..
        } => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(base64_data)
                .map_err(|e| {
                    BuildRequestError::BodySerialization(format!(
                        "invalid base64 {kind_label} data: {e}"
                    ))
                })?;
            Ok(ResolvedMedia::Bytes(bytes))
        }
        baml_builtins::MediaContent::File {
            base64_data: None, ..
        } => Err(BuildRequestError::FileNotResolved(format!(
            "{kind_label} file content was not resolved properly"
        ))),
    }
}

/// Parse a MIME type string into a Bedrock `ImageFormat`.
fn parse_image_format(mime: &str) -> Result<ImageFormat, BuildRequestError> {
    match mime {
        "image/png" => Ok(ImageFormat::Png),
        "image/jpeg" | "image/jpg" => Ok(ImageFormat::Jpeg),
        "image/gif" => Ok(ImageFormat::Gif),
        "image/webp" => Ok(ImageFormat::Webp),
        other => Err(BuildRequestError::UnsupportedMedia(format!(
            "unsupported image format for Bedrock: {other}"
        ))),
    }
}

/// Parse a MIME type string into a Bedrock `VideoFormat`.
fn parse_video_format(mime: &str) -> Result<VideoFormat, BuildRequestError> {
    match mime {
        "video/mp4" => Ok(VideoFormat::Mp4),
        "video/mpeg" => Ok(VideoFormat::Mpeg),
        "video/x-matroska" => Ok(VideoFormat::Mkv),
        "video/quicktime" => Ok(VideoFormat::Mov),
        "video/x-flv" => Ok(VideoFormat::Flv),
        "video/webm" => Ok(VideoFormat::Webm),
        "video/3gpp" => Ok(VideoFormat::ThreeGp),
        other => Err(BuildRequestError::UnsupportedMedia(format!(
            "unsupported video format for Bedrock: {other}"
        ))),
    }
}

/// Parse a MIME type string into a Bedrock `AudioFormat`.
fn parse_audio_format(mime: &str) -> Result<bedrock::types::AudioFormat, BuildRequestError> {
    use bedrock::types::AudioFormat;
    match mime {
        "audio/mpeg" | "audio/mp3" => Ok(AudioFormat::Mp3),
        "audio/wav" | "audio/x-wav" => Ok(AudioFormat::Wav),
        "audio/flac" | "audio/x-flac" => Ok(AudioFormat::Flac),
        "audio/ogg" => Ok(AudioFormat::Ogg),
        "audio/webm" => Ok(AudioFormat::Webm),
        other => Err(BuildRequestError::UnsupportedMedia(format!(
            "unsupported audio format for Bedrock: {other}"
        ))),
    }
}

/// Build an `S3Location` from a URI string.
fn s3_location(uri: String) -> bedrock::types::S3Location {
    bedrock::types::S3Location::builder()
        .uri(uri)
        .build()
        .unwrap()
}

/// Convert a BAML `MediaValue` into Bedrock `ContentBlock`(s).
fn media_to_content_block(
    media: &baml_builtins::MediaValue,
    content: &baml_builtins::MediaContent,
) -> Result<Vec<ContentBlock>, BuildRequestError> {
    let mime = mime_type_as_ok(media)?;
    let kind_label = media.kind.to_string();
    let source = resolve_media_source(content, &kind_label)?;

    match media.kind {
        MediaKind::Image => {
            let format = parse_image_format(mime)?;
            let img_source = match source {
                ResolvedMedia::S3Uri(uri) => ImageSource::S3Location(s3_location(uri)),
                ResolvedMedia::Bytes(bytes) => {
                    ImageSource::Bytes(aws_smithy_types::Blob::new(bytes))
                }
            };
            let block = ImageBlock::builder()
                .format(format)
                .source(img_source)
                .build()
                .map_err(|e| {
                    BuildRequestError::BodySerialization(format!(
                        "failed to build image block: {e}"
                    ))
                })?;
            Ok(vec![ContentBlock::Image(block)])
        }
        MediaKind::Video => {
            let format = parse_video_format(mime)?;
            let vid_source = match source {
                ResolvedMedia::S3Uri(uri) => VideoSource::S3Location(s3_location(uri)),
                ResolvedMedia::Bytes(bytes) => {
                    VideoSource::Bytes(aws_smithy_types::Blob::new(bytes))
                }
            };
            let block = VideoBlock::builder()
                .format(format)
                .source(vid_source)
                .build()
                .map_err(|e| {
                    BuildRequestError::BodySerialization(format!(
                        "failed to build video block: {e}"
                    ))
                })?;
            Ok(vec![ContentBlock::Video(block)])
        }
        MediaKind::Pdf => {
            let doc_source = match source {
                ResolvedMedia::S3Uri(uri) => DocumentSource::S3Location(s3_location(uri)),
                ResolvedMedia::Bytes(bytes) => {
                    DocumentSource::Bytes(aws_smithy_types::Blob::new(bytes))
                }
            };
            let block = DocumentBlock::builder()
                .format(DocumentFormat::Pdf)
                .name("document")
                .source(doc_source)
                .build()
                .map_err(|e| {
                    BuildRequestError::BodySerialization(format!(
                        "failed to build document block: {e}"
                    ))
                })?;
            Ok(vec![ContentBlock::Document(block)])
        }
        MediaKind::Audio => {
            let format = parse_audio_format(mime)?;
            let aud_source = match source {
                ResolvedMedia::S3Uri(_) => {
                    return Err(BuildRequestError::UnsupportedMedia(
                        "Bedrock does not support S3 URIs for audio".into(),
                    ));
                }
                ResolvedMedia::Bytes(bytes) => {
                    bedrock::types::AudioSource::Bytes(aws_smithy_types::Blob::new(bytes))
                }
            };
            let block = bedrock::types::AudioBlock::builder()
                .format(format)
                .source(aud_source)
                .build()
                .map_err(|e| {
                    BuildRequestError::BodySerialization(format!(
                        "failed to build audio block: {e}"
                    ))
                })?;
            Ok(vec![ContentBlock::Audio(block)])
        }
        MediaKind::Generic => Err(BuildRequestError::UnsupportedMedia(
            "generic media type is not supported — specify image, video, audio, or pdf".into(),
        )),
    }
}

/// Build an `InferenceConfiguration` from client options, if any are set.
#[allow(clippy::cast_possible_truncation)]
fn build_sdk_inference_config(client: &LlmPrimitiveClient) -> Option<InferenceConfiguration> {
    use bex_external_types::BexExternalValue;

    let mut builder = InferenceConfiguration::builder();
    let mut has_config = false;

    if let Some(BexExternalValue::Int(v)) = client.options.get("max_tokens") {
        builder = builder.max_tokens(*v as i32);
        has_config = true;
    }
    if let Some(BexExternalValue::Float(v)) = client.options.get("temperature") {
        builder = builder.temperature(*v as f32);
        has_config = true;
    }
    if let Some(BexExternalValue::Float(v)) = client.options.get("top_p") {
        builder = builder.top_p(*v as f32);
        has_config = true;
    }
    if let Some(BexExternalValue::Array { items, .. }) = client.options.get("stop_sequences") {
        let seqs: Vec<String> = items
            .iter()
            .filter_map(|v| match v {
                BexExternalValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        if !seqs.is_empty() {
            builder = builder.set_stop_sequences(Some(seqs));
            has_config = true;
        }
    }

    if has_config {
        Some(builder.build())
    } else {
        None
    }
}

/// Collect non-skipped options as `additionalModelRequestFields` (`Document`).
fn collect_additional_fields(client: &LlmPrimitiveClient) -> Option<aws_smithy_types::Document> {
    use super::{BUILD_REQUEST_SKIP_KEYS, SPECIALIZE_PROMPT_SKIP_KEYS};

    let mut fields = std::collections::HashMap::new();
    for (key, value) in &client.options {
        if SPECIALIZE_PROMPT_SKIP_KEYS.contains(&key.as_str())
            || BUILD_REQUEST_SKIP_KEYS.contains(&key.as_str())
            || BEDROCK_SKIP_KEYS.contains(&key.as_str())
        {
            continue;
        }
        if let Some(doc) = bex_value_to_document(value) {
            fields.insert(key.clone(), doc);
        }
    }

    if fields.is_empty() {
        None
    } else {
        Some(aws_smithy_types::Document::Object(fields))
    }
}

/// Convert a `BexExternalValue` to a Smithy `Document`.
fn bex_value_to_document(
    value: &bex_external_types::BexExternalValue,
) -> Option<aws_smithy_types::Document> {
    use aws_smithy_types::{Document, Number};
    use bex_external_types::BexExternalValue;

    match value {
        BexExternalValue::Null => Some(Document::Null),
        BexExternalValue::Int(i) => {
            if *i >= 0 {
                Some(Document::Number(Number::PosInt((*i).cast_unsigned())))
            } else {
                Some(Document::Number(Number::NegInt(*i)))
            }
        }
        BexExternalValue::Float(f) => Some(Document::Number(Number::Float(*f))),
        BexExternalValue::Bool(b) => Some(Document::Bool(*b)),
        BexExternalValue::String(s) => Some(Document::String(s.clone())),
        BexExternalValue::Array { items, .. } => {
            let arr: Vec<Document> = items.iter().filter_map(bex_value_to_document).collect();
            Some(Document::Array(arr))
        }
        BexExternalValue::Map { entries, .. } => {
            let map: std::collections::HashMap<String, Document> = entries
                .iter()
                .filter_map(|(k, v)| bex_value_to_document(v).map(|d| (k.clone(), d)))
                .collect();
            Some(Document::Object(map))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_builtins::{MediaContent, MediaValue, PromptAst, PromptAstSimple};
    use bex_external_types::BexExternalValue;
    use indexmap::IndexMap;

    use super::*;
    use crate::build_request::build_request;

    fn make_client(options: Vec<(&str, BexExternalValue)>) -> LlmPrimitiveClient {
        let mut opts = IndexMap::new();
        for (k, v) in options {
            opts.insert(k.to_string(), v);
        }
        LlmPrimitiveClient {
            name: "test-bedrock".to_string(),
            provider: "aws-bedrock".to_string(),
            default_role: "user".to_string(),
            allowed_roles: vec![
                "system".to_string(),
                "user".to_string(),
                "assistant".to_string(),
            ],
            options: opts,
        }
    }

    fn base_options() -> Vec<(&'static str, BexExternalValue)> {
        vec![
            ("region", BexExternalValue::String("us-east-1".into())),
            (
                "model",
                BexExternalValue::String("anthropic.claude-3-haiku-20240307-v1:0".into()),
            ),
            (
                "access_key_id",
                BexExternalValue::String("AKIAIOSFODNN7EXAMPLE".into()),
            ),
            (
                "secret_access_key",
                BexExternalValue::String("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into()),
            ),
        ]
    }

    fn msg(role: &str, text: &str) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(text.to_string().into()),
            metadata: serde_json::Value::Null,
        })
    }

    /// Build a message containing a single media item.
    fn media_msg(
        role: &str,
        kind: baml_base::MediaKind,
        mime: &str,
        content: MediaContent,
    ) -> Arc<PromptAst> {
        let media = Arc::new(MediaValue::new(kind, content, Some(mime.to_string())));
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(PromptAstSimple::Media(media)),
            metadata: serde_json::Value::Null,
        })
    }

    /// Build a message with text + media parts.
    fn multipart_msg(
        role: &str,
        text: &str,
        kind: baml_base::MediaKind,
        mime: &str,
        content: MediaContent,
    ) -> Arc<PromptAst> {
        let media = Arc::new(MediaValue::new(kind, content, Some(mime.to_string())));
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(PromptAstSimple::Multiple(vec![
                Arc::new(PromptAstSimple::String(text.to_string())),
                Arc::new(PromptAstSimple::Media(media)),
            ])),
            metadata: serde_json::Value::Null,
        })
    }

    /// Helper: build a request and return the parsed body JSON.
    async fn body_for(client: &LlmPrimitiveClient, prompt: Arc<PromptAst>) -> serde_json::Value {
        let result = {
            let (h, e, f) = crate::build_request::stub_callbacks();
            build_request(client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        serde_json::from_str(&result.body).unwrap()
    }

    // -----------------------------------------------------------------------
    // Body snapshot tests — each asserts the full JSON body
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn bedrock_system_and_user_text() {
        let client = make_client(base_options());
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("user", "Hello"),
        ]));
        assert_eq!(
            body_for(&client, prompt).await,
            serde_json::json!({
                "system": [{"text": "You are helpful."}],
                "messages": [
                    {"role": "user", "content": [{"text": "Hello"}]}
                ]
            })
        );
    }

    #[tokio::test]
    async fn bedrock_user_only() {
        let client = make_client(base_options());
        assert_eq!(
            body_for(&client, msg("user", "Hello")).await,
            serde_json::json!({
                "messages": [
                    {"role": "user", "content": [{"text": "Hello"}]}
                ]
            })
        );
    }

    #[tokio::test]
    async fn bedrock_multi_turn() {
        let client = make_client(base_options());
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "Be concise."),
            msg("user", "What is 2+2?"),
            msg("assistant", "4"),
            msg("user", "And 3+3?"),
        ]));
        assert_eq!(
            body_for(&client, prompt).await,
            serde_json::json!({
                "system": [{"text": "Be concise."}],
                "messages": [
                    {"role": "user", "content": [{"text": "What is 2+2?"}]},
                    {"role": "assistant", "content": [{"text": "4"}]},
                    {"role": "user", "content": [{"text": "And 3+3?"}]},
                ]
            })
        );
    }

    #[tokio::test]
    async fn bedrock_inference_config() {
        // Use f32-exact values so the snapshot comparison is deterministic.
        let mut opts = base_options();
        opts.push(("max_tokens", BexExternalValue::Int(500)));
        opts.push(("temperature", BexExternalValue::Float(0.5)));
        opts.push(("top_p", BexExternalValue::Float(0.75)));
        let client = make_client(opts);
        assert_eq!(
            body_for(&client, msg("user", "Hi")).await,
            serde_json::json!({
                "messages": [
                    {"role": "user", "content": [{"text": "Hi"}]}
                ],
                "inferenceConfig": {
                    "maxTokens": 500,
                    "temperature": 0.5,
                    "topP": 0.75,
                }
            })
        );
    }

    #[tokio::test]
    async fn bedrock_no_model_or_creds_in_body() {
        let client = make_client(base_options());
        let body = body_for(&client, msg("user", "Hello")).await;
        assert!(
            body.get("model").is_none(),
            "model should be in URL, not body"
        );
        assert!(
            body.get("modelId").is_none(),
            "modelId should be in URL, not body"
        );
        assert!(body.get("access_key_id").is_none());
        assert!(body.get("secret_access_key").is_none());
        assert!(body.get("region").is_none());
    }

    // -----------------------------------------------------------------------
    // Media tests
    // -----------------------------------------------------------------------

    /// `"SGVsbG8="` is base64 for the bytes [0x48, 0x65, 0x6c, 0x6c, 0x6f] ("Hello").
    const TINY_B64: &str = "SGVsbG8=";

    #[tokio::test]
    async fn bedrock_image_base64() {
        let client = make_client(base_options());
        let prompt = media_msg(
            "user",
            baml_base::MediaKind::Image,
            "image/png",
            MediaContent::Base64 {
                base64_data: TINY_B64.into(),
            },
        );
        assert_eq!(
            body_for(&client, prompt).await,
            serde_json::json!({
                "messages": [{
                    "role": "user",
                    "content": [{
                        "image": {
                            "format": "png",
                            "source": {"bytes": TINY_B64}
                        }
                    }]
                }]
            })
        );
    }

    #[tokio::test]
    async fn bedrock_image_s3() {
        let client = make_client(base_options());
        let prompt = media_msg(
            "user",
            baml_base::MediaKind::Image,
            "image/jpeg",
            MediaContent::Url {
                url: "s3://my-bucket/photo.jpg".into(),
                base64_data: None,
            },
        );
        assert_eq!(
            body_for(&client, prompt).await,
            serde_json::json!({
                "messages": [{
                    "role": "user",
                    "content": [{
                        "image": {
                            "format": "jpeg",
                            "source": {"s3Location": {"uri": "s3://my-bucket/photo.jpg"}}
                        }
                    }]
                }]
            })
        );
    }

    #[tokio::test]
    async fn bedrock_video_base64() {
        let client = make_client(base_options());
        let prompt = media_msg(
            "user",
            baml_base::MediaKind::Video,
            "video/mp4",
            MediaContent::Base64 {
                base64_data: TINY_B64.into(),
            },
        );
        assert_eq!(
            body_for(&client, prompt).await,
            serde_json::json!({
                "messages": [{
                    "role": "user",
                    "content": [{
                        "video": {
                            "format": "mp4",
                            "source": {"bytes": TINY_B64}
                        }
                    }]
                }]
            })
        );
    }

    #[tokio::test]
    async fn bedrock_pdf_base64() {
        let client = make_client(base_options());
        let prompt = media_msg(
            "user",
            baml_base::MediaKind::Pdf,
            "application/pdf",
            MediaContent::Base64 {
                base64_data: TINY_B64.into(),
            },
        );
        assert_eq!(
            body_for(&client, prompt).await,
            serde_json::json!({
                "messages": [{
                    "role": "user",
                    "content": [{
                        "document": {
                            "format": "pdf",
                            "name": "document",
                            "source": {"bytes": TINY_B64}
                        }
                    }]
                }]
            })
        );
    }

    #[tokio::test]
    async fn bedrock_audio_base64() {
        let client = make_client(base_options());
        let prompt = media_msg(
            "user",
            baml_base::MediaKind::Audio,
            "audio/mp3",
            MediaContent::Base64 {
                base64_data: TINY_B64.into(),
            },
        );
        assert_eq!(
            body_for(&client, prompt).await,
            serde_json::json!({
                "messages": [{
                    "role": "user",
                    "content": [{
                        "audio": {
                            "format": "mp3",
                            "source": {"bytes": TINY_B64}
                        }
                    }]
                }]
            })
        );
    }

    #[tokio::test]
    async fn bedrock_text_and_image_multipart() {
        let client = make_client(base_options());
        let prompt = multipart_msg(
            "user",
            "Describe this image",
            baml_base::MediaKind::Image,
            "image/jpeg",
            MediaContent::Base64 {
                base64_data: TINY_B64.into(),
            },
        );
        assert_eq!(
            body_for(&client, prompt).await,
            serde_json::json!({
                "messages": [{
                    "role": "user",
                    "content": [
                        {"text": "Describe this image"},
                        {"image": {
                            "format": "jpeg",
                            "source": {"bytes": TINY_B64}
                        }}
                    ]
                }]
            })
        );
    }

    #[tokio::test]
    async fn bedrock_video_s3() {
        let client = make_client(base_options());
        let prompt = media_msg(
            "user",
            baml_base::MediaKind::Video,
            "video/mp4",
            MediaContent::Url {
                url: "s3://bucket/clip.mp4".into(),
                base64_data: None,
            },
        );
        assert_eq!(
            body_for(&client, prompt).await,
            serde_json::json!({
                "messages": [{
                    "role": "user",
                    "content": [{
                        "video": {
                            "format": "mp4",
                            "source": {"s3Location": {"uri": "s3://bucket/clip.mp4"}}
                        }
                    }]
                }]
            })
        );
    }

    #[tokio::test]
    async fn bedrock_non_s3_url_rejected() {
        let client = make_client(base_options());
        let prompt = media_msg(
            "user",
            baml_base::MediaKind::Image,
            "image/png",
            MediaContent::Url {
                url: "https://example.com/img.png".into(),
                base64_data: None,
            },
        );
        let result = {
            let (h, e, f) = crate::build_request::stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        };
        assert!(result.is_err(), "non-s3 URLs should be rejected");
    }

    // -----------------------------------------------------------------------
    // URL, headers, error cases
    // -----------------------------------------------------------------------

    #[test]
    fn bedrock_url_contains_model_and_region() {
        let client = make_client(base_options());
        let url = BedrockBuilder.build_url(&client, false).unwrap();
        assert_eq!(
            url,
            "https://bedrock-runtime.us-east-1.amazonaws.com/model/anthropic.claude-3-haiku-20240307-v1:0/converse"
        );
    }

    #[test]
    fn bedrock_stream_url_uses_converse_stream() {
        let client = make_client(base_options());
        let url = BedrockBuilder.build_url(&client, true).unwrap();
        assert!(url.ends_with("/converse-stream"));
    }

    #[test]
    fn bedrock_endpoint_url_overrides_base() {
        let mut opts = base_options();
        opts.push((
            "endpoint_url",
            BexExternalValue::String("http://localhost:4566".into()),
        ));
        let client = make_client(opts);
        let url = BedrockBuilder.build_url(&client, false).unwrap();
        assert_eq!(
            url,
            "http://localhost:4566/model/anthropic.claude-3-haiku-20240307-v1:0/converse"
        );
    }

    #[test]
    fn bedrock_endpoint_url_with_trailing_slash() {
        let mut opts = base_options();
        opts.push((
            "endpoint_url",
            BexExternalValue::String("http://localhost:4566/".into()),
        ));
        let client = make_client(opts);
        let url = BedrockBuilder.build_url(&client, false).unwrap();
        assert_eq!(
            url,
            "http://localhost:4566/model/anthropic.claude-3-haiku-20240307-v1:0/converse"
        );
    }

    #[test]
    fn bedrock_endpoint_url_does_not_require_region() {
        let client = make_client(vec![
            (
                "model",
                BexExternalValue::String("anthropic.claude-3-haiku-20240307-v1:0".into()),
            ),
            (
                "endpoint_url",
                BexExternalValue::String("http://localhost:4566".into()),
            ),
        ]);
        let url = BedrockBuilder.build_url(&client, false).unwrap();
        assert!(url.starts_with("http://localhost:4566/"));
    }

    #[test]
    fn bedrock_missing_region_errors() {
        let client = make_client(vec![("model", BexExternalValue::String("m".into()))]);
        assert!(BedrockBuilder.build_url(&client, false).is_err());
    }

    #[test]
    fn bedrock_missing_model_errors() {
        let client = make_client(vec![(
            "region",
            BexExternalValue::String("us-east-1".into()),
        )]);
        assert!(BedrockBuilder.build_url(&client, false).is_err());
    }

    #[tokio::test]
    async fn bedrock_sigv4_headers_present() {
        let client = make_client(base_options());
        let result = {
            let (h, e, f) = crate::build_request::stub_callbacks();
            build_request(&client, msg("user", "Hi"), false, &h, &e, &f).await
        }
        .unwrap();
        assert!(result.headers.contains_key("authorization"));
        assert!(result.headers.contains_key("x-amz-date"));
    }

    #[tokio::test]
    async fn bedrock_fails_without_explicit_credentials() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let send_fn = mock_http_send(call_count, 404, "");
        let (e, f) = crate::build_request::noop_env_fs_callbacks();
        let client = make_client(vec![
            ("region", BexExternalValue::String("us-east-1".into())),
            ("model", BexExternalValue::String("some-model".into())),
        ]);
        let result = build_request(&client, msg("user", "Hi"), false, &send_fn, &e, &f).await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // BamlHttpConnector wiring tests
    // -----------------------------------------------------------------------

    use std::sync::atomic::{AtomicUsize, Ordering};

    fn mock_http_send(
        call_count: Arc<AtomicUsize>,
        status: u16,
        body: &'static str,
    ) -> crate::HttpSendFn {
        Arc::new(move |_req| {
            call_count.fetch_add(1, Ordering::SeqCst);
            let body = body.to_string();
            Box::pin(async move {
                Ok(crate::HttpSendResponse {
                    status_code: status,
                    headers: IndexMap::new(),
                    body,
                })
            })
        })
    }

    #[tokio::test]
    async fn bedrock_http_send_invoked_during_credential_resolution() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let send_fn = mock_http_send(call_count.clone(), 404, "");
        let client = make_client(vec![
            ("region", BexExternalValue::String("us-east-1".into())),
            ("model", BexExternalValue::String("some-model".into())),
        ]);
        let (e, f) = crate::build_request::noop_env_fs_callbacks();
        let _result = build_request(&client, msg("user", "Hi"), false, &send_fn, &e, &f).await;
        assert!(call_count.load(Ordering::SeqCst) > 0);
    }

    #[tokio::test]
    async fn bedrock_http_send_not_invoked_with_explicit_credentials() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let send_fn = mock_http_send(call_count.clone(), 200, "");
        let client = make_client(base_options());
        let result = {
            let (_h, e, f) = crate::build_request::stub_callbacks();
            build_request(&client, msg("user", "Hi"), false, &send_fn, &e, &f).await
        };
        assert!(result.is_ok());
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }
}
