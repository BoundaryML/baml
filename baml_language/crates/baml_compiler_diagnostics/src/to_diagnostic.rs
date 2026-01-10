//! Implementation of `ToDiagnostic` for all BAML error types.
//!
//! This module provides conversions from the various compiler error types
//! to the unified `Diagnostic` type.

use std::fmt::Write;

use crate::{
    diagnostic::{Diagnostic, DiagnosticId, DiagnosticPhase, ToDiagnostic},
    errors::{HirDiagnostic, NameError, ParseError, TypeError},
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

impl<T: std::fmt::Display> ToDiagnostic for TypeError<T> {
    fn to_diagnostic(&self) -> Diagnostic {
        let diag = match self {
            TypeError::TypeMismatch {
                expected,
                found,
                span,
                info_span,
            } => {
                let diag = Diagnostic::error(
                    DiagnosticId::TypeMismatch,
                    format!("Expected `{expected}`, found `{found}`"),
                )
                .with_primary_span(*span);
                if let Some(info_span) = info_span {
                    diag.with_related(*info_span, "Type defined here")
                } else {
                    diag
                }
            }
            TypeError::UnknownType { name, span } => {
                Diagnostic::error(DiagnosticId::UnknownType, format!("Unknown type `{name}`"))
                    .with_primary_span(*span)
            }

            TypeError::UnknownVariable { name, span } => Diagnostic::error(
                DiagnosticId::UnknownVariable,
                format!("Unknown variable `{name}`"),
            )
            .with_primary_span(*span),

            TypeError::InvalidBinaryOp { op, lhs, rhs, span } => Diagnostic::error(
                DiagnosticId::InvalidOperator,
                format!("Cannot apply operator '{op}' to types `{lhs}` and `{rhs}`"),
            )
            .with_primary_span(*span),

            TypeError::InvalidUnaryOp { op, operand, span } => Diagnostic::error(
                DiagnosticId::InvalidOperator,
                format!("Cannot apply operator '{op}' to type `{operand}`"),
            )
            .with_primary_span(*span),

            TypeError::ArgumentCountMismatch {
                expected,
                found,
                span,
            } => Diagnostic::error(
                DiagnosticId::ArgumentCountMismatch,
                format!("Expected {expected} arguments, found {found}"),
            )
            .with_primary_span(*span),

            TypeError::NotCallable { ty, span } => Diagnostic::error(
                DiagnosticId::NotCallable,
                format!("Type `{ty}` is not callable"),
            )
            .with_primary_span(*span),

            TypeError::NoSuchField { ty, field, span } => Diagnostic::error(
                DiagnosticId::NoSuchField,
                format!("Type `{ty}` has no field `{field}`"),
            )
            .with_primary_span(*span),

            TypeError::NotIndexable { ty, span } => Diagnostic::error(
                DiagnosticId::NotIndexable,
                format!("Type `{ty}` is not indexable"),
            )
            .with_primary_span(*span),

            TypeError::NonExhaustiveMatch {
                scrutinee_type,
                missing_cases,
                span,
            } => {
                let missing = missing_cases.join(", ");
                Diagnostic::error(
                    DiagnosticId::NonExhaustiveMatch,
                    format!("Non-exhaustive match on `{scrutinee_type}`: missing cases {missing}"),
                )
                .with_primary_span(*span)
            }

            TypeError::UnreachableArm { span } => {
                Diagnostic::error(DiagnosticId::UnreachableArm, "Unreachable match arm")
                    .with_primary_span(*span)
            }

            TypeError::UnknownEnumVariant {
                enum_name,
                variant_name,
                span,
            } => Diagnostic::error(
                DiagnosticId::UnknownEnumVariant,
                format!("Unknown variant `{variant_name}` for enum `{enum_name}`"),
            )
            .with_primary_span(*span),

            TypeError::WatchOnNonVariable { span } => Diagnostic::error(
                DiagnosticId::WatchOnNonVariable,
                "$watch can only be used on simple variable expressions",
            )
            .with_primary_span(*span),

            TypeError::WatchOnUnwatchedVariable { name, span } => Diagnostic::error(
                DiagnosticId::WatchOnUnwatchedVariable,
                format!(
                    "Cannot use $watch on `{name}`: variable must be declared with `watch let`"
                ),
            )
            .with_primary_span(*span),

            TypeError::MissingReturnExpression { expected, span } => Diagnostic::error(
                DiagnosticId::MissingReturnExpression,
                format!(
                    "Missing return expression. Function expects `{expected}` but body has no final expression."
                ),
            )
            .with_primary_span(*span),
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
        };
        diag.with_phase(DiagnosticPhase::Validation)
    }
}

// ============================================================================
// HirDiagnostic
// ============================================================================

impl ToDiagnostic for HirDiagnostic {
    fn to_diagnostic(&self) -> Diagnostic {
        let diag = match self {
            HirDiagnostic::DuplicateField {
                class_name,
                field_name,
                first_span,
                second_span,
            } => Diagnostic::error(
                DiagnosticId::DuplicateField,
                format!("Duplicate field `{field_name}` in class `{class_name}`"),
            )
            .with_primary(*second_span, "duplicate definition")
            .with_secondary(*first_span, "first definition here"),

            HirDiagnostic::DuplicateVariant {
                enum_name,
                variant_name,
                first_span,
                second_span,
            } => Diagnostic::error(
                DiagnosticId::DuplicateVariant,
                format!("Duplicate variant `{variant_name}` in enum `{enum_name}`"),
            )
            .with_primary(*second_span, "duplicate definition")
            .with_secondary(*first_span, "first definition here"),

            HirDiagnostic::DuplicateBlockAttribute {
                item_kind,
                item_name,
                attr_name,
                first_span,
                second_span,
            } => Diagnostic::error(
                DiagnosticId::DuplicateAttribute,
                format!(
                    "Attribute `@@{attr_name}` can only be defined once on {item_kind} `{item_name}`"
                ),
            )
            .with_primary(*second_span, "duplicate attribute")
            .with_secondary(*first_span, "first definition here"),

            HirDiagnostic::DuplicateFieldAttribute {
                container_kind,
                container_name,
                field_name,
                attr_name,
                first_span,
                second_span,
            } => Diagnostic::error(
                DiagnosticId::DuplicateAttribute,
                format!(
                    "Attribute `@{attr_name}` can only be defined once on field `{field_name}` in {container_kind} `{container_name}`"
                ),
            )
            .with_primary(*second_span, "duplicate attribute")
            .with_secondary(*first_span, "first definition here"),

            HirDiagnostic::UnknownAttribute {
                attr_name,
                span,
                valid_attributes,
            } => {
                let suggestions = if valid_attributes.is_empty() {
                    String::new()
                } else {
                    format!(". Valid attributes: {}", valid_attributes.join(", "))
                };
                Diagnostic::error(
                    DiagnosticId::UnknownAttribute,
                    format!("Unknown attribute `{attr_name}`{suggestions}"),
                )
                .with_primary_span(*span)
            }

            HirDiagnostic::InvalidAttributeContext {
                attr_name,
                context,
                allowed_contexts,
                span,
            } => Diagnostic::error(
                DiagnosticId::InvalidAttributeContext,
                format!(
                    "Attribute `{attr_name}` is not valid on {context}. Allowed on: {allowed_contexts}"
                ),
            )
            .with_primary_span(*span),

            HirDiagnostic::UnknownGeneratorProperty {
                generator_name,
                property_name,
                span,
                valid_properties,
            } => Diagnostic::error(
                DiagnosticId::UnknownGeneratorProperty,
                format!(
                    "Unknown property `{property_name}` in generator `{generator_name}`. Valid properties: {}",
                    valid_properties.join(", ")
                ),
            )
            .with_primary_span(*span),

            HirDiagnostic::MissingGeneratorProperty {
                generator_name,
                property_name,
                span,
            } => Diagnostic::error(
                DiagnosticId::MissingGeneratorProperty,
                format!(
                    "Generator `{generator_name}` is missing required property `{property_name}`"
                ),
            )
            .with_primary_span(*span),

            HirDiagnostic::InvalidGeneratorPropertyValue {
                generator_name,
                property_name,
                value,
                span,
                valid_values,
                help,
            } => {
                let mut msg = format!(
                    "Invalid value `{value}` for property `{property_name}` in generator `{generator_name}`"
                );
                if let Some(valid) = valid_values {
                    let _ = write!(msg, ". Valid values: {}", valid.join(", "));
                }
                if let Some(h) = help {
                    let _ = write!(msg, ". {h}");
                }
                Diagnostic::error(DiagnosticId::InvalidGeneratorPropertyValue, msg)
                    .with_primary_span(*span)
            }

            HirDiagnostic::ReservedFieldName {
                item_kind,
                item_name,
                field_name,
                span,
                target_languages,
            } => {
                let field_type = match *item_kind {
                    "class" => "Class field",
                    "enum" => "Enum value",
                    "function" => "Function parameter",
                    _ => "Field",
                };
                Diagnostic::error(
                    DiagnosticId::ReservedFieldName,
                    format!(
                        "{field_type} `{field_name}` in {item_kind} `{item_name}` is a reserved keyword in {}",
                        target_languages.join(", ")
                    ),
                )
                .with_primary_span(*span)
            }

            HirDiagnostic::FieldNameMatchesTypeName {
                class_name,
                field_name,
                type_name,
                span,
            } => Diagnostic::error(
                DiagnosticId::FieldNameMatchesTypeName,
                format!(
                    "Field `{field_name}` in class `{class_name}` has the same name as its type `{type_name}`, which is not supported in generated Python code."
                ),
            )
            .with_primary_span(*span),

            HirDiagnostic::InvalidClientResponseType {
                client_name: _,
                value,
                span,
                valid_values,
            } => Diagnostic::error(
                DiagnosticId::InvalidClientResponseType,
                format!(
                    "client_response_type must be one of {}. Got: {value}",
                    valid_values.join(", ")
                ),
            )
            .with_primary_span(*span),

            HirDiagnostic::HttpConfigNotBlock {
                client_name: _,
                span,
            } => Diagnostic::error(
                DiagnosticId::HttpConfigNotBlock,
                "http must be a configuration block with timeout settings",
            )
            .with_primary_span(*span),

            HirDiagnostic::UnknownHttpConfigField {
                client_name: _,
                field_name,
                span,
                suggestion,
                is_composite,
            } => {
                let valid_fields = if *is_composite {
                    "total_timeout_ms"
                } else {
                    "connect_timeout_ms, request_timeout_ms, time_to_first_token_timeout_ms, idle_timeout_ms"
                };

                let mut msg =
                    format!("Unrecognized field `{field_name}` in http configuration block.");

                if let Some(suggested) = suggestion {
                    let _ = write!(msg, " Did you mean `{suggested}`?");
                }

                if *is_composite {
                    let _ = write!(
                        msg,
                        " Composite clients (fallback/round-robin) only support: {valid_fields}"
                    );
                } else if field_name == "total_timeout_ms" {
                    let _ = write!(
                        msg,
                        " `total_timeout_ms` is only available for composite clients. For regular clients, use: {valid_fields}"
                    );
                }

                Diagnostic::error(DiagnosticId::UnknownHttpConfigField, msg).with_primary_span(*span)
            }

            HirDiagnostic::NegativeTimeout {
                client_name: _,
                field_name,
                value,
                span,
            } => Diagnostic::error(
                DiagnosticId::NegativeTimeout,
                format!("{field_name} must be non-negative, got: {value}ms"),
            )
            .with_primary_span(*span),

            HirDiagnostic::MissingProvider {
                client_name: _,
                span,
            } => Diagnostic::error(
                DiagnosticId::MissingProvider,
                "Missing `provider` field in client. e.g. `provider openai`",
            )
            .with_primary_span(*span),

            HirDiagnostic::UnknownClientProperty {
                client_name: _,
                field_name,
                span,
            } => Diagnostic::error(
                DiagnosticId::UnknownClientProperty,
                format!(
                    "Unknown field `{field_name}` in client. Only `provider` and `options` are supported."
                ),
            )
            .with_primary_span(*span),

            HirDiagnostic::MissingSemicolon { span } => Diagnostic::error(
                DiagnosticId::MissingSemicolon,
                "Statement must end with a semicolon.",
            )
            .with_primary_span(*span),

            HirDiagnostic::MissingReturnExpression { span } => Diagnostic::error(
                DiagnosticId::MissingReturnExpression,
                "Missing return expression. Function body must have a final expression or explicit return.",
            )
            .with_primary_span(*span),

            HirDiagnostic::MissingConditionParens { kind, span } => Diagnostic::error(
                DiagnosticId::MissingConditionParens,
                format!("Condition in `{kind}` statement must be wrapped in parentheses."),
            )
            .with_primary_span(*span),

            HirDiagnostic::UnmatchedDelimiter { token, span } => Diagnostic::error(
                DiagnosticId::UnmatchedDelimiter,
                format!("Unmatched `{token}`."),
            )
            .with_primary_span(*span),

            HirDiagnostic::InvalidConstraintSyntax { attr_name, span } => Diagnostic::error(
                DiagnosticId::InvalidConstraintSyntax,
                format!(
                    "Invalid @{attr_name} syntax. Expected a Jinja expression block.\n\
                     Examples:\n  \
                     @check(name, {{{{ this > 0 }}}})\n  \
                     @assert({{{{ this|length > 0 }}}})"
                ),
            )
            .with_primary(*span, "missing Jinja expression {{ }}"),

            HirDiagnostic::UnsupportedFloatLiteral { value, span } => Diagnostic::error(
                DiagnosticId::UnsupportedFloatLiteral,
                format!("Float literal values are not supported: {value}"),
            )
            .with_primary_span(*span),

            HirDiagnostic::UnknownTestProperty {
                test_name: _,
                property_name,
                span,
                valid_properties,
            } => {
                // Check if this looks like a misplaced attribute
                let message = if property_name == "check" || property_name == "assert" {
                    format!(
                        "@{property_name} is not allowed on test fields. Use @@{property_name} at the test block level instead."
                    )
                } else {
                    format!(
                        "Property not known: \"{property_name}\". Did you mean one of these: {}?",
                        valid_properties
                            .iter()
                            .map(|p| format!("\"{p}\""))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                Diagnostic::error(DiagnosticId::UnknownTestProperty, message)
                    .with_primary_span(*span)
            }

            HirDiagnostic::MissingTestProperty {
                test_name: _,
                property_name,
                span,
            } => Diagnostic::error(
                DiagnosticId::MissingTestProperty,
                format!("Missing `{property_name}` property"),
            )
            .with_primary_span(*span),

            HirDiagnostic::TestFieldAttribute { attr_name, span } => Diagnostic::error(
                DiagnosticId::TestFieldAttribute,
                format!(
                    "@{attr_name} is not allowed on test fields. Use @@{attr_name} at the test block level instead."
                ),
            )
            .with_primary_span(*span),
        };
        diag.with_phase(DiagnosticPhase::Hir)
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
        assert!(diag.message.contains("Expected"));
        assert_eq!(diag.phase, DiagnosticPhase::Parse);
    }

    #[test]
    fn test_type_error_to_diagnostic() {
        let error: TypeError<String> = TypeError::TypeMismatch {
            expected: "int".to_string(),
            found: "string".to_string(),
            span: test_span(),
            info_span: None,
        };

        let diag = error.to_diagnostic();
        assert_eq!(diag.code(), "E0001");
        assert!(diag.message.contains("int"));
        assert!(diag.message.contains("string"));
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

    #[test]
    fn test_hir_diagnostic_to_diagnostic() {
        let first_span = test_span();
        let second_span = Span {
            file_id: baml_base::FileId::new(0),
            range: TextRange::new(20.into(), 30.into()),
        };

        let error = HirDiagnostic::DuplicateField {
            class_name: "Person".to_string(),
            field_name: "name".to_string(),
            first_span,
            second_span,
        };

        let diag = error.to_diagnostic();
        assert_eq!(diag.code(), "E0012");
        assert!(diag.message.contains("Duplicate field"));
        assert_eq!(diag.phase, DiagnosticPhase::Hir);
    }
}
