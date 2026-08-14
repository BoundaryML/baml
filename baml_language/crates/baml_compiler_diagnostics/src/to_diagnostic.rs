//! Implementation of `ToDiagnostic` for all BAML error types.
//!
//! This module provides conversions from the various compiler error types
//! to the unified `Diagnostic` type.

use baml_base::Span;

use crate::{
    diagnostic::{Diagnostic, DiagnosticId, DiagnosticPhase, ToDiagnostic},
    errors::{ErrorContext, NameError, ParseError, TypeError},
    message::{DiagnosticIdentifierKind, DiagnosticText},
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
        };
        diag.with_phase(DiagnosticPhase::Parse)
    }
}

// ============================================================================
// TypeError
// ============================================================================

impl<C: ErrorContext> TypeError<C> {
    /// Convert this type error to a `Diagnostic`.
    ///
    /// Takes mapping functions to resolve types and locations from the
    /// error's context to displayable strings and spans.
    pub fn to_diagnostic(
        &self,
        ty_fn: impl Fn(&C::Ty) -> String,
        loc_fn: impl Fn(&C::Location) -> Span,
    ) -> Diagnostic {
        let diag = match self {
            TypeError::TypeMismatch {
                expected,
                found,
                location,
                info_location,
            } => {
                let message = DiagnosticText::new()
                    .text("expected ")
                    .type_expr(ty_fn(expected))
                    .text(", found ")
                    .type_expr(ty_fn(found));
                let diag = Diagnostic::error(DiagnosticId::TypeMismatch, "mismatched types")
                    .with_primary(loc_fn(location), message);
                if let Some(info_location) = info_location {
                    diag.with_secondary(loc_fn(info_location), "expected due to this")
                } else {
                    diag
                }
            }
            TypeError::UnknownType { name, location } => Diagnostic::error(
                DiagnosticId::UnknownType,
                DiagnosticText::new().text("unknown type ").identifier(
                    name,
                    DiagnosticIdentifierKind::Type,
                ),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::UnknownVariable { name, location } => Diagnostic::error(
                DiagnosticId::UnknownVariable,
                DiagnosticText::new()
                    .text("unknown variable ")
                    .identifier(name, DiagnosticIdentifierKind::Variable),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::InvalidBinaryOp {
                op,
                lhs,
                rhs,
                location,
            } => {
                let message = DiagnosticText::new()
                    .text("cannot apply operator ")
                    .code(op)
                    .text(" to types ")
                    .type_expr(ty_fn(lhs))
                    .text(" and ")
                    .type_expr(ty_fn(rhs));
                Diagnostic::error(DiagnosticId::InvalidOperator, message)
                    .with_primary_span(loc_fn(location))
            }

            TypeError::InvalidUnaryOp {
                op,
                operand,
                location,
            } => {
                let message = DiagnosticText::new()
                    .text("cannot apply operator ")
                    .code(op)
                    .text(" to type ")
                    .type_expr(ty_fn(operand));
                Diagnostic::error(DiagnosticId::InvalidOperator, message)
                    .with_primary_span(loc_fn(location))
            }

            TypeError::ArgumentCountMismatch {
                expected,
                found,
                location,
            } => Diagnostic::error(
                DiagnosticId::ArgumentCountMismatch,
                format!("expected {expected} arguments, found {found}"),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::NotCallable { ty, location } => {
                let message = DiagnosticText::new()
                    .text("type ")
                    .type_expr(ty_fn(ty))
                    .text(" is not callable");
                Diagnostic::error(DiagnosticId::NotCallable, message)
                    .with_primary_span(loc_fn(location))
            }

            TypeError::NoSuchField {
                ty,
                field,
                location,
            } => {
                let message = DiagnosticText::new()
                    .text("type ")
                    .type_expr(ty_fn(ty))
                    .text(" has no field ")
                    .identifier(field, DiagnosticIdentifierKind::Field);
                Diagnostic::error(DiagnosticId::NoSuchField, message)
                    .with_primary_span(loc_fn(location))
            }

            TypeError::NotIndexable { ty, location } => {
                let message = DiagnosticText::new()
                    .text("type ")
                    .type_expr(ty_fn(ty))
                    .text(" is not indexable");
                Diagnostic::error(DiagnosticId::NotIndexable, message)
                    .with_primary_span(loc_fn(location))
            }

            TypeError::NonExhaustiveMatch {
                scrutinee_type,
                missing_cases,
                location,
            } => {
                let missing = missing_cases.join(", ");
                Diagnostic::error(
                    DiagnosticId::NonExhaustiveMatch,
                    format!(
                        "non-exhaustive match on `{}`: missing cases {missing}",
                        ty_fn(scrutinee_type)
                    ),
                )
                .with_primary_span(loc_fn(location))
            }

            TypeError::UnreachableArm { location } => {
                Diagnostic::error(DiagnosticId::UnreachableArm, "unreachable match arm")
                    .with_primary_span(loc_fn(location))
            }

            TypeError::UnreachableCatchArm { location } => {
                Diagnostic::warning(DiagnosticId::UnreachableCatchArm, "unreachable catch arm")
                    .with_primary_span(loc_fn(location))
            }

            TypeError::UnknownEnumVariant {
                enum_name,
                variant_name,
                location,
            } => {
                let message = DiagnosticText::new()
                    .text("unknown variant ")
                    .identifier(variant_name, DiagnosticIdentifierKind::EnumVariant)
                    .text(" for enum ")
                    .identifier(enum_name, DiagnosticIdentifierKind::Type);
                Diagnostic::error(DiagnosticId::UnknownEnumVariant, message)
                    .with_primary_span(loc_fn(location))
            }

            TypeError::MissingReturnExpression { expected, location } => {
                let label = DiagnosticText::new()
                    .text("expected return value of type ")
                    .type_expr(ty_fn(expected));
                Diagnostic::error(
                    DiagnosticId::MissingReturnExpression,
                    "missing return expression",
                )
                .with_primary(loc_fn(location), label)
            }

            TypeError::NonExhaustiveCatch {
                unhandled_types,
                location,
            } => {
                let unhandled = unhandled_types.join(", ");
                Diagnostic::error(
                    DiagnosticId::NonExhaustiveCatch,
                    format!("non-exhaustive catch chain: unhandled throw types {unhandled}"),
                )
                .with_primary_span(loc_fn(location))
            }

            TypeError::ThrowsContractViolation {
                extra_types,
                location,
            } => {
                let extras = extra_types.join(", ");
                Diagnostic::error(
                    DiagnosticId::ThrowsContractViolation,
                    format!("function throws types not covered by `throws` declaration: {extras}"),
                )
                .with_primary_span(loc_fn(location))
            }

            TypeError::ThrowsContractExtraneous {
                unused_types,
                location,
            } => {
                let unused = unused_types.join(", ");
                Diagnostic::warning(
                    DiagnosticId::ThrowsContractExtraneous,
                    format!("`throws` declaration includes types the function never throws: {unused}"),
                )
                .with_primary_span(loc_fn(location))
            }

            TypeError::InvalidMapKeyType { ty, location } => Diagnostic::error(
                DiagnosticId::InvalidMapKeyType,
                format!(
                    "invalid type {} for map key. Only strings, string literals and enums are valid map keys.",
                    ty_fn(ty)
                )
            ).with_primary_span(loc_fn(location)),

            TypeError::AliasCycle { cycle_path, location } => Diagnostic::error(
                DiagnosticId::AliasCycle,
                format!("these aliases form a dependency cycle: {cycle_path}"),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::ClassCycle { cycle_path, location } => Diagnostic::error(
                DiagnosticId::ClassCycle,
                format!("these classes form a dependency cycle: {cycle_path}"),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::InvalidCatchBindingType {
                type_name,
                location,
            } => Diagnostic::error(
                DiagnosticId::InvalidCatchBindingType,
                format!("type `{type_name}` is not allowed in catch bindings"),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::InstanceofRemoved { location } => Diagnostic::error(
                DiagnosticId::InstanceofRemoved,
                "`instanceof` is no longer supported. Use a `match` expression for type checking instead.".to_string(),
            )
            .with_primary_span(loc_fn(location)),
        };
        diag.with_phase(DiagnosticPhase::Type)
    }
}

// ============================================================================
// NameError
// ============================================================================

impl ToDiagnostic for NameError {
    fn to_diagnostic(&self) -> Diagnostic {
        let diag = match self {
            NameError::DuplicateName {
                name,
                kind,
                first,
                first_path,
                second,
                second_path: _,
            } => Diagnostic::error(
                DiagnosticId::DuplicateName,
                format!("duplicate {kind} `{name}`"),
            )
            .with_primary(*second, format!("{kind} `{name}` redefined here"))
            .with_secondary(*first, format!("`{name}` first defined in {first_path}")),

            NameError::DuplicateTestForFunction {
                test_name,
                function_name,
                first,
                first_path,
                second,
                second_path: _,
            } => Diagnostic::error(
                DiagnosticId::DuplicateName,
                format!("duplicate test `{test_name}` for function `{function_name}`"),
            )
            .with_primary(
                *second,
                format!("test `{test_name}` for `{function_name}` redefined here"),
            )
            .with_secondary(
                *first,
                format!("test `{test_name}` for `{function_name}` first defined in {first_path}"),
            ),

            NameError::UnknownFunctionInTest {
                function_name,
                span,
            } => Diagnostic::error(
                DiagnosticId::UnknownFunctionInTest,
                format!("unknown function `{function_name}` in test block"),
            )
            .with_primary(*span, format!("no function named `{function_name}` exists")),
        };
        diag.with_phase(DiagnosticPhase::Validation)
    }
}

#[cfg(test)]
mod tests {
    use baml_base::Span;
    use text_size::TextRange;

    use super::*;
    use crate::{diagnostic::DiagnosticPhase, errors::SpanContext};

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

    #[test]
    fn test_type_error_to_diagnostic() {
        let error: TypeError<SpanContext> = TypeError::TypeMismatch {
            expected: "int".to_string(),
            found: "string".to_string(),
            location: test_span(),
            info_location: None,
        };

        let diag = error.to_diagnostic(Clone::clone, |s| *s);
        assert_eq!(diag.code(), "E0001");
        assert_eq!(diag.message, "mismatched types");
        let label = diag.annotations[0].message.as_deref().unwrap();
        assert!(label.contains("int"));
        assert!(label.contains("string"));
        assert_eq!(diag.phase, DiagnosticPhase::Type);
    }

    #[test]
    fn test_type_error_with_info_location_to_diagnostic() {
        let error: TypeError<SpanContext> = TypeError::TypeMismatch {
            expected: "int".to_string(),
            found: "string".to_string(),
            location: Span {
                file_id: baml_base::FileId::new(0),
                range: TextRange::new(20.into(), 30.into()),
            },
            info_location: Some(test_span()),
        };

        let diag = error.to_diagnostic(Clone::clone, |s| *s);
        assert_eq!(diag.code(), "E0001");
        assert_eq!(diag.annotations.len(), 2); // primary + info span
        assert!(diag.related_info.is_empty());
        assert_eq!(diag.phase, DiagnosticPhase::Type);
    }

    #[test]
    fn test_name_error_to_diagnostic() {
        let first_span = test_span();
        let second_span = Span {
            file_id: baml_base::FileId::new(1),
            range: TextRange::new(20.into(), 30.into()),
        };

        let error = NameError::DuplicateName {
            name: "Foo".to_string(),
            kind: "class",
            first: first_span,
            first_path: "first.baml".to_string(),
            second: second_span,
            second_path: "second.baml".to_string(),
        };

        let diag = error.to_diagnostic();
        assert_eq!(diag.code(), "E0011");
        assert!(diag.message.contains("duplicate"));
        assert_eq!(diag.annotations.len(), 2); // primary + secondary
        assert_eq!(diag.phase, DiagnosticPhase::Validation);
    }
}
