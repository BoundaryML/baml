use ariadne::{Label, ReportBuilder};
use baml_base::Span;

use super::{
    ARGUMENT_COUNT_MISMATCH, CompilerError, DUPLICATE_NAME, ErrorCode, INVALID_OPERATOR,
    NO_SUCH_FIELD, NON_EXHAUSTIVE_MATCH, NOT_CALLABLE, NOT_INDEXABLE, NameError, ParseError,
    Report, ReportKind, TYPE_MISMATCH, TypeError, UNEXPECTED_EOF, UNEXPECTED_TOKEN, UNKNOWN_TYPE,
    UNKNOWN_VARIABLE, UNREACHABLE_ARM,
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
        },
        CompilerError::TypeError(type_error) => match type_error {
            // TODO: This error should provide a second span that indicates the source
            // of the type judgment - the reason why we thought this type is a mismatch.
            // ... where the expectation came from.
            TypeError::TypeMismatch {
                expected,
                found,
                span,
            } => simple_error(
                format!("Expected {expected}, found {found}"),
                span,
                TYPE_MISMATCH,
            ),
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
