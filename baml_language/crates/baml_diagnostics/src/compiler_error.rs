pub mod error_format;
pub mod hir_diagnostic;
pub mod name_error;
pub mod parse_error;
pub mod type_error;

use std::collections::HashMap;

use ariadne::{Report, ReportKind};
use baml_base::{FileId, Span};
pub use hir_diagnostic::HirDiagnostic;
pub use name_error::NameError;
pub use parse_error::ParseError;
pub use type_error::TypeError;

/// Every compiler error that can occur in the compiler.
/// It is parameterized by several types that are owned by the different compiler phases,
/// which we don't want to collect in this module. We only care that those values can
/// be displayed (for rendering in the message).
pub enum CompilerError<Ty> {
    ParseError(ParseError),
    TypeError(TypeError<Ty>),
    NameError(NameError),
    HirDiagnostic(HirDiagnostic),
}

pub struct ErrorCode(u32);

/// Error codes are formated like E0001, (E followed by a number padded to 4 digits).
impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "E{:04}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ColorMode {
    Color,
    NoColor,
}

pub fn render_error<'a, Ty: std::fmt::Display>(
    color_mode: &ColorMode,
    err: CompilerError<Ty>,
) -> Report<'a, Span> {
    let (report_builder, code) = error_format::error_report_and_code(err);
    report_builder
        .with_config(ariadne::Config::default().with_color(*color_mode == ColorMode::Color))
        .with_note(format!("Error code: {code}"))
        .finish()
}

const TYPE_MISMATCH: ErrorCode = ErrorCode(1);
const UNKNOWN_TYPE: ErrorCode = ErrorCode(2);
const UNKNOWN_VARIABLE: ErrorCode = ErrorCode(3);
const INVALID_OPERATOR: ErrorCode = ErrorCode(4);
const ARGUMENT_COUNT_MISMATCH: ErrorCode = ErrorCode(5);
const NOT_CALLABLE: ErrorCode = ErrorCode(6);
const NO_SUCH_FIELD: ErrorCode = ErrorCode(7);
const NOT_INDEXABLE: ErrorCode = ErrorCode(8);

const UNEXPECTED_EOF: ErrorCode = ErrorCode(9);
const UNEXPECTED_TOKEN: ErrorCode = ErrorCode(10);
const DUPLICATE_NAME: ErrorCode = ErrorCode(11);

// HIR lowering diagnostics (per-file validation)
const DUPLICATE_FIELD: ErrorCode = ErrorCode(12);
const DUPLICATE_VARIANT: ErrorCode = ErrorCode(13);
const DUPLICATE_ATTRIBUTE: ErrorCode = ErrorCode(14);
const UNKNOWN_ATTRIBUTE: ErrorCode = ErrorCode(15);
const INVALID_ATTRIBUTE_CONTEXT: ErrorCode = ErrorCode(16);

// Generator diagnostics
const UNKNOWN_GENERATOR_PROPERTY: ErrorCode = ErrorCode(17);
const MISSING_GENERATOR_PROPERTY: ErrorCode = ErrorCode(18);
const INVALID_GENERATOR_PROPERTY_VALUE: ErrorCode = ErrorCode(19);

// Reserved names diagnostics
const RESERVED_FIELD_NAME: ErrorCode = ErrorCode(20);
const FIELD_NAME_MATCHES_TYPE_NAME: ErrorCode = ErrorCode(21);

// Client diagnostics
const INVALID_CLIENT_RESPONSE_TYPE: ErrorCode = ErrorCode(22);
const HTTP_CONFIG_NOT_BLOCK: ErrorCode = ErrorCode(23);
const UNKNOWN_HTTP_CONFIG_FIELD: ErrorCode = ErrorCode(24);
const NEGATIVE_TIMEOUT: ErrorCode = ErrorCode(25);
const MISSING_PROVIDER: ErrorCode = ErrorCode(26);
const UNKNOWN_CLIENT_PROPERTY: ErrorCode = ErrorCode(27);

const NON_EXHAUSTIVE_MATCH: ErrorCode = ErrorCode(62);
const UNREACHABLE_ARM: ErrorCode = ErrorCode(63);
const UNKNOWN_ENUM_VARIANT: ErrorCode = ErrorCode(64);

/// Render an ariadne Report to a String.
///
/// The `sources` map should contain the source text for each `FileId` referenced
/// in the report's spans.
pub fn render_report_to_string(
    report: &Report<'_, Span>,
    sources: &HashMap<FileId, String>,
) -> String {
    let mut output = Vec::new();

    // ariadne::sources expects types that implement AsRef<str>, so we pass String directly
    let ariadne_sources: HashMap<FileId, String> = sources.clone();

    // Use ariadne's sources helper which creates a cache from a HashMap
    let mut cache = ariadne::sources(ariadne_sources);

    report.write(&mut cache, &mut output).unwrap_or_else(|_| {
        // If writing fails, provide a fallback
        output.clear();
        output.extend_from_slice(b"<error rendering diagnostic>");
    });

    String::from_utf8_lossy(&output).into_owned()
}

/// Convenience function to render a `ParseError` directly to a string.
///
/// This combines `render_error` and `render_report_to_string` for the common case
/// of rendering parse errors.
pub fn render_parse_error(
    error: &ParseError,
    sources: &HashMap<FileId, String>,
    color: bool,
) -> String {
    let color_mode = if color {
        ColorMode::Color
    } else {
        ColorMode::NoColor
    };
    let compiler_error: CompilerError<String> = CompilerError::ParseError(error.clone());
    let report = render_error(&color_mode, compiler_error);
    render_report_to_string(&report, sources)
}

/// Convenience function to render a `TypeError` directly to a string.
///
/// This combines `render_error` and `render_report_to_string` for the common case
/// of rendering type errors. The type parameter `Ty` must implement `Display` and `Clone`.
pub fn render_type_error<Ty: std::fmt::Display + Clone>(
    error: &TypeError<Ty>,
    sources: &HashMap<FileId, String>,
    color: bool,
) -> String {
    let color_mode = if color {
        ColorMode::Color
    } else {
        ColorMode::NoColor
    };
    let compiler_error: CompilerError<Ty> = CompilerError::TypeError(error.clone());
    let report = render_error(&color_mode, compiler_error);
    render_report_to_string(&report, sources)
}

/// Convenience function to render a `NameError` directly to a string.
///
/// This combines `render_error` and `render_report_to_string` for the common case
/// of rendering name resolution errors.
pub fn render_name_error(
    error: &NameError,
    sources: &HashMap<FileId, String>,
    color: bool,
) -> String {
    let color_mode = if color {
        ColorMode::Color
    } else {
        ColorMode::NoColor
    };
    // Use String as the type parameter since NameError doesn't use it
    let compiler_error: CompilerError<String> = CompilerError::NameError(error.clone());
    let report = render_error(&color_mode, compiler_error);
    render_report_to_string(&report, sources)
}

/// Convenience function to render a `HirDiagnostic` directly to a string.
///
/// This combines `render_error` and `render_report_to_string` for the common case
/// of rendering HIR lowering diagnostics.
pub fn render_hir_diagnostic(
    error: &HirDiagnostic,
    sources: &HashMap<FileId, String>,
    color: bool,
) -> String {
    let color_mode = if color {
        ColorMode::Color
    } else {
        ColorMode::NoColor
    };
    let compiler_error: CompilerError<String> = CompilerError::HirDiagnostic(error.clone());
    let report = render_error(&color_mode, compiler_error);
    render_report_to_string(&report, sources)
}
