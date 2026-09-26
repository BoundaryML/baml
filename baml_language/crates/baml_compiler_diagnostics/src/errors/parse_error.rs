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
    /// A pipe can belong to a function's return/throws union or join two functions.
    AmbiguousUnion {
        span: Span,
    },
    /// Use of a removed language feature (E0098), e.g. legacy `type_builder`
    /// blocks or `dynamic class`/`dynamic enum` definitions (BEP-066).
    RemovedFeature {
        message: String,
        span: Span,
    },
}

impl ParseError {
    /// Headline for [`ParseError::AmbiguousUnion`].
    pub const AMBIGUOUS_UNION_MESSAGE: &str = "ambiguous union";
    /// Pipe label for [`ParseError::AmbiguousUnion`]; one line so concise renderers stay readable.
    pub const AMBIGUOUS_UNION_LABEL: &str = "use parentheses to make the intended grouping explicit, e.g. `((A) -> B) | ((C) -> D)` or `(A) -> (B | (C) -> D)`";
}
