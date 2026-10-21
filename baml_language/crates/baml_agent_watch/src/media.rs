//! Images the page can render: format detection, content addressing, and the
//! in-memory store behind `GET /api/agent-activity/media/{id}`.
//!
//! A change to an image carries a [`MediaRef`] instead of content. The bytes
//! stay on the server, keyed by their SHA-256, so the same image written twice
//! is stored once and every version the session saw can still be shown.

use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    sync::Arc,
};

use sha2::{Digest, Sha256};

use crate::protocol::MediaRef;

/// Images larger than this are not read, stored, or shown.
pub(crate) const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// The store drops its oldest images past this total; their URLs then 404.
const MAX_STORE_BYTES: usize = 256 * 1024 * 1024;

/// An image read from disk or git.
#[derive(Clone)]
pub(crate) struct Image {
    pub(crate) mime: &'static str,
    pub(crate) bytes: Arc<[u8]>,
}

/// The MIME type of an image format browsers render, from its magic bytes.
/// SVG is text and never reaches here; the page renders it from its content.
pub(crate) fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    if starts(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if starts(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Some("image/gif")
    } else if starts(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else if bytes.get(4..8) == Some(b"ftyp")
        && matches!(bytes.get(8..12), Some(b"avif" | b"avis"))
    {
        Some("image/avif")
    } else if starts(b"BM") && bytes.len() > 26 {
        Some("image/bmp")
    } else if starts(b"\x00\x00\x01\x00") {
        Some("image/x-icon")
    } else {
        None
    }
}

/// Whether a file too large to read as text may still be an image worth reading.
pub(crate) fn has_image_extension(path: &Path) -> bool {
    const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "avif", "bmp", "ico"];
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Content address for bytes: the first 128 bits of their SHA-256, as hex.
pub(crate) fn digest(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut hex = String::with_capacity(32);
    for b in &Sha256::digest(bytes)[..16] {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

pub(crate) fn url_for(id: &str) -> String {
    format!("/api/agent-activity/media/{id}")
}

pub(crate) struct Stored {
    pub(crate) mime: &'static str,
    pub(crate) bytes: Arc<[u8]>,
}

/// Images by content address, oldest dropped first past [`MAX_STORE_BYTES`].
#[derive(Default)]
pub(crate) struct MediaStore {
    images: HashMap<String, Stored>,
    order: VecDeque<String>,
    total: usize,
}

impl MediaStore {
    /// Keep `image` (under the content address `id`) and return how the page loads it.
    pub(crate) fn insert(&mut self, id: &str, image: &Image) -> MediaRef {
        if !self.images.contains_key(id) {
            self.total += image.bytes.len();
            self.images.insert(
                id.to_string(),
                Stored {
                    mime: image.mime,
                    bytes: Arc::clone(&image.bytes),
                },
            );
            self.order.push_back(id.to_string());
            // Never evict the image just added, even if it alone is over the cap.
            while self.total > MAX_STORE_BYTES && self.order.len() > 1 {
                let Some(old) = self.order.pop_front() else {
                    break;
                };
                if let Some(gone) = self.images.remove(&old) {
                    self.total -= gone.bytes.len();
                }
            }
        }
        MediaRef {
            url: url_for(id),
            mime: image.mime.to_string(),
        }
    }

    pub(crate) fn get(&self, id: &str) -> Option<&Stored> {
        self.images.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_common_formats_and_rejects_everything_else() {
        assert_eq!(sniff_image(b"\x89PNG\r\n\x1a\n...."), Some("image/png"));
        assert_eq!(sniff_image(b"\xff\xd8\xff\xe0"), Some("image/jpeg"));
        assert_eq!(sniff_image(b"GIF89a"), Some("image/gif"));
        assert_eq!(sniff_image(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(
            sniff_image(b"\0\0\0\x1cftypavif\0\0\0\0"),
            Some("image/avif")
        );
        assert_eq!(sniff_image(b"%PDF-1.7"), None);
        assert_eq!(sniff_image(b"PK\x03\x04"), None);
        assert!(has_image_extension(Path::new("a/b.PNG")));
        assert!(!has_image_extension(Path::new("a/b.svg")));
    }

    #[test]
    fn stores_each_image_once_by_content() {
        let mut store = MediaStore::default();
        let image = Image {
            mime: "image/png",
            bytes: Arc::from(&b"\x89PNG\r\n\x1a\nabc"[..]),
        };
        let id = digest(&image.bytes);
        assert_eq!(id.len(), 32);
        let first = store.insert(&id, &image);
        let second = store.insert(&id, &image);
        assert_eq!(first, second);
        assert_eq!(first.url, format!("/api/agent-activity/media/{id}"));
        assert_eq!(store.total, image.bytes.len());
        assert_eq!(store.get(&id).map(|s| s.mime), Some("image/png"));
    }
}
