//! `MediaValue` — opaque representation of a BAML media value.
//!
//! Lives behind `Arc<MediaValue>` everywhere it crosses an API
//! boundary. Construction goes through the `from_url` /
//! `from_file_content` / `from_base64` static constructors; readers go
//! through the `url` / `base64` / `mime_type` / `name` accessors.
//!
//! A media value is a URL or base64 content. A file is read before its value
//! is built: nothing holds a path to read later, and nothing here reads one.

use std::{
    cell::UnsafeCell,
    sync::{Arc, RwLock},
};

use baml_base::ConcreteMediaKind;
use bex_str::BexStr;

// Do not clone. Only clone as `Arc<MediaValue>`.
#[derive(Debug)]
pub struct MediaValue {
    pub kind: ConcreteMediaKind,
    mime_type: RwLock<Option<String>>,
    // `UnsafeCell` because `MediaContent` is mutated through
    // `write_content`. Access is guarded by `content_rw_lock`; the
    // `unsafe Sync` impl below promises that.
    content: UnsafeCell<MediaContent>,
    content_rw_lock: RwLock<()>,
}

// `UnsafeCell` is not `Sync`; we make `MediaValue` `Sync` manually
// because every access to `content` goes through the explicit
// `read_content` / `write_content` methods, which serialize via
// `content_rw_lock`.
#[allow(unsafe_code)]
unsafe impl Sync for MediaValue {}

/// A media value is its value: its kind, its MIME type, and its content as
/// BAML reads it (a URL, or base64 data with the name it was read under). The
/// base64 a URL's content is fetched into later is a cache of the URL, not
/// part of the value, so fetching never changes what a value equals.
impl PartialEq for MediaValue {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
            || (self.kind == other.kind
                && self.mime_type() == other.mime_type()
                && self.read_content(|mine| {
                    other.read_content(|theirs| match (mine, theirs) {
                        (
                            MediaContent::Url { url: mine, .. },
                            MediaContent::Url { url: theirs, .. },
                        ) => mine == theirs,
                        (MediaContent::Url { .. }, MediaContent::Base64 { .. }) => false,
                        (
                            MediaContent::Base64 {
                                base64_data: mine,
                                name: my_name,
                            },
                            MediaContent::Base64 {
                                base64_data: theirs,
                                name: their_name,
                            },
                        ) => mine == theirs && my_name == their_name,
                        (MediaContent::Base64 { .. }, MediaContent::Url { .. }) => false,
                    })
                }))
    }
}

impl Eq for MediaValue {}

impl std::hash::Hash for MediaValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
        self.mime_type().hash(state);
        self.read_content(|content| match content {
            MediaContent::Url { url, .. } => url.hash(state),
            MediaContent::Base64 { base64_data, name } => {
                base64_data.hash(state);
                name.hash(state);
            }
        });
    }
}

impl MediaValue {
    pub fn new(kind: ConcreteMediaKind, content: MediaContent, mime_type: Option<String>) -> Self {
        Self {
            content_rw_lock: RwLock::new(()),
            kind,
            content: UnsafeCell::new(content),
            mime_type: RwLock::new(mime_type),
        }
    }

    /// Construct an `Arc<MediaValue>` from a URL. Used by the four
    /// `Baml{Image,Video,Audio,Pdf}.from_url` static constructors on
    /// the bridge side and by the corresponding `BamlClassMedia*` VM
    /// trait impls.
    pub fn from_url(kind: ConcreteMediaKind, url: &str, mime_type: Option<&str>) -> Arc<Self> {
        Arc::new(Self::new(
            kind,
            MediaContent::Url {
                url: url.to_string(),
                base64_data: None,
            },
            mime_type.map(str::to_string),
        ))
    }

