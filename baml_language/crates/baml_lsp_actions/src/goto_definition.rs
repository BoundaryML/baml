//! Go to definition for BAML files.
//!
//! NOTE: Stubbed — pending compiler2 LSP action reimplementation.

use std::path::PathBuf;

use baml_db::{FileId, Span};
use baml_project::ProjectDatabase;
use text_size::{TextRange, TextSize};

/// A navigation target representing a definition location.
#[derive(Debug, Clone)]
pub struct NavigationTarget {
    /// The name of the symbol.
    pub name: String,
    /// The file containing the definition.
    pub file_path: PathBuf,
    /// The span of the definition.
    pub span: Span,
}

impl NavigationTarget {
    /// Create a new navigation target.
    pub fn new(name: impl Into<String>, file_path: PathBuf, span: Span) -> Self {
        Self {
            name: name.into(),
            file_path,
            span,
        }
    }
}

/// Find the word (identifier) at the given offset.
pub fn find_word_at_offset(text: &str, offset: TextSize) -> Option<TextRange> {
    let offset: usize = offset.into();
    if offset > text.len() {
        return None;
    }

    let bytes = text.as_bytes();

    // Find start of word
    let mut start = offset;
    while start > 0 && is_identifier_char(bytes[start - 1]) {
        start -= 1;
    }

    // Find end of word
    let mut end = offset;
    while end < bytes.len() && is_identifier_char(bytes[end]) {
        end += 1;
    }

    if start == end {
        return None;
    }

    #[allow(clippy::cast_possible_truncation)]
    Some(TextRange::new(
        TextSize::new(start as u32),
        TextSize::new(end as u32),
    ))
}

/// Check if a byte is a valid identifier character.
fn is_identifier_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Go to the definition of the symbol at the given position.
///
/// NOTE: Stubbed — returns None. Will be reimplemented with compiler2 HIR.
#[allow(unused_variables)]
pub fn goto_definition(
    db: &ProjectDatabase,
    file_id: FileId,
    position: TextSize,
) -> Option<NavigationTarget> {
    None
}

/// Look up the definition of a named symbol.
///
/// NOTE: Stubbed — returns None. Will be reimplemented with compiler2 HIR.
#[allow(unused_variables, dead_code)]
pub(crate) fn lookup_symbol_definition(
    _db: &ProjectDatabase,
    _name: &str,
) -> Option<NavigationTarget> {
    None
}
