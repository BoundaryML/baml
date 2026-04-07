//! Implementation of `ToDiagnostic` for all BAML error types.
//!
//! This module provides conversions from the various compiler error types
//! to the unified `Diagnostic` type.

use baml_base::Span;

use crate::{
    diagnostic::{Diagnostic, DiagnosticId, DiagnosticPhase, ToDiagnostic},
    errors::{ErrorContext, NameError, ParseError, TypeError},
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
            } => Diagnostic::error(
                DiagnosticId::UnexpectedToken,
                format!("Expected {expected}, found {found}"),
            )
            .with_primary_span(*span),

            ParseError::UnexpectedEof { expected, span } => Diagnostic::error(
                DiagnosticId::UnexpectedEof,
                format!("Expected {expected}, found EOF"),
            )
            .with_primary_span(*span),

            ParseError::InvalidSyntax { message, span } => {
                Diagnostic::error(DiagnosticId::InvalidSyntax, message.clone())
                    .with_primary_span(*span)
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
                let diag = Diagnostic::error(
                    DiagnosticId::TypeMismatch,
                    format!("Expected `{}`, found `{}`", ty_fn(expected), ty_fn(found)),
                )
                .with_primary_span(loc_fn(location));
                if let Some(info_location) = info_location {
                    diag.with_secondary(loc_fn(info_location), "Type required here")
                } else {
                    diag
                }
            }
            TypeError::UnknownType { name, location } => {
                Diagnostic::error(DiagnosticId::UnknownType, format!("Unknown type `{name}`"))
                    .with_primary_span(loc_fn(location))
            }

            TypeError::UnknownVariable { name, location } => Diagnostic::error(
                DiagnosticId::UnknownVariable,
                format!("Unknown variable `{name}`"),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::InvalidBinaryOp {
                op,
                lhs,
                rhs,
                location,
            } => Diagnostic::error(
                DiagnosticId::InvalidOperator,
                format!(
                    "Cannot apply operator '{op}' to types `{}` and `{}`",
                    ty_fn(lhs),
                    ty_fn(rhs)
                ),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::InvalidUnaryOp {
                op,
                operand,
                location,
            } => Diagnostic::error(
                DiagnosticId::InvalidOperator,
                format!("Cannot apply operator '{op}' to type `{}`", ty_fn(operand)),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::ArgumentCountMismatch {
                expected,
                found,
                location,
            } => Diagnostic::error(
                DiagnosticId::ArgumentCountMismatch,
                format!("Expected {expected} arguments, found {found}"),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::NotCallable { ty, location } => Diagnostic::error(
                DiagnosticId::NotCallable,
                format!("Type `{}` is not callable", ty_fn(ty)),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::NoSuchField {
                ty,
                field,
                location,
            } => Diagnostic::error(
                DiagnosticId::NoSuchField,
                format!("Type `{}` has no field `{field}`", ty_fn(ty)),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::NotIndexable { ty, location } => Diagnostic::error(
                DiagnosticId::NotIndexable,
                format!("Type `{}` is not indexable", ty_fn(ty)),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::NonExhaustiveMatch {
                scrutinee_type,
                missing_cases,
                location,
            } => {
                let missing = missing_cases.join(", ");
                Diagnostic::error(
                    DiagnosticId::NonExhaustiveMatch,
                    format!(
                        "Non-exhaustive match on `{}`: missing cases {missing}",
                        ty_fn(scrutinee_type)
                    ),
                )
                .with_primary_span(loc_fn(location))
            }

            TypeError::UnreachableArm { location } => {
                Diagnostic::error(DiagnosticId::UnreachableArm, "Unreachable match arm")
                    .with_primary_span(loc_fn(location))
            }

            TypeError::UnreachableCatchArm { location } => {
                Diagnostic::warning(DiagnosticId::UnreachableCatchArm, "Unreachable catch arm")
                    .with_primary_span(loc_fn(location))
            }

            TypeError::UnknownEnumVariant {
                enum_name,
                variant_name,
                location,
            } => Diagnostic::error(
                DiagnosticId::UnknownEnumVariant,
                format!("Unknown variant `{variant_name}` for enum `{enum_name}`"),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::WatchOnNonVariable { location } => Diagnostic::error(
                DiagnosticId::WatchOnNonVariable,
                "$watch can only be used on simple variable expressions",
            )
            .with_primary_span(loc_fn(location)),

            TypeError::WatchOnUnwatchedVariable { name, location } => Diagnostic::error(
                DiagnosticId::WatchOnUnwatchedVariable,
                format!(
                    "Cannot use $watch on `{name}`: variable must be declared with `watch let`"
                ),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::MissingReturnExpression { expected, location } => Diagnostic::error(
                DiagnosticId::MissingReturnExpression,
                format!(
                    "Missing return expression. Function expects `{}` but body has no final expression.",
                    ty_fn(expected)
                ),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::NonExhaustiveCatch {
                unhandled_types,
                location,
            } => {
                let unhandled = unhandled_types.join(", ");
                Diagnostic::error(
                    DiagnosticId::NonExhaustiveCatch,
                    format!("Non-exhaustive catch chain: unhandled throw types {unhandled}"),
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
                    format!("Function throws types not covered by `throws` declaration: {extras}"),
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
                    "Invalid type {} for map key. Only strings, string literals and enums are valid map keys.",
                    ty_fn(ty)
                )
            ).with_primary_span(loc_fn(location)),

            TypeError::AliasCycle { cycle_path, location } => Diagnostic::error(
                DiagnosticId::AliasCycle,
                format!("These aliases form a dependency cycle: {cycle_path}"),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::ClassCycle { cycle_path, location } => Diagnostic::error(
                DiagnosticId::ClassCycle,
                format!("These classes form a dependency cycle: {cycle_path}"),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::JinjaUnresolvedVariable {
                name,
                suggestions,
                location,
            } => {
                let message = if suggestions.is_empty() {
                    format!("Variable `{name}` does not exist.")
                } else if suggestions.len() == 1 {
                    format!(
                        "Variable `{name}` does not exist. Did you mean `{}`?",
                        suggestions[0]
                    )
                } else {
                    format!(
                        "Variable `{name}` does not exist. Did you mean one of these: `{}`?",
                        suggestions.join("`, `")
                    )
                };
                Diagnostic::warning(DiagnosticId::JinjaUnresolvedVariable, message)
                    .with_primary_span(loc_fn(location))
            }

            TypeError::JinjaFunctionReferenceWithoutCall {
                function_name,
                location,
            } => Diagnostic::warning(
                DiagnosticId::JinjaFunctionReferenceWithoutCall,
                format!(
                    "Function '{function_name}' referenced without parentheses. Did you mean '{function_name}()'?"
                ),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::JinjaInvalidFilter {
                filter_name,
                suggestions,
                location,
            } => {
                let message = if suggestions.is_empty() {
                    format!("Filter '{filter_name}' does not exist")
                } else if suggestions.len() == 1 {
                    format!(
                        "Filter '{filter_name}' does not exist. Did you mean '{}'?",
                        suggestions[0]
                    )
                } else {
                    format!(
                        "Filter '{filter_name}' does not exist. Did you mean one of these: '{}'?",
                        suggestions.join("', '")
                    )
                };
                Diagnostic::warning(
                    DiagnosticId::JinjaInvalidFilter,
                    format!(
                        "{message}\n\nSee: https://docs.rs/minijinja/latest/minijinja/filters/index.html#functions"
                    ),
                )
                .with_primary_span(loc_fn(location))
            }

            TypeError::JinjaInvalidType {
                expression,
                expected,
                found,
                location,
            } => {
                let found_desc = if found == "undefined" {
                    "undefined".to_string()
                } else {
                    format!("a {found}")
                };
                Diagnostic::warning(
                    DiagnosticId::JinjaInvalidType,
                    format!("'{expression}' is {found_desc}, expected {expected}"),
                )
                .with_primary_span(loc_fn(location))
            }

            TypeError::JinjaPropertyNotDefined {
                variable,
                class_name,
                property,
                location,
            } => Diagnostic::warning(
                DiagnosticId::JinjaPropertyNotDefined,
                format!("class {class_name} ({variable}) does not have a property '{property}'"),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::JinjaEnumValuePropertyAccess {
                variable,
                enum_value,
                property,
                location,
            } => Diagnostic::warning(
                DiagnosticId::JinjaEnumValuePropertyAccess,
                format!(
                    "enum value {enum_value} ({variable}) does not have a property '{property}'"
                ),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::JinjaEnumStringComparison { enum_name, location } => Diagnostic::warning(
                DiagnosticId::JinjaEnumStringComparison,
                format!(
                    "Comparing enum {enum_name} to string - enum-string comparisons will soon be deprecated. Please see https://github.com/BoundaryML/baml/issues/2339."
                ),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::JinjaPropertyNotFoundInUnion {
                property,
                missing_on,
                location,
            } => {
                let classes_str = missing_on.join(", ");
                Diagnostic::warning(
                    DiagnosticId::JinjaPropertyNotFoundInUnion,
                    format!("property '{property}' does not exist on {classes_str}"),
                )
                .with_primary_span(loc_fn(location))
            }

            TypeError::JinjaPropertyTypeMismatchInUnion { property, location } => {
                Diagnostic::warning(
                    DiagnosticId::JinjaPropertyTypeMismatchInUnion,
                    format!(
                        "property '{property}' has inconsistent types across union members"
                    ),
                )
                .with_primary_span(loc_fn(location))
            }

            TypeError::JinjaNonClassInUnion {
                variable,
                property,
                non_class_type,
                location,
            } => Diagnostic::warning(
                DiagnosticId::JinjaNonClassInUnion,
                format!(
                    "cannot access property '{property}' on '{variable}': union contains non-class type {non_class_type}"
                ),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::JinjaWrongArgCount {
                function_name,
                expected,
                found,
                location,
            } => Diagnostic::warning(
                DiagnosticId::JinjaWrongArgCount,
                format!(
                    "Function '{function_name}' expects {expected} arguments, but got {found}"
                ),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::JinjaMissingArg {
                function_name,
                arg_name,
                location,
            } => Diagnostic::warning(
                DiagnosticId::JinjaMissingArg,
                format!("Function '{function_name}' expects argument '{arg_name}'"),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::JinjaUnknownArg {
                function_name,
                arg_name,
                suggestions,
                location,
            } => {
                let message = if suggestions.is_empty() {
                    format!(
                        "Function '{function_name}' does not have an argument '{arg_name}'"
                    )
                } else if suggestions.len() == 1 {
                    format!(
                        "Function '{function_name}' does not have an argument '{arg_name}'. Did you mean '{}'?",
                        suggestions[0]
                    )
                } else {
                    format!(
                        "Function '{function_name}' does not have an argument '{arg_name}'. Did you mean one of these: '{}'?",
                        suggestions.join("', '")
                    )
                };
                Diagnostic::warning(DiagnosticId::JinjaUnknownArg, message)
                    .with_primary_span(loc_fn(location))
            }

            TypeError::JinjaWrongArgType {
                function_name,
                arg_name,
                expected,
                found,
                location,
            } => Diagnostic::warning(
                DiagnosticId::JinjaWrongArgType,
                format!(
                    "Function '{function_name}' expects argument '{arg_name}' to be of type {expected}, but got {found}"
                ),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::JinjaParseError { message, location } => {
                Diagnostic::warning(
                    DiagnosticId::JinjaParseError,
                    format!("Failed to parse Jinja template: {message}"),
                )
                .with_primary_span(loc_fn(location))
            }

            TypeError::JinjaUnsupportedFeature { feature, location } => Diagnostic::warning(
                DiagnosticId::JinjaUnsupportedFeature,
                format!("{feature} are not yet supported"),
            )
            .with_primary_span(loc_fn(location)),

            TypeError::JinjaInvalidSyntax { message, location } => {
                Diagnostic::warning(DiagnosticId::JinjaInvalidSyntax, message.clone())
                    .with_primary_span(loc_fn(location))
            }

            TypeError::JinjaInvalidTest {
                test_name,
                suggestions,
                location,
            } => {
                let msg = if suggestions.is_empty() {
                    format!("unknown test '{test_name}'")
                } else {
                    format!(
                        "unknown test '{test_name}'. Valid tests: {}",
                        suggestions.join(", ")
                    )
                };
                Diagnostic::warning(DiagnosticId::JinjaInvalidTest, msg)
                    .with_primary_span(loc_fn(location))
            }

            TypeError::InvalidCatchBindingType {
                type_name,
                location,
            } => Diagnostic::error(
                DiagnosticId::InvalidCatchBindingType,
                format!("Type `{type_name}` is not allowed in catch bindings"),
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
                format!("Duplicate {kind} `{name}`"),
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
                format!("Duplicate test `{test_name}` for function `{function_name}`"),
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
                format!("Unknown function `{function_name}` in test block"),
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
        assert!(diag.message.contains("Expected"));
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
        assert!(diag.message.contains("int"));
        assert!(diag.message.contains("string"));
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
        assert!(diag.message.contains("Duplicate"));
        assert_eq!(diag.annotations.len(), 2); // primary + secondary
        assert_eq!(diag.phase, DiagnosticPhase::Validation);
    }
}
