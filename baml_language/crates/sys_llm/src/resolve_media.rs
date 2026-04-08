//! Media resolution: fetch URLs / read files and populate `MediaValue` content
//! before provider-specific request building.

use std::sync::Arc;

use baml_base::MediaKind;
use baml_builtins2::{MediaContent, MediaValue, PromptAstSimple};
use base64::Engine as _;

use crate::build_request::BuildRequestError;
use crate::BuildRequestCallbacks;
use crate::LlmProvider;

// ============================================================================
// Resolution strategy
// ============================================================================

/// How a media URL should be handled before sending to the provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResolveMediaUrls {
    /// Always download and convert to base64.
    SendBase64,
    /// Pass the URL through unchanged.
    SendUrl,
    /// Keep the URL but ensure a MIME type is present (may fetch to infer).
    SendUrlAddMimeType,
    /// Convert to base64 unless the URL is a Google Cloud Storage (`gs://`) URI.
    SendBase64UnlessGoogleUrl,
}

/// Per-media-type resolution strategy for a provider.
pub(crate) struct MediaUrlHandler {
    pub image: ResolveMediaUrls,
    pub audio: ResolveMediaUrls,
    pub video: ResolveMediaUrls,
    pub pdf: ResolveMediaUrls,
}

impl MediaUrlHandler {
    /// Build handler from client options, falling back to provider defaults.
    pub fn from_client(client: &crate::baml_std::PrimitiveClient, provider: LlmProvider) -> Self {
        let defaults = Self::defaults_for_provider(provider);
        let Some(ref user_handler) = client.options.media_url_handler else {
            return defaults;
        };
        Self {
            image: parse_strategy(&user_handler.image).unwrap_or(defaults.image),
            audio: parse_strategy(&user_handler.audio).unwrap_or(defaults.audio),
            video: parse_strategy(&user_handler.video).unwrap_or(defaults.video),
            pdf: parse_strategy(&user_handler.pdf).unwrap_or(defaults.pdf),
        }
    }

    /// Provider defaults, matching the old engine's per-provider configuration.
    fn defaults_for_provider(provider: LlmProvider) -> Self {
        use LlmProvider::*;
        use ResolveMediaUrls::*;

        match provider {
            OpenAi | OpenAiGeneric | AzureOpenAi | Ollama | OpenRouter | OpenAiResponses => Self {
                image: SendUrl,
                audio: SendBase64,
                video: SendUrl,
                pdf: SendUrl,
            },
            Anthropic => Self {
                image: SendUrl,
                audio: SendUrl,
                video: SendUrl,
                pdf: SendUrl,
            },
            GoogleAi => Self {
                image: SendBase64UnlessGoogleUrl,
                audio: SendBase64,
                video: SendBase64,
                pdf: SendBase64,
            },
            VertexAi => Self {
                image: SendUrlAddMimeType,
                audio: SendUrlAddMimeType,
                video: SendUrl,
                pdf: SendUrl,
            },
            AwsBedrock => Self {
                image: SendBase64,
                audio: SendBase64,
                video: SendUrl,
                pdf: SendBase64,
            },
            BamlFallback | BamlRoundRobin => Self {
                image: SendBase64,
                audio: SendBase64,
                video: SendBase64,
                pdf: SendBase64,
            },
        }
    }

    fn strategy_for(&self, kind: MediaKind) -> ResolveMediaUrls {
        match kind {
            MediaKind::Image => self.image,
            MediaKind::Audio => self.audio,
            MediaKind::Video => self.video,
            MediaKind::Pdf => self.pdf,
            MediaKind::Generic => ResolveMediaUrls::SendBase64,
        }
    }
}

// ============================================================================
// Resolution pass
// ============================================================================

/// Walk the prompt tree and resolve all media according to the handler strategy.
pub(crate) async fn resolve_media(
    prompt: &bex_vm_types::PromptAst,
    handler: &MediaUrlHandler,
    callbacks: &BuildRequestCallbacks,
) -> Result<(), BuildRequestError> {
    resolve_prompt_node(&**prompt, handler, callbacks).await
}

fn resolve_prompt_node<'a>(
    prompt: &'a baml_builtins2::PromptAst,
    handler: &'a MediaUrlHandler,
    callbacks: &'a BuildRequestCallbacks,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), BuildRequestError>> + Send + 'a>>
{
    Box::pin(async move {
        match prompt {
            baml_builtins2::PromptAst::Simple(content) => {
                resolve_content_node(content, handler, callbacks).await
            }
            baml_builtins2::PromptAst::Message { content, .. } => {
                resolve_content_node(content, handler, callbacks).await
            }
            baml_builtins2::PromptAst::Vec(items) => {
                for item in items {
                    resolve_prompt_node(item, handler, callbacks).await?;
                }
                Ok(())
            }
        }
    })
}

fn resolve_content_node<'a>(
    content: &'a PromptAstSimple,
    handler: &'a MediaUrlHandler,
    callbacks: &'a BuildRequestCallbacks,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), BuildRequestError>> + Send + 'a>>
{
    Box::pin(async move {
        match content {
            PromptAstSimple::String(_) => Ok(()),
            PromptAstSimple::Media(media) => {
                resolve_single_media(media, handler, callbacks).await
            }
            PromptAstSimple::Multiple(items) => {
                for item in items {
                    resolve_content_node(item, handler, callbacks).await?;
                }
                Ok(())
            }
        }
    })
}

