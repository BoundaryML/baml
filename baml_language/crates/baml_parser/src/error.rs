//! Diagnostics the parser produces.

use baml_base::escape::EscapeIssue;
use text_size::TextRange;

/// A syntax error. Ranges are offsets into the parsed text; the layer that
/// knows which file this was adds the file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParseError {
    /// A required token or construct was not found where the grammar needs it.
    UnexpectedToken {
        expected: String,
        found: String,
        range: TextRange,
    },
    /// The input ended where the grammar needs more.
    UnexpectedEof { expected: String, range: TextRange },
    /// A construct that parses but is not allowed, with a message saying why.
    InvalidSyntax { message: String, range: TextRange },
    /// A `|` that can belong to a function's return/throws union or join two
    /// functions (E0175).
    AmbiguousUnion {
        range: TextRange,
        /// Two explicitly parenthesized spellings of the expression that group
        /// this pipe differently; empty when none could be derived.
        groupings: Vec<String>,
    },
    /// Use of a removed language feature (E0098).
    RemovedFeature { message: String, range: TextRange },
    /// An escape sequence in a string literal that does not decode (E0109).
    /// `range` covers the sequence; the literal is read with it verbatim.
    InvalidStringEscape {
        issue: EscapeIssue,
        range: TextRange,
    },
    /// A `//baml:` directive or `/// baml:$trace` hook that names no
    /// directive, has no hook path, repeats a hook, or precedes a declaration
    /// that takes none (E0105).
    InvalidDirective { message: String, range: TextRange },
}

impl ParseError {
    pub fn range(&self) -> TextRange {
        match self {
            Self::UnexpectedToken { range, .. }
            | Self::UnexpectedEof { range, .. }
            | Self::InvalidSyntax { range, .. }
            | Self::AmbiguousUnion { range, .. }
            | Self::RemovedFeature { range, .. }
            | Self::InvalidStringEscape { range, .. }
            | Self::InvalidDirective { range, .. } => *range,
        }
    }
}
