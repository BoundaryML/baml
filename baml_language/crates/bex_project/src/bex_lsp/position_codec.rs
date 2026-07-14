//! Negotiated LSP position boundary.
//!
//! One codec owns all conversions between compiler byte offsets and LSP
//! positions. Compiler APIs stay byte-based; every LSP handler converts at
//! this boundary with the encoding negotiated during `initialize`: UTF-8
//! when the client offers it, otherwise UTF-16.
//!
//! The codec recognizes LF, CRLF, and bare CR line terminators. Incoming
//! overlong character positions clamp to their line's content end;
//! nonexistent lines, malformed ranges, and positions inside an encoding
//! unit are `InvalidParams`. Outgoing conversions are lenient (compiler
//! spans are trusted): out-of-bounds offsets clamp.

use text_size::{TextRange, TextSize};

/// Position encodings supported for negotiation.
///
/// Ordered least- to greatest-priority for the derived `Ord`: UTF-16 is the
/// mandatory baseline, UTF-8 is BAML's preferred fast path. UTF-32 is not
/// negotiated and exists only to convert if a future protocol decision
/// adds it.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PositionEncoding {
    /// The encoding every LSP client must support.
    #[default]
    UTF16,
    /// Fixed 4-byte units; conversion is codepoint counting.
    #[allow(dead_code)]
    UTF32,
    /// BAML's preferred encoding: characters are byte offsets.
    UTF8,
}

impl PositionEncoding {
    /// Select the encoding for this session: UTF-8 when the client offers
    /// it, otherwise UTF-16. UTF-32 is never selected.
    pub(crate) fn negotiate(
        client_offered: Option<&[lsp_types::PositionEncodingKind]>,
    ) -> PositionEncoding {
        let offered_utf8 = client_offered
            .unwrap_or_default()
            .contains(&lsp_types::PositionEncodingKind::UTF8);
        if offered_utf8 {
            PositionEncoding::UTF8
        } else {
            PositionEncoding::UTF16
        }
    }

    pub(crate) fn to_lsp_kind(self) -> lsp_types::PositionEncodingKind {
        match self {
            PositionEncoding::UTF8 => lsp_types::PositionEncodingKind::UTF8,
            PositionEncoding::UTF16 => lsp_types::PositionEncodingKind::UTF16,
            PositionEncoding::UTF32 => lsp_types::PositionEncodingKind::UTF32,
        }
    }
}

/// Why an incoming LSP position or range could not be converted. Serialized
/// as `InvalidParams` (`-32602`) at the error boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PositionCodecError {
    LineOutOfRange {
        line: u32,
        line_count: u32,
    },
    /// Range end precedes range start.
    MalformedRange,
    /// The character index falls strictly inside one encoded character
    /// (e.g. between UTF-16 surrogate halves or inside a UTF-8 sequence).
    InsideEncodingUnit {
        line: u32,
        character: u32,
    },
}

impl std::fmt::Display for PositionCodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PositionCodecError::LineOutOfRange { line, line_count } => {
                write!(
                    f,
                    "line {line} out of range (document has {line_count} lines)"
                )
            }
            PositionCodecError::MalformedRange => write!(f, "range end precedes range start"),
            PositionCodecError::InsideEncodingUnit { line, character } => write!(
                f,
                "position {line}:{character} falls inside an encoded character"
            ),
        }
    }
}

impl From<PositionCodecError> for crate::bex_lsp::LspError {
    fn from(e: PositionCodecError) -> Self {
        crate::bex_lsp::LspError::InvalidParams(e.to_string())
    }
}

/// One same-line semantic-token segment in negotiated encoding units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TokenSegment {
    pub line: u32,
    pub start_character: u32,
    pub length: u32,
}

/// Line-aware position codec for one document snapshot.
pub(crate) struct PositionCodec<'a> {
    text: &'a str,
    encoding: PositionEncoding,
    /// Byte offset where line `i` starts. `line_starts[0] == 0`.
    line_starts: Vec<u32>,
    /// Byte offset where line `i`'s content ends (excludes the LF / CRLF /
    /// CR terminator). Parallel to `line_starts`.
    line_content_ends: Vec<u32>,
}

