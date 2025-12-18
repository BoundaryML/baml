//! Position and span conversion utilities for LSP integration.
//!
//! This module provides conversions between `baml_base::Span` (byte offsets)
//! and LSP positions/ranges (line/column based).

use baml_db::{FileId, SourceFile, Span};
use lsp_types::{Position, Range};
use text_size::TextSize;

/// A line index for efficient offset-to-position conversion.
///
/// This is a simple implementation that stores line start offsets.
/// For a file with N lines, we store N+1 offsets (including the end).
pub struct LineIndex {
    /// Byte offsets of line starts. `line_starts[0]` is always 0.
    /// `line_starts[i]` is the byte offset of the start of line `i`.
    line_starts: Vec<u32>,
    /// Total length of the text in bytes.
    len: u32,
}

impl LineIndex {
    /// Create a new line index from source text.
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0];

        for (offset, c) in text.char_indices() {
            if c == '\n' {
                // The next line starts after the newline character
                line_starts.push((offset + 1) as u32);
            }
        }

        Self {
            line_starts,
            len: text.len() as u32,
        }
    }

    /// Convert a byte offset to an LSP position (0-indexed line and column).
    ///
    /// Returns `None` if the offset is out of bounds.
    pub fn offset_to_position(&self, offset: u32) -> Option<Position> {
        if offset > self.len {
            return None;
        }

        // Binary search for the line containing this offset
        let line = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,          // Exact match - start of a line
            Err(line) => line.saturating_sub(1), // Between lines - use previous line
        };

        let line_start = self.line_starts[line];
        let column = offset - line_start;

        Some(Position {
            line: line as u32,
            character: column,
        })
    }

    /// Convert an LSP position to a byte offset.
    ///
    /// Returns `None` if the position is out of bounds.
    pub fn position_to_offset(&self, pos: &Position) -> Option<u32> {
        let line = pos.line as usize;

        if line >= self.line_starts.len() {
            return None;
        }

        let line_start = self.line_starts[line];
        let offset = line_start + pos.character;

        if offset > self.len {
            // Clamp to end of file
            Some(self.len)
        } else {
            Some(offset)
        }
    }

    /// Get the number of lines in the file.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }
}

/// Convert a `baml_base::Span` to an LSP `Range`.
///
/// This requires access to the source text to build a line index.
pub fn span_to_lsp_range(text: &str, span: &Span) -> Range {
    let line_index = LineIndex::new(text);

    let start_offset: u32 = span.range.start().into();
    let end_offset: u32 = span.range.end().into();

    let start = line_index
        .offset_to_position(start_offset)
        .unwrap_or(Position {
            line: 0,
            character: 0,
        });
    let end = line_index
        .offset_to_position(end_offset)
        .unwrap_or(start);

    Range { start, end }
}

/// Convert a `baml_base::Span` to an LSP `Range` using a pre-built line index.
///
/// This is more efficient when converting multiple spans from the same file.
pub fn span_to_lsp_range_with_index(line_index: &LineIndex, span: &Span) -> Range {
    let start_offset: u32 = span.range.start().into();
    let end_offset: u32 = span.range.end().into();

    let start = line_index
        .offset_to_position(start_offset)
        .unwrap_or(Position {
            line: 0,
            character: 0,
        });
    let end = line_index
        .offset_to_position(end_offset)
        .unwrap_or(start);

    Range { start, end }
}

/// Convert an LSP `Position` to a byte offset.
pub fn lsp_position_to_offset(text: &str, pos: &Position) -> usize {
    let line_index = LineIndex::new(text);
    line_index
        .position_to_offset(pos)
        .map(|o| o as usize)
        .unwrap_or(text.len())
}

/// Convert a byte offset to an LSP `Position`.
pub fn offset_to_lsp_position(text: &str, offset: usize) -> Position {
    let line_index = LineIndex::new(text);
    line_index
        .offset_to_position(offset as u32)
        .unwrap_or(Position {
            line: 0,
            character: 0,
        })
}

