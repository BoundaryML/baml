//! Diagnostics the parser produces.

use std::fmt;

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

impl fmt::Display for ParseError {
    /// The message alone; the range is [`ParseError::range`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedToken {
                expected, found, ..
            } => write!(f, "expected {expected}, found {found}"),
            Self::UnexpectedEof { expected, .. } => write!(f, "expected {expected}, found EOF"),
            Self::InvalidSyntax { message, .. }
            | Self::RemovedFeature { message, .. }
            | Self::InvalidDirective { message, .. } => f.write_str(message),
            Self::InvalidStringEscape { issue, .. } => write!(f, "{issue}"),
            Self::AmbiguousUnion { groupings, .. } => {
                f.write_str("ambiguous union")?;
                for grouping in groupings {
                    write!(f, "; {grouping}")?;
                }
                Ok(())
            }
        }
    }
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
