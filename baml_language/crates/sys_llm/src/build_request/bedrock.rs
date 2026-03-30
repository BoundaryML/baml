//! AWS Bedrock Converse API HTTP request builder.
//!
//! Builds raw HTTP requests for the Bedrock Converse API. Text and media
//! (image, video, audio, PDF) are supported. Body serialization is delegated
//! to the `aws-sdk-bedrockruntime` crate via a dry-run interception pattern.
//!
//! Auth (`SigV4` signing, credential resolution) is NOT handled here -- that
//! belongs in `auth_request`.

use std::sync::Arc;

use aws_credential_types::Credentials;
use aws_sdk_bedrockruntime as bedrock;
use baml_base::MediaKind;
use baml_builtins::{PromptAst, PromptAstSimple};
use bedrock::types::{
    ContentBlock, ConversationRole, DocumentBlock, DocumentFormat, DocumentSource, ImageBlock,
    ImageFormat, ImageSource, InferenceConfiguration, Message, SystemContentBlock, VideoBlock,
    VideoFormat, VideoSource,
};

use super::BuildRequestError;

// ============================================================================
// Public entry point
// ============================================================================

pub(crate) async fn build_request(
    client: &crate::baml_std::PrimitiveClient,
    prompt: &bex_vm_types::PromptAst,
    callbacks: Option<&crate::BuildRequestCallbacks>,
) -> Result<crate::baml_std::HttpRequest, BuildRequestError> {
    // Convert BAML prompt to SDK types.
    let (system_blocks, messages) = prompt_to_sdk_types(prompt, &client.default_role)?;
    let inference_config = build_inference_config(client)?;
    let additional_fields = collect_additional_fields(client);

    // Serialize body via the SDK's own serialization pipeline.
    let body = serialize_body_via_sdk(
        &client.model,
        system_blocks,
        messages,
        inference_config,
        additional_fields,
    )
    .await?;

    let url = resolve_url(client, callbacks).await?;

    let mut headers = indexmap::IndexMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());
    headers.insert("accept".to_string(), "application/json".to_string());
    for (key, value) in &client.options.headers {
        headers.insert(key.clone(), value.clone());
    }

    Ok(crate::baml_std::HttpRequest {
        method: "POST".to_string(),
        url,
        headers,
        body,
    })
}

/// Build the Bedrock URL, resolving the region from options or the AWS provider chain.
async fn resolve_url(
    client: &crate::baml_std::PrimitiveClient,
    callbacks: Option<&crate::BuildRequestCallbacks>,
) -> Result<String, BuildRequestError> {
    let bedrock_opts = match &client.options.provider_options {
        Some(crate::baml_std::ProviderOptions::Bedrock(opts)) => opts.clone(),
        _ => crate::baml_std::BedrockOptions::default(),
    };

    if let Some(endpoint) = &bedrock_opts.endpoint_url {
        let endpoint = endpoint.trim_end_matches('/');
        return Ok(format!("{endpoint}/model/{}/converse", client.model));
    }

    let region = crate::auth_request::bedrock::resolve_region(&bedrock_opts, callbacks).await?;
    Ok(format!(
        "https://bedrock-runtime.{region}.amazonaws.com/model/{}/converse",
        client.model
    ))
}

// ============================================================================
// SDK body serialization via map_request interception
// ============================================================================

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
fn dry_run_sdk_config() -> bedrock::Config {
    #[allow(unused_mut)]
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
async fn serialize_body_via_sdk(
    model: &str,
    system_blocks: Vec<SystemContentBlock>,
    messages: Vec<Message>,
    inference_config: Option<InferenceConfiguration>,
    additional_fields: Option<aws_smithy_types::Document>,
) -> Result<String, BuildRequestError> {
    let captured = Arc::new(std::sync::Mutex::new(String::new()));
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
        return Err(BuildRequestError::Other(
            "SDK serialization produced no body (dry-run interception failed)".into(),
        ));
    }
    Ok(body)
}

// ============================================================================
// BAML -> SDK type conversions
// ============================================================================

