//! `MediaValue` — opaque representation of a BAML media value.
//!
//! Lives behind `Arc<MediaValue>` everywhere it crosses an API
//! boundary. Construction goes through the `from_url` / `from_file` /
//! `from_base64` static constructors; readers go through the
//! `url` / `file` / `base64` / `mime_type` accessors.

use std::{
    cell::UnsafeCell,
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use baml_base::MediaKind;

// Do not clone. Only clone as `Arc<MediaValue>`.
#[derive(Debug)]
pub struct MediaValue {
    pub random_id: usize,
    pub kind: MediaKind,
    mime_type: RwLock<Option<String>>,
    // `UnsafeCell` because `MediaContent` is mutated through
    // `write_content`. Access is guarded by `content_rw_lock`; the
    // `unsafe Sync` impl below promises that.
    content: UnsafeCell<MediaContent>,
    content_rw_lock: RwLock<()>,
}

// `UnsafeCell` is not `Sync`; we make `MediaValue` `Sync` manually
// because every access to `content` goes through the explicit
// `read_content` / `write_content` / `read_content_unguarded` methods,
// which serialize via `content_rw_lock`.
#[allow(unsafe_code)]
unsafe impl Sync for MediaValue {}

impl PartialEq for MediaValue {
    fn eq(&self, other: &Self) -> bool {
        self.random_id == other.random_id
    }
}

impl Eq for MediaValue {}

impl std::hash::Hash for MediaValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.random_id.hash(state);
    }
}

static GLOBAL_MEDIA_VALUE_ID: AtomicUsize = AtomicUsize::new(0);

impl MediaValue {
    pub fn new(kind: MediaKind, content: MediaContent, mime_type: Option<String>) -> Self {
        Self {
            content_rw_lock: RwLock::new(()),
            random_id: GLOBAL_MEDIA_VALUE_ID.fetch_add(1, Ordering::Relaxed),
            kind,
            content: UnsafeCell::new(content),
            mime_type: RwLock::new(mime_type),
        }
    }

    /// Construct an `Arc<MediaValue>` from a URL. Used by the four
    /// `Baml{Image,Video,Audio,Pdf}.from_url` static constructors on
    /// the bridge side and by the corresponding `BamlClassMedia*` VM
    /// trait impls.
    pub fn from_url(kind: MediaKind, url: &str, mime_type: Option<&str>) -> Arc<Self> {
        Arc::new(Self::new(
            kind,
            MediaContent::Url {
                url: url.to_string(),
                base64_data: None,
            },
            mime_type.map(str::to_string),
        ))
    }

    /// Construct an `Arc<MediaValue>` from a local file path.
    pub fn from_file(kind: MediaKind, file: &str, mime_type: Option<&str>) -> Arc<Self> {
        Arc::new(Self::new(
            kind,
            MediaContent::File {
                file: file.to_string(),
                base64_data: None,
            },
            mime_type.map(str::to_string),
        ))
    }

    /// Construct an `Arc<MediaValue>` from a base64 payload.
    pub fn from_base64(kind: MediaKind, base64: &str, mime_type: Option<&str>) -> Arc<Self> {
        Arc::new(Self::new(
            kind,
            MediaContent::Base64 {
                base64_data: base64.to_string(),
            },
            mime_type.map(str::to_string),
        ))
    }

    /// Get the MIME type, if set.
    pub fn mime_type(&self) -> Option<String> {
        self.mime_type.read().unwrap().clone()
    }

    /// Set the MIME type. Used by media resolution to store inferred MIME types.
    pub fn set_mime_type(&self, mime: String) {
        *self.mime_type.write().unwrap() = Some(mime);
    }

    /// Original URL, if this media was sourced from one. `None` for
    /// base64 / file content.
    pub fn url(&self) -> Option<String> {
        self.read_content(|c| match c {
            MediaContent::Url { url, .. } => Some(url.clone()),
            _ => None,
        })
    }

    /// Local file path, if this media references a local file. `None`
    /// for url / base64 content.
    pub fn file(&self) -> Option<String> {
        self.read_content(|c| match c {
            MediaContent::File { file, .. } => Some(file.clone()),
            _ => None,
        })
    }

    /// Base64 payload. Returns the stored base64 for `Base64` content,
    /// or pre-fetched bytes for `Url` / `File` content. Returns the
    /// empty string when no base64 data is available.
    pub fn base64(&self) -> String {
        self.read_content(|c| match c {
            MediaContent::Base64 { base64_data } => base64_data.clone(),
            MediaContent::File {
                base64_data: Some(b),
                ..
            }
            | MediaContent::Url {
                base64_data: Some(b),
                ..
            } => b.clone(),
            _ => String::new(),
        })
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
        base64_data: Option<String>,
    },
    Base64 {
        base64_data: String,
    },
    File {
        file: String,
        base64_data: Option<String>,
    },
}

