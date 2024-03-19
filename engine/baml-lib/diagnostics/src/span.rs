use super::SourceFile;

/// Represents a location in a datamodel's text representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub file: SourceFile,
    pub start: usize,
    pub end: usize,
}

impl Span {
    /// Constructor.
    pub fn new(file: SourceFile, start: usize, end: usize) -> Span {
        Span { file, start, end }
    }

    /// Creates a new empty span.
    pub fn empty(file: SourceFile) -> Span {
        Span {
            file,
            start: 0,
            end: 0,
        }
    }

    /// Is the given position inside the span? (boundaries included)
    pub fn contains(&self, position: usize) -> bool {
        position >= self.start && position <= self.end
    }

    /// Is the given span overlapping with the current span.
    pub fn overlaps(self, other: Span) -> bool {
        self.file == other.file && (self.contains(other.start) || self.contains(other.end))
    }
}

impl From<(SourceFile, pest::Span<'_>)> for Span {
    fn from((file, s): (SourceFile, pest::Span<'_>)) -> Self {
        Span {
            file,
            start: s.start(),
            end: s.end(),
        }
    }
}
