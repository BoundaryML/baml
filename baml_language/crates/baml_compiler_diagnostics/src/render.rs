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
//! use baml_compiler_diagnostics::{Diagnostic, DiagnosticFormat, RenderConfig, render_diagnostic};
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

use std::{collections::HashMap, fmt, path::PathBuf};

use ariadne::{Label, Report, ReportKind, Source};
use baml_base::{FileId, Span};

use crate::diagnostic::{Diagnostic, Severity};

// ============================================================================
// SourceCache - Ariadne cache that displays filenames instead of file IDs
// ============================================================================

/// A cache for ariadne that displays filenames instead of file IDs.
///
/// This implements `ariadne::Cache<FileId>` with a `display()` method that
/// returns the filename from `file_paths` instead of the raw `FileId` integer.
///
/// ## Example
///
/// ```ignore
/// let cache = SourceCache::new(sources, file_paths);
/// report.write(&mut cache, &mut output)?;
/// // Output shows: syntax_errors.baml:18:19 (not 0:18:19)
/// ```
pub struct SourceCache {
    sources: HashMap<FileId, Source<String>>,
    file_paths: HashMap<FileId, PathBuf>,
}

/// Sentinel file ID used for fake/default spans.
const SENTINEL_FILE_ID: u32 = u32::MAX;

impl SourceCache {
    /// Create a new source cache from source text and file paths.
    pub fn new(sources: HashMap<FileId, String>, file_paths: HashMap<FileId, PathBuf>) -> Self {
        let mut ariadne_sources: HashMap<FileId, Source<String>> = sources
            .into_iter()
            .map(|(id, text)| (id, Source::from(text)))
            .collect();

        // Add a dummy source for the sentinel file ID to avoid errors when
        // diagnostics have fake/default spans (e.g., for errors without location)
        ariadne_sources.insert(FileId::new(SENTINEL_FILE_ID), Source::from(String::new()));

        Self {
            sources: ariadne_sources,
            file_paths,
        }
    }
}

/// Helper struct for displaying file IDs as filenames.
struct FilePathDisplay {
    file_id: FileId,
    path: Option<PathBuf>,
}

impl fmt::Display for FilePathDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref path) = self.path {
            // Use just the filename for cleaner output
            if let Some(name) = path.file_name() {
                return write!(f, "{}", name.to_string_lossy());
            }
            // Fall back to full path if no filename
            return write!(f, "{}", path.display());
        }
        // Fall back to file ID if no path available
        write!(f, "{}", self.file_id)
    }
}

#[allow(refining_impl_trait)]
impl ariadne::Cache<FileId> for SourceCache {
    type Storage = String;

    fn fetch(&mut self, id: &FileId) -> Result<&Source<Self::Storage>, Box<dyn fmt::Debug + '_>> {
        self.sources
            .get(id)
            .ok_or_else(|| Box::new(format!("Unknown file ID: {id}")) as Box<dyn fmt::Debug>)
    }

    fn display<'a>(&self, id: &'a FileId) -> Option<Box<dyn fmt::Display + 'a>> {
        let path = self.file_paths.get(id).cloned();
        Some(Box::new(FilePathDisplay { file_id: *id, path }))
    }
}

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
///
/// The `file_paths` map is used to display filenames in the output instead of
/// raw file IDs. Pass an empty map to fall back to file ID display.
pub fn render_diagnostic(
    diagnostic: &Diagnostic,
    sources: &HashMap<FileId, String>,
    file_paths: &HashMap<FileId, PathBuf>,
    config: &RenderConfig,
) -> String {
    match config.format {
        DiagnosticFormat::Ariadne => render_ariadne(diagnostic, sources, file_paths, config.color),
        DiagnosticFormat::Concise => render_concise(diagnostic, sources, file_paths),
    }
}

/// Render multiple diagnostics to a string.
///
/// The `file_paths` map is used to display filenames in the output instead of
/// raw file IDs. Pass an empty map to fall back to file ID display.
pub fn render_diagnostics(
    diagnostics: &[Diagnostic],
    sources: &HashMap<FileId, String>,
    file_paths: &HashMap<FileId, PathBuf>,
    config: &RenderConfig,
) -> String {
    diagnostics
        .iter()
        .map(|d| render_diagnostic(d, sources, file_paths, config))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render a diagnostic using Ariadne (pretty CLI output).
fn render_ariadne(
    diagnostic: &Diagnostic,
    sources: &HashMap<FileId, String>,
    file_paths: &HashMap<FileId, PathBuf>,
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
            // Use sentinel for fake spans (matches Span::fake())
            .unwrap_or(Span {
                file_id: FileId::new(SENTINEL_FILE_ID),
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

    // Render to string using SourceCache for proper filename display
    render_report_to_string(&report, sources, file_paths)
}

/// Render a diagnostic in concise one-line format.
fn render_concise(
    diagnostic: &Diagnostic,
    sources: &HashMap<FileId, String>,
    file_paths: &HashMap<FileId, PathBuf>,
) -> String {
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

            // Use filename if available
            let filename = file_paths
                .get(&span.file_id)
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("{}", span.file_id));

            format!("{filename}:{line}:{col}:")
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

/// Render an ariadne Report to a String using `SourceCache` for proper filename display.
fn render_report_to_string(
    report: &Report<'_, Span>,
    sources: &HashMap<FileId, String>,
    file_paths: &HashMap<FileId, PathBuf>,
) -> String {
    let mut output = Vec::new();

    // Use SourceCache for proper filename display
    let mut cache = SourceCache::new(sources.clone(), file_paths.clone());

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

    fn make_file_paths() -> HashMap<FileId, PathBuf> {
        let mut paths = HashMap::new();
        paths.insert(FileId::new(0), PathBuf::from("test.baml"));
        paths
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
        let file_paths = make_file_paths();
        let output = render_diagnostic(&diag, &sources, &file_paths, &RenderConfig::concise());

        assert!(output.contains("[E0011]"));
        assert!(output.contains("Duplicate class 'Foo'"));
        assert!(output.contains("test.baml:1:7:")); // filename, line 1, column 7
    }

    #[test]
    fn test_render_ariadne() {
        let diag = Diagnostic::error(DiagnosticId::TypeMismatch, "Expected int, found string")
            .with_primary_span(test_span());

        let sources = make_source();
        let file_paths = make_file_paths();
        let output = render_diagnostic(&diag, &sources, &file_paths, &RenderConfig::test());

        assert!(output.contains("Expected int, found string"));
        assert!(output.contains("Error code: E0001"));
        assert!(output.contains("test.baml")); // Should show filename
    }

    #[test]
    fn test_render_ariadne_shows_filename() {
        let diag = Diagnostic::error(DiagnosticId::TypeMismatch, "Test error")
            .with_primary_span(test_span());

        let sources = make_source();
        let file_paths = make_file_paths();
        let output = render_diagnostic(&diag, &sources, &file_paths, &RenderConfig::test());

        // Should show "test.baml:1:7" instead of "0:1:7"
        assert!(
            output.contains("test.baml"),
            "Expected filename in output, got: {output}"
        );
        assert!(
            !output.contains("─[ 0:"),
            "Should not show file ID, got: {output}"
        );
    }
}
