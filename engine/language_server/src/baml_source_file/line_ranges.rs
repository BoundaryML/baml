use std::ops::Add;

use memchr::{memchr2, memrchr2};

use crate::{
    baml_source_file::newlines::find_newline,
    baml_text_size::{TextLen, TextRange, TextSize},
};

/// Extension trait for [`str`] that provides methods for working with ranges of lines.
pub trait LineRanges {
    /// Computes the start position of the line of `offset`.
    fn line_start(&self, offset: TextSize) -> TextSize;

    /// Computes the start position of the file contents: either the first byte, or the byte after
    /// the BOM.
    fn bom_start_offset(&self) -> TextSize;

    /// Returns `true` if `offset` is at the start of a line.
    fn is_at_start_of_line(&self, offset: TextSize) -> bool {
        self.line_start(offset) == offset
    }

    /// Computes the offset that is right after the newline character that ends `offset`'s line.
    fn full_line_end(&self, offset: TextSize) -> TextSize;

    /// Computes the offset that is right before the newline character that ends `offset`'s line.
    fn line_end(&self, offset: TextSize) -> TextSize;

    /// Computes the range of this `offset`s line.
    fn full_line_range(&self, offset: TextSize) -> TextRange {
        TextRange::new(self.line_start(offset), self.full_line_end(offset))
    }

    /// Computes the range of this `offset`s line ending before the newline character.
    fn line_range(&self, offset: TextSize) -> TextRange {
        TextRange::new(self.line_start(offset), self.line_end(offset))
    }

    /// Returns the text of the `offset`'s line.
    fn full_line_str(&self, offset: TextSize) -> &str;

    /// Returns the text of the `offset`'s line.
    ///
    /// Excludes the newline characters at the end of the line.
    fn line_str(&self, offset: TextSize) -> &str;

    /// Computes the range of all lines that this `range` covers.
    ///
    /// The range starts at the beginning of the line at `range.start()` and goes up to, and including, the new line character
    /// at the end of `range.ends()`'s line.
    fn full_lines_range(&self, range: TextRange) -> TextRange {
        TextRange::new(
            self.line_start(range.start()),
            self.full_line_end(range.end()),
        )
    }

    /// Computes the range of all lines that this `range` covers.
    ///
    /// The range starts at the beginning of the line at `range.start()` and goes up to, but excluding, the new line character
    /// at the end of `range.end()`'s line.
    fn lines_range(&self, range: TextRange) -> TextRange {
        TextRange::new(self.line_start(range.start()), self.line_end(range.end()))
    }

    /// Returns true if the text of `range` contains any line break.
    fn contains_line_break(&self, range: TextRange) -> bool;

    /// Returns the text of all lines that include `range`.
    fn lines_str(&self, range: TextRange) -> &str;

    /// Returns the text of all lines that include `range`.
    fn full_lines_str(&self, range: TextRange) -> &str;

    /// The number of lines `range` spans.
    fn count_lines(&self, range: TextRange) -> u32 {
        let mut count = 0;
        let mut line_end = self.line_end(range.start());

        loop {
            let next_line_start = self.full_line_end(line_end);

            // Reached the end of the string
            if next_line_start == line_end {
                break count;
            }

            // Range ends at the line boundary
            if line_end >= range.end() {
                break count;
            }

            count += 1;

            line_end = self.line_end(next_line_start);
        }
    }
}

impl LineRanges for str {
    fn line_start(&self, offset: TextSize) -> TextSize {
        let bytes = self[TextRange::up_to(offset)].as_bytes();
        if let Some(index) = memrchr2(b'\n', b'\r', bytes) {
            // SAFETY: Safe because `index < offset`
            TextSize::try_from(index).unwrap().add(TextSize::from(1))
        } else {
            self.bom_start_offset()
        }
    }

    fn bom_start_offset(&self) -> TextSize {
        if self.starts_with('\u{feff}') {
            // Skip the BOM.
            '\u{feff}'.text_len()
        } else {
            // Start of file.
            TextSize::default()
        }
    }

    fn full_line_end(&self, offset: TextSize) -> TextSize {
        let slice = &self[usize::from(offset)..];
        if let Some((index, line_ending)) = find_newline(slice) {
            offset + TextSize::try_from(index).unwrap() + line_ending.text_len()
        } else {
            self.text_len()
        }
    }

    fn line_end(&self, offset: TextSize) -> TextSize {
        let slice = &self[offset.to_usize()..];
        if let Some(index) = memchr2(b'\n', b'\r', slice.as_bytes()) {
            offset + TextSize::try_from(index).unwrap()
        } else {
            self.text_len()
        }
    }

    fn full_line_str(&self, offset: TextSize) -> &str {
        &self[self.full_line_range(offset)]
    }

    fn line_str(&self, offset: TextSize) -> &str {
        &self[self.line_range(offset)]
    }

    fn contains_line_break(&self, range: TextRange) -> bool {
        memchr2(b'\n', b'\r', self[range].as_bytes()).is_some()
    }

    fn lines_str(&self, range: TextRange) -> &str {
        &self[self.lines_range(range)]
    }

    fn full_lines_str(&self, range: TextRange) -> &str {
        &self[self.full_lines_range(range)]
    }
}
