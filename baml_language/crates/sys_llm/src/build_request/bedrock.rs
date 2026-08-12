//! AWS Bedrock Converse API HTTP request builder.
//!
//! Builds raw HTTP requests for the Bedrock Converse API. Text and media
//! (image, video, audio, PDF) are supported. Body serialization uses the slim
//! `aws-bedrock` fork's serde model, which produces JSON compatible with the
//! Bedrock Converse endpoint without pulling in the AWS SDK / Smithy runtime.
//!
//! Auth (`SigV4` signing, credential resolution) is NOT handled here -- that
//! belongs in `auth_request`.

use std::sync::Arc;

use aws_bedrock::{
    AudioBlock, AudioFormat, AudioSource, Blob, ContentBlock, ConversationRole, ConverseRequest,
    DocumentBlock, DocumentFormat, DocumentSource, ImageBlock, ImageFormat, ImageSource,
    InferenceConfiguration, Message, S3Location, SystemContentBlock, VideoBlock, VideoFormat,
    VideoSource, converse_model_path,
};
use baml_base::MediaKind;
use baml_builtins2::{PromptAst, PromptAstSimple};

use super::BuildRequestError;

// ============================================================================
// Public entry point
// ============================================================================

pub(crate) async fn build_request(
    client: &crate::baml_std::PrimitiveClient,
    prompt: &bex_vm_types::PromptAst,
    io: Arc<dyn ::sys_types::runtime_io::RuntimeIo>,
) -> Result<crate::baml_std::HttpRequest, BuildRequestError> {
    let (system_blocks, messages) = prompt_to_sdk_types(prompt, &client.default_role)?;
    let inference_config = build_inference_config(client)?;
    let additional_fields = collect_additional_fields(client);

    let request = ConverseRequest {
        messages,
        system: if system_blocks.is_empty() {
            None
        } else {
            Some(system_blocks)
        },
        inference_config,
        additional_model_request_fields: additional_fields,
    };
    let body = request
        .to_json()
        .map_err(|e| BuildRequestError::Other(format!("failed to serialize Converse body: {e}")))?;

    let url = resolve_url(client, io, &client.model).await?;

    let mut headers = indexmap::IndexMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());
    headers.insert("accept".to_string(), "application/json".to_string());

    Ok(crate::baml_std::HttpRequest {
        method: "POST".to_string(),
        url,
        headers,
        body,
    })
}

/// Build the Bedrock URL: `<base>/model/{encoded model id}/converse`. The model
/// id is percent-encoded as a single path segment (ARN slashes become `%2F`,
/// `:` becomes `%3A`), matching the AWS SDK's URI construction.
async fn resolve_url(
    client: &crate::baml_std::PrimitiveClient,
    io: Arc<dyn ::sys_types::runtime_io::RuntimeIo>,
    model: &str,
) -> Result<String, BuildRequestError> {
    let path = converse_model_path(model);

    let bedrock_opts = match &client.provider_options {
        Some(crate::baml_std::ProviderOptions::Bedrock(opts)) => opts.clone(),
        _ => crate::baml_std::BedrockOptions::default(),
    };

    if let Some(endpoint) = &bedrock_opts.endpoint_url {
        let endpoint = endpoint.trim_end_matches('/');
        return Ok(format!("{endpoint}{path}"));
    }

    let region = crate::auth_request::bedrock::resolve_region(&bedrock_opts, io).await?;
    Ok(format!(
        "https://bedrock-runtime.{region}.amazonaws.com{path}",
    ))
}

