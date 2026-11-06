//! Implementation of `ToDiagnostic` for all BAML error types.
//!
//! This module provides conversions from the various compiler error types
//! to the unified `Diagnostic` type.

use crate::{
    diagnostic::{Diagnostic, DiagnosticId, DiagnosticPhase, ToDiagnostic},
    errors::ParseError,
    message::DiagnosticText,
};

// ============================================================================
// ParseError
// ============================================================================

impl ToDiagnostic for ParseError {
    fn to_diagnostic(&self) -> Diagnostic {
        let diag = match self {
            ParseError::UnexpectedToken {
                expected,
                found,
                span,
            } => {
                let label = DiagnosticText::new()
                    .text("expected ")
                    .code(expected)
                    .text(", found ")
                    .code(found);
                Diagnostic::error(DiagnosticId::UnexpectedToken, "unexpected token")
                    .with_primary(*span, label)
            }

            ParseError::UnexpectedEof { expected, span } => {
                let label = DiagnosticText::new()
                    .text("expected ")
                    .code(expected)
                    .text(", found end of file");
                Diagnostic::error(DiagnosticId::UnexpectedEof, "unexpected end of file")
                    .with_primary(*span, label)
            }

            ParseError::InvalidSyntax { message, span } => {
                Diagnostic::error(DiagnosticId::InvalidSyntax, "invalid syntax")
                    .with_primary(*span, message.clone())
            }

            ParseError::AmbiguousUnion { span, groupings } => Diagnostic::error(
                DiagnosticId::AmbiguousUnion,
                ParseError::AMBIGUOUS_UNION_MESSAGE,
            )
            .with_primary(*span, ParseError::ambiguous_union_label(groupings)),

            ParseError::RemovedFeature { message, span } => {
                Diagnostic::error(DiagnosticId::RemovedFeature, "removed language feature")
                    .with_primary(*span, message.clone())
            }
        };
        diag.with_phase(DiagnosticPhase::Parse)
    }
}

#[cfg(test)]
mod tests {
    use baml_base::Span;
    use text_size::TextRange;

    use super::*;
    use crate::diagnostic::DiagnosticPhase;

    fn test_span() -> Span {
        Span {
            file_id: baml_base::FileId::new(0),
            range: TextRange::new(0.into(), 10.into()),
        }
    }

    #[test]
    fn test_parse_error_to_diagnostic() {
        let error = ParseError::UnexpectedToken {
            expected: "identifier".to_string(),
            found: "'{'".to_string(),
            span: test_span(),
        };

        let diag = error.to_diagnostic();
        assert_eq!(diag.code(), "E0010");
        assert_eq!(diag.message, "unexpected token");
        assert!(
            diag.annotations[0]
                .message
                .as_deref()
                .is_some_and(|message| message.contains("expected"))
        );
        assert_eq!(diag.phase, DiagnosticPhase::Parse);
    }
}
