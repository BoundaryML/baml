use baml_base::Span;

/// Parse errors that can occur during parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedToken {
        expected: String,
        found: String,
        span: Span,
    },
    UnexpectedEof {
        expected: String,
        span: Span,
    },
    /// A syntax hint with a custom message (not using "Expected/found" format)
    InvalidSyntax {
        message: String,
        span: Span,
    },
}