    /// Construct an `Arc<MediaValue>` from base64 content that was read from
    /// `file`. The one place a path becomes a media value: its base name is
    /// the value's name, and its MIME type is `mime_type` or else what that
    /// name implies ([`mime_for`]). Nothing is read here.
    pub fn from_file_content(
        kind: ConcreteMediaKind,
        file: &str,
        base64: BexStr,
        mime_type: Option<&str>,
    ) -> Arc<Self> {
        let name = std::path::Path::new(file)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        let mime_type =
            mime_type.unwrap_or_else(|| mime_for(name.as_deref().unwrap_or_default(), kind));
        Arc::new(Self::new(
            kind,
            MediaContent::Base64 {
                base64_data: base64,
                name,
            },
            Some(mime_type.to_owned()),
        ))
    }

    /// Construct an `Arc<MediaValue>` from a base64 payload. The payload is
    /// held by handle: passing a clone of an existing `BexStr` shares its
    /// storage.
    pub fn from_base64(
        kind: ConcreteMediaKind,
        base64: BexStr,
        mime_type: Option<&str>,
    ) -> Arc<Self> {
        Arc::new(Self::new(
            kind,
            MediaContent::Base64 {
                base64_data: base64,
                name: None,
            },
            mime_type.map(str::to_string),
        ))
    }

    /// Get the MIME type, if set.
    pub fn mime_type(&self) -> Option<String> {
        self.mime_type.read().unwrap().clone()
    }

    pub fn read_mime_type<R>(&self, read: impl FnOnce(Option<&str>) -> R) -> R {
        let mime_type = self.mime_type.read().unwrap();
        read(mime_type.as_deref())
    }

    /// Set the MIME type. Used by media resolution to store inferred MIME types.
    pub fn set_mime_type(&self, mime: String) {
        *self.mime_type.write().unwrap() = Some(mime);
    }

    /// Original URL, if this media was sourced from one. `None` for
    /// base64 content.
    pub fn url(&self) -> Option<String> {
        self.read_content(|c| c.url().map(str::to_owned))
    }

    /// The name of the content, if it has one: the base name of the file it
    /// was read from.
    pub fn name(&self) -> Option<String> {
        self.read_content(|c| c.name().map(str::to_owned))
    }

    /// Base64 payload. Returns the stored base64 for `Base64` content,
    /// or pre-fetched bytes for `Url` content. Returns the
    /// empty string when no base64 data is available. The result shares
    /// the stored payload rather than copying it.
    pub fn base64(&self) -> BexStr {
        self.read_content(|c| c.base64_data().cloned().unwrap_or_else(BexStr::empty))
    }

    pub fn read_content<T>(&self, f: impl FnOnce(&MediaContent) -> T) -> T {
        let _guard = self.content_rw_lock.read().unwrap();
        #[allow(unsafe_code)]
        let content = unsafe { &*self.content.get() };
        f(content)
    }

    pub fn write_content<T>(&self, f: impl FnOnce(&mut MediaContent) -> T) -> T {
        let _guard = self.content_rw_lock.write().unwrap();
        #[allow(unsafe_code)]
        let content = unsafe { &mut *self.content.get() };
        f(content)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MediaContent {
    Url {
        url: String,
        base64_data: Option<BexStr>,
    },
    Base64 {
        base64_data: BexStr,
        /// What to call the content where a receiver wants a file name: the
        /// base name of the file it was read from.
        name: Option<String>,
    },
}

/// The MIME type that `source`, a file name or URL, implies for media of
/// `kind`: by its extension, or the kind's usual type when the extension says
/// nothing. The one table of its kind: hosts and BAML code both ask here.
pub fn mime_for(source: &str, kind: ConcreteMediaKind) -> &'static str {
    // Containers that hold audio as well as video go by the kind.
    let audio = matches!(kind, ConcreteMediaKind::Audio);
    match extension(source).to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        "webm" if audio => "audio/webm",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        // Bedrock expects audio/mp4 for m4a-style input.
        "mp4" if audio => "audio/mp4",
        "mp4" => "video/mp4",
        "avi" => "video/x-msvideo",
        "mkv" if audio => "audio/x-matroska",
        "mkv" => "video/x-matroska",
        "pdf" => "application/pdf",
        _ => match kind {
            ConcreteMediaKind::Image => "image/png",
            ConcreteMediaKind::Audio => "audio/mpeg",
            ConcreteMediaKind::Video => "video/mp4",
            ConcreteMediaKind::Pdf => "application/pdf",
        },
    }
}

