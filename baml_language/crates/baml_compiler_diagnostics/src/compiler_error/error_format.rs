use ariadne::{Label, ReportBuilder};
use baml_base::Span;

use super::{
    ARGUMENT_COUNT_MISMATCH, CompilerError, DUPLICATE_ATTRIBUTE, DUPLICATE_FIELD, DUPLICATE_NAME,
    DUPLICATE_VARIANT, ErrorCode, FIELD_NAME_MATCHES_TYPE_NAME, HTTP_CONFIG_NOT_BLOCK,
    HirDiagnostic, INVALID_ATTRIBUTE_CONTEXT, INVALID_CLIENT_RESPONSE_TYPE,
    INVALID_CONSTRAINT_SYNTAX, INVALID_GENERATOR_PROPERTY_VALUE, INVALID_OPERATOR,
    MISSING_CONDITION_PARENS, MISSING_GENERATOR_PROPERTY, MISSING_PROVIDER,
    MISSING_RETURN_EXPRESSION, MISSING_SEMICOLON, NEGATIVE_TIMEOUT, NO_SUCH_FIELD,
    NON_EXHAUSTIVE_MATCH, NOT_CALLABLE, NOT_INDEXABLE, NameError, ParseError, RESERVED_FIELD_NAME,
    Report, ReportKind, TYPE_MISMATCH, TypeError, UNEXPECTED_EOF, UNEXPECTED_TOKEN,
    UNKNOWN_ATTRIBUTE, UNKNOWN_CLIENT_PROPERTY, UNKNOWN_ENUM_VARIANT, UNKNOWN_GENERATOR_PROPERTY,
    UNKNOWN_HTTP_CONFIG_FIELD, UNKNOWN_TYPE, UNKNOWN_VARIABLE, UNMATCHED_DELIMITER, UNREACHABLE_ARM,
    WATCH_ON_NON_VARIABLE, WATCH_ON_UNWATCHED_VARIABLE,
};

