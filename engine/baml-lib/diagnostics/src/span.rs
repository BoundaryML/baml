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

    // TODO: Parser should keep track of this information and set it when the
    // span is created. Otherwise we'll have to read the entire file again and
    // again every time we call this function on a span.
    pub fn line_and_column(&self) -> ((usize, usize), (usize, usize)) {
        let contents = self.file.as_str();
        let mut line = 0;
        let mut column = 0;

        let mut start = None;
        let mut end = None;

        for (idx, c) in contents.chars().enumerate() {
            if idx == self.start {
                start = Some((line, column));
            }
            if idx == self.end {
                end = Some((line, column));
                break;
            }

            if c == '\n' {
                line += 1;
                column = 0;
            } else {
                column += 1;
            }
        }

        match (start, end) {
            (Some(start), Some(end)) => (start, end),
            (Some(start), None) => (start, (line, column)),
            _ => ((0, 0), (0, 0)),
        }
    }

    pub fn line_number(&self) -> usize {
        self.line_and_column().0 .0
    }

    /// Create a fake span. Useful when generating test data that requires
    /// spans but doesn't check spans.
    pub fn fake() -> Span {
        let fake_source = ("fake-file.baml".into(), "fake contents").into();
        Span::empty(fake_source)
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

/// A special-purpose span used for communicating with the JS playground.
/// Currently its only job is indicating the span of a currently-active
/// LLM Function.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SerializedSpan {
    pub file_path: String,
    pub start_line: usize,
    pub start: usize,
    pub end_line: usize,
    pub end: usize,
}

impl SerializedSpan {
    pub fn serialize(span: &Span) -> Self {
        let (start, end) = span.line_and_column();
        SerializedSpan {
            file_path: span.file.path().to_string(),
            start_line: start.0,
            start: start.1,
            end_line: end.0,
            end: end.1,
        }
    }
}
