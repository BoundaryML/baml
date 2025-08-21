//! Types and utilities for working with text, modifying source files, and `BAML <-> LSP` type conversion.
mod range;
mod text_document;

use std::path::{Path, PathBuf};

use lsp_types::{PositionEncodingKind, Url};
pub(crate) use range::RangeExt;
use serde::{Deserialize, Serialize};
pub(crate) use text_document::DocumentVersion;
pub use text_document::TextDocument;

/// A convenient enumeration for supported text encodings. Can be converted to [`lsp_types::PositionEncodingKind`].
// Please maintain the order from least to greatest priority for the derived `Ord` impl.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PositionEncoding {
    /// UTF 16 is the encoding supported by all LSP clients.
    #[default]
    UTF16,

    /// Second choice because UTF32 uses a fixed 4 byte encoding for each character (makes conversion relatively easy)
    UTF32,

    /// BAML's preferred encoding
    UTF8,
}

/// A unique document ID, derived from a URL passed as part of an LSP request.
/// This document ID can point to either be a standalone Python file, a full notebook, or a cell within a notebook.
/// It is always backed by an absolute filepath.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct DocumentKey(PathBuf);

impl DocumentKey {
    /// Returns the URL associated with the key.
    pub(crate) fn url(&self) -> Url {
        Url::from_file_path(self.0.clone())
            .expect("The DocumentKey should always be a valid filepath on the system")
    }
    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    /// A flexible constructor that can take any path delivered by the LSP
    /// client and convert it to a `DocumentKey`.
    ///
    /// We sometimes see:
    ///   - file:///Users/someone/baml_src/test.baml
    ///   - /Users/someone/baml_src/test.baml
    ///   - test.baml
    ///   - /test.baml
    ///
    /// These should all be converted to a URL with an absolute
    /// path appropriate for the user's system (e.g. Linux, MacOS, Windows)
    pub fn from_path(root_path: &Path, file_path: &Path) -> anyhow::Result<Self> {
        // Ensure that we have a relative path, by taking file_path
        // and stripping the root_path from it, if it's absolute.
        let relative_path = file_path.strip_prefix(root_path).unwrap_or(file_path);

        // Ensure our relative path doesn't begin with a path separator.
        let relative_path = relative_path
            .strip_prefix(std::path::MAIN_SEPARATOR_STR)
            .unwrap_or(relative_path);

        let absolute_path = root_path.join(relative_path);
        // let aboslute_url = Url::from_file_path(absolute_path)
        //     .map_err(|_| anyhow::anyhow!("Could not convert path to URL"))?;
        Ok(DocumentKey(absolute_path))
    }

    /// A flexible constructor that can take any URL delivered by the LSP.
    /// It uses the same logic as `DocumentKey::from_path`.
    pub fn from_url(root_path: &Path, url: &Url) -> anyhow::Result<Self> {
        Self::from_path(
            root_path,
            &PathBuf::from(
                &url.to_file_path()
                    .map_err(|e| anyhow::anyhow!("Could not convert url to path {}: {e:?}", url))?,
            ),
        )
    }

    pub fn unchecked_to_string(&self) -> String {
        self.0
            .as_os_str()
            .to_str()
            .expect("TODO: Assumed valid string")
            .to_string()
    }
}

impl std::fmt::Display for DocumentKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO: Fix.
        let str = format!("{:?}", self.0);
        str.fmt(f)
    }
}

impl From<PositionEncoding> for lsp_types::PositionEncodingKind {
    fn from(value: PositionEncoding) -> Self {
        match value {
            PositionEncoding::UTF8 => lsp_types::PositionEncodingKind::UTF8,
            PositionEncoding::UTF16 => lsp_types::PositionEncodingKind::UTF16,
            PositionEncoding::UTF32 => lsp_types::PositionEncodingKind::UTF32,
        }
    }
}

impl TryFrom<&lsp_types::PositionEncodingKind> for PositionEncoding {
    type Error = ();

    fn try_from(value: &PositionEncodingKind) -> Result<Self, Self::Error> {
        Ok(if value == &PositionEncodingKind::UTF8 {
            PositionEncoding::UTF8
        } else if value == &PositionEncodingKind::UTF16 {
            PositionEncoding::UTF16
        } else if value == &PositionEncodingKind::UTF32 {
            PositionEncoding::UTF32
        } else {
            return Err(());
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use url::Url;

    use super::*;

    #[test]
    fn parse_windows_path() {
        let root_path = PathBuf::from("c:\\Users\\ImAls\\code\\tmp2\\baml_src");
        let example_path = PathBuf::from("c:\\Users\\ImAls\\code\\tmp2\\baml_src\\test.baml");
        let document_key = DocumentKey::from_path(&root_path, &example_path).expect("Should parse");
        dbg!(document_key.path());
        // assert!(false)
    }
}
