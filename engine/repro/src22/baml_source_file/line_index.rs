use std::{
    fmt,
    fmt::{Debug, Formatter},
    num::{NonZeroUsize, ParseIntError},
    ops::Deref,
    str::FromStr,
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::{
    baml_source_file::SourceLocation,
    baml_text_size::{TextLen, TextRange, TextSize},
};

/// Index for fast [byte offset](TextSize) to [`SourceLocation`] conversions.
///
/// Cloning a [`LineIndex`] is cheap because it only requires bumping a reference count.
#[derive(Clone, Eq, PartialEq)]
pub struct LineIndex {
    inner: Arc<LineIndexInner>,
}

#[derive(Eq, PartialEq)]
struct LineIndexInner {
    line_starts: Vec<TextSize>,
    kind: IndexKind,
}

impl LineIndex {
    /// Builds the [`LineIndex`] from the source text of a file.
    pub fn from_source_text(text: &str) -> Self {
        let mut line_starts: Vec<TextSize> = Vec::with_capacity(text.len() / 88);
        line_starts.push(TextSize::default());

        let bytes = text.as_bytes();
        let mut utf8 = false;

        assert!(u32::try_from(bytes.len()).is_ok());

        for (i, byte) in bytes.iter().enumerate() {
            utf8 |= !byte.is_ascii();

            match byte {
                // Only track one line break for `\r\n`.
                b'\r' if bytes.get(i + 1) == Some(&b'\n') => continue,
                b'\n' | b'\r' => {
                    // SAFETY: Assertion above guarantees `i <= u32::MAX`
                    #[allow(clippy::cast_possible_truncation)]
                    line_starts.push(TextSize::from(i as u32) + TextSize::from(1));
                }
                _ => {}
            }
        }

        let kind = if utf8 {
            IndexKind::Utf8
        } else {
            IndexKind::Ascii
        };

        Self {
            inner: Arc::new(LineIndexInner { line_starts, kind }),
        }
    }

    fn kind(&self) -> IndexKind {
        self.inner.kind
    }

    /// Returns the row and column index for an offset.
    pub fn source_location(&self, offset: TextSize, content: &str) -> SourceLocation {
        match self.line_starts().binary_search(&offset) {
            // Offset is at the start of a line
            Ok(row) => SourceLocation {
                row: OneIndexed::from_zero_indexed(row),
                column: OneIndexed::from_zero_indexed(0),
            },
            Err(next_row) => {
                // SAFETY: Safe because the index always contains an entry for the offset 0
                let row = next_row - 1;
                let mut line_start = self.line_starts()[row];

                let column = if self.kind().is_ascii() {
                    usize::from(offset) - usize::from(line_start)
                } else {
                    // Don't count the BOM character as a column.
                    if line_start == TextSize::from(0) && content.starts_with('\u{feff}') {
                        line_start = '\u{feff}'.text_len();
                    }

                    content[TextRange::new(line_start, offset)].chars().count()
                };

                SourceLocation {
                    row: OneIndexed::from_zero_indexed(row),
                    column: OneIndexed::from_zero_indexed(column),
                }
            }
        }
    }

    /// Return the number of lines in the source code.
    pub fn line_count(&self) -> usize {
        self.line_starts().len()
    }

    /// Returns `true` if the text only consists of ASCII characters
    pub fn is_ascii(&self) -> bool {
        self.kind().is_ascii()
    }

    /// Returns the row number for a given offset.
    pub fn line_index(&self, offset: TextSize) -> OneIndexed {
        match self.line_starts().binary_search(&offset) {
            // Offset is at the start of a line
            Ok(row) => OneIndexed::from_zero_indexed(row),
            Err(row) => {
                // SAFETY: Safe because the index always contains an entry for the offset 0
                OneIndexed::from_zero_indexed(row - 1)
            }
        }
    }

    /// Returns the [byte offset](TextSize) for the `line` with the given index.
    pub fn line_start(&self, line: OneIndexed, contents: &str) -> TextSize {
        let row_index = line.to_zero_indexed();
        let starts = self.line_starts();

        // If start-of-line position after last line
        if row_index == starts.len() {
            contents.text_len()
        } else {
            starts[row_index]
        }
    }

    /// Returns the [byte offset](TextSize) of the `line`'s end.
    /// The offset is the end of the line, up to and including the newline character ending the line (if any).
    pub fn line_end(&self, line: OneIndexed, contents: &str) -> TextSize {
        let row_index = line.to_zero_indexed();
        let starts = self.line_starts();

        // If start-of-line position after last line
        if row_index.saturating_add(1) >= starts.len() {
            contents.text_len()
        } else {
            starts[row_index + 1]
        }
    }

    /// Returns the [byte offset](TextSize) of the `line`'s end.
    /// The offset is the end of the line, excluding the newline character ending the line (if any).
    pub fn line_end_exclusive(&self, line: OneIndexed, contents: &str) -> TextSize {
        let row_index = line.to_zero_indexed();
        let starts = self.line_starts();

        // If start-of-line position after last line
        if row_index.saturating_add(1) >= starts.len() {
            contents.text_len()
        } else {
            starts[row_index + 1] - TextSize::new(1)
        }
    }

    /// Returns the [`TextRange`] of the `line` with the given index.
    /// The start points to the first character's [byte offset](TextSize), the end up to, and including
    /// the newline character ending the line (if any).
    pub fn line_range(&self, line: OneIndexed, contents: &str) -> TextRange {
        let starts = self.line_starts();

        if starts.len() == line.to_zero_indexed() {
            TextRange::empty(contents.text_len())
        } else {
            TextRange::new(
                self.line_start(line, contents),
                self.line_start(line.saturating_add(1), contents),
            )
        }
    }

    /// Returns the [byte offset](TextSize) at `line` and `column`.
    pub fn offset(&self, line: OneIndexed, column: OneIndexed, contents: &str) -> TextSize {
        // If start-of-line position after last line
        if line.to_zero_indexed() > self.line_starts().len() {
            return contents.text_len();
        }

        let line_range = self.line_range(line, contents);

        match self.kind() {
            IndexKind::Ascii => {
                line_range.start()
                    + TextSize::try_from(column.to_zero_indexed())
                        .unwrap_or(line_range.len())
                        .clamp(TextSize::new(0), line_range.len())
            }
            IndexKind::Utf8 => {
                let rest = &contents[line_range];
                let column_offset: TextSize = rest
                    .chars()
                    .take(column.to_zero_indexed())
                    .map(crate::baml_text_size::TextLen::text_len)
                    .sum();
                line_range.start() + column_offset
            }
        }
    }

    /// Returns the [byte offsets](TextSize) for every line
    pub fn line_starts(&self) -> &[TextSize] {
        &self.inner.line_starts
    }
}

impl Deref for LineIndex {
    type Target = [TextSize];

    fn deref(&self) -> &Self::Target {
        self.line_starts()
    }
}

impl Debug for LineIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.line_starts()).finish()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum IndexKind {
    /// Optimized index for an ASCII only document
    Ascii,

