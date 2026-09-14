//! Byte-offset ↔ (line, UTF-16 column) mapping over one file's text.
//!
//! The playground and editor wire protocols express columns in UTF-16 code
//! units; this is the minimal converter the symbol listing needs. The LSP
//! layer owns the full negotiated-encoding codec.

/// Line start table over a borrowed text.
pub struct LineIndex<'a> {
    text: &'a str,
    /// Byte offset of the start of each line; `line_starts[0] == 0`.
    line_starts: Vec<u32>,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut line_starts = vec![0u32];
        line_starts.extend(
            text.match_indices('\n')
                .map(|(byte, _)| u32::try_from(byte + 1).unwrap_or(u32::MAX)),
        );
        Self { text, line_starts }
    }

    /// Convert a UTF-8 byte offset to a zero-based `(line, utf16_column)`.
    ///
    /// Returns `None` if the offset is out of bounds or inside a UTF-8 code
    /// point.
    pub fn offset_to_position(&self, offset: u32) -> Option<(u32, u32)> {
        let offset_usize = usize::try_from(offset).ok()?;
        if offset_usize > self.text.len() || !self.text.is_char_boundary(offset_usize) {
            return None;
        }
        let line = self
            .line_starts
            .partition_point(|&start| start <= offset)
            .checked_sub(1)?;
        let line_start = usize::try_from(self.line_starts[line]).ok()?;
        let column = self.text[line_start..offset_usize].encode_utf16().count();
        Some((
            u32::try_from(line).unwrap_or(u32::MAX),
            u32::try_from(column).unwrap_or(u32::MAX),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::LineIndex;

    #[test]
    fn utf16_columns_and_lines() {
        let text = "ab\n🐑c\nlast";
        let index = LineIndex::new(text);
        assert_eq!(index.offset_to_position(0), Some((0, 0)));
        assert_eq!(index.offset_to_position(2), Some((0, 2)));
        assert_eq!(index.offset_to_position(3), Some((1, 0)));
        // 🐑 is 4 UTF-8 bytes and 2 UTF-16 code units.
        assert_eq!(index.offset_to_position(7), Some((1, 2)));
        assert_eq!(index.offset_to_position(4), None, "inside a code point");
        assert_eq!(index.offset_to_position(9), Some((2, 0)));
        assert_eq!(index.offset_to_position(13), Some((2, 4)), "end of text");
        assert_eq!(index.offset_to_position(14), None, "out of bounds");
    }
}
