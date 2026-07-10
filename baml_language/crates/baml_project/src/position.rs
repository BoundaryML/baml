//! Position and span conversion utilities for LSP integration.
//!
//! This module provides conversions between `baml_base::Span` (byte offsets)
//! and LSP positions/ranges (line/column based).

use baml_db::Span;
use lsp_types::{Position, Range};

/// Position encodings supported by the BAML LSP boundary.
///
/// UTF-32 is intentionally excluded: UTF-16 is the protocol fallback and
/// UTF-8 is selected when the client explicitly offers it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PositionEncoding {
    #[default]
    Utf16,
    Utf8,
}

impl PositionEncoding {
    /// Select UTF-8 when offered by the client, otherwise use mandatory UTF-16.
    pub fn negotiate(offered: Option<&[lsp_types::PositionEncodingKind]>) -> Self {
        if offered.is_some_and(|encodings| {
            encodings
                .iter()
                .any(|encoding| encoding == &lsp_types::PositionEncodingKind::UTF8)
        }) {
            Self::Utf8
        } else {
            Self::Utf16
        }
    }

    pub fn as_lsp_kind(self) -> lsp_types::PositionEncodingKind {
        match self {
            Self::Utf8 => lsp_types::PositionEncodingKind::UTF8,
            Self::Utf16 => lsp_types::PositionEncodingKind::UTF16,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PositionCodecError {
    LineOutOfBounds { line: u32, line_count: usize },
    OffsetOutOfBounds { offset: u32, text_len: usize },
    InvalidEncodingBoundary { line: u32, character: u32 },
    InvalidByteBoundary { offset: u32 },
    ReversedRange { start: u32, end: u32 },
    MultilineSpan,
}

impl std::fmt::Display for PositionCodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LineOutOfBounds { line, line_count } => {
                write!(f, "line {line} is outside document with {line_count} lines")
            }
            Self::OffsetOutOfBounds { offset, text_len } => {
                write!(
                    f,
                    "byte offset {offset} is outside document of {text_len} bytes"
                )
            }
            Self::InvalidEncodingBoundary { line, character } => write!(
                f,
                "position {line}:{character} falls inside an encoded character"
            ),
            Self::InvalidByteBoundary { offset } => {
                write!(f, "byte offset {offset} is not a character boundary")
            }
            Self::ReversedRange { start, end } => {
                write!(f, "range start {start} is after end {end}")
            }
            Self::MultilineSpan => write!(f, "span crosses a line boundary"),
        }
    }
}

impl std::error::Error for PositionCodecError {}

#[derive(Debug, Clone, Copy)]
struct LineBounds {
    start: u32,
    /// Byte offset immediately before LF, CRLF, or bare CR.
    content_end: u32,
}

/// One same-line semantic-token segment in negotiated LSP units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticTokenSegment {
    pub start: Position,
    pub length: u32,
    pub start_offset: u32,
    pub end_offset: u32,
}

/// The only converter used at the LSP wire boundary.
///
/// Compiler APIs remain byte-based. This codec recognizes LF, CRLF, and bare
/// CR, validates encoding-unit boundaries, and clamps overlong characters to
/// the selected line's content end as required by LSP.
pub struct LspPositionCodec<'text> {
    text: &'text str,
    encoding: PositionEncoding,
    lines: Vec<LineBounds>,
}

impl<'text> LspPositionCodec<'text> {
    pub fn new(text: &'text str, encoding: PositionEncoding) -> Self {
        let bytes = text.as_bytes();
        let mut lines = Vec::new();
        let mut line_start = 0usize;
        let mut cursor = 0usize;

        while cursor < bytes.len() {
            let newline_len = match bytes[cursor] {
                b'\r' if bytes.get(cursor + 1) == Some(&b'\n') => 2,
                b'\r' | b'\n' => 1,
                _ => {
                    cursor += 1;
                    continue;
                }
            };

            let next_line = cursor + newline_len;
            lines.push(LineBounds {
                start: to_u32_saturating(line_start),
                content_end: to_u32_saturating(cursor),
            });
            line_start = next_line;
            cursor = next_line;
        }

        lines.push(LineBounds {
            start: to_u32_saturating(line_start),
            content_end: to_u32_saturating(text.len()),
        });

        Self {
            text,
            encoding,
            lines,
        }
    }