fn prompt_to_sdk_types(
    prompt: &bex_vm_types::PromptAst,
    default_role: &str,
) -> Result<(Vec<SystemContentBlock>, Vec<Message>), BuildRequestError> {
    let mut system_blocks = Vec::new();
    let mut messages = Vec::new();

    let items = match prompt.as_ref() {
        PromptAst::Vec(v) => v.clone(),
        _ => vec![prompt.clone()],
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
                            BuildRequestError::UnsupportedMedia(format!(
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
                            BuildRequestError::UnsupportedMedia(format!(
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
        other => Err(BuildRequestError::UnsupportedMedia(format!(
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

// ============================================================================
// Media handling
// ============================================================================

enum ResolvedMedia {
    S3Uri(String),
    Bytes(Vec<u8>),
}

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
                    BuildRequestError::UnsupportedMedia(format!(
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

fn s3_location(uri: String) -> bedrock::types::S3Location {
    bedrock::types::S3Location::builder()
        .uri(uri)
        .build()
        .unwrap()
}

fn media_to_content_block(
    media: &baml_builtins::MediaValue,
    content: &baml_builtins::MediaContent,
) -> Result<Vec<ContentBlock>, BuildRequestError> {
    let mime = super::mime_type_as_ok(media)?;
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
                    BuildRequestError::UnsupportedMedia(format!("failed to build image block: {e}"))
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
                    BuildRequestError::UnsupportedMedia(format!("failed to build video block: {e}"))
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
                    BuildRequestError::UnsupportedMedia(format!(
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
                    BuildRequestError::UnsupportedMedia(format!("failed to build audio block: {e}"))
                })?;
            Ok(vec![ContentBlock::Audio(block)])
        }
        MediaKind::Generic => Err(BuildRequestError::UnsupportedMedia(
            "generic media type is not supported -- specify image, video, audio, or pdf".into(),
        )),
    }
}

// ============================================================================
// Inference config + additional fields
// ============================================================================

fn build_inference_config(
    client: &crate::baml_std::PrimitiveClient,
) -> Result<Option<InferenceConfiguration>, BuildRequestError> {
    let mut builder = InferenceConfiguration::builder();
    let mut has_config = false;

    if let Some(max_tokens) = client.max_tokens {
        let narrow = i32::try_from(max_tokens).map_err(|_| BuildRequestError::InvalidOption {
            key: "max_tokens".into(),
            reason: format!(
                "value {max_tokens} is out of the supported range (0..={})",
                i32::MAX
            ),
        })?;
        builder = builder.max_tokens(narrow);
        has_config = true;
    }

    #[allow(clippy::cast_possible_truncation)]
    if let Some(t) = client.options.temperature {
        builder = builder.temperature(t as f32);
        has_config = true;
    }

    #[allow(clippy::cast_possible_truncation)]
    if let Some(p) = client.options.top_p {
        builder = builder.top_p(p as f32);
        has_config = true;
    }

    if let Some(crate::baml_std::ProviderOptions::Bedrock(bedrock_opts)) =
        &client.options.provider_options
    {
        if let Some(seqs) = &bedrock_opts.stop_sequences {
            if !seqs.is_empty() {
                builder = builder.set_stop_sequences(Some(seqs.clone()));
                has_config = true;
            }
        }
    }

    if has_config {
        Ok(Some(builder.build()))
    } else {
        Ok(None)
    }
}

fn collect_additional_fields(
    client: &crate::baml_std::PrimitiveClient,
) -> Option<aws_smithy_types::Document> {
    let mut fields = std::collections::HashMap::new();
    for (key, value) in &client.extra_body {
        if let Some(doc) = json_value_to_document(value) {
            fields.insert(key.clone(), doc);
        }
    }

    if fields.is_empty() {
        None
    } else {
        Some(aws_smithy_types::Document::Object(fields))
    }
}

fn json_value_to_document(value: &serde_json::Value) -> Option<aws_smithy_types::Document> {
    use aws_smithy_types::{Document, Number};
    match value {
        serde_json::Value::Null => Some(Document::Null),
        serde_json::Value::Bool(b) => Some(Document::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                if i >= 0 {
                    #[allow(clippy::cast_sign_loss)]
                    Some(Document::Number(Number::PosInt(i as u64)))
                } else {
                    Some(Document::Number(Number::NegInt(i)))
                }
            } else {
                n.as_f64().map(|f| Document::Number(Number::Float(f)))
            }
        }
        serde_json::Value::String(s) => Some(Document::String(s.clone())),
        serde_json::Value::Array(arr) => {
            let docs: Vec<Document> = arr.iter().filter_map(json_value_to_document).collect();
            Some(Document::Array(docs))
        }
        serde_json::Value::Object(map) => {
            let docs: std::collections::HashMap<String, Document> = map
                .iter()
                .filter_map(|(k, v)| json_value_to_document(v).map(|d| (k.clone(), d)))
                .collect();
            Some(Document::Object(docs))
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_builtins::{MediaContent, MediaValue, PromptAst, PromptAstSimple};

    use super::*;

    fn make_client(
        region: Option<&str>,
        endpoint_url: Option<&str>,
        model: &str,
    ) -> crate::baml_std::PrimitiveClient {
        let options = crate::baml_std::PrimitiveClientOptions {
            model: Some(model.to_string()),
            provider_options: Some(crate::baml_std::ProviderOptions::Bedrock(
                crate::baml_std::BedrockOptions {
                    region: region.map(String::from),
                    endpoint_url: endpoint_url.map(String::from),
                    ..Default::default()
                },
            )),
            default_role: Some("user".to_string()),
            allowed_roles: Some(vec![
                "system".to_string(),
                "user".to_string(),
                "assistant".to_string(),
            ]),
            ..Default::default()
        };
        let defaults = crate::baml_std::PrimitiveClientOptions::provider_defaults(
            crate::LlmProvider::AwsBedrock,
        );
        crate::baml_std::PrimitiveClient::new(
            "test-bedrock".to_string(),
            "aws-bedrock".to_string(),
            options.with_defaults(defaults),
        )
        .unwrap()
    }

    fn make_default_client() -> crate::baml_std::PrimitiveClient {
        make_client(
            Some("us-east-1"),
            None,
            "anthropic.claude-3-haiku-20240307-v1:0",
        )
    }

    fn msg(role: &str, text: &str) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(text.to_string().into()),
            metadata: serde_json::Value::Null,
        })
    }

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

    async fn body_for(
        client: &crate::baml_std::PrimitiveClient,
        prompt: Arc<PromptAst>,
    ) -> serde_json::Value {
        let result = build_request(client, &prompt, None).await.unwrap();
        serde_json::from_str(&result.body).unwrap()
    }

    // -----------------------------------------------------------------------
    // Body snapshot tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn bedrock_system_and_user_text() {
        let client = make_default_client();
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
        let client = make_default_client();
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
        let client = make_default_client();
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
        let options = crate::baml_std::PrimitiveClientOptions {
            model: Some("anthropic.claude-3-haiku-20240307-v1:0".to_string()),
            max_tokens: Some(500),
            temperature: Some(0.5),
            top_p: Some(0.75),
            provider_options: Some(crate::baml_std::ProviderOptions::Bedrock(
                crate::baml_std::BedrockOptions {
                    region: Some("us-east-1".to_string()),
                    ..Default::default()
                },
            )),
            default_role: Some("user".to_string()),
            allowed_roles: Some(vec![
                "system".to_string(),
                "user".to_string(),
                "assistant".to_string(),
            ]),
            ..Default::default()
        };
        let defaults = crate::baml_std::PrimitiveClientOptions::provider_defaults(
            crate::LlmProvider::AwsBedrock,
        );
        let client = crate::baml_std::PrimitiveClient::new(
            "test".to_string(),
            "aws-bedrock".to_string(),
            options.with_defaults(defaults),
        )
        .unwrap();
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
    async fn bedrock_url_contains_model_and_region() {
        let client = make_default_client();
        let result = build_request(&client, &msg("user", "hi"), None)
            .await
            .unwrap();
        assert_eq!(
            result.url,
            "https://bedrock-runtime.us-east-1.amazonaws.com/model/anthropic.claude-3-haiku-20240307-v1:0/converse"
        );
    }

    #[tokio::test]
    async fn bedrock_endpoint_url_overrides_base() {
        let client = make_client(
            Some("us-east-1"),
            Some("http://localhost:4566"),
            "anthropic.claude-3-haiku-20240307-v1:0",
        );
        let result = build_request(&client, &msg("user", "hi"), None)
            .await
            .unwrap();
        assert_eq!(
            result.url,
            "http://localhost:4566/model/anthropic.claude-3-haiku-20240307-v1:0/converse"
        );
    }

    #[tokio::test]
    async fn bedrock_no_model_or_creds_in_body() {
        let client = make_default_client();
        let body = body_for(&client, msg("user", "Hello")).await;
        assert!(body.get("model").is_none());
        assert!(body.get("modelId").is_none());
    }

    // -----------------------------------------------------------------------
    // Media tests
    // -----------------------------------------------------------------------

    const TINY_B64: &str = "SGVsbG8=";

    #[tokio::test]
    async fn bedrock_image_base64() {
        let client = make_default_client();
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
        let client = make_default_client();
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
        let client = make_default_client();
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
        let client = make_default_client();
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
        let client = make_default_client();
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
        let client = make_default_client();
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
    async fn bedrock_non_s3_url_rejected() {
        let client = make_default_client();
        let prompt = media_msg(
            "user",
            baml_base::MediaKind::Image,
            "image/png",
            MediaContent::Url {
                url: "https://example.com/img.png".into(),
                base64_data: None,
            },
        );
        let result = build_request(&client, &prompt, None).await;
        assert!(result.is_err(), "non-s3 URLs should be rejected");
    }

    #[tokio::test]
    async fn bedrock_video_s3() {
        let client = make_default_client();
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
    async fn bedrock_endpoint_url_with_trailing_slash() {
        let client = make_client(
            Some("us-east-1"),
            Some("http://localhost:4566/"),
            "anthropic.claude-3-haiku-20240307-v1:0",
        );
        let result = build_request(&client, &msg("user", "hi"), None)
            .await
            .unwrap();
        assert_eq!(
            result.url,
            "http://localhost:4566/model/anthropic.claude-3-haiku-20240307-v1:0/converse"
        );
    }

    #[tokio::test]
    async fn bedrock_endpoint_url_does_not_require_region() {
        let client = make_client(
            None,
            Some("http://localhost:4566"),
            "anthropic.claude-3-haiku-20240307-v1:0",
        );
        let result = build_request(&client, &msg("user", "hi"), None)
            .await
            .unwrap();
        assert!(result.url.starts_with("http://localhost:4566/"));
    }

    #[tokio::test]
    async fn bedrock_max_tokens_overflow_rejected() {
        let options = crate::baml_std::PrimitiveClientOptions {
            model: Some("anthropic.claude-3-haiku-20240307-v1:0".to_string()),
            max_tokens: Some(i64::from(i32::MAX) + 1),
            provider_options: Some(crate::baml_std::ProviderOptions::Bedrock(
                crate::baml_std::BedrockOptions {
                    region: Some("us-east-1".to_string()),
                    ..Default::default()
                },
            )),
            default_role: Some("user".to_string()),
            allowed_roles: Some(vec![
                "system".to_string(),
                "user".to_string(),
                "assistant".to_string(),
            ]),
            ..Default::default()
        };
        let defaults = crate::baml_std::PrimitiveClientOptions::provider_defaults(
            crate::LlmProvider::AwsBedrock,
        );
        let client = crate::baml_std::PrimitiveClient::new(
            "test".to_string(),
            "aws-bedrock".to_string(),
            options.with_defaults(defaults),
        )
        .unwrap();
        let result = build_request(&client, &msg("user", "hi"), None).await;
        assert!(
            matches!(&result, Err(BuildRequestError::InvalidOption { key, .. }) if key == "max_tokens"),
            "expected InvalidOption for max_tokens, got: {result:?}"
        );
    }
}