async fn resolve_single_media(
    media: &Arc<MediaValue>,
    handler: &MediaUrlHandler,
    callbacks: &BuildRequestCallbacks,
) -> Result<(), BuildRequestError> {
    let strategy = handler.strategy_for(media.kind);

    // Check if already resolved (base64 data present).
    let already_resolved = media.read_content(|c| c.base64_data().is_some());
    if already_resolved && strategy != ResolveMediaUrls::SendUrlAddMimeType {
        return Ok(());
    }

    // Read the current content state to decide what to do.
    let content_info = media.read_content(|c| match c {
        MediaContent::Url { url, .. } => ContentInfo::Url(url.clone()),
        MediaContent::File { file, .. } => ContentInfo::File(file.clone()),
        MediaContent::Base64 { .. } => ContentInfo::Base64,
    });

    match content_info {
        ContentInfo::Base64 => Ok(()),
        ContentInfo::Url(url) => resolve_url(media, &url, strategy, callbacks).await,
        ContentInfo::File(path) => resolve_file(media, &path, callbacks).await,
    }
}

enum ContentInfo {
    Url(String),
    File(String),
    Base64,
}

async fn resolve_url(
    media: &Arc<MediaValue>,
    url: &str,
    strategy: ResolveMediaUrls,
    callbacks: &BuildRequestCallbacks,
) -> Result<(), BuildRequestError> {
    match strategy {
        ResolveMediaUrls::SendUrl => Ok(()),
        ResolveMediaUrls::SendBase64UnlessGoogleUrl if url.starts_with("gs://") => Ok(()),
        ResolveMediaUrls::SendBase64 | ResolveMediaUrls::SendBase64UnlessGoogleUrl => {
            let response = (callbacks.fetch_bytes)(url.to_string())
                .await
                .map_err(|e| {
                    BuildRequestError::Other(format!("failed to fetch media URL {url}: {e}"))
                })?;

            if response.status_code < 200 || response.status_code >= 300 {
                return Err(BuildRequestError::Other(format!(
                    "media URL {url} returned status {}",
                    response.status_code
                )));
            }

            let b64 = base64::engine::general_purpose::STANDARD.encode(&response.bytes);

            // Infer MIME type from Content-Type header if not already set.
            if media.mime_type().is_none() {
                if let Some(ct) = response.headers.get("content-type") {
                    // Strip parameters (e.g., "image/png; charset=utf-8" -> "image/png")
                    let mime = ct.split(';').next().unwrap_or(ct).trim();
                    media.set_mime_type(mime.to_string());
                }
            }

            media.write_content(|c| {
                if let MediaContent::Url { base64_data, .. } = c {
                    *base64_data = Some(b64);
                }
            });
            Ok(())
        }
        ResolveMediaUrls::SendUrlAddMimeType => {
            if media.mime_type().is_some() {
                return Ok(());
            }
            // Need to fetch just to infer MIME type.
            let response = (callbacks.fetch_bytes)(url.to_string())
                .await
                .map_err(|e| {
                    BuildRequestError::Other(format!("failed to fetch media URL {url}: {e}"))
                })?;

            if let Some(ct) = response.headers.get("content-type") {
                let mime = ct.split(';').next().unwrap_or(ct).trim();
                media.set_mime_type(mime.to_string());
            }
            Ok(())
        }
    }
}

async fn resolve_file(
    media: &Arc<MediaValue>,
    path: &str,
    callbacks: &BuildRequestCallbacks,
) -> Result<(), BuildRequestError> {
    let bytes = (callbacks.fs_read)(path.to_string()).await.map_err(|e| {
        BuildRequestError::FileNotResolved(format!("failed to read file {path}: {e}"))
    })?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    // Infer MIME from file extension if not set.
    if media.mime_type().is_none() {
        if let Some(mime) = mime_from_extension(path) {
            media.set_mime_type(mime.to_string());
        }
    }

    media.write_content(|c| {
        if let MediaContent::File { base64_data, .. } = c {
            *base64_data = Some(b64);
        }
    });
    Ok(())
}

/// Infer MIME type from a file extension.
fn mime_from_extension(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        // Images
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        // Audio
        "mp3" => Some("audio/mpeg"),
        "wav" => Some("audio/wav"),
        "ogg" => Some("audio/ogg"),
        "flac" => Some("audio/flac"),
        "webm" => Some("audio/webm"),
        // Video
        "mp4" => Some("video/mp4"),
        "avi" => Some("video/x-msvideo"),
        "mov" => Some("video/quicktime"),
        "mkv" => Some("video/x-matroska"),
        // Documents
        "pdf" => Some("application/pdf"),
        _ => None,
    }
}

/// Parse a strategy string (from user config) into the enum.
fn parse_strategy(value: &Option<String>) -> Option<ResolveMediaUrls> {
    value.as_deref().and_then(|s| match s {
        "send_base64" => Some(ResolveMediaUrls::SendBase64),
        "send_url" => Some(ResolveMediaUrls::SendUrl),
        "send_url_add_mime_type" => Some(ResolveMediaUrls::SendUrlAddMimeType),
        "send_base64_unless_google_url" => Some(ResolveMediaUrls::SendBase64UnlessGoogleUrl),
        _ => None,
    })
}