/// The message format and id of each compiler error variant.
/// This internal function encodes the properties of an error. It is called
/// by `render_error`, which finalizes the error report by attaching
/// the error code and coloring it appropriately.
pub fn error_report_and_code<'a, Ty>(err: CompilerError<Ty>) -> (ReportBuilder<'a, Span>, ErrorCode)
where
    Ty: std::fmt::Display,
{
    match err {
        CompilerError::ParseError(parse_error) => match parse_error {
            ParseError::UnexpectedToken {
                expected,
                found,
                span,
            } => simple_error(
                format!("Expected {expected}, found {found}"),
                span,
                UNEXPECTED_TOKEN,
            ),
            ParseError::UnexpectedEof { expected, span } => simple_error(
                format!("Expected {expected}, found EOF"),
                span,
                UNEXPECTED_EOF,
            ),
            ParseError::InvalidSyntax { message, span } => {
                simple_error(message, span, UNEXPECTED_TOKEN)
            }
        },
        CompilerError::TypeError(type_error) => match type_error {
            TypeError::TypeMismatch {
                expected,
                found,
                span,
                info_span,
            } => {
                let message = format!("Expected `{expected}`, found `{found}`");
                // Use info_span as the primary location if available, since it's the "cause"
                let primary_span = info_span.unwrap_or(span);
                let mut report = Report::build(ReportKind::Error, primary_span)
                    .with_message(&message);
                // Add the info label first (the cause/constraint source)
                if let Some(info) = info_span {
                    report = report.with_label(
                        Label::new(info)
                            .with_message(format!("expected `{expected}` because of return type"))
                            .with_order(0),
                    );
                }
                // Add the error label second (the actual mismatch)
                report = report.with_label(
                    Label::new(span)
                        .with_message(&message)
                        .with_order(1),
                );
                (report, TYPE_MISMATCH)
            }
            TypeError::UnknownType { name, span } => {
                simple_error(format!("Unknown type {name}"), span, UNKNOWN_TYPE)
            }
            TypeError::UnknownVariable { name, span } => {
                simple_error(format!("Unknown variable {name}"), span, UNKNOWN_VARIABLE)
            }
            TypeError::InvalidBinaryOp { op, lhs, rhs, span } => simple_error(
                format!("Cannot apply operator '{op}' to types {lhs} and {rhs}"),
                span,
                INVALID_OPERATOR,
            ),
            TypeError::InvalidUnaryOp { op, operand, span } => simple_error(
                format!("Cannot apply operator '{op}' to type {operand}"),
                span,
                INVALID_OPERATOR,
            ),
            // TODO: Include a span for the original fn definition.
            TypeError::ArgumentCountMismatch {
                expected,
                found,
                span,
            } => simple_error(
                format!("Expected {expected} arguments, found {found}"),
                span,
                ARGUMENT_COUNT_MISMATCH,
            ),
            TypeError::NotCallable { ty, span } => {
                simple_error(format!("Type {ty} is not callable"), span, NOT_CALLABLE)
            }
            // TODO: Span for the type definition.
            TypeError::NoSuchField { ty, field, span } => simple_error(
                format!("Type {ty} has no field '{field}'"),
                span,
                NO_SUCH_FIELD,
            ),
            TypeError::NotIndexable { ty, span } => {
                simple_error(format!("Type {ty} is not indexable"), span, NOT_INDEXABLE)
            }
            TypeError::NonExhaustiveMatch {
                scrutinee_type,
                missing_cases,
                span,
            } => {
                let missing = missing_cases.join(", ");
                simple_error(
                    format!(
                        "Non-exhaustive match: type {scrutinee_type} not fully covered. Missing: {missing}"
                    ),
                    span,
                    NON_EXHAUSTIVE_MATCH,
                )
            }
            TypeError::UnreachableArm { span } => simple_error(
                "Unreachable match arm: previous arms already cover all cases".to_string(),
                span,
                UNREACHABLE_ARM,
            ),
            TypeError::UnknownEnumVariant {
                enum_name,
                variant_name,
                span,
            } => simple_error(
                format!("Enum '{enum_name}' has no variant '{variant_name}'"),
                span,
                UNKNOWN_ENUM_VARIANT,
            ),
            TypeError::WatchOnNonVariable { span } => simple_error(
                "$watch can only be used on simple variable expressions".to_string(),
                span,
                WATCH_ON_NON_VARIABLE,
            ),
            TypeError::WatchOnUnwatchedVariable { name, span } => simple_error(
                format!(
                    "Cannot use $watch on '{name}': variable must be declared with `watch let`"
                ),
                span,
                WATCH_ON_UNWATCHED_VARIABLE,
            ),
        },
        CompilerError::NameError(name_error) => match name_error {
            NameError::DuplicateName {
                name,
                kind,
                first,
                first_path,
                second,
                second_path,
            } => (
                Report::build(ReportKind::Error, second)
                    .with_message(format!("Duplicate {kind} '{name}'"))
                    .with_label(
                        Label::new(second)
                            .with_message(format!("{kind} '{name}' defined in {second_path}")),
                    )
                    .with_label(
                        Label::new(first)
                            .with_message(format!("'{name}' previously defined in {first_path}")),
                    ),
                DUPLICATE_NAME,
            ),
            NameError::DuplicateTestForFunction {
                test_name,
                function_name,
                first,
                first_path,
                second,
                second_path,
            } => (
                Report::build(ReportKind::Error, second)
                    .with_message(format!(
                        "Duplicate test '{test_name}' for function '{function_name}'"
                    ))
                    .with_label(Label::new(second).with_message(format!(
                        "test '{test_name}' for function '{function_name}' defined in {second_path}"
                    )))
                    .with_label(Label::new(first).with_message(format!(
                        "'{test_name}' for '{function_name}' previously defined in {first_path}"
                    ))),
                DUPLICATE_NAME,
            ),
        },
        CompilerError::HirDiagnostic(hir_diag) => match hir_diag {
            HirDiagnostic::DuplicateField {
                class_name,
                field_name,
                first_span,
                second_span,
            } => (
                Report::build(ReportKind::Error, second_span)
                    .with_message(format!(
                        "Duplicate field '{field_name}' in class '{class_name}'"
                    ))
                    .with_label(
                        Label::new(second_span).with_message("duplicate definition"),
                    )
                    .with_label(
                        Label::new(first_span).with_message("first definition here"),
                    ),
                DUPLICATE_FIELD,
            ),
            HirDiagnostic::DuplicateVariant {
                enum_name,
                variant_name,
                first_span,
                second_span,
            } => (
                Report::build(ReportKind::Error, second_span)
                    .with_message(format!(
                        "Duplicate variant '{variant_name}' in enum '{enum_name}'"
                    ))
                    .with_label(
                        Label::new(second_span).with_message("duplicate definition"),
                    )
                    .with_label(
                        Label::new(first_span).with_message("first definition here"),
                    ),
                DUPLICATE_VARIANT,
            ),
            HirDiagnostic::DuplicateBlockAttribute {
                item_kind,
                item_name,
                attr_name,
                first_span,
                second_span,
            } => (
                Report::build(ReportKind::Error, second_span)
                    .with_message(format!(
                        "Attribute '@@{attr_name}' can only be defined once on {item_kind} '{item_name}'"
                    ))
                    .with_label(
                        Label::new(second_span).with_message("duplicate attribute"),
                    )
                    .with_label(
                        Label::new(first_span).with_message("first definition here"),
                    ),
                DUPLICATE_ATTRIBUTE,
            ),
            HirDiagnostic::DuplicateFieldAttribute {
                container_kind,
                container_name,
                field_name,
                attr_name,
                first_span,
                second_span,
            } => (
                Report::build(ReportKind::Error, second_span)
                    .with_message(format!(
                        "Attribute '@{attr_name}' can only be defined once on field '{field_name}' in {container_kind} '{container_name}'"
                    ))
                    .with_label(
                        Label::new(second_span).with_message("duplicate attribute"),
                    )
                    .with_label(
                        Label::new(first_span).with_message("first definition here"),
                    ),
                DUPLICATE_ATTRIBUTE,
            ),
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
                simple_error(
                    format!("Unknown attribute '{attr_name}'{suggestions}"),
                    span,
                    UNKNOWN_ATTRIBUTE,
                )
            }
            HirDiagnostic::InvalidAttributeContext {
                attr_name,
                context,
                allowed_contexts,
                span,
            } => simple_error(
                format!(
                    "Attribute '{attr_name}' is not allowed on {context}. Allowed on: {allowed_contexts}"
                ),
                span,
                INVALID_ATTRIBUTE_CONTEXT,
            ),
            HirDiagnostic::UnknownGeneratorProperty {
                generator_name,
                property_name,
                span,
                valid_properties,
            } => {
                let suggestions = format!("Valid properties: {}", valid_properties.join(", "));
                simple_error(
                    format!(
                        "Unknown property '{property_name}' in generator '{generator_name}'. {suggestions}"
                    ),
                    span,
                    UNKNOWN_GENERATOR_PROPERTY,
                )
            }
            HirDiagnostic::MissingGeneratorProperty {
                generator_name,
                property_name,
                span,
            } => simple_error(
                format!(
                    "Generator '{generator_name}' is missing required property '{property_name}'"
                ),
                span,
                MISSING_GENERATOR_PROPERTY,
            ),
            HirDiagnostic::InvalidGeneratorPropertyValue {
                generator_name,
                property_name,
                value,
                span,
                valid_values,
                help,
            } => {
                let mut msg = format!(
                    "Invalid value '{value}' for property '{property_name}' in generator '{generator_name}'"
                );
                if let Some(valid) = valid_values {
                    use std::fmt::Write;
                    let _ = write!(msg, ". Valid values: {}", valid.join(", "));
                }
                if let Some(h) = help {
                    use std::fmt::Write;
                    let _ = write!(msg, ". {h}");
                }
                simple_error(msg, span, INVALID_GENERATOR_PROPERTY_VALUE)
            }
            HirDiagnostic::ReservedFieldName {
                item_kind,
                item_name,
                field_name,
                span,
                target_languages,
            } => {
                let field_type = match item_kind {
                    "class" => "Class field",
                    "enum" => "Enum value",
                    "function" => "Function parameter",
                    _ => "Field",
                };
                simple_error(
                    format!(
                        "{field_type} '{field_name}' in {item_kind} '{item_name}' is a reserved keyword in {}",
                        target_languages.join(", ")
                    ),
                    span,
                    RESERVED_FIELD_NAME,
                )
            }
            HirDiagnostic::FieldNameMatchesTypeName {
                class_name,
                field_name,
                type_name,
                span,
            } => simple_error(
                format!(
                    "Error validating field `{field_name}` in class `{class_name}`: When using the python/pydantic generator, a field name must not be exactly equal to the type name (`{type_name}`). Consider changing the field name or using an @alias."
                ),
                span,
                FIELD_NAME_MATCHES_TYPE_NAME,
            ),
            HirDiagnostic::InvalidClientResponseType {
                client_name: _,
                value,
                span,
                valid_values,
            } => {
                let valid = valid_values.join(", ");
                simple_error(
                    format!(
                        "client_response_type must be one of {valid}. Got: {value}"
                    ),
                    span,
                    INVALID_CLIENT_RESPONSE_TYPE,
                )
            }
            HirDiagnostic::HttpConfigNotBlock { client_name: _, span } => simple_error(
                "http must be a configuration block with timeout settings".to_string(),
                span,
                HTTP_CONFIG_NOT_BLOCK,
            ),
            HirDiagnostic::UnknownHttpConfigField {
                client_name: _,
                field_name,
                span,
                suggestion,
                is_composite,
            } => {
                let valid_fields = if is_composite {
                    "total_timeout_ms"
                } else {
                    "connect_timeout_ms, request_timeout_ms, time_to_first_token_timeout_ms, idle_timeout_ms"
                };

                let mut msg = format!("Unrecognized field '{field_name}' in http configuration block.");

                if let Some(ref suggested) = suggestion {
                    use std::fmt::Write;
                    let _ = write!(msg, " Did you mean '{suggested}'?");
                }

                if is_composite {
                    use std::fmt::Write;
                    let _ = write!(
                        msg,
                        " Composite clients (fallback/round-robin) only support: {valid_fields}"
                    );
                } else if field_name == "total_timeout_ms" {
                    use std::fmt::Write;
                    let _ = write!(
                        msg,
                        " 'total_timeout_ms' is only available for composite clients (fallback/round-robin). For regular clients, use: {valid_fields}"
                    );
                }

                simple_error(msg, span, UNKNOWN_HTTP_CONFIG_FIELD)
            }
            HirDiagnostic::NegativeTimeout {
                client_name: _,
                field_name,
                value,
                span,
            } => simple_error(
                format!("{field_name} must be non-negative, got: {value}ms"),
                span,
                NEGATIVE_TIMEOUT,
            ),
            HirDiagnostic::MissingProvider {
                client_name: _,
                span,
            } => simple_error(
                "Missing `provider` field in client. e.g. `provider openai`".to_string(),
                span,
                MISSING_PROVIDER,
            ),
            HirDiagnostic::UnknownClientProperty {
                client_name: _,
                field_name,
                span,
            } => simple_error(
                format!("Unknown field `{field_name}` in client. Only `provider` and `options` are supported."),
                span,
                UNKNOWN_CLIENT_PROPERTY,
            ),
            HirDiagnostic::MissingSemicolon { span } => simple_error(
                "Missing semicolon after statement".to_string(),
                span,
                MISSING_SEMICOLON,
            ),
            HirDiagnostic::MissingReturnExpression { span } => simple_error(
                "Missing return expression in function".to_string(),
                span,
                MISSING_RETURN_EXPRESSION,
            ),
            HirDiagnostic::MissingConditionParens { kind, span } => simple_error(
                format!("Condition in `{kind}` statement must be wrapped in parentheses"),
                span,
                MISSING_CONDITION_PARENS,
            ),
            HirDiagnostic::UnmatchedDelimiter { token, span } => simple_error(
                format!("Unmatched `{token}`"),
                span,
                UNMATCHED_DELIMITER,
            ),
            HirDiagnostic::InvalidConstraintSyntax { attr_name, span } => simple_error(
                format!(
                    "Invalid @{attr_name} syntax. Expected a Jinja expression block.\n\
                     Examples:\n  \
                     @check(name, {{{{ this > 0 }}}})\n  \
                     @assert({{{{ this|length > 0 }}}})"
                ),
                span,
                INVALID_CONSTRAINT_SYNTAX,
            ),
        },
    }
}

/// Helper function for constructing error reports that don't need any special handling,
/// like multiple spans.
fn simple_error<'a>(
    message: String,
    span: Span,
    code: ErrorCode,
) -> (ReportBuilder<'a, Span>, ErrorCode) {
    (
        Report::build(ReportKind::Error, span)
            .with_message(&message)
            .with_label(Label::new(span).with_message(message)),
        code,
    )
}