    pub fn encoding(&self) -> PositionEncoding {
        self.encoding
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn position_to_offset(&self, position: Position) -> Result<u32, PositionCodecError> {
        let line_index = usize::try_from(position.line).unwrap_or(usize::MAX);
        let Some(line) = self.lines.get(line_index) else {
            return Err(PositionCodecError::LineOutOfBounds {
                line: position.line,
                line_count: self.lines.len(),
            });
        };

        let line_text = &self.text[line.start as usize..line.content_end as usize];
        let mut encoded_offset = 0u32;
        for (byte_offset, ch) in line_text.char_indices() {
            if position.character == encoded_offset {
                return Ok(line.start + to_u32_saturating(byte_offset));
            }
            let width = match self.encoding {
                PositionEncoding::Utf8 => to_u32_saturating(ch.len_utf8()),
                PositionEncoding::Utf16 => to_u32_saturating(ch.len_utf16()),
            };
            let next = encoded_offset.saturating_add(width);
            if position.character < next {
                return Err(PositionCodecError::InvalidEncodingBoundary {
                    line: position.line,
                    character: position.character,
                });
            }
            encoded_offset = next;
        }

        // LSP specifies that overlong characters clamp to the end of the line.
        Ok(line.content_end)
    }

    pub fn offset_to_position(&self, offset: u32) -> Result<Position, PositionCodecError> {
        if offset as usize > self.text.len() {
            return Err(PositionCodecError::OffsetOutOfBounds {
                offset,
                text_len: self.text.len(),
            });
        }
        if !self.text.is_char_boundary(offset as usize) {
            return Err(PositionCodecError::InvalidByteBoundary { offset });
        }

        let line_index = self
            .lines
            .partition_point(|line| line.start <= offset)
            .saturating_sub(1);
        let line = self.lines[line_index];
        if offset > line.content_end {
            // The only reachable case is the middle of a CRLF pair. Newline
            // endpoints otherwise resolve to the following line's exact start.
            return Err(PositionCodecError::InvalidByteBoundary { offset });
        }

        let prefix = &self.text[line.start as usize..offset as usize];
        let character = match self.encoding {
            PositionEncoding::Utf8 => to_u32_saturating(prefix.len()),
            PositionEncoding::Utf16 => to_u32_saturating(prefix.encode_utf16().count()),
        };

        Ok(Position {
            line: to_u32_saturating(line_index),
            character,
        })
    }

    pub fn range_to_text_range(
        &self,
        range: Range,
    ) -> Result<text_size::TextRange, PositionCodecError> {
        let start = self.position_to_offset(range.start)?;
        let end = self.position_to_offset(range.end)?;
        if end < start {
            return Err(PositionCodecError::ReversedRange { start, end });
        }
        Ok(text_size::TextRange::new(start.into(), end.into()))
    }

    pub fn text_range_to_range(
        &self,
        range: text_size::TextRange,
    ) -> Result<Range, PositionCodecError> {
        let start_offset: u32 = range.start().into();
        let end_offset: u32 = range.end().into();
        if end_offset < start_offset {
            return Err(PositionCodecError::ReversedRange {
                start: start_offset,
                end: end_offset,
            });
        }
        Ok(Range {
            start: self.offset_to_position(start_offset)?,
            end: self.offset_to_position(end_offset)?,
        })
    }

    pub fn span_to_range(&self, span: &Span) -> Result<Range, PositionCodecError> {
        self.text_range_to_range(span.range)
    }

    pub fn encoded_length(&self, range: text_size::TextRange) -> Result<u32, PositionCodecError> {
        let start = self.offset_to_position(range.start().into())?;
        let end = self.offset_to_position(range.end().into())?;
        if start.line != end.line {
            return Err(PositionCodecError::MultilineSpan);
        }
        Ok(end.character.saturating_sub(start.character))
    }

    pub fn document_end(&self) -> Position {
        self.offset_to_position(to_u32_saturating(self.text.len()))
            .expect("document length is always a valid byte boundary")
    }

    pub fn document_range(&self) -> Range {
        Range {
            start: Position::new(0, 0),
            end: self.document_end(),
        }
    }

    /// Split a byte span into ordered same-line semantic-token segments.
    pub fn semantic_token_segments(
        &self,
        range: text_size::TextRange,
    ) -> Result<Vec<SemanticTokenSegment>, PositionCodecError> {
        let start: u32 = range.start().into();
        let end: u32 = range.end().into();
        if end < start {
            return Err(PositionCodecError::ReversedRange { start, end });
        }
        // Validate the original endpoints before clipping away newline bytes.
        let _ = self.offset_to_position(start)?;
        let _ = self.offset_to_position(end)?;

        let mut segments = Vec::new();
        for line in &self.lines {
            if line.start >= end {
                break;
            }
            if line.content_end <= start {
                continue;
            }
            let segment_start = start.max(line.start);
            let segment_end = end.min(line.content_end);
            if segment_start >= segment_end {
                continue;
            }
            let start_position = self.offset_to_position(segment_start)?;
            let segment_range = text_size::TextRange::new(segment_start.into(), segment_end.into());
            let length = self.encoded_length(segment_range)?;
            if length == 0 {
                continue;
            }
            segments.push(SemanticTokenSegment {
                start: start_position,
                length,
                start_offset: segment_start,
                end_offset: segment_end,
            });
        }
        Ok(segments)
    }
}

fn to_u32_saturating(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use text_size::TextRange;

    use super::*;

    #[test]
    fn negotiation_prefers_utf8_and_falls_back_to_utf16() {
        assert_eq!(PositionEncoding::negotiate(None), PositionEncoding::Utf16);
        assert_eq!(
            PositionEncoding::negotiate(Some(&[lsp_types::PositionEncodingKind::UTF32])),
            PositionEncoding::Utf16
        );
        assert_eq!(
            PositionEncoding::negotiate(Some(&[
                lsp_types::PositionEncodingKind::UTF16,
                lsp_types::PositionEncodingKind::UTF8,
            ])),
            PositionEncoding::Utf8
        );
    }

    #[test]
    fn codec_round_trips_utf8_and_utf16_across_unicode() {
        let text = "aé😀中\r\nx\rz\n";
        let utf8 = LspPositionCodec::new(text, PositionEncoding::Utf8);
        let utf16 = LspPositionCodec::new(text, PositionEncoding::Utf16);

        for (offset, utf8_character, utf16_character) in
            [(0, 0, 0), (1, 1, 1), (3, 3, 2), (7, 7, 4), (10, 10, 5)]
        {
            assert_eq!(
                utf8.offset_to_position(offset).unwrap(),
                Position::new(0, utf8_character)
            );
            assert_eq!(
                utf16.offset_to_position(offset).unwrap(),
                Position::new(0, utf16_character)
            );
            assert_eq!(
                utf8.position_to_offset(Position::new(0, utf8_character))
                    .unwrap(),
                offset
            );
            assert_eq!(
                utf16
                    .position_to_offset(Position::new(0, utf16_character))
                    .unwrap(),
                offset
            );
        }

        assert_eq!(
            LspPositionCodec::new("é😀", PositionEncoding::Utf8).document_end(),
            Position::new(0, 6)
        );
        assert_eq!(
            LspPositionCodec::new("é😀", PositionEncoding::Utf16).document_end(),
            Position::new(0, 3)
        );
    }

    #[test]
    fn codec_handles_all_line_endings_and_encoded_boundaries() {
        let text = "aé😀中\r\nx\rz\n";
        let utf8 = LspPositionCodec::new(text, PositionEncoding::Utf8);
        let utf16 = LspPositionCodec::new(text, PositionEncoding::Utf16);

        assert_eq!(utf16.line_count(), 4);
        assert_eq!(utf16.offset_to_position(12).unwrap(), Position::new(1, 0));
        assert_eq!(utf16.offset_to_position(14).unwrap(), Position::new(2, 0));
        assert_eq!(utf16.document_end(), Position::new(3, 0));
        assert_eq!(
            utf16.position_to_offset(Position::new(0, 999)).unwrap(),
            10,
            "overlong positions clamp to the line content end"
        );

        assert!(matches!(
            utf8.position_to_offset(Position::new(0, 2)),
            Err(PositionCodecError::InvalidEncodingBoundary { .. })
        ));
        assert!(matches!(
            utf16.position_to_offset(Position::new(0, 3)),
            Err(PositionCodecError::InvalidEncodingBoundary { .. })
        ));
        assert!(matches!(
            utf16.offset_to_position(11),
            Err(PositionCodecError::InvalidByteBoundary { .. })
        ));
        assert!(matches!(
            utf16.position_to_offset(Position::new(4, 0)),
            Err(PositionCodecError::LineOutOfBounds { .. })
        ));
        assert!(matches!(
            utf16.range_to_text_range(Range::new(Position::new(1, 0), Position::new(0, 0))),
            Err(PositionCodecError::ReversedRange { .. })
        ));
    }

    #[test]
    fn codec_splits_multiline_semantic_tokens_in_negotiated_units() {
        let text = "aé😀中\r\nx\rz\n";
        let range = TextRange::new(1.into(), 15.into());

        let utf8 = LspPositionCodec::new(text, PositionEncoding::Utf8)
            .semantic_token_segments(range)
            .unwrap();
        assert_eq!(utf8.len(), 3);
        assert_eq!((utf8[0].start, utf8[0].length), (Position::new(0, 1), 9));
        assert_eq!((utf8[1].start, utf8[1].length), (Position::new(1, 0), 1));
        assert_eq!((utf8[2].start, utf8[2].length), (Position::new(2, 0), 1));

        let utf16 = LspPositionCodec::new(text, PositionEncoding::Utf16)
            .semantic_token_segments(range)
            .unwrap();
        assert_eq!(utf16.len(), 3);
        assert_eq!((utf16[0].start, utf16[0].length), (Position::new(0, 1), 4));
        assert_eq!((utf16[1].start, utf16[1].length), (Position::new(1, 0), 1));
        assert_eq!((utf16[2].start, utf16[2].length), (Position::new(2, 0), 1));
    }
}