/// What follows the last `.` of the last path segment of `source`, or the
/// empty string. In a URL (`scheme://…`) the query and fragment are not part
/// of the path; in a file name `?` and `#` are ordinary characters.
fn extension(source: &str) -> &str {
    let path = if source.contains("://") {
        source.find(['?', '#']).map_or(source, |end| &source[..end])
    } else {
        source
    };
    match path.rsplit_once('.') {
        Some((_, extension)) if !extension.contains(['/', '\\']) => extension,
        Some(_) | None => "",
    }
}

impl MediaContent {
    /// Get the base64 data regardless of variant.
    ///
    /// Returns `Some` for `Base64`, and for `Url` when the data has
    /// been pre-fetched. Returns `None` when no base64 data is available.
    pub fn base64_data(&self) -> Option<&BexStr> {
        match self {
            MediaContent::Base64 { base64_data, .. } => Some(base64_data),
            MediaContent::Url { base64_data, .. } => base64_data.as_ref(),
        }
    }

    /// Get the original URL, if this content was sourced from one.
    pub fn url(&self) -> Option<&str> {
        match self {
            MediaContent::Url { url, .. } => Some(url),
            MediaContent::Base64 { .. } => None,
        }
    }

    /// Get the name of the content, if it has one.
    pub fn name(&self) -> Option<&str> {
        match self {
            MediaContent::Base64 { name, .. } => name.as_deref(),
            MediaContent::Url { .. } => None,
        }
    }
}

impl std::fmt::Display for MediaValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.read_content(|content| write!(f, "{}::{}", self.kind, content))
    }
}

impl std::fmt::Display for MediaContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaContent::Url { url, base64_data } => {
                write!(f, "url({url}, loaded={})", base64_data.is_some())
            }
            MediaContent::Base64 { base64_data, .. } => {
                // Show first 5, last 5, and total length for context
                let len = base64_data.byte_len();
                if len <= 10 {
                    write!(f, "base64({base64_data}, len={len})")
                } else {
                    let start = &base64_data[..5];
                    let end = &base64_data[len.saturating_sub(5)..];
                    write!(f, "base64({start}...{end}, len={len})")
                }
            }
        }
    }
}

