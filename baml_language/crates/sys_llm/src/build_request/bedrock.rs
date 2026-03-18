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

use std::time::SystemTime;

use aws_credential_types::{Credentials, provider::ProvideCredentials};
use aws_sdk_bedrockruntime as bedrock;
use baml_base::MediaKind;
use bedrock::types::{
    ContentBlock, ConversationRole, DocumentBlock, DocumentFormat, DocumentSource, ImageBlock,
    ImageFormat, ImageSource, InferenceConfiguration, Message, SystemContentBlock, VideoBlock,
    VideoFormat, VideoSource,
};

/// Platform-aware `SystemTime::now()`.
///
/// On WASM, `std::time::SystemTime::now()` panics — use `web_time` instead.
fn now() -> SystemTime {
    #[cfg(not(target_arch = "wasm32"))]
    {
        SystemTime::now()
    }
    #[cfg(target_arch = "wasm32")]
    {
        let offset = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .unwrap();
        std::time::UNIX_EPOCH + offset
    }
}
use aws_sigv4::{
    http_request::{SignableBody, SignableRequest, SigningSettings, sign},
    sign::v4,
};
use aws_smithy_runtime_api::{
    client::{
        http::{HttpConnectorFuture, SharedHttpConnector},
        result::ConnectorError,
    },
    http as smithy_http,
};
use aws_smithy_types::body::SdkBody;
use baml_builtins::{PromptAst, PromptAstSimple};
use indexmap::IndexMap;

use super::{
    BuildRequestCallbacks, BuildRequestError, LlmPrimitiveClient, LlmRequestBuilder,
    RawHttpRequest, get_string_option, mime_type_as_ok,
};

// ---------------------------------------------------------------------------
// Native: sync env/fs providers using block_on (safe on multi-threaded runtimes)
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
mod native_providers {
    use std::future::Future;

    use aws_types::os_shim_internal::{ProvideEnv, ProvideFs};

    use crate::{EnvReadFn, FsReadFn};

    pub(super) struct BexEnvProvider {
        pub env_read_fn: EnvReadFn,
    }

    impl std::fmt::Debug for BexEnvProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BexEnvProvider").finish()
        }
    }

    impl ProvideEnv for BexEnvProvider {
        fn get(&self, k: &str) -> Result<String, std::env::VarError> {
            let fut = (self.env_read_fn)(k.to_string());
            match futures::executor::block_on(fut) {
                Ok(Some(v)) => Ok(v),
                Ok(None) | Err(_) => Err(std::env::VarError::NotPresent),
            }
        }
    }

    pub(super) struct BexFsProvider {
        pub fs_read_fn: FsReadFn,
    }

    impl std::fmt::Debug for BexFsProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BexFsProvider").finish()
        }
    }

    impl ProvideFs for BexFsProvider {
        fn read_to_end(
            &self,
            path: &std::path::Path,
        ) -> std::pin::Pin<Box<dyn Future<Output = std::io::Result<Vec<u8>>> + Send + '_>> {
            let fut = (self.fs_read_fn)(path.to_string_lossy().into_owned());
            Box::pin(async move {
                match fut.await {
                    Ok(v) => Ok(v),
                    Err(_) => Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "file not found",
                    )),
                }
            })
        }

        fn write(
            &self,
            _path: &std::path::Path,
            _contents: &[u8],
        ) -> std::pin::Pin<Box<dyn Future<Output = std::io::Result<()>> + Send + '_>> {
            unreachable!()
        }
    }
}

// ---------------------------------------------------------------------------
// WASM: async-safe providers (no block_on — would deadlock on single-threaded runtime)
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
mod wasm_providers {
    use aws_credential_types::{
        Credentials,
        provider::{self, future::ProvideCredentials},
    };
    use aws_smithy_async::{
        rt::sleep::{AsyncSleep, Sleep},
        time::TimeSource,
    };

    use crate::EnvReadFn;