impl<'a> PositionCodec<'a> {
    pub(crate) fn new(text: &'a str, encoding: PositionEncoding) -> Self {
        let bytes = text.as_bytes();
        let mut line_starts = vec![0u32];
        let mut line_content_ends = Vec::new();
        let mut i = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'\n' => {
                    line_content_ends.push(to_u32(i));
                    i += 1;
                    line_starts.push(to_u32(i));
                }
                b'\r' => {
                    line_content_ends.push(to_u32(i));
                    i += if bytes.get(i + 1) == Some(&b'\n') {
                        2
                    } else {
                        1
                    };
                    line_starts.push(to_u32(i));
                }
                _ => i += 1,
            }
        }
        line_content_ends.push(to_u32(bytes.len()));
        debug_assert_eq!(line_starts.len(), line_content_ends.len());
        Self {
            text,
            encoding,
            line_starts,
            line_content_ends,
        }
    }

    fn line_count(&self) -> u32 {
        to_u32(self.line_starts.len())
    }

    /// Line index containing `offset` (clamped into the document).
    fn line_of_offset(&self, offset: u32) -> usize {
        self.line_starts.partition_point(|&s| s <= offset) - 1
    }

    /// Encoded length of a byte slice known to lie within one line.
    fn encoded_len(&self, slice: &str) -> u32 {
        match self.encoding {
            PositionEncoding::UTF8 => to_u32(slice.len()),
            PositionEncoding::UTF16 => to_u32(slice.chars().map(char::len_utf16).sum::<usize>()),
            PositionEncoding::UTF32 => to_u32(slice.chars().count()),
        }
    }

    // ── Incoming: LSP position/range → byte offset/range ────────────────

    /// Convert an incoming LSP position to a byte offset.
    ///
    /// Overlong `character` clamps to the line's content end. A nonexistent
    /// line or a position inside one encoded character is an error.
    pub(crate) fn position_to_offset(
        &self,
        position: lsp_types::Position,
    ) -> Result<TextSize, PositionCodecError> {
        let line = position.line as usize;
        if line >= self.line_starts.len() {
            return Err(PositionCodecError::LineOutOfRange {
                line: position.line,
                line_count: self.line_count(),
            });
        }
        let line_start = self.line_starts[line] as usize;
        let content_end = self.line_content_ends[line] as usize;
        let slice = &self.text[line_start..content_end];

        if position.character == 0 {
            return Ok(TextSize::from(to_u32(line_start)));
        }

        let mut units = 0u32;
        for (byte_in_line, ch) in slice.char_indices() {
            if units == position.character {
                return Ok(TextSize::from(to_u32(line_start + byte_in_line)));
            }
            let width = match self.encoding {
                PositionEncoding::UTF8 => to_u32(ch.len_utf8()),
                PositionEncoding::UTF16 => to_u32(ch.len_utf16()),
                PositionEncoding::UTF32 => 1,
            };
            if units + width > position.character {
                return Err(PositionCodecError::InsideEncodingUnit {
                    line: position.line,
                    character: position.character,
                });
            }
            units += width;
        }
        // `character` is at or beyond the line's end: clamp to content end.
        Ok(TextSize::from(to_u32(content_end)))
    }

    /// Convert an incoming LSP range to a byte range. Both positions must be
    /// valid and start must not exceed end.
    pub(crate) fn range_to_byte_range(
        &self,
        range: lsp_types::Range,
    ) -> Result<TextRange, PositionCodecError> {
        let start = self.position_to_offset(range.start)?;
        let end = self.position_to_offset(range.end)?;
        if start > end {
            return Err(PositionCodecError::MalformedRange);
        }
        Ok(TextRange::new(start, end))
    }

    // ── Outgoing: byte offset/span → LSP position/range ─────────────────

    /// Convert a byte offset to an LSP position. Lenient: out-of-bounds
    /// offsets clamp to the document end, offsets inside a UTF-8 sequence
    /// snap back to the character start, and offsets inside a line
    /// terminator report the line's content end.
    pub(crate) fn offset_to_position(&self, offset: u32) -> lsp_types::Position {
        let mut offset = (offset as usize).min(self.text.len());
        while offset > 0 && !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = self.line_of_offset(to_u32(offset));
        let line_start = self.line_starts[line] as usize;
        let content_end = self.line_content_ends[line] as usize;
        let clamped = offset.min(content_end).max(line_start);
        lsp_types::Position {
            line: to_u32(line),
            character: self.encoded_len(&self.text[line_start..clamped]),
        }
    }

    /// Convert a byte span to an LSP range.
    pub(crate) fn byte_range_to_lsp(&self, range: TextRange) -> lsp_types::Range {
        lsp_types::Range {
            start: self.offset_to_position(range.start().into()),
            end: self.offset_to_position(range.end().into()),
        }
    }

    /// Position of the document end (for formatting's full-document range).
    pub(crate) fn document_end(&self) -> lsp_types::Position {
        self.offset_to_position(to_u32(self.text.len()))
    }

    // ── Semantic tokens ──────────────────────────────────────────────────

    /// Split a byte span into same-line segments with negotiated-encoding
    /// starts and lengths. Newline units are excluded and zero-length
    /// segments discarded — valid for every client and required for
    /// VS Code's missing multiline-token capability.
    pub(crate) fn token_segments(&self, range: TextRange) -> Vec<TokenSegment> {
        let start = (u32::from(range.start()) as usize).min(self.text.len());
        let end = (u32::from(range.end()) as usize).min(self.text.len());
        if start >= end {
            return Vec::new();
        }

        let first_line = self.line_of_offset(to_u32(start));
        let mut segments = Vec::new();
        for line in first_line..self.line_starts.len() {
            let line_start = self.line_starts[line] as usize;
            if line_start >= end {
                break;
            }
            let content_end = self.line_content_ends[line] as usize;
            let seg_start = start.max(line_start).min(content_end);
            let seg_end = end.min(content_end);
            if seg_start >= seg_end {
                continue;
            }
            segments.push(TokenSegment {
                line: to_u32(line),
                start_character: self.encoded_len(&self.text[line_start..seg_start]),
                length: self.encoded_len(&self.text[seg_start..seg_end]),
            });
        }
        segments
    }
}