    /// Index for UTF8 documents
    Utf8,
}

impl IndexKind {
    const fn is_ascii(self) -> bool {
        matches!(self, IndexKind::Ascii)
    }
}

/// Type-safe wrapper for a value whose logical range starts at `1`, for
/// instance the line or column numbers in a file
///
/// Internally this is represented as a [`NonZeroUsize`], this enables some
/// memory optimizations
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OneIndexed(NonZeroUsize);

impl OneIndexed {
    /// The largest value that can be represented by this integer type
    pub const MAX: Self = unwrap(Self::new(usize::MAX));
    // SAFETY: These constants are being initialized with non-zero values
    /// The smallest value that can be represented by this integer type.
    pub const MIN: Self = unwrap(Self::new(1));
    pub const ONE: NonZeroUsize = unwrap(NonZeroUsize::new(1));

    /// Creates a non-zero if the given value is not zero.
    pub const fn new(value: usize) -> Option<Self> {
        match NonZeroUsize::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Construct a new [`OneIndexed`] from a zero-indexed value
    pub const fn from_zero_indexed(value: usize) -> Self {
        Self(Self::ONE.saturating_add(value))
    }

    /// Returns the value as a primitive type.
    pub const fn get(self) -> usize {
        self.0.get()
    }

    /// Return the zero-indexed primitive value for this [`OneIndexed`]
    pub const fn to_zero_indexed(self) -> usize {
        self.0.get() - 1
    }

    /// Saturating integer addition. Computes `self + rhs`, saturating at
    /// the numeric bounds instead of overflowing.
    #[must_use]
    pub const fn saturating_add(self, rhs: usize) -> Self {
        match NonZeroUsize::new(self.0.get().saturating_add(rhs)) {
            Some(value) => Self(value),
            None => Self::MAX,
        }
    }

    /// Saturating integer subtraction. Computes `self - rhs`, saturating
    /// at the numeric bounds instead of overflowing.
    #[must_use]
    pub const fn saturating_sub(self, rhs: usize) -> Self {
        match NonZeroUsize::new(self.0.get().saturating_sub(rhs)) {
            Some(value) => Self(value),
            None => Self::MIN,
        }
    }

    /// Checked addition. Returns `None` if overflow occurred.
    #[must_use]
    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0.get()).map(Self)
    }

    /// Checked subtraction. Returns `None` if overflow occurred.
    #[must_use]
    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.0.get().checked_sub(rhs.get()).and_then(Self::new)
    }
}

impl fmt::Display for OneIndexed {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        std::fmt::Debug::fmt(&self.0.get(), f)
    }
}

/// A const `Option::unwrap` without nightly features:
/// [Tracking issue](https://github.com/rust-lang/rust/issues/67441)
const fn unwrap<T: Copy>(option: Option<T>) -> T {
    match option {
        Some(value) => value,
        None => panic!("unwrapping None"),
    }
}

impl FromStr for OneIndexed {
    type Err = ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(OneIndexed(NonZeroUsize::from_str(s)?))
    }
}