    /// Browser-compatible time source using `web_time`.
    #[derive(Debug)]
    pub(super) struct BrowserTime;

    impl TimeSource for BrowserTime {
        fn now(&self) -> std::time::SystemTime {
            let offset = web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .unwrap();
            std::time::UNIX_EPOCH + offset
        }
    }

    /// Browser-compatible async sleep using `futures_timer`.
    #[derive(Debug, Clone)]
    pub(super) struct BrowserSleep;

    impl AsyncSleep for BrowserSleep {
        fn sleep(&self, duration: std::time::Duration) -> Sleep {
            Sleep::new(futures_timer::Delay::new(duration))
        }
    }

    /// Async credential provider that reads AWS env vars via `EnvReadFn`.
    ///
    /// Mirrors the engine's `WasmAwsCreds` but uses the callback architecture
    /// instead of a JS callback provider.
    pub(super) struct EnvCredentialProvider {
        pub env_read: EnvReadFn,
    }

    impl std::fmt::Debug for EnvCredentialProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("EnvCredentialProvider").finish()
        }
    }

    impl EnvCredentialProvider {
        async fn resolve(&self) -> provider::Result {
            let access_key_id = (self.env_read)("AWS_ACCESS_KEY_ID".into())
                .await
                .ok()
                .flatten()
                .ok_or_else(|| {
                    provider::error::CredentialsError::unhandled("AWS_ACCESS_KEY_ID not set")
                })?;

            let secret_access_key = (self.env_read)("AWS_SECRET_ACCESS_KEY".into())
                .await
                .ok()
                .flatten()
                .ok_or_else(|| {
                    provider::error::CredentialsError::unhandled("AWS_SECRET_ACCESS_KEY not set")
                })?;

            let session_token = (self.env_read)("AWS_SESSION_TOKEN".into())
                .await
                .ok()
                .flatten();

            Ok(Credentials::new(
                access_key_id,
                secret_access_key,
                session_token,
                None,
                "baml-bedrock-wasm",
            ))
        }
    }

    impl aws_credential_types::provider::ProvideCredentials for EnvCredentialProvider {
        fn provide_credentials<'a>(&'a self) -> ProvideCredentials<'a>
        where
            Self: 'a,
        {
            ProvideCredentials::new(self.resolve())
        }
    }
}

/// Builder for the AWS Bedrock Converse provider.
pub(crate) struct BedrockBuilder;

/// Provider-specific option keys consumed by the builder (not forwarded to body).
const BEDROCK_SKIP_KEYS: &[&str] = &[
    "region",
    "access_key_id",
    "secret_access_key",
    "session_token",
    // inference config fields handled specially
    "max_tokens",
    "temperature",
    "top_p",
    "stop_sequences",
];

