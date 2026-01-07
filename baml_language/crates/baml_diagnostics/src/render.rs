//! Multi-format rendering for diagnostics.
//!
//! This module provides rendering of unified `Diagnostic` types to various formats:
//! - **Ariadne**: Beautiful CLI output with colors and source snippets
//! - **Concise**: One-line format like `file:line:col: [E0001] message`
//! - **LSP**: Converts to `lsp_types::Diagnostic` for editor integration
//!
//! ## Example
//!
//! ```ignore
//! use baml_diagnostics::{Diagnostic, DiagnosticFormat, RenderConfig, render_diagnostic};
//!
//! let diag = Diagnostic::error(DiagnosticId::TypeMismatch, "Expected int, found string")
//!     .with_primary_span(span);
//!
//! // Render for CLI
//! let cli_output = render_diagnostic(&diag, &sources, RenderConfig::cli());
//!
//! // Render concise (for tests)
//! let concise = render_diagnostic(&diag, &sources, RenderConfig::concise());
//! ```

use std::collections::HashMap;

use ariadne::{Label, Report, ReportKind};
use baml_base::{FileId, Span};

use crate::diagnostic::{Diagnostic, Severity};

/// Output format for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagnosticFormat {
    /// Full Ariadne output with colors and source context.
    #[default]
    Ariadne,
    /// Concise one-line format: `file:line:col: [E0001] message`
    Concise,
}

/// Configuration for rendering diagnostics.
#[derive(Debug, Clone)]
pub struct RenderConfig {
    /// The output format.
    pub format: DiagnosticFormat,
    /// Whether to use colors in output.
    pub color: bool,
    /// Whether to show error codes.
    pub show_error_codes: bool,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            format: DiagnosticFormat::Ariadne,
            color: true,
            show_error_codes: true,
        }
    }
}

impl RenderConfig {
    /// Configuration for CLI output (colored Ariadne).
    pub fn cli() -> Self {
        Self {
            format: DiagnosticFormat::Ariadne,
            color: true,
            show_error_codes: true,
        }
    }

    /// Configuration for test output (no color Ariadne).
    pub fn test() -> Self {
        Self {
            format: DiagnosticFormat::Ariadne,
            color: false,
            show_error_codes: true,
        }
    }

    /// Configuration for concise one-line output.
    pub fn concise() -> Self {
        Self {
            format: DiagnosticFormat::Concise,
            color: false,
            show_error_codes: true,
        }
    }
}

/// Render a single diagnostic to a string.
pub fn render_diagnostic(
    diagnostic: &Diagnostic,
    sources: &HashMap<FileId, String>,
    config: &RenderConfig,
) -> String {
    match config.format {
        DiagnosticFormat::Ariadne => render_ariadne(diagnostic, sources, config.color),
        DiagnosticFormat::Concise => render_concise(diagnostic, sources),
    }
}

/// Render multiple diagnostics to a string.
pub fn render_diagnostics(
    diagnostics: &[Diagnostic],
    sources: &HashMap<FileId, String>,
    config: &RenderConfig,
) -> String {
    diagnostics
        .iter()
        .map(|d| render_diagnostic(d, sources, config))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render a diagnostic using Ariadne (pretty CLI output).
fn render_ariadne(
    diagnostic: &Diagnostic,
    sources: &HashMap<FileId, String>,
    color: bool,
) -> String {
    let report_kind = match diagnostic.severity {
        Severity::Error => ReportKind::Error,
        Severity::Warning => ReportKind::Warning,
        Severity::Info => ReportKind::Advice,
    };

    // Get the primary span for the report location
    let primary_span = diagnostic.primary_span().unwrap_or_else(|| {
        // Fallback: use first annotation if no primary
        diagnostic
            .annotations
            .first()
            .map(|a| a.span)
            .unwrap_or(Span {
                file_id: FileId::new(0),
                range: text_size::TextRange::new(0.into(), 0.into()),
            })
    });

    // Build the report
    let mut builder = Report::build(report_kind, primary_span).with_message(&diagnostic.message);

    // Add labels for each annotation
    for annotation in &diagnostic.annotations {
        let label = if let Some(msg) = &annotation.message {
            Label::new(annotation.span).with_message(msg)
        } else {
            Label::new(annotation.span)
        };
        builder = builder.with_label(label);
    }

    // Add note with error code
    builder = builder.with_note(format!("Error code: {}", diagnostic.code()));

    let report = builder
        .with_config(ariadne::Config::default().with_color(color))
        .finish();

    // Render to string
    render_report_to_string(&report, sources)
}

/// Render a diagnostic in concise one-line format.
fn render_concise(diagnostic: &Diagnostic, sources: &HashMap<FileId, String>) -> String {
    let span = diagnostic.primary_span();

    let location = if let Some(span) = span {
        if let Some(source) = sources.get(&span.file_id) {
            let line = source[..span.range.start().into()]
                .chars()
                .filter(|c| *c == '\n')
                .count()
                + 1;
            let line_start = source[..span.range.start().into()]
                .rfind('\n')
                .map(|p| p + 1)
                .unwrap_or(0);
            let col: usize = span.range.start().into();
            let col = col - line_start + 1;
            format!("{line}:{col}:")
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    format!(
        "{} [{}] {}",
        location,
        diagnostic.code(),
        diagnostic.message
    )
}

/// Render an ariadne Report to a String.
fn render_report_to_string(report: &Report<'_, Span>, sources: &HashMap<FileId, String>) -> String {
    let mut output = Vec::new();

    // ariadne::sources expects types that implement AsRef<str>
    let ariadne_sources: HashMap<FileId, String> = sources.clone();
    let mut cache = ariadne::sources(ariadne_sources);

    report.write(&mut cache, &mut output).unwrap_or_else(|_| {
        output.clear();
        output.extend_from_slice(b"<error rendering diagnostic>");
    });

    String::from_utf8_lossy(&output).into_owned()
}

#[cfg(test)]
mod tests {
    use text_size::TextRange;

    use super::*;
    use crate::diagnostic::DiagnosticId;

    fn make_source() -> HashMap<FileId, String> {
        let mut sources = HashMap::new();
        sources.insert(FileId::new(0), "class Foo {\n  name string\n}".to_string());
        sources
    }

    fn test_span() -> Span {
        Span {
            file_id: FileId::new(0),
            range: TextRange::new(6.into(), 9.into()), // "Foo"
        }
    }

    #[test]
    fn test_render_concise() {
        let diag = Diagnostic::error(DiagnosticId::DuplicateName, "Duplicate class 'Foo'")
            .with_primary_span(test_span());

        let sources = make_source();
        let output = render_diagnostic(&diag, &sources, &RenderConfig::concise());

        assert!(output.contains("[E0011]"));
        assert!(output.contains("Duplicate class 'Foo'"));
        assert!(output.contains("1:7:")); // line 1, column 7
    }

    #[test]
    fn test_render_ariadne() {
        let diag = Diagnostic::error(DiagnosticId::TypeMismatch, "Expected int, found string")
            .with_primary_span(test_span());

        let sources = make_source();
        let output = render_diagnostic(&diag, &sources, &RenderConfig::test());

        assert!(output.contains("Expected int, found string"));
        assert!(output.contains("Error code: E0001"));
    }
}
