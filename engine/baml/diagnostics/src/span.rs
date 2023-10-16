/// Represents a location in a datamodel's text representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    /// Constructor.
    pub fn new(start: usize, end: usize) -> Span {
        Span { start, end }
    }

    /// Creates a new empty span.
    pub fn empty() -> Span {
        Span { start: 0, end: 0 }
    }

    /// Is the given position inside the span? (boundaries included)
    pub fn contains(&self, position: usize) -> bool {
        position >= self.start && position <= self.end
    }

    /// Is the given span overlapping with the current span.
    pub fn overlaps(self, other: Span) -> bool {
        self.contains(other.start) || self.contains(other.end)
    }
}

impl From<pest::Span<'_>> for Span {
    fn from(s: pest::Span<'_>) -> Self {
        Span {
            start: s.start(),
            end: s.end(),
        }
    }
}
