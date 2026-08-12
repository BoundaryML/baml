//! Media resolution: fetch URLs / read files and populate `MediaValue` content
//! before provider-specific request building.

use std::{str::FromStr, sync::Arc};

use baml_base::MediaKind;
use baml_builtins2::{MediaContent, MediaValue, PromptAstSimple};
use base64::Engine as _;
use sys_types::{BexExternalValue, runtime_io::RuntimeIo};

use crate::build_request::BuildRequestError;

// ============================================================================
// Resolution strategy
// ============================================================================

/// How a media URL should be handled before sending to the provider.
///
/// String values match what users write in `media_url_handler` config blocks
/// and what `apply_provider_defaults()` sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum ResolveMediaUrls {
    SendBase64,
    SendUrl,
    SendUrlAddMimeType,
    SendBase64UnlessGoogleUrl,
}

pub(crate) struct MediaUrlHandler {
    pub image: ResolveMediaUrls,
    pub audio: ResolveMediaUrls,
    pub video: ResolveMediaUrls,
    pub pdf: ResolveMediaUrls,
}

impl MediaUrlHandler {
    /// Build handler from client options. Defaults are already applied by
    /// `apply_provider_defaults()` in `baml_std.rs`, so this just parses
    /// the string values into the enum.
    pub(crate) fn from_client(client: &crate::baml_std::PrimitiveClient) -> Self {
        let handler = client.options.media_url_handler.as_ref();
        Self {
            image: parse_field(handler.and_then(|h| h.image.as_deref())),
            audio: parse_field(handler.and_then(|h| h.audio.as_deref())),
            video: parse_field(handler.and_then(|h| h.video.as_deref())),
            pdf: parse_field(handler.and_then(|h| h.pdf.as_deref())),
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

/// Parse a strategy string into the enum, defaulting to `SendBase64` if missing or invalid.
fn parse_field(value: Option<&str>) -> ResolveMediaUrls {
    value
        .and_then(|s| ResolveMediaUrls::from_str(s).ok())
        .unwrap_or(ResolveMediaUrls::SendBase64)
}

// ============================================================================
// Resolution pass
// ============================================================================

pub(crate) async fn resolve_media(
    prompt: &bex_vm_types::PromptAst,
    handler: &MediaUrlHandler,
    io: &dyn RuntimeIo,
) -> Result<(), BuildRequestError> {
    resolve_prompt_node(prompt, handler, io).await
}

fn resolve_prompt_node<'a>(
    prompt: &'a baml_builtins2::PromptAst,
    handler: &'a MediaUrlHandler,
    io: &'a dyn RuntimeIo,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), BuildRequestError>> + Send + 'a>>
{
    Box::pin(async move {
        match prompt {
            baml_builtins2::PromptAst::Simple(content) => {
                resolve_content_node(content, handler, io).await
            }
            baml_builtins2::PromptAst::Message { content, .. } => {
                resolve_content_node(content, handler, io).await
            }
            baml_builtins2::PromptAst::Vec(items) => {
                for item in items {
                    resolve_prompt_node(item, handler, io).await?;
                }
                Ok(())
            }
        }
    })
}

fn resolve_content_node<'a>(
    content: &'a PromptAstSimple,
    handler: &'a MediaUrlHandler,
    io: &'a dyn RuntimeIo,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), BuildRequestError>> + Send + 'a>>
{
    Box::pin(async move {
        match content {
            PromptAstSimple::String(_) => Ok(()),
            PromptAstSimple::Media(media) => resolve_single_media(media, handler, io).await,
            PromptAstSimple::Multiple(items) => {
                for item in items {
                    resolve_content_node(item, handler, io).await?;
                }
                Ok(())
            }
        }
    })
}

async fn resolve_single_media(
    media: &Arc<MediaValue>,
    handler: &MediaUrlHandler,
    io: &dyn RuntimeIo,
) -> Result<(), BuildRequestError> {
    let strategy = handler.strategy_for(media.kind);

    let already_resolved = media.read_content(|c| c.base64_data().is_some());
    if already_resolved && strategy != ResolveMediaUrls::SendUrlAddMimeType {
        return Ok(());
    }

    let content_info = media.read_content(|c| match c {
        MediaContent::Url { url, .. } => ContentInfo::Url(url.clone()),
        MediaContent::File { file, .. } => ContentInfo::File(file.clone()),
        MediaContent::Base64 { .. } => ContentInfo::Base64,
    });

    match content_info {
        ContentInfo::Base64 => Ok(()),
        ContentInfo::Url(url) => resolve_url(media, &url, strategy, io).await,
        ContentInfo::File(path) => resolve_file(media, &path, io).await,
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
    io: &dyn RuntimeIo,
) -> Result<(), BuildRequestError> {
    // Handle data: URLs inline -- no HTTP fetch needed.
    if let Some((mime, b64)) = parse_data_url(url) {
        if media.mime_type().is_none() {
            media.set_mime_type(mime);
        }
        media.write_content(|c| {
            if let MediaContent::Url { base64_data, .. } = c {
                *base64_data = Some(b64);
            }
        });
        return Ok(());
    }

    match strategy {
        ResolveMediaUrls::SendUrl => Ok(()),
        ResolveMediaUrls::SendBase64UnlessGoogleUrl if url.starts_with("gs://") => Ok(()),
        ResolveMediaUrls::SendBase64 | ResolveMediaUrls::SendBase64UnlessGoogleUrl => {
            let (resp, bytes) = fetch_url_bytes(io, url).await?;

            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

            if media.mime_type().is_none() {
                infer_mime_from_headers_or_bytes(media, &resp.headers, &bytes);
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
            let resp = fetch_url_response(io, url).await?;

            // Try headers first to avoid downloading the full body.
            if infer_mime_from_headers(media, &resp.headers) {
                return Ok(());
            }

            let bytes = io.http_response_bytes(&resp).await.map_err(|e| {
                BuildRequestError::Other(format!("failed to read response bytes for {url}: {e}"))
            })?;
            if let Some(kind) = infer::get(&bytes) {
                media.set_mime_type(kind.mime_type().to_string());
            }
            Ok(())
        }
    }
}

/// Build a GET request, send it via `RuntimeIo`, and check the status.
async fn fetch_url_response(
    io: &dyn RuntimeIo,
    url: &str,
) -> Result<sys_types::runtime_io::HttpResponseHandle, BuildRequestError> {
    let req = sys_types::generated::owned::http::Request {
        method: "GET".into(),
        url: url.to_string(),
        headers: indexmap::IndexMap::new(),
        body: String::new(),
    };
    let resp = io
        // No client-side deadline: preserve the prior unbounded behavior (`0n`
        // maps to `None` in `timeout_from_nanos`).
        .http__send(req, std::sync::Arc::new(num_bigint::BigInt::from(0i64)))
        .await
        .map_err(|e| BuildRequestError::Other(format!("failed to fetch media URL {url}: {e}")))?;

    if resp.status_code < 200 || resp.status_code >= 300 {
        return Err(BuildRequestError::Other(format!(
            "media URL {url} returned status {}",
            resp.status_code
        )));
    }

    Ok(resp)
}

/// Build a GET request, send it via `RuntimeIo`, check the status, and return
/// the response handle together with the body bytes.
async fn fetch_url_bytes(
    io: &dyn RuntimeIo,
    url: &str,
) -> Result<(sys_types::runtime_io::HttpResponseHandle, Vec<u8>), BuildRequestError> {
    let resp = fetch_url_response(io, url).await?;

    let bytes = io.http_response_bytes(&resp).await.map_err(|e| {
        BuildRequestError::Other(format!("failed to read response bytes for {url}: {e}"))
    })?;

    Ok((resp, bytes))
}

async fn resolve_file(
    media: &Arc<MediaValue>,
    path: &str,
    io: &dyn RuntimeIo,
) -> Result<(), BuildRequestError> {
    let file = io
        .fs_open(path.to_string(), BexExternalValue::String("r".into()))
        .await
        .map_err(|e| {
            BuildRequestError::FileNotResolved(format!("failed to open file {path}: {e}"))
        })?;
    let bytes = io.fs_file_bytes(&file).await.map_err(|e| {
        BuildRequestError::FileNotResolved(format!("failed to read file {path}: {e}"))
    })?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    if media.mime_type().is_none() {
        if let Some(mime) = mime_from_extension(path) {
            media.set_mime_type(mime.to_string());
        } else if let Some(kind) = infer::get(&bytes) {
            media.set_mime_type(kind.mime_type().to_string());
        }
    }

    media.write_content(|c| {
        if let MediaContent::File { base64_data, .. } = c {
            *base64_data = Some(b64);
        }
    });
    Ok(())
}

fn mime_from_extension(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "mp3" => Some("audio/mpeg"),
        "wav" => Some("audio/wav"),
        "ogg" => Some("audio/ogg"),
        "flac" => Some("audio/flac"),
        "mp4" => Some("video/mp4"),
        "avi" => Some("video/x-msvideo"),
        "mov" => Some("video/quicktime"),
        "mkv" => Some("video/x-matroska"),
        "webm" => Some("video/webm"),
        "pdf" => Some("application/pdf"),
        _ => None,
    }
}

/// Parse a `data:` URL into (`mime_type`, `base64_data`).
/// Supports the format: `data:<mime>;base64,<data>`
fn parse_data_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    if !meta.ends_with(";base64") {
        return None;
    }
    let mime = meta.strip_suffix(";base64")?.split(';').next()?.trim();
    if mime.is_empty() {
        return None;
    }
    Some((mime.to_string(), data.to_string()))
}

/// Try to extract a MIME type from the `Content-Type` header alone.
/// Returns `true` if a MIME was found and set.
fn infer_mime_from_headers(
    media: &MediaValue,
    headers: &indexmap::IndexMap<String, String>,
) -> bool {
    if let Some(ct) = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .map(|(_, value)| value)
    {
        let mime = ct.split(';').next().unwrap_or(ct).trim();
        if !mime.is_empty() {
            media.set_mime_type(mime.to_string());
            return true;
        }
    }
    false
}

/// Infer MIME type from HTTP headers, falling back to byte-level detection.
fn infer_mime_from_headers_or_bytes(
    media: &MediaValue,
    headers: &indexmap::IndexMap<String, String>,
    bytes: &[u8],
) {
    if infer_mime_from_headers(media, headers) {
        return;
    }
    if let Some(kind) = infer::get(bytes) {
        media.set_mime_type(kind.mime_type().to_string());
    }
}

#[cfg(test)]
mod tests {
    use std::{future::Future, pin::Pin, sync::Arc};

    use sys_types::runtime_io::{RuntimeIo, RuntimeIoError};

    use super::*;

    fn make_url_media(url: &str, mime: Option<&str>) -> Arc<MediaValue> {
        Arc::new(MediaValue::new(
            MediaKind::Image,
            MediaContent::Url {
                url: url.to_string(),
                base64_data: None,
            },
            mime.map(String::from),
        ))
    }

    fn make_file_media(path: &str, mime: Option<&str>) -> Arc<MediaValue> {
        Arc::new(MediaValue::new(
            MediaKind::Image,
            MediaContent::File {
                file: path.to_string(),
                base64_data: None,
            },
            mime.map(String::from),
        ))
    }

    fn make_base64_media(data: &str, mime: Option<&str>) -> Arc<MediaValue> {
        Arc::new(MediaValue::new(
            MediaKind::Image,
            MediaContent::Base64 {
                base64_data: data.to_string(),
            },
            mime.map(String::from),
        ))
    }

    // -----------------------------------------------------------------------
    // Test mock RuntimeIo implementations
    // -----------------------------------------------------------------------

    /// Mock `RuntimeIo` for HTTP fetch tests. Returns configurable status code,
    /// headers, and body bytes from `http_send` + `http_response_bytes`.
    struct MockHttpIo {
        status_code: i64,
        headers: indexmap::IndexMap<String, String>,
        bytes: Vec<u8>,
    }

    impl RuntimeIo for MockHttpIo {
        fn http__send(
            &self,
            _request: sys_types::generated::owned::http::Request,
            _timeout_nanos: std::sync::Arc<num_bigint::BigInt>,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<sys_types::runtime_io::HttpResponseHandle, RuntimeIoError>,
                    > + Send
                    + '_,
            >,
        > {
            let status = self.status_code;
            let headers = self.headers.clone();
            Box::pin(async move {
                Ok(sys_types::runtime_io::HttpResponseHandle {
                    raw: bex_external_types::BexExternalValue::Null,
                    status_code: status,
                    headers,
                    url: String::new(),
                })
            })
        }

        fn http_response_bytes(
            &self,
            _: &sys_types::runtime_io::HttpResponseHandle,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, RuntimeIoError>> + Send + '_>> {
            let bytes = self.bytes.clone();
            Box::pin(async move { Ok(bytes) })
        }
    }

    /// Mock `RuntimeIo` for file-read tests. Returns configurable file content
    /// from `fs_open` + `fs_file_bytes`.
    struct MockFsIo {
        content: Vec<u8>,
    }

    impl RuntimeIo for MockFsIo {
        fn fs_open(
            &self,
            _path: String,
            _mode: BexExternalValue,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<sys_types::runtime_io::FsFileHandle, RuntimeIoError>>
                    + Send
                    + '_,
            >,
        > {
            Box::pin(async {
                Ok(sys_types::runtime_io::FsFileHandle {
                    raw: bex_external_types::BexExternalValue::Null,
                })
            })
        }

        fn fs_file_bytes(
            &self,
            _: &sys_types::runtime_io::FsFileHandle,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, RuntimeIoError>> + Send + '_>> {
            let bytes = self.content.clone();
            Box::pin(async move { Ok(bytes) })
        }
    }

    // -- parse_data_url -------------------------------------------------------

    #[test]
    fn data_url_valid() {
        let (mime, data) = parse_data_url("data:image/png;base64,iVBOR").unwrap();
        assert_eq!(mime, "image/png");
        assert_eq!(data, "iVBOR");
    }

    #[test]
    fn data_url_no_base64_marker() {
        assert!(parse_data_url("data:image/png,raw_data").is_none());
    }

    #[test]
    fn data_url_not_data_scheme() {
        assert!(parse_data_url("https://example.com/img.png").is_none());
    }

    #[test]
    fn data_url_empty_mime() {
        assert!(parse_data_url("data:;base64,abc").is_none());
    }

    #[test]
    fn data_url_strips_extra_params() {
        let (mime, data) =
            parse_data_url("data:image/svg+xml;charset=utf-8;base64,PHN2Zz4=").unwrap();
        assert_eq!(mime, "image/svg+xml");
        assert_eq!(data, "PHN2Zz4=");
    }

    // -- mime_from_extension --------------------------------------------------

    #[test]
    fn extension_png() {
        assert_eq!(mime_from_extension("photo.png"), Some("image/png"));
    }

    #[test]
    fn extension_jpeg() {
        assert_eq!(mime_from_extension("photo.jpg"), Some("image/jpeg"));
        assert_eq!(mime_from_extension("photo.jpeg"), Some("image/jpeg"));
    }

    #[test]
    fn extension_pdf() {
        assert_eq!(mime_from_extension("doc.pdf"), Some("application/pdf"));
    }

    #[test]
    fn extension_unknown() {
        assert_eq!(mime_from_extension("file.xyz"), None);
    }

    #[test]
    fn extension_no_ext() {
        // Single-component filename -- rsplit('.') returns the whole name, which
        // won't match any known extension.
        assert_eq!(mime_from_extension("Makefile"), None);
    }

    // -- parse_field (strum EnumString) ---------------------------------------

    #[test]
    fn parse_field_valid_strategies() {
        assert_eq!(
            parse_field(Some("send_base64")),
            ResolveMediaUrls::SendBase64
        );
        assert_eq!(parse_field(Some("send_url")), ResolveMediaUrls::SendUrl);
        assert_eq!(
            parse_field(Some("send_url_add_mime_type")),
            ResolveMediaUrls::SendUrlAddMimeType
        );
        assert_eq!(
            parse_field(Some("send_base64_unless_google_url")),
            ResolveMediaUrls::SendBase64UnlessGoogleUrl
        );
    }

    #[test]
    fn parse_field_invalid_falls_back() {
        assert_eq!(parse_field(Some("unknown")), ResolveMediaUrls::SendBase64);
        assert_eq!(parse_field(None), ResolveMediaUrls::SendBase64);
    }

    // -- resolve_single_media (data URL) --------------------------------------

    #[tokio::test]
    async fn data_url_resolved_without_fetch() {
        let media = make_url_media("data:image/png;base64,iVBORw0KGgo=", None);
        let handler = MediaUrlHandler {
            image: ResolveMediaUrls::SendBase64,
            audio: ResolveMediaUrls::SendBase64,
            video: ResolveMediaUrls::SendBase64,
            pdf: ResolveMediaUrls::SendBase64,
        };
        let io = sys_types::runtime_io::NoopRuntimeIo;
        resolve_single_media(&media, &handler, &io).await.unwrap();

        assert_eq!(media.mime_type().as_deref(), Some("image/png"));
        media.read_content(|c| {
            assert_eq!(c.base64_data(), Some("iVBORw0KGgo="));
        });
    }

    // -- resolve_single_media (base64 already set) ----------------------------

    #[tokio::test]
    async fn base64_content_is_noop() {
        let media = make_base64_media("abc123", Some("image/jpeg"));
        let handler = MediaUrlHandler {
            image: ResolveMediaUrls::SendBase64,
            audio: ResolveMediaUrls::SendBase64,
            video: ResolveMediaUrls::SendBase64,
            pdf: ResolveMediaUrls::SendBase64,
        };
        let io = sys_types::runtime_io::NoopRuntimeIo;
        resolve_single_media(&media, &handler, &io).await.unwrap();

        // Should remain unchanged.
        media.read_content(|c| {
            assert_eq!(c.base64_data(), Some("abc123"));
        });
    }

    // -- resolve_single_media (SendUrl skips fetch) ---------------------------

    #[tokio::test]
    async fn send_url_skips_fetch() {
        let media = make_url_media("https://example.com/img.png", None);
        let handler = MediaUrlHandler {
            image: ResolveMediaUrls::SendUrl,
            audio: ResolveMediaUrls::SendBase64,
            video: ResolveMediaUrls::SendBase64,
            pdf: ResolveMediaUrls::SendBase64,
        };
        // NoopRuntimeIo would error if called -- proving no fetch happens.
        let io = sys_types::runtime_io::NoopRuntimeIo;
        resolve_single_media(&media, &handler, &io).await.unwrap();

        // base64_data should still be None.
        media.read_content(|c| {
            assert!(c.base64_data().is_none());
            assert_eq!(c.url(), Some("https://example.com/img.png"));
        });
    }

    // -- resolve_single_media (gs:// skipped for SendBase64UnlessGoogleUrl) ---

    #[tokio::test]
    async fn google_url_skipped() {
        let media = make_url_media("gs://bucket/object.png", None);
        let handler = MediaUrlHandler {
            image: ResolveMediaUrls::SendBase64UnlessGoogleUrl,
            audio: ResolveMediaUrls::SendBase64,
            video: ResolveMediaUrls::SendBase64,
            pdf: ResolveMediaUrls::SendBase64,
        };
        let io = sys_types::runtime_io::NoopRuntimeIo;
        resolve_single_media(&media, &handler, &io).await.unwrap();

        media.read_content(|c| {
            assert!(c.base64_data().is_none());
        });
    }

    // -- resolve_single_media (SendBase64 fetches) ----------------------------

    #[tokio::test]
    async fn send_base64_fetches_url() {
        let png_bytes: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47]; // PNG magic bytes
        let media = make_url_media("https://example.com/img.png", None);
        let handler = MediaUrlHandler {
            image: ResolveMediaUrls::SendBase64,
            audio: ResolveMediaUrls::SendBase64,
            video: ResolveMediaUrls::SendBase64,
            pdf: ResolveMediaUrls::SendBase64,
        };
        let io = MockHttpIo {
            status_code: 200,
            headers: indexmap::indexmap! {
                "content-type".to_string() => "image/png".to_string(),
            },
            bytes: png_bytes,
        };
        resolve_single_media(&media, &handler, &io).await.unwrap();

        assert_eq!(media.mime_type().as_deref(), Some("image/png"));
        media.read_content(|c| {
            assert!(c.base64_data().is_some());
        });
    }

    // -- resolve_single_media (infer MIME from bytes) -------------------------

    #[tokio::test]
    async fn infer_mime_from_bytes_when_no_header() {
        // Real PNG header bytes so `infer` can detect it.
        let png_bytes: Vec<u8> = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, // IHDR chunk length
        ];
        let media = make_url_media("https://example.com/noext", None);
        let handler = MediaUrlHandler {
            image: ResolveMediaUrls::SendBase64,
            audio: ResolveMediaUrls::SendBase64,
            video: ResolveMediaUrls::SendBase64,
            pdf: ResolveMediaUrls::SendBase64,
        };
        let io = MockHttpIo {
            status_code: 200,
            headers: indexmap::IndexMap::new(), // no content-type
            bytes: png_bytes,
        };
        resolve_single_media(&media, &handler, &io).await.unwrap();

        assert_eq!(media.mime_type().as_deref(), Some("image/png"));
    }

    // -- resolve_file with infer fallback -------------------------------------

    #[tokio::test]
    async fn resolve_file_infers_mime_from_bytes() {
        let png_bytes: Vec<u8> = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
        ];
        let media = make_file_media("/tmp/noext", None);
        let handler = MediaUrlHandler {
            image: ResolveMediaUrls::SendBase64,
            audio: ResolveMediaUrls::SendBase64,
            video: ResolveMediaUrls::SendBase64,
            pdf: ResolveMediaUrls::SendBase64,
        };
        let io = MockFsIo { content: png_bytes };
        resolve_single_media(&media, &handler, &io).await.unwrap();

        assert_eq!(media.mime_type().as_deref(), Some("image/png"));
        media.read_content(|c| {
            assert!(c.base64_data().is_some());
        });
    }

    // -- resolve_file prefers extension over infer ----------------------------

    #[tokio::test]
    async fn resolve_file_prefers_extension() {
        let media = make_file_media("/tmp/photo.jpeg", None);
        let io = MockFsIo {
            content: vec![0x00, 0x01, 0x02], // random bytes
        };
        let handler = MediaUrlHandler {
            image: ResolveMediaUrls::SendBase64,
            audio: ResolveMediaUrls::SendBase64,
            video: ResolveMediaUrls::SendBase64,
            pdf: ResolveMediaUrls::SendBase64,
        };
        resolve_single_media(&media, &handler, &io).await.unwrap();

        // Extension wins over infer (which would return None for random bytes).
        assert_eq!(media.mime_type().as_deref(), Some("image/jpeg"));
    }

    // -- HTTP error -----------------------------------------------------------

    #[tokio::test]
    async fn fetch_returns_error_on_http_failure() {
        let media = make_url_media("https://example.com/missing.png", None);
        let handler = MediaUrlHandler {
            image: ResolveMediaUrls::SendBase64,
            audio: ResolveMediaUrls::SendBase64,
            video: ResolveMediaUrls::SendBase64,
            pdf: ResolveMediaUrls::SendBase64,
        };
        let io = MockHttpIo {
            status_code: 404,
            headers: indexmap::IndexMap::new(),
            bytes: vec![],
        };
        let err = resolve_single_media(&media, &handler, &io).await;
        assert!(err.is_err());
        assert!(format!("{}", err.unwrap_err()).contains("404"));
    }

    #[tokio::test]
    async fn send_url_add_mime_type_rejects_non_2xx() {
        let media = make_url_media("https://example.com/missing.png", None);
        let handler = MediaUrlHandler {
            image: ResolveMediaUrls::SendUrlAddMimeType,
            audio: ResolveMediaUrls::SendUrlAddMimeType,
            video: ResolveMediaUrls::SendUrlAddMimeType,
            pdf: ResolveMediaUrls::SendUrlAddMimeType,
        };
        let io = MockHttpIo {
            status_code: 404,
            headers: indexmap::IndexMap::new(),
            bytes: vec![],
        };
        let err = resolve_single_media(&media, &handler, &io).await;
        assert!(err.is_err());
        assert!(format!("{}", err.unwrap_err()).contains("404"));
    }

    // -- existing mime_type is preserved --------------------------------------

    #[tokio::test]
    async fn existing_mime_type_not_overwritten() {
        let media = make_url_media("https://example.com/img", Some("image/webp"));
        let handler = MediaUrlHandler {
            image: ResolveMediaUrls::SendBase64,
            audio: ResolveMediaUrls::SendBase64,
            video: ResolveMediaUrls::SendBase64,
            pdf: ResolveMediaUrls::SendBase64,
        };
        let io = MockHttpIo {
            status_code: 200,
            headers: indexmap::indexmap! {
                "content-type".to_string() => "image/png".to_string(),
            },
            bytes: vec![0x89, 0x50, 0x4E, 0x47],
        };
        resolve_single_media(&media, &handler, &io).await.unwrap();

        // Original mime_type should be preserved, not overwritten by Content-Type.
        assert_eq!(media.mime_type().as_deref(), Some("image/webp"));
    }

    // -- case-insensitive Content-Type header ---------------------------------

    #[tokio::test]
    async fn content_type_header_case_insensitive() {
        let media = make_url_media("https://example.com/img.svg", None);
        let handler = MediaUrlHandler {
            image: ResolveMediaUrls::SendUrlAddMimeType,
            audio: ResolveMediaUrls::SendUrlAddMimeType,
            video: ResolveMediaUrls::SendUrlAddMimeType,
            pdf: ResolveMediaUrls::SendUrlAddMimeType,
        };
        let io = MockHttpIo {
            status_code: 200,
            headers: indexmap::indexmap! {
                "Content-Type".to_string() => "image/svg+xml".to_string(),
            },
            bytes: vec![],
        };
        resolve_single_media(&media, &handler, &io).await.unwrap();

        assert_eq!(media.mime_type().as_deref(), Some("image/svg+xml"));
    }
}
