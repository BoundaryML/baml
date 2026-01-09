pub mod error_format;
pub mod hir_diagnostic;
pub mod name_error;
pub mod parse_error;
pub mod type_error;

use std::{collections::HashMap, fmt};

use ariadne::{Report, ReportKind, Source};
use baml_base::{FileId, Span};
use baml_workspace::Project;
pub use hir_diagnostic::HirDiagnostic;
pub use name_error::NameError;
pub use parse_error::ParseError;
pub use type_error::TypeError;

/// A cache for ariadne that lazily loads sources from the Salsa database.
///
/// This avoids copying source text by using `&str` references into the database.
pub struct DbSourceCache<'db> {
    db: &'db dyn salsa::Database,
    project: Project,
    sources: HashMap<FileId, Source<&'db str>>,
    filenames: HashMap<FileId, String>,
}

impl<'db> DbSourceCache<'db> {
    /// Create a new source cache backed by the database.
    pub fn new(db: &'db dyn salsa::Database, project: Project) -> Self {
        DbSourceCache {
            db,
            project,
            sources: HashMap::new(),
            filenames: HashMap::new(),
        }
    }
}

/// A wrapper type for displaying file IDs as filenames.
struct FileIdDisplay {
    file_id: FileId,
    filename: Option<String>,
}

impl fmt::Display for FileIdDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref name) = self.filename {
            write!(f, "{name}")
        } else {
            write!(f, "{}", self.file_id)
        }
    }
}

#[allow(refining_impl_trait)]
impl<'db> ariadne::Cache<FileId> for DbSourceCache<'db> {
    type Storage = &'db str;

    fn fetch(&mut self, id: &FileId) -> Result<&Source<Self::Storage>, Box<dyn fmt::Debug + '_>> {
        if !self.sources.contains_key(id) {
            // Find the file with this FileId
            let file = self
                .project
                .files(self.db)
                .iter()
                .find(|f| f.file_id(self.db) == *id)
                .copied()
                .ok_or_else(|| Box::new(format!("Unknown file ID: {id}")) as Box<dyn fmt::Debug>)?;

            let text: &'db str = file.text(self.db);
            self.sources.insert(*id, Source::from(text));
            self.filenames
                .insert(*id, file.path(self.db).display().to_string());
        }

        self.sources
            .get(id)
            .ok_or_else(|| Box::new(format!("Unknown file ID: {id}")) as Box<dyn fmt::Debug>)
    }

    fn display<'a>(&self, id: &'a FileId) -> Option<Box<dyn fmt::Display + 'a>> {
        // Return the filename if available, otherwise fall back to the file ID
        let filename = self.filenames.get(id).cloned();
        Some(Box::new(FileIdDisplay {
            file_id: *id,
            filename,
        }))
    }
}

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
        .with_config(
            ariadne::Config::default()
                .with_color(*color_mode == ColorMode::Color)
                .with_compact(false),
        )
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

// Constraint attribute diagnostics
const INVALID_CONSTRAINT_SYNTAX: ErrorCode = ErrorCode(32);

// Syntax diagnostics
const MISSING_SEMICOLON: ErrorCode = ErrorCode(28);
const MISSING_RETURN_EXPRESSION: ErrorCode = ErrorCode(29);
const MISSING_CONDITION_PARENS: ErrorCode = ErrorCode(30);
const UNMATCHED_DELIMITER: ErrorCode = ErrorCode(31);

const NON_EXHAUSTIVE_MATCH: ErrorCode = ErrorCode(62);
const UNREACHABLE_ARM: ErrorCode = ErrorCode(63);
const UNKNOWN_ENUM_VARIANT: ErrorCode = ErrorCode(64);
const WATCH_ON_NON_VARIABLE: ErrorCode = ErrorCode(65);
const WATCH_ON_UNWATCHED_VARIABLE: ErrorCode = ErrorCode(66);

/// Render an ariadne Report to a String.
///
/// Uses the provided cache to look up source text and filenames for the spans
/// referenced in the report. Reusing the same cache across multiple renders
/// avoids redundant lookups.
pub fn render_report_to_string(report: &Report<'_, Span>, cache: &mut DbSourceCache<'_>) -> String {
    let mut output = Vec::new();

    report.write(cache, &mut output).unwrap_or_else(|_| {
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
    cache: &mut DbSourceCache<'_>,
    color: bool,
) -> String {
    let color_mode = if color {
        ColorMode::Color
    } else {
        ColorMode::NoColor
    };
    let compiler_error: CompilerError<String> = CompilerError::ParseError(error.clone());
    let report = render_error(&color_mode, compiler_error);
    render_report_to_string(&report, cache)
}

/// Convenience function to render a `TypeError` directly to a string.
///
/// This combines `render_error` and `render_report_to_string` for the common case
/// of rendering type errors. The type parameter `Ty` must implement `Display` and `Clone`.
pub fn render_type_error<Ty: std::fmt::Display + Clone>(
    error: &TypeError<Ty>,
    cache: &mut DbSourceCache<'_>,
    color: bool,
) -> String {
    let color_mode = if color {
        ColorMode::Color
    } else {
        ColorMode::NoColor
    };
    let compiler_error: CompilerError<Ty> = CompilerError::TypeError(error.clone());
    let report = render_error(&color_mode, compiler_error);
    render_report_to_string(&report, cache)
}

/// Convenience function to render a `NameError` directly to a string.
///
/// This combines `render_error` and `render_report_to_string` for the common case
/// of rendering name resolution errors.
pub fn render_name_error(error: &NameError, cache: &mut DbSourceCache<'_>, color: bool) -> String {
    let color_mode = if color {
        ColorMode::Color
    } else {
        ColorMode::NoColor
    };
    // Use String as the type parameter since NameError doesn't use it
    let compiler_error: CompilerError<String> = CompilerError::NameError(error.clone());
    let report = render_error(&color_mode, compiler_error);
    render_report_to_string(&report, cache)
}

/// Convenience function to render a `HirDiagnostic` directly to a string.
///
/// This combines `render_error` and `render_report_to_string` for the common case
/// of rendering HIR lowering diagnostics.
pub fn render_hir_diagnostic(
    error: &HirDiagnostic,
    cache: &mut DbSourceCache<'_>,
    color: bool,
) -> String {
    let color_mode = if color {
        ColorMode::Color
    } else {
        ColorMode::NoColor
    };
    let compiler_error: CompilerError<String> = CompilerError::HirDiagnostic(error.clone());
    let report = render_error(&color_mode, compiler_error);
    render_report_to_string(&report, cache)
}
