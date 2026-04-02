use ariadne::{Label, ReportBuilder};
use baml_base::Span;

use super::{
    ARGUMENT_COUNT_MISMATCH, CompilerError, DUPLICATE_NAME, ErrorCode, INVALID_OPERATOR,
    NO_SUCH_FIELD, NON_EXHAUSTIVE_MATCH, NOT_CALLABLE, NOT_INDEXABLE, NameError, ParseError,
    Report, ReportKind, TYPE_MISMATCH, TypeError, UNEXPECTED_EOF, UNEXPECTED_TOKEN,
    UNKNOWN_ENUM_VARIANT, UNKNOWN_TYPE, UNKNOWN_VARIABLE, UNREACHABLE_ARM,
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
            TypeError::UnreachableCatchArm { span } => simple_error(
                "Unreachable catch arm: it cannot match any remaining throw type".to_string(),
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
            TypeError::NonExhaustiveCatch {
                unhandled_types,
                span,
            } => simple_error(
                format!(
                    "Non-exhaustive catch chain: unhandled throw types {}",
                    unhandled_types.join(", ")
                ),
                span,
                NON_EXHAUSTIVE_MATCH,
            ),
            TypeError::ThrowsContractViolation {
                extra_types,
                span,
            } => simple_error(
                format!(
                    "Function throws types not covered by `throws` declaration: {}",
                    extra_types.join(", ")
                ),
                span,
                NON_EXHAUSTIVE_MATCH,
            ),
            TypeError::ThrowsContractExtraneous {
                unused_types,
                span,
            } => simple_error(
                format!(
                    "`throws` declaration includes types the function never throws: {}",
                    unused_types.join(", ")
                ),
                span,
                NON_EXHAUSTIVE_MATCH,
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