fn to_u32(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(line: u32, character: u32) -> lsp_types::Position {
        lsp_types::Position { line, character }
    }

    #[test]
    fn line_terminators_lf_crlf_and_bare_cr() {
        let codec = PositionCodec::new("a\nb\r\nc\rd", PositionEncoding::UTF8);
        // Lines: "a" @0, "b" @2, "c" @5, "d" @7.
        assert_eq!(codec.position_to_offset(pos(0, 0)).unwrap(), 0.into());
        assert_eq!(codec.position_to_offset(pos(1, 0)).unwrap(), 2.into());
        assert_eq!(codec.position_to_offset(pos(2, 0)).unwrap(), 5.into());
        assert_eq!(codec.position_to_offset(pos(3, 1)).unwrap(), 8.into());
        assert_eq!(codec.offset_to_position(7), pos(3, 0));
        // Offset inside the CRLF terminator reports the content end of line 1.
        assert_eq!(codec.offset_to_position(4), pos(1, 1));
    }

    #[test]
    fn utf16_and_utf8_round_trip_multibyte() {
        // "é" is 2 bytes / 1 UTF-16 unit; "😀" is 4 bytes / 2 UTF-16 units.
        let text = "é😀x";
        let utf8 = PositionCodec::new(text, PositionEncoding::UTF8);
        let utf16 = PositionCodec::new(text, PositionEncoding::UTF16);

        // Offset of 'x' is 6 bytes.
        assert_eq!(utf8.position_to_offset(pos(0, 6)).unwrap(), 6.into());
        assert_eq!(utf16.position_to_offset(pos(0, 3)).unwrap(), 6.into());
        assert_eq!(utf8.offset_to_position(6), pos(0, 6));
        assert_eq!(utf16.offset_to_position(6), pos(0, 3));
    }

    #[test]
    fn position_inside_encoding_unit_is_error() {
        let text = "😀"; // 4 bytes, 2 UTF-16 units.
        let utf16 = PositionCodec::new(text, PositionEncoding::UTF16);
        assert!(matches!(
            utf16.position_to_offset(pos(0, 1)),
            Err(PositionCodecError::InsideEncodingUnit { .. })
        ));
        let utf8 = PositionCodec::new(text, PositionEncoding::UTF8);
        assert!(matches!(
            utf8.position_to_offset(pos(0, 2)),
            Err(PositionCodecError::InsideEncodingUnit { .. })
        ));
    }

    #[test]
    fn overlong_character_clamps_to_line_end_and_missing_line_errors() {
        let codec = PositionCodec::new("ab\ncd", PositionEncoding::UTF8);
        assert_eq!(codec.position_to_offset(pos(0, 99)).unwrap(), 2.into());
        assert!(matches!(
            codec.position_to_offset(pos(9, 0)),
            Err(PositionCodecError::LineOutOfRange { .. })
        ));
    }

    #[test]
    fn malformed_range_is_rejected() {
        let codec = PositionCodec::new("abc", PositionEncoding::UTF8);
        let range = lsp_types::Range {
            start: pos(0, 2),
            end: pos(0, 1),
        };
        assert_eq!(
            codec.range_to_byte_range(range),
            Err(PositionCodecError::MalformedRange)
        );
    }

    #[test]
    fn outgoing_offset_snaps_inside_utf8_sequence_and_clamps_out_of_bounds() {
        let codec = PositionCodec::new("é", PositionEncoding::UTF16);
        assert_eq!(codec.offset_to_position(1), pos(0, 0)); // inside 'é'
        assert_eq!(codec.offset_to_position(99), pos(0, 1)); // clamp to end
    }

    #[test]
    fn document_end_for_trailing_newline_and_without() {
        let with_nl = PositionCodec::new("a\n", PositionEncoding::UTF8);
        assert_eq!(with_nl.document_end(), pos(1, 0));
        let without = PositionCodec::new("a\nbc", PositionEncoding::UTF8);
        assert_eq!(without.document_end(), pos(1, 2));
    }

    #[test]
    fn token_segments_split_multiline_and_skip_newline_units() {
        let text = "ab\ncd\r\nef";
        let codec = PositionCodec::new(text, PositionEncoding::UTF8);
        // Span covering "b\ncd\r\ne" = bytes 1..8.
        let segments = codec.token_segments(TextRange::new(1.into(), 8.into()));
        assert_eq!(
            segments,
            vec![
                TokenSegment {
                    line: 0,
                    start_character: 1,
                    length: 1
                },
                TokenSegment {
                    line: 1,
                    start_character: 0,
                    length: 2
                },
                TokenSegment {
                    line: 2,
                    start_character: 0,
                    length: 1
                },
            ]
        );
    }

    #[test]
    fn token_segments_lengths_are_encoded_units() {
        let text = "😀x";
        let utf16 = PositionCodec::new(text, PositionEncoding::UTF16);
        let segments = utf16.token_segments(TextRange::new(0.into(), 5.into()));
        assert_eq!(
            segments,
            vec![TokenSegment {
                line: 0,
                start_character: 0,
                length: 3
            }]
        );
    }

    #[test]
    fn token_segments_discard_zero_length() {
        let text = "a\n\nb";
        let codec = PositionCodec::new(text, PositionEncoding::UTF8);
        // Span covering "\n\n" only — the empty middle line yields nothing.
        let segments = codec.token_segments(TextRange::new(1.into(), 3.into()));
        assert!(segments.is_empty());
    }

    #[test]
    fn negotiation_prefers_utf8_and_falls_back_to_utf16() {
        let offered = vec![
            lsp_types::PositionEncodingKind::UTF16,
            lsp_types::PositionEncodingKind::UTF8,
        ];
        assert_eq!(
            PositionEncoding::negotiate(Some(&offered)),
            PositionEncoding::UTF8
        );
        let utf16_only = vec![lsp_types::PositionEncodingKind::UTF16];
        assert_eq!(
            PositionEncoding::negotiate(Some(&utf16_only)),
            PositionEncoding::UTF16
        );
        assert_eq!(PositionEncoding::negotiate(None), PositionEncoding::UTF16);
        // UTF-32 alone is never selected.
        let utf32_only = vec![lsp_types::PositionEncodingKind::UTF32];
        assert_eq!(
            PositionEncoding::negotiate(Some(&utf32_only)),
            PositionEncoding::UTF16
        );
    }
}