impl MediaContent {
    /// Get the base64 data regardless of variant.
    ///
    /// Returns `Some` for `Base64`, and for `Url`/`File` when the data has
    /// been pre-fetched. Returns `None` when no base64 data is available.
    pub fn base64_data(&self) -> Option<&str> {
        match self {
            MediaContent::Base64 { base64_data } => Some(base64_data),
            MediaContent::Url { base64_data, .. } => base64_data.as_deref(),
            MediaContent::File { base64_data, .. } => base64_data.as_deref(),
        }
    }

    /// Get the original URL, if this content was sourced from one.
    pub fn url(&self) -> Option<&str> {
        match self {
            MediaContent::Url { url, .. } => Some(url),
            _ => None,
        }
    }

    /// Get the file path, if this content references a local file.
    pub fn file_path(&self) -> Option<&str> {
        match self {
            MediaContent::File { file, .. } => Some(file),
            _ => None,
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
                let len = base64_data.len();
                if len <= 10 {
                    write!(f, "base64({base64_data}, len={len})")
                } else {
                    let start = &base64_data[..5];
                    let end = &base64_data[len.saturating_sub(5)..];
                    write!(f, "base64({start}...{end}, len={len})")
                }
            }
            MediaContent::File { file, base64_data } => {
                write!(f, "file({file}, loaded={})", base64_data.is_some())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_value_mime_type_roundtrip() {
        let media = MediaValue::new(
            MediaKind::Image,
            MediaContent::Base64 {
                base64_data: "abc".to_string(),
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
            base64_data: "abc123".to_string(),
        };
        assert_eq!(base64.base64_data(), Some("abc123"));

        let url_no_data = MediaContent::Url {
            url: "http://example.com".to_string(),
            base64_data: None,
        };
        assert_eq!(url_no_data.base64_data(), None);

        let url_with_data = MediaContent::Url {
            url: "http://example.com".to_string(),
            base64_data: Some("xyz".to_string()),
        };
        assert_eq!(url_with_data.base64_data(), Some("xyz"));

        let file_no_data = MediaContent::File {
            file: "/path/to/file".to_string(),
            base64_data: None,
        };
        assert_eq!(file_no_data.base64_data(), None);

        let file_with_data = MediaContent::File {
            file: "/path/to/file".to_string(),
            base64_data: Some("data".to_string()),
        };
        assert_eq!(file_with_data.base64_data(), Some("data"));
    }

    #[test]
    fn test_media_content_url() {
        let url = MediaContent::Url {
            url: "http://example.com".to_string(),
            base64_data: None,
        };
        assert_eq!(url.url(), Some("http://example.com"));

        let base64 = MediaContent::Base64 {
            base64_data: "abc".to_string(),
        };
        assert_eq!(base64.url(), None);

        let file = MediaContent::File {
            file: "/path".to_string(),
            base64_data: None,
        };
        assert_eq!(file.url(), None);
    }

    #[test]
    fn test_media_content_file_path() {
        let file = MediaContent::File {
            file: "/path/to/file".to_string(),
            base64_data: None,
        };
        assert_eq!(file.file_path(), Some("/path/to/file"));

        let url = MediaContent::Url {
            url: "http://example.com".to_string(),
            base64_data: None,
        };
        assert_eq!(url.file_path(), None);

        let base64 = MediaContent::Base64 {
            base64_data: "abc".to_string(),
        };
        assert_eq!(base64.file_path(), None);
    }

    #[test]
    fn from_url_constructs_arc_with_correct_kind() {
        let arc = MediaValue::from_url(
            MediaKind::Pdf,
            "https://example/x.pdf",
            Some("application/pdf"),
        );
        assert_eq!(arc.kind, MediaKind::Pdf);
        assert_eq!(arc.url().as_deref(), Some("https://example/x.pdf"));
        assert!(arc.file().is_none());
        assert_eq!(arc.mime_type().as_deref(), Some("application/pdf"));
    }

    #[test]
    fn from_file_constructs_arc() {
        let arc = MediaValue::from_file(MediaKind::Image, "/tmp/x.png", Some("image/png"));
        assert_eq!(arc.kind, MediaKind::Image);
        assert_eq!(arc.file().as_deref(), Some("/tmp/x.png"));
        assert!(arc.url().is_none());
    }

    #[test]
    fn from_base64_constructs_arc() {
        let arc = MediaValue::from_base64(MediaKind::Audio, "Zm9v", Some("audio/wav"));
        assert_eq!(arc.kind, MediaKind::Audio);
        assert_eq!(arc.base64(), "Zm9v");
        assert!(arc.url().is_none());
        assert!(arc.file().is_none());
    }
}