/// Convert a `text_size::TextRange` to an LSP `Range`.
pub fn text_range_to_lsp_range(text: &str, range: text_size::TextRange) -> Range {
    let line_index = LineIndex::new(text);

    let start_offset: u32 = range.start().into();
    let end_offset: u32 = range.end().into();

    let start = line_index
        .offset_to_position(start_offset)
        .unwrap_or(Position {
            line: 0,
            character: 0,
        });
    let end = line_index
        .offset_to_position(end_offset)
        .unwrap_or(start);

    Range { start, end }
}

/// Get the word at a given position in the text.
///
/// Returns the word and its byte range in the text.
pub fn get_word_at_position(text: &str, pos: &Position) -> Option<(String, std::ops::Range<usize>)> {
    let offset = lsp_position_to_offset(text, pos);

    if offset > text.len() {
        return None;
    }

    let bytes = text.as_bytes();

    // Find word start (scan backwards)
    let mut start = offset;
    while start > 0 {
        let c = bytes[start - 1] as char;
        if !is_identifier_char(c) {
            break;
        }
        start -= 1;
    }

    // Find word end (scan forwards)
    let mut end = offset;
    while end < bytes.len() {
        let c = bytes[end] as char;
        if !is_identifier_char(c) {
            break;
        }
        end += 1;
    }

    if start == end {
        return None;
    }

    let word = &text[start..end];
    Some((word.to_string(), start..end))
}

/// Check if a character is valid in an identifier.
fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use text_size::TextRange;

    #[test]
    fn test_line_index_simple() {
        let text = "hello\nworld\n";
        let index = LineIndex::new(text);

        assert_eq!(index.line_count(), 3); // "hello", "world", ""

        // Line 0: "hello\n"
        assert_eq!(
            index.offset_to_position(0),
            Some(Position {
                line: 0,
                character: 0
            })
        );
        assert_eq!(
            index.offset_to_position(5),
            Some(Position {
                line: 0,
                character: 5
            })
        ); // newline

        // Line 1: "world\n"
        assert_eq!(
            index.offset_to_position(6),
            Some(Position {
                line: 1,
                character: 0
            })
        );
        assert_eq!(
            index.offset_to_position(11),
            Some(Position {
                line: 1,
                character: 5
            })
        ); // newline

        // Line 2: ""
        assert_eq!(
            index.offset_to_position(12),
            Some(Position {
                line: 2,
                character: 0
            })
        );
    }

    #[test]
    fn test_position_to_offset() {
        let text = "hello\nworld";
        let index = LineIndex::new(text);

        assert_eq!(
            index.position_to_offset(&Position {
                line: 0,
                character: 0
            }),
            Some(0)
        );
        assert_eq!(
            index.position_to_offset(&Position {
                line: 0,
                character: 5
            }),
            Some(5)
        );
        assert_eq!(
            index.position_to_offset(&Position {
                line: 1,
                character: 0
            }),
            Some(6)
        );
        assert_eq!(
            index.position_to_offset(&Position {
                line: 1,
                character: 5
            }),
            Some(11)
        );
    }

    #[test]
    fn test_span_to_range() {
        let text = "class Foo {\n  name string\n}";
        let span = Span::new(
            baml_db::FileId::new(0),
            TextRange::new(6.into(), 9.into()), // "Foo"
        );

        let range = span_to_lsp_range(text, &span);
        assert_eq!(
            range,
            Range {
                start: Position {
                    line: 0,
                    character: 6
                },
                end: Position {
                    line: 0,
                    character: 9
                },
            }
        );
    }

    #[test]
    fn test_get_word_at_position() {
        let text = "class Foo { name string }";

        // Position at "Foo"
        let (word, range) = get_word_at_position(
            text,
            &Position {
                line: 0,
                character: 7,
            },
        )
        .unwrap();
        assert_eq!(word, "Foo");
        assert_eq!(range, 6..9);

        // Position at "name"
        let (word, range) = get_word_at_position(
            text,
            &Position {
                line: 0,
                character: 12,
            },
        )
        .unwrap();
        assert_eq!(word, "name");
        assert_eq!(range, 12..16);
    }
}