impl crate::BexRustData for MediaValue {
    fn measure(&self, meter: &mut crate::Meter) {
        // The content lock has no writer once the value is built, so a read
        // cannot block. The MIME type is a few bytes and is not counted.
        self.read_content(|content| match content {
            MediaContent::Url { url, base64_data } => {
                meter.bytes(url.capacity());
                if let Some(data) = base64_data {
                    meter.string(data);
                }
            }
            MediaContent::Base64 { base64_data, name } => {
                meter.string(base64_data);
                if let Some(name) = name {
                    meter.bytes(name.capacity());
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_value_mime_type_roundtrip() {
        let media = MediaValue::new(
            ConcreteMediaKind::Image,
            MediaContent::Base64 {
                base64_data: "abc".into(),
                name: None,
            },
            None,
        );
        assert_eq!(media.mime_type(), None);
        media.set_mime_type("image/png".to_string());
        assert_eq!(media.mime_type().as_deref(), Some("image/png"));
        // Overwrite
        media.set_mime_type("image/jpeg".to_string());
        assert_eq!(media.mime_type().as_deref(), Some("image/jpeg"));
    }

    #[test]
    fn test_media_content_base64_data() {
        let base64 = MediaContent::Base64 {
            base64_data: "abc123".into(),
            name: None,
        };
        assert_eq!(base64.base64_data().map(BexStr::as_str), Some("abc123"));

        let url_no_data = MediaContent::Url {
            url: "http://example.com".to_string(),
            base64_data: None,
        };
        assert_eq!(url_no_data.base64_data(), None);

        let url_with_data = MediaContent::Url {
            url: "http://example.com".to_string(),
            base64_data: Some("xyz".into()),
        };
        assert_eq!(url_with_data.base64_data().map(BexStr::as_str), Some("xyz"));
    }

    #[test]
    fn test_media_content_url() {
        let url = MediaContent::Url {
            url: "http://example.com".to_string(),
            base64_data: None,
        };
        assert_eq!(url.url(), Some("http://example.com"));

        let base64 = MediaContent::Base64 {
            base64_data: "abc".into(),
            name: None,
        };
        assert_eq!(base64.url(), None);
    }

    #[test]
    fn from_url_constructs_arc_with_correct_kind() {
        let arc = MediaValue::from_url(
            ConcreteMediaKind::Pdf,
            "https://example/x.pdf",
            Some("application/pdf"),
        );
        assert_eq!(arc.kind, ConcreteMediaKind::Pdf);
        assert_eq!(arc.url().as_deref(), Some("https://example/x.pdf"));
        assert!(arc.name().is_none());
        assert_eq!(arc.mime_type().as_deref(), Some("application/pdf"));
    }

    #[test]
    fn from_base64_constructs_arc() {
        let arc =
            MediaValue::from_base64(ConcreteMediaKind::Audio, "Zm9v".into(), Some("audio/wav"));
        assert_eq!(arc.kind, ConcreteMediaKind::Audio);
        assert_eq!(arc.base64().as_str(), "Zm9v");
        assert!(arc.url().is_none());
        assert!(arc.name().is_none());
    }

    #[test]
    fn base64_shares_the_payload_it_was_constructed_from() {
        // Longer than the inline capacity, so the payload is heap-backed.
        let payload = BexStr::from("QUJD".repeat(64));
        let BexStr::Flat(source) = &payload else {
            panic!("expected a heap-backed payload, got {payload:?}")
        };
        let plain = MediaValue::from_base64(ConcreteMediaKind::Image, payload.clone(), None);
        let named = MediaValue::from_file_content(
            ConcreteMediaKind::Image,
            "cat.png",
            payload.clone(),
            None,
        );
        for read in [plain.base64(), plain.base64(), named.base64()] {
            let BexStr::Flat(shared) = &read else {
                panic!("expected a heap-backed payload, got {read:?}")
            };
            assert!(Arc::ptr_eq(source, shared));
        }
        assert_eq!(plain.base64().content_hash(), payload.content_hash());
    }

    #[test]
    fn base64_is_empty_without_a_payload() {
        let media =
            MediaValue::from_url(ConcreteMediaKind::Image, "https://example.test/x.png", None);
        assert!(media.base64().is_empty());
        media.read_content(|content| assert_eq!(content.base64_data(), None));
    }

    #[test]
    fn content_read_from_a_file_is_named_by_its_base_name() {
        let media = MediaValue::from_file_content(
            ConcreteMediaKind::Pdf,
            "/reports/Q3.PDF",
            "JVBERi0xLjc=".into(),
            None,
        );
        assert_eq!(media.base64().as_str(), "JVBERi0xLjc=");
        assert_eq!(media.name().as_deref(), Some("Q3.PDF"));
        assert_eq!(media.url(), None);
        assert_eq!(media.mime_type().as_deref(), Some("application/pdf"));
        // A MIME type that is given is kept.
        let given = MediaValue::from_file_content(
            ConcreteMediaKind::Image,
            "/tmp/photo.jpg",
            "abc".into(),
            Some("image/x-custom"),
        );
        assert_eq!(given.mime_type().as_deref(), Some("image/x-custom"));
        assert_eq!(given.name().as_deref(), Some("photo.jpg"));
        // An inferred MIME type goes by the name, not by the directories above it.
        let nested = MediaValue::from_file_content(
            ConcreteMediaKind::Pdf,
            "/scans.png/q3.pdf",
            "abc".into(),
            None,
        );
        assert_eq!(nested.mime_type().as_deref(), Some("application/pdf"));
        assert_eq!(nested.name().as_deref(), Some("q3.pdf"));
        // Only content read from a file has a name.
        assert_eq!(
            MediaValue::from_url(ConcreteMediaKind::Image, "https://example.test/a.png", None)
                .name(),
            None
        );
        assert_eq!(
            MediaValue::from_base64(ConcreteMediaKind::Image, "abc".into(), None).name(),
            None
        );
    }

    #[test]
    fn a_mime_type_goes_by_the_extension_then_by_the_kind() {
        for (source, kind, expected) in [
            ("photo.JPG", ConcreteMediaKind::Image, "image/jpeg"),
            ("photo.jpeg", ConcreteMediaKind::Image, "image/jpeg"),
            ("a.webp", ConcreteMediaKind::Image, "image/webp"),
            ("a.gif", ConcreteMediaKind::Image, "image/gif"),
            ("a.svg", ConcreteMediaKind::Image, "image/svg+xml"),
            ("a.wav", ConcreteMediaKind::Audio, "audio/wav"),
            ("a.mp3", ConcreteMediaKind::Audio, "audio/mpeg"),
            ("a.flac", ConcreteMediaKind::Audio, "audio/flac"),
            ("a.ogg", ConcreteMediaKind::Audio, "audio/ogg"),
            ("a.webm", ConcreteMediaKind::Audio, "audio/webm"),
            ("a.webm", ConcreteMediaKind::Video, "video/webm"),
            ("a.mov", ConcreteMediaKind::Video, "video/quicktime"),
            ("a.mp4", ConcreteMediaKind::Audio, "audio/mp4"),
            ("a.mp4", ConcreteMediaKind::Video, "video/mp4"),
            ("a.avi", ConcreteMediaKind::Video, "video/x-msvideo"),
            ("a.mkv", ConcreteMediaKind::Audio, "audio/x-matroska"),
            ("a.mkv", ConcreteMediaKind::Video, "video/x-matroska"),
            ("a.pdf", ConcreteMediaKind::Pdf, "application/pdf"),
            // The extension wins over the kind.
            ("a.pdf", ConcreteMediaKind::Image, "application/pdf"),
            // Only the last extension of the last segment counts.
            ("photo.png.pdf", ConcreteMediaKind::Pdf, "application/pdf"),
            (
                "/scans.png/report.pdf",
                ConcreteMediaKind::Pdf,
                "application/pdf",
            ),
            (
                "/scans.png/report",
                ConcreteMediaKind::Pdf,
                "application/pdf",
            ),
            (
                "C:\\scans.png\\report",
                ConcreteMediaKind::Audio,
                "audio/mpeg",
            ),
            // An extension is matched whole.
            ("photo.avif", ConcreteMediaKind::Image, "image/png"),
            ("a.pngx", ConcreteMediaKind::Audio, "audio/mpeg"),
            // A URL's query and fragment are not part of its path.
            (
                "https://example.test/a.gif?as=.png#x.jpg",
                ConcreteMediaKind::Image,
                "image/gif",
            ),
            (
                "https://example.test/a?as=.png",
                ConcreteMediaKind::Video,
                "video/mp4",
            ),
            // A file name's `?` and `#` are part of it.
            ("what?.pdf", ConcreteMediaKind::Image, "application/pdf"),
            ("a#1.gif", ConcreteMediaKind::Image, "image/gif"),
            // No extension that says anything: the kind's usual type.
            ("", ConcreteMediaKind::Image, "image/png"),
            ("notes.txt", ConcreteMediaKind::Audio, "audio/mpeg"),
            ("clip", ConcreteMediaKind::Video, "video/mp4"),
            ("scan", ConcreteMediaKind::Pdf, "application/pdf"),
        ] {
            assert_eq!(mime_for(source, kind), expected, "{source} as {kind}");
        }
    }
}