impl LlmRequestBuilder for BedrockBuilder {
    fn provider_skip_keys(&self) -> &'static [&'static str] {
        BEDROCK_SKIP_KEYS
    }

    fn build_url(&self, client: &LlmPrimitiveClient) -> Result<String, BuildRequestError> {
        let region = get_string_option(client, "region")
            .ok_or_else(|| BuildRequestError::MissingOption("region".into()))?;
        let model = get_string_option(client, "model")
            .ok_or_else(|| BuildRequestError::MissingOption("model".into()))?;
        Ok(format!(
            "https://bedrock-runtime.{region}.amazonaws.com/model/{model}/converse"
        ))
    }

    fn build_auth_headers(&self, _client: &LlmPrimitiveClient) -> IndexMap<String, String> {
        IndexMap::new()
    }

    fn build_prompt_body(
        &self,
        client: &LlmPrimitiveClient,
        prompt: bex_vm_types::PromptAst,
    ) -> Result<serde_json::Map<String, serde_json::Value>, BuildRequestError> {
        // Fallback manual implementation — only used if `build_body` is called
        // directly (the normal path goes through `build_request` which uses
        // SDK-based serialization).
        let mut map = serde_json::Map::new();
        let (system_blocks, messages) = prompt_to_sdk_types(prompt, &client.default_role)?;
        if !system_blocks.is_empty() {
            let parts: Vec<serde_json::Value> = system_blocks
                .into_iter()
                .map(|b| match b {
                    SystemContentBlock::Text(s) => serde_json::json!({"text": s}),
                    _ => serde_json::json!({}),
                })
                .collect();
            map.insert("system".to_string(), serde_json::Value::Array(parts));
        }
        let msgs: Vec<serde_json::Value> = messages
            .into_iter()
            .map(|m| {
                let role = m.role().as_str().to_string();
                let content: Vec<serde_json::Value> = m
                    .content()
                    .iter()
                    .map(|b| match b {
                        ContentBlock::Text(s) => serde_json::json!({"text": s}),
                        _ => serde_json::json!({}),
                    })
                    .collect();
                serde_json::json!({"role": role, "content": content})
            })
            .collect();
        map.insert("messages".to_string(), serde_json::Value::Array(msgs));
        if let Some(cfg) = build_sdk_inference_config(client) {
            let mut ic = serde_json::Map::new();
            if let Some(v) = cfg.max_tokens() {
                ic.insert("maxTokens".to_string(), serde_json::json!(v));
            }
            if let Some(v) = cfg.temperature() {
                ic.insert("temperature".to_string(), serde_json::json!(v));
            }
            if let Some(v) = cfg.top_p() {
                ic.insert("topP".to_string(), serde_json::json!(v));
            }
            if !cfg.stop_sequences().is_empty() {
                ic.insert(
                    "stopSequences".to_string(),
                    serde_json::json!(cfg.stop_sequences()),
                );
            }
            if !ic.is_empty() {
                map.insert("inferenceConfig".to_string(), serde_json::Value::Object(ic));
            }
        }
        Ok(map)
    }

    /// Resolves credentials from options or the default AWS provider chain, then
    /// builds the body via the SDK's own serializer and SigV4-signs the request.
    async fn build_request(
        &self,
        client: &LlmPrimitiveClient,
        prompt: bex_vm_types::PromptAst,
        _stream: bool,
        callbacks: &BuildRequestCallbacks<'_>,
    ) -> Result<RawHttpRequest, BuildRequestError> {
        let (credentials, region) = resolve_aws_credentials_and_region(
            client,
            callbacks.http_send,
            callbacks.env_read,
            callbacks.fs_read,
        )
        .await?;

        let model = get_string_option(client, "model")
            .ok_or_else(|| BuildRequestError::MissingOption("model".into()))?;
        let url = format!("https://bedrock-runtime.{region}.amazonaws.com/model/{model}/converse");

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

        // Sign the request.
        let signed_headers = sign_with_credentials(
            &credentials,
            &region,
            "POST",
            &url,
            &headers,
            body.as_bytes(),
        )?;
        headers.extend(signed_headers);

        // Forward custom headers from client.options["headers"] (after signing).
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
            .sleep_impl(wasm_providers::BrowserSleep)
            .time_source(wasm_providers::BrowserTime);
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
        return Err(BuildRequestError::InvalidOption {
            key: "body".into(),
            reason: "SDK serialization produced no body (dry-run interception failed)".into(),
        });
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
                        .map_err(|e| BuildRequestError::InvalidOption {
                            key: "message".into(),
                            reason: e.to_string(),
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
                        .map_err(|e| BuildRequestError::InvalidOption {
                            key: "message".into(),
                            reason: e.to_string(),
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
        other => Err(BuildRequestError::InvalidOption {
            key: "role".into(),
            reason: format!("unsupported conversation role for Bedrock: {other}"),
        }),
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
/// wasteful decode→re-encode round-trip. If this becomes a perf issue, we
/// should expose the SDK's `protocol_serde` serializers directly (the fork
/// supports it) instead of going through `map_request`.
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
                .map_err(|e| BuildRequestError::InvalidOption {
                    key: kind_label.into(),
                    reason: format!("invalid base64 data: {e}"),
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

/// Parse a MIME type string into a Bedrock `DocumentFormat`.
#[allow(dead_code)] // Will be needed when non-PDF document types are supported.
fn parse_document_format(mime: &str) -> Result<DocumentFormat, BuildRequestError> {
    match mime {
        "application/pdf" => Ok(DocumentFormat::Pdf),
        "text/plain" => Ok(DocumentFormat::Txt),
        "text/csv" => Ok(DocumentFormat::Csv),
        "text/html" => Ok(DocumentFormat::Html),
        "text/markdown" => Ok(DocumentFormat::Md),
        "application/msword" => Ok(DocumentFormat::Doc),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
            Ok(DocumentFormat::Docx)
        }
        "application/vnd.ms-excel" => Ok(DocumentFormat::Xls),
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => {
            Ok(DocumentFormat::Xlsx)
        }
        other => Err(BuildRequestError::UnsupportedMedia(format!(
            "unsupported document format for Bedrock: {other}"
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
                .map_err(|e| BuildRequestError::InvalidOption {
                    key: "image".into(),
                    reason: e.to_string(),
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
                .map_err(|e| BuildRequestError::InvalidOption {
                    key: "video".into(),
                    reason: e.to_string(),
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
                .map_err(|e| BuildRequestError::InvalidOption {
                    key: "document".into(),
                    reason: e.to_string(),
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
                .map_err(|e| BuildRequestError::InvalidOption {
                    key: "audio".into(),
                    reason: e.to_string(),
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

// ---------------------------------------------------------------------------
// Credential resolution
// ---------------------------------------------------------------------------

/// Try to extract explicit credentials from client options.
fn credentials_from_options(client: &LlmPrimitiveClient) -> Option<Credentials> {
    let access_key_id = get_string_option(client, "access_key_id")?;
    let secret_access_key = get_string_option(client, "secret_access_key")?;
    let session_token = get_string_option(client, "session_token");
    Some(Credentials::new(
        access_key_id,
        secret_access_key,
        session_token,
        None,
        "baml-bedrock",
    ))
}

/// Resolve AWS credentials and region.
///
/// 1. If explicit `access_key_id`/`secret_access_key` are in client options, use those.
/// 2. Otherwise, load from the AWS default provider chain (`aws_config`).
///
/// Region follows the same pattern: explicit option first, then default chain.
async fn resolve_aws_credentials_and_region(
    client: &LlmPrimitiveClient,
    http_send: &crate::HttpSendFn,
    env_read: &crate::EnvReadFn,
    #[cfg_attr(target_arch = "wasm32", allow(unused))] fs_read: &crate::FsReadFn,
) -> Result<(Credentials, String), BuildRequestError> {
    // Try explicit credentials first.
    if let Some(creds) = credentials_from_options(client) {
        let region = get_string_option(client, "region")
            .ok_or_else(|| BuildRequestError::MissingOption("region".into()))?;
        return Ok((creds, region));
    }

    // Fall back to the default provider chain with platform-specific config.
    #[cfg(not(target_arch = "wasm32"))]
    let sdk_config = {
        use aws_types::os_shim_internal::{Env, Fs};
        aws_config::defaults(aws_config::BehaviorVersion::latest())
            .http_client(baml_http_client(http_send.clone()))
            .env(Env::from_custom(native_providers::BexEnvProvider {
                env_read_fn: env_read.clone(),
            }))
            .fs(Fs::from_custom(native_providers::BexFsProvider {
                fs_read_fn: fs_read.clone(),
            }))
            .load()
            .await
    };

    #[cfg(target_arch = "wasm32")]
    let sdk_config = {
        aws_config::defaults(aws_config::BehaviorVersion::latest())
            .sleep_impl(wasm_providers::BrowserSleep)
            .time_source(wasm_providers::BrowserTime)
            .http_client(baml_http_client(http_send.clone()))
            .credentials_provider(wasm_providers::EnvCredentialProvider {
                env_read: env_read.clone(),
            })
            .load()
            .await
    };

    let region = get_string_option(client, "region")
        .or_else(|| sdk_config.region().map(std::string::ToString::to_string))
        .ok_or_else(|| {
            BuildRequestError::MissingOption(
                "region (not found in client options or AWS default provider chain)".into(),
            )
        })?;

    let credentials_provider = sdk_config.credentials_provider().ok_or_else(|| {
        BuildRequestError::MissingOption(
            "AWS credentials provider not found in default provider chain".into(),
        )
    })?;

    let creds = credentials_provider
        .provide_credentials()
        .await
        .map_err(|e| BuildRequestError::InvalidOption {
            key: "aws_credentials".into(),
            reason: format!("failed to load credentials from default provider chain: {e}"),
        })?;

    Ok((creds, region))
}

// ---------------------------------------------------------------------------
// Custom HTTP connector bridging to HttpSendFn
// ---------------------------------------------------------------------------

/// An [`aws_smithy_runtime_api::client::http::HttpConnector`] that delegates
/// all HTTP traffic to a BAML [`HttpSendFn`](crate::HttpSendFn) closure.
#[derive(Clone)]
struct BamlHttpConnector {
    send_fn: crate::HttpSendFn,
}

impl std::fmt::Debug for BamlHttpConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BamlHttpConnector").finish()
    }
}

impl aws_smithy_runtime_api::client::http::HttpConnector for BamlHttpConnector {
    fn call(&self, request: smithy_http::Request) -> HttpConnectorFuture {
        let send_fn = self.send_fn.clone();
        HttpConnectorFuture::new(async move {
            // Convert AWS SDK Request<SdkBody> to a BAML HttpRequest.
            let method = request.method().to_string();
            let url = request.uri().to_string();
            let mut headers = IndexMap::new();
            for (name, value) in request.headers() {
                headers.insert(name.to_string(), value.to_string());
            }
            let body = request
                .body()
                .bytes()
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_default();

            let baml_req = bex_heap::builtin_types::owned::HttpRequest {
                method,
                url,
                headers,
                body,
            };

            // Call the BAML HTTP send closure.
            let resp = send_fn(baml_req)
                .await
                .map_err(|e| ConnectorError::other(e.into(), None))?;

            // Convert BAML HttpSendResponse to a AWS SDK Response<SdkBody>.
            let status = smithy_http::StatusCode::try_from(resp.status_code)
                .map_err(|e| ConnectorError::other(Box::new(e), None))?;
            let sdk_body = SdkBody::from(resp.body);
            let mut aws_resp = smithy_http::Response::new(status, sdk_body);
            for (name, value) in resp.headers {
                aws_resp
                    .headers_mut()
                    .try_insert(name, value)
                    .map_err(|e| ConnectorError::other(e.into(), None))?;
            }

            Ok(aws_resp)
        })
    }
}

/// Build a [`SharedHttpClient`](aws_smithy_runtime_api::client::http::SharedHttpClient)
/// that delegates to the given [`HttpSendFn`](crate::HttpSendFn).
fn baml_http_client(
    send_fn: crate::HttpSendFn,
) -> aws_smithy_runtime_api::client::http::SharedHttpClient {
    use aws_smithy_runtime_api::client::http::http_client_fn;
    let connector = SharedHttpConnector::new(BamlHttpConnector { send_fn });
    http_client_fn(move |_settings, _components| connector.clone())
}

// ---------------------------------------------------------------------------
// SigV4 signing
// ---------------------------------------------------------------------------

/// Sign the request with `SigV4` given resolved credentials and region.
fn sign_with_credentials(
    credentials: &Credentials,
    region: &str,
    method: &str,
    url: &str,
    existing_headers: &IndexMap<String, String>,
    body: &[u8],
) -> Result<IndexMap<String, String>, BuildRequestError> {
    let identity = credentials.clone().into();

    let signing_settings = SigningSettings::default();
    let signing_params = v4::SigningParams::builder()
        .identity(&identity)
        .region(region)
        .name("bedrock")
        .time(now())
        .settings(signing_settings)
        .build()
        .map_err(|e| BuildRequestError::InvalidOption {
            key: "signing".into(),
            reason: e.to_string(),
        })?
        .into();

    let header_pairs: Vec<(&str, &str)> = existing_headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let signable = SignableRequest::new(
        method,
        url,
        header_pairs.into_iter(),
        SignableBody::Bytes(body),
    )
    .map_err(|e| BuildRequestError::InvalidOption {
        key: "signable_request".into(),
        reason: e.to_string(),
    })?;

    let (instructions, _signature) = sign(signable, &signing_params)
        .map_err(|e| BuildRequestError::InvalidOption {
            key: "signing".into(),
            reason: e.to_string(),
        })?
        .into_parts();

    let mut signed_headers = IndexMap::new();
    for (name, value) in instructions.headers() {
        signed_headers.insert(name.to_string(), value.to_string());
    }

    Ok(signed_headers)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_builtins::{MediaContent, MediaValue, PromptAst, PromptAstSimple};
    use bex_external_types::BexExternalValue;
    use indexmap::IndexMap;

    use super::*;
    use crate::build_request::{LlmRequestBuilder, build_request};

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
        let url = BedrockBuilder.build_url(&client).unwrap();
        assert_eq!(
            url,
            "https://bedrock-runtime.us-east-1.amazonaws.com/model/anthropic.claude-3-haiku-20240307-v1:0/converse"
        );
    }

    #[test]
    fn bedrock_missing_region_errors() {
        let client = make_client(vec![("model", BexExternalValue::String("m".into()))]);
        assert!(BedrockBuilder.build_url(&client).is_err());
    }

    #[test]
    fn bedrock_missing_model_errors() {
        let client = make_client(vec![(
            "region",
            BexExternalValue::String("us-east-1".into()),
        )]);
        assert!(BedrockBuilder.build_url(&client).is_err());
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

    #[tokio::test]
    async fn baml_http_connector_converts_request_and_response() {
        use aws_smithy_runtime_api::client::http::HttpConnector;

        let call_count = Arc::new(AtomicUsize::new(0));
        let captured_url = Arc::new(std::sync::Mutex::new(String::new()));
        let captured_body = Arc::new(std::sync::Mutex::new(String::new()));
        let cu = captured_url.clone();
        let cb = captured_body.clone();
        let cc = call_count.clone();

        let send_fn: crate::HttpSendFn = Arc::new(move |req| {
            cc.fetch_add(1, Ordering::SeqCst);
            *cu.lock().unwrap() = req.url.clone();
            *cb.lock().unwrap() = req.body;
            Box::pin(async {
                let mut headers = IndexMap::new();
                headers.insert("x-test".to_string(), "hello".to_string());
                Ok(crate::HttpSendResponse {
                    status_code: 200,
                    headers,
                    body: r#"{"ok": true}"#.to_string(),
                })
            })
        });

        let connector = BamlHttpConnector { send_fn };
        let mut aws_req = smithy_http::Request::new(SdkBody::from(r#"{"test": 1}"#));
        aws_req.set_uri("https://example.com/test").unwrap();
        aws_req
            .headers_mut()
            .insert("content-type", "application/json");

        let aws_resp = connector.call(aws_req).await.unwrap();

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
        assert_eq!(*captured_url.lock().unwrap(), "https://example.com/test");
        assert_eq!(*captured_body.lock().unwrap(), r#"{"test": 1}"#);
        assert_eq!(aws_resp.status().as_u16(), 200);
        assert_eq!(aws_resp.headers().get("x-test"), Some("hello"));
        let body_bytes = aws_resp.body().bytes().unwrap();
        assert_eq!(std::str::from_utf8(body_bytes).unwrap(), r#"{"ok": true}"#);
    }
}
