//! Parsing BAML source into Rowan syntax trees.
//!
//! Implements incremental parsing with error recovery.

use baml_base::{SourceFile, Span};
use baml_lexer::lex_file;
use baml_syntax::SyntaxNode;
use rowan::GreenNode;

mod parser;
pub use parser::{parse_file, parse_file_with_cache};

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
    // Add more variants as needed
}

impl baml_base::Diagnostic for ParseError {
    fn message(&self) -> String {
        match self {
            ParseError::UnexpectedToken {
                expected, found, ..
            } => {
                format!("Expected {expected}, found {found}")
            }
            ParseError::UnexpectedEof { expected, .. } => {
                format!("Unexpected end of file, expected {expected}")
            }
        }
    }

    fn span(&self) -> Option<Span> {
        match self {
            ParseError::UnexpectedToken { span, .. } | ParseError::UnexpectedEof { span, .. } => {
                Some(*span)
            }
        }
    }

    fn severity(&self) -> baml_base::Severity {
        baml_base::Severity::Error
    }
}

/// Tracked: parse file into green tree (immutable, position-independent)
#[salsa::tracked]
pub fn parse_green(db: &dyn salsa::Database, file: SourceFile) -> GreenNode {
    let tokens = lex_file(db, file);
    let (green, _errors) = parse_file(&tokens);
    green
}

/// Tracked: get parse errors for a file
#[salsa::tracked]
pub fn parse_errors(db: &dyn salsa::Database, file: SourceFile) -> Vec<ParseError> {
    let tokens = lex_file(db, file);
    let (_green, errors) = parse_file(&tokens);
    errors
}

/// Helper to build a red tree from the green tree.
pub fn syntax_tree(db: &dyn salsa::Database, file: SourceFile) -> SyntaxNode {
    let green = parse_green(db, file);
    SyntaxNode::new_root(green)
}
