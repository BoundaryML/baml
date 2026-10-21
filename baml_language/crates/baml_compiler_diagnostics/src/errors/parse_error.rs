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
        /// Two explicitly parenthesized spellings of the expression that group
        /// this pipe differently; empty when none could be derived.
        groupings: Vec<String>,
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

    /// Pipe label for [`ParseError::AmbiguousUnion`]; one line so concise
    /// renderers stay readable.
    pub fn ambiguous_union_label(groupings: &[String]) -> String {
        let explicit = "use parentheses to make the intended grouping explicit";
        if groupings.is_empty() {
            explicit.to_string()
        } else {
            let options = groupings
                .iter()
                .map(|grouping| format!("`{grouping}`"))
                .collect::<Vec<_>>()
                .join(" or ");
            format!("{explicit}: {options}")
        }
    }
}
