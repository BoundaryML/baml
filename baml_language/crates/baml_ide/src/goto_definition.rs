//! Go to definition for BAML files.
//!
//! This module provides LSP-agnostic goto-definition types.
//! Given a cursor position, it finds the definition of the symbol under the cursor.

use std::path::PathBuf;

use baml_db::Span;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_word_at_offset() {
        let text = "class Foo { name string }";

        // At 'F' in Foo
        let word = find_word_at_offset(text, TextSize::new(6));
        assert!(word.is_some());
        let range = word.unwrap();
        assert_eq!(&text[range.start().into()..range.end().into()], "Foo");

        // At 'n' in name
        let word = find_word_at_offset(text, TextSize::new(12));
        assert!(word.is_some());
        let range = word.unwrap();
        assert_eq!(&text[range.start().into()..range.end().into()], "name");

        // At space after "class" - finds "class" because cursor is at word boundary
        let word = find_word_at_offset(text, TextSize::new(5));
        assert!(word.is_some());
        let range = word.unwrap();
        assert_eq!(&text[range.start().into()..range.end().into()], "class");

        // At opening brace (pure punctuation with no adjacent identifier)
        // "{ " at offset 10 - byte 10 is '{', byte 9 is ' '
        // This should return None since we're not adjacent to an identifier
        let word = find_word_at_offset(text, TextSize::new(10));
        assert!(word.is_none());
    }

    #[test]
    fn test_is_identifier_char() {
        assert!(is_identifier_char(b'a'));
        assert!(is_identifier_char(b'Z'));
        assert!(is_identifier_char(b'0'));
        assert!(is_identifier_char(b'_'));
        assert!(!is_identifier_char(b' '));
        assert!(!is_identifier_char(b'{'));
    }
}