// ============================================================================
// BAML -> Converse type conversions
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
                messages.push(Message {
                    role: conv_role,
                    content: blocks,
                });
            }
            PromptAst::Simple(content) => {
                let conv_role = parse_conversation_role(default_role)?;
                let blocks = content_to_content_blocks(content)?;
                messages.push(Message {
                    role: conv_role,
                    content: blocks,
                });
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
    content: &baml_builtins2::MediaContent,
    kind_label: &str,
) -> Result<ResolvedMedia, BuildRequestError> {
    use base64::Engine;
    match content {
        baml_builtins2::MediaContent::Url {
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
        baml_builtins2::MediaContent::Base64 { base64_data, .. }
        | baml_builtins2::MediaContent::Url {
            base64_data: Some(base64_data),
            ..
        }
        | baml_builtins2::MediaContent::File {
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
        baml_builtins2::MediaContent::File {
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

fn parse_audio_format(mime: &str) -> Result<AudioFormat, BuildRequestError> {
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

fn media_to_content_block(
    media: &baml_builtins2::MediaValue,
    content: &baml_builtins2::MediaContent,
) -> Result<Vec<ContentBlock>, BuildRequestError> {
    let mime = super::mime_type_as_ok(media)?;
    let kind_label = media.kind.to_string();
    let source = resolve_media_source(content, &kind_label)?;

    match media.kind {
        MediaKind::Image => {
            let format = parse_image_format(&mime)?;
            let img_source = match source {
                ResolvedMedia::S3Uri(uri) => ImageSource::S3Location(S3Location::new(uri)),
                ResolvedMedia::Bytes(bytes) => ImageSource::Bytes(Blob::new(bytes)),
            };
            Ok(vec![ContentBlock::Image(ImageBlock {
                format,
                source: img_source,
            })])
        }
        MediaKind::Video => {
            let format = parse_video_format(&mime)?;
            let vid_source = match source {
                ResolvedMedia::S3Uri(uri) => VideoSource::S3Location(S3Location::new(uri)),
                ResolvedMedia::Bytes(bytes) => VideoSource::Bytes(Blob::new(bytes)),
            };
            Ok(vec![ContentBlock::Video(VideoBlock {
                format,
                source: vid_source,
            })])
        }
        MediaKind::Pdf => {
            if mime != "application/pdf" {
                return Err(BuildRequestError::UnsupportedMedia(format!(
                    "unsupported document format for Bedrock: {mime}"
                )));
            }
            let doc_source = match source {
                ResolvedMedia::S3Uri(uri) => DocumentSource::S3Location(S3Location::new(uri)),
                ResolvedMedia::Bytes(bytes) => DocumentSource::Bytes(Blob::new(bytes)),
            };
            Ok(vec![ContentBlock::Document(DocumentBlock {
                format: DocumentFormat::Pdf,
                name: "document".to_string(),
                source: doc_source,
            })])
        }
        MediaKind::Audio => {
            let format = parse_audio_format(&mime)?;
            let aud_source = match source {
                ResolvedMedia::S3Uri(_) => {
                    return Err(BuildRequestError::UnsupportedMedia(
                        "Bedrock does not support S3 URIs for audio".into(),
                    ));
                }
                ResolvedMedia::Bytes(bytes) => AudioSource::Bytes(Blob::new(bytes)),
            };
            Ok(vec![ContentBlock::Audio(AudioBlock {
                format,
                source: aud_source,
            })])
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
    let mut config = InferenceConfiguration::default();

    if let Some(crate::baml_std::ProviderOptions::Bedrock(bedrock_opts)) = &client.provider_options
    {
        if let Some(max_tokens) = bedrock_opts.max_tokens {
            let narrow =
                i32::try_from(max_tokens).map_err(|_| BuildRequestError::InvalidOption {
                    key: "max_tokens".into(),
                    reason: format!(
                        "value {max_tokens} is out of the supported range (0..={})",
                        i32::MAX
                    ),
                })?;
            config.max_tokens = Some(narrow);
        }

        #[allow(clippy::cast_possible_truncation)]
        if let Some(t) = bedrock_opts.temperature {
            config.temperature = Some(t as f32);
        }

        #[allow(clippy::cast_possible_truncation)]
        if let Some(p) = bedrock_opts.top_p {
            config.top_p = Some(p as f32);
        }

        if let Some(seqs) = &bedrock_opts.stop_sequences {
            if !seqs.is_empty() {
                config.stop_sequences = Some(seqs.clone());
            }
        }
    }

    if config.is_empty() {
        Ok(None)
    } else {
        Ok(Some(config))
    }
}

/// Collect `extra_body` fields into the `additionalModelRequestFields` JSON
/// object, or `None` when there are none.
fn collect_additional_fields(
    client: &crate::baml_std::PrimitiveClient,
) -> Option<serde_json::Value> {
    if client.extra_body.is_empty() {
        return None;
    }
    let map: serde_json::Map<String, serde_json::Value> = client
        .extra_body
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Some(serde_json::Value::Object(map))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_builtins2::{MediaContent, MediaValue, PromptAst, PromptAstSimple};
    use bex_external_types::AsBexExternalValue;

    use super::*;

    fn make_client(
        region: Option<&str>,
        endpoint_url: Option<&str>,
        model: &str,
    ) -> crate::baml_std::PrimitiveClient {
        let options = crate::baml_std::PrimitiveClientOptions {
            model: Some(model.to_string()),
            provider_options: crate::baml_std::BedrockOptions {
                region: region.map(String::from),
                endpoint_url: endpoint_url.map(String::from),
                ..Default::default()
            }
            .into_bex_external_value(),
            default_role: Some("user".to_string()),
            allowed_roles: Some(vec![
                "system".to_string(),
                "user".to_string(),
                "assistant".to_string(),
            ]),
            ..Default::default()
        };
        crate::baml_std::PrimitiveClient::new(
            "test-bedrock".to_string(),
            "aws-bedrock".to_string(),
            options,
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
        let result = build_request(
            client,
            &prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
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
            provider_options: crate::baml_std::BedrockOptions {
                region: Some("us-east-1".to_string()),
                max_tokens: Some(500),
                temperature: Some(0.5),
                top_p: Some(0.75),
                ..Default::default()
            }
            .into_bex_external_value(),
            default_role: Some("user".to_string()),
            allowed_roles: Some(vec![
                "system".to_string(),
                "user".to_string(),
                "assistant".to_string(),
            ]),
            ..Default::default()
        };
        let client = crate::baml_std::PrimitiveClient::new(
            "test".to_string(),
            "aws-bedrock".to_string(),
            options,
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
        let result = build_request(
            &client,
            &msg("user", "hi"),
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert_eq!(
            result.url,
            "https://bedrock-runtime.us-east-1.amazonaws.com/model/anthropic.claude-3-haiku-20240307-v1%3A0/converse"
        );
    }

    #[tokio::test]
    async fn bedrock_endpoint_url_overrides_base() {
        let client = make_client(
            Some("us-east-1"),
            Some("http://localhost:4566"),
            "anthropic.claude-3-haiku-20240307-v1:0",
        );
        let result = build_request(
            &client,
            &msg("user", "hi"),
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert_eq!(
            result.url,
            "http://localhost:4566/model/anthropic.claude-3-haiku-20240307-v1%3A0/converse"
        );
    }

    #[tokio::test]
    async fn bedrock_arn_model_id_encoded_in_url() {
        let client = make_client(
            Some("us-west-2"),
            None,
            "arn:aws:bedrock:us-west-2:123456789012:foundation-model/anthropic.claude-3-sonnet",
        );
        let result = build_request(
            &client,
            &msg("user", "hi"),
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert_eq!(
            result.url,
            "https://bedrock-runtime.us-west-2.amazonaws.com/model/arn%3Aaws%3Abedrock%3Aus-west-2%3A123456789012%3Afoundation-model%2Fanthropic.claude-3-sonnet/converse"
        );
        // The `/` in the ARN must be encoded as %2F so it stays in one path segment.
        assert!(result.url.contains("%2F"));
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
        let result = build_request(
            &client,
            &prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await;
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
        let result = build_request(
            &client,
            &msg("user", "hi"),
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert_eq!(
            result.url,
            "http://localhost:4566/model/anthropic.claude-3-haiku-20240307-v1%3A0/converse"
        );
    }

    #[tokio::test]
    async fn bedrock_endpoint_url_does_not_require_region() {
        let client = make_client(
            None,
            Some("http://localhost:4566"),
            "anthropic.claude-3-haiku-20240307-v1:0",
        );
        let result = build_request(
            &client,
            &msg("user", "hi"),
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert!(result.url.starts_with("http://localhost:4566/"));
    }

    #[tokio::test]
    async fn bedrock_max_tokens_overflow_rejected() {
        let options = crate::baml_std::PrimitiveClientOptions {
            model: Some("anthropic.claude-3-haiku-20240307-v1:0".to_string()),
            provider_options: crate::baml_std::BedrockOptions {
                region: Some("us-east-1".to_string()),
                max_tokens: Some(i64::from(i32::MAX) + 1),
                ..Default::default()
            }
            .into_bex_external_value(),
            default_role: Some("user".to_string()),
            allowed_roles: Some(vec![
                "system".to_string(),
                "user".to_string(),
                "assistant".to_string(),
            ]),
            ..Default::default()
        };
        let client = crate::baml_std::PrimitiveClient::new(
            "test".to_string(),
            "aws-bedrock".to_string(),
            options,
        )
        .unwrap();
        let result = build_request(
            &client,
            &msg("user", "hi"),
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await;
        assert!(
            matches!(&result, Err(BuildRequestError::InvalidOption { key, .. }) if key == "max_tokens"),
            "expected InvalidOption for max_tokens, got: {result:?}"
        );
    }
}
