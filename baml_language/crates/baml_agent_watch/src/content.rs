//! What a file holds, read from disk or from git.

use std::{path::Path, sync::Arc};

use crate::{
    git::{MAX_TEXT_BYTES, text_of},
    media::{self, Image, MAX_IMAGE_BYTES},
};

#[derive(Clone)]
pub(crate) enum FileRead {
    Missing,
    Text(String),
    /// Bytes that are not text: their content address, and the image when
    /// they are one the page can render.
    Binary {
        size: u64,
        digest: String,
        image: Option<Image>,
    },
    /// Too large to read, so its content is unknown.
    TooLarge {
        size: u64,
    },
}

impl FileRead {
    pub(crate) fn from_bytes(bytes: Vec<u8>) -> Self {
        let size = bytes.len() as u64;
        if size <= MAX_TEXT_BYTES
            && let Some(text) = text_of(&bytes)
        {
            return Self::Text(text);
        }
        let mime = media::sniff_image(&bytes);
        if size > MAX_TEXT_BYTES && mime.is_none() {
            return Self::TooLarge { size };
        }
        Self::Binary {
            size,
            digest: media::digest(&bytes),
            image: mime.map(|mime| Image {
                mime,
                bytes: Arc::from(bytes),
            }),
        }
    }

    /// The largest file worth reading at `path`: images may be bigger than text.
    pub(crate) fn read_cap(path: &Path) -> u64 {
        if media::has_image_extension(path) {
            MAX_IMAGE_BYTES
        } else {
            MAX_TEXT_BYTES
        }
    }
}

/// What `path` holds now; `None` for directories and other non-files.
pub(crate) fn read_path(path: &Path) -> Option<FileRead> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Some(FileRead::Missing);
    };
    if !meta.is_file() {
        return None;
    }
    let size = meta.len();
    if size > FileRead::read_cap(path) {
        return Some(FileRead::TooLarge { size });
    }
    match std::fs::read(path) {
        Ok(bytes) => Some(FileRead::from_bytes(bytes)),
        Err(_) => Some(FileRead::Missing),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_text_images_and_other_binaries() {
        assert!(matches!(FileRead::from_bytes(b"hi\n".to_vec()), FileRead::Text(t) if t == "hi\n"));
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        assert!(matches!(
            FileRead::from_bytes(png),
            FileRead::Binary {
                image: Some(Image {
                    mime: "image/png",
                    ..
                }),
                ..
            }
        ));
        assert!(matches!(
            FileRead::from_bytes(b"\0\x01\x02".to_vec()),
            FileRead::Binary {
                image: None,
                size: 3,
                ..
            }
        ));
    }
}
