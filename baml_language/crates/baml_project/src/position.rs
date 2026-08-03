//! Position and span conversion utilities for LSP integration.
//!
//! This module provides conversions between `baml_base::Span` (byte offsets)
//! and LSP positions/ranges (line/column based).

use baml_db::Span;
use lsp_types::{Position, Range};

/// A line index for efficient offset-to-position conversion.
///
/// This stores each line's UTF-8 byte start and converts columns to and from
/// the UTF-16 code units used by VS Code and the playground protocol.
pub struct LineIndex<'a> {
    text: &'a str,
    /// Byte offsets of line starts. `line_starts[0]` is always 0.
    /// `line_starts[i]` is the byte offset of the start of line `i`.
    line_starts: Vec<u32>,
    /// Total length of the text in bytes.
    len: u32,
}

impl<'a> LineIndex<'a> {
    /// Create a new line index from source text.
    pub fn new(text: &'a str) -> Self {
        let mut line_starts = vec![0];
        let bytes = text.as_bytes();
        let mut offset = 0;
        while offset < bytes.len() {
            match bytes[offset] {
                b'\n' => {
                    offset += 1;
                    line_starts.push(to_u32_saturating(offset));
                }
                b'\r' => {
                    offset += if bytes.get(offset + 1) == Some(&b'\n') {
                        2
                    } else {
                        1
                    };
                    line_starts.push(to_u32_saturating(offset));
                }
                _ => offset += 1,
            }
        }

        Self {
            text,
            line_starts,
            len: to_u32_saturating(text.len()),
        }
    }

    /// Convert a UTF-8 byte offset to an LSP position with a UTF-16 column.
    ///
    /// VS Code and the playground wire use UTF-16 code units for columns.
    /// Returns `None` if the offset is out of bounds or inside a UTF-8 code point.
    pub fn offset_to_position(&self, offset: u32) -> Option<Position> {
        let offset_usize = offset as usize;
        if offset > self.len || !self.text.is_char_boundary(offset_usize) {
            return None;
        }

        // Binary search for the line containing this offset
        let line = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,                    // Exact match - start of a line
            Err(line) => line.saturating_sub(1), // Between lines - use previous line
        };

        let line_start = self.line_starts[line] as usize;
        let content_end = self.line_content_end(line);
        let clamped_offset = offset_usize.min(content_end);
        let column =
            to_u32_saturating(self.text[line_start..clamped_offset].encode_utf16().count());

        Some(Position {
            line: to_u32_saturating(line),
            character: column,
        })
    }

    /// Convert an LSP position with a UTF-16 column to a UTF-8 byte offset.
    ///
    /// Returns `None` if the line is out of bounds or the column falls inside
    /// a surrogate pair. Columns beyond the line clamp to its content end.
    pub fn position_to_offset(&self, pos: &Position) -> Option<u32> {
        let line = pos.line as usize;

        if line >= self.line_starts.len() {
            return None;
        }

        let line_start = self.line_starts[line] as usize;
        let content_end = self.line_content_end(line);
        let line_text = &self.text[line_start..content_end];
        let mut utf16_column = 0;
        for (byte_column, ch) in line_text.char_indices() {
            if utf16_column == pos.character {
                return Some(to_u32_saturating(line_start + byte_column));
            }
            let next_column = utf16_column
                + u32::try_from(ch.len_utf16()).expect("a char uses at most two UTF-16 code units");
            if next_column > pos.character {
                return None;
            }
            utf16_column = next_column;
        }
        Some(to_u32_saturating(content_end))
    }

    /// Get the number of lines in the file.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    fn line_content_end(&self, line: usize) -> usize {
        let start = self.line_starts[line] as usize;
        let mut end = self.line_starts.get(line + 1).copied().unwrap_or(self.len) as usize;
        if end > start && self.text.as_bytes()[end - 1] == b'\n' {
            end -= 1;
        }
        if end > start && self.text.as_bytes()[end - 1] == b'\r' {
            end -= 1;
        }
        end
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
    let end = line_index.offset_to_position(end_offset).unwrap_or(start);

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

/// Get the word at a given position in the text.
///
/// Returns the word and its byte range in the text.
pub fn get_word_at_position(
    text: &str,
    pos: &Position,
) -> Option<(String, std::ops::Range<usize>)> {
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

fn to_u32_saturating(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use text_size::TextRange;

    use super::*;

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
    fn test_line_index_uses_utf16_columns() {
        let text = "a🚀é\r\nnext";
        let index = LineIndex::new(text);

        assert_eq!(
            index.offset_to_position(5),
            Some(Position {
                line: 0,
                character: 3
            })
        );
        assert_eq!(
            index.offset_to_position(7),
            Some(Position {
                line: 0,
                character: 4
            })
        );
        assert_eq!(
            index.position_to_offset(&Position {
                line: 0,
                character: 3,
            }),
            Some(5)
        );
        assert_eq!(
            index.position_to_offset(&Position {
                line: 0,
                character: 2,
            }),
            None,
            "a UTF-16 column inside the emoji surrogate pair is invalid"
        );
        assert_eq!(
            index.offset_to_position(9),
            Some(Position {
                line: 1,
                character: 0
            })
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
