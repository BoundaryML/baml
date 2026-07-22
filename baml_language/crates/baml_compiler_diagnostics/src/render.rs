//! Multi-format rendering for diagnostics.
//!
//! This module provides rendering of unified `Diagnostic` types to various formats:
//! - **Ariadne**: Beautiful CLI output with colors and source snippets
//! - **Agent**: Compact exact locations without source diagrams
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

use ariadne::{Fmt, Label, Report, ReportKind, Source};
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

impl SourceCache {
    /// Create a new source cache from source text and file paths.
    pub fn new(sources: HashMap<FileId, String>, file_paths: HashMap<FileId, PathBuf>) -> Self {
        let mut ariadne_sources: HashMap<FileId, Source<String>> = sources
            .into_iter()
            .map(|(id, text)| (id, Source::from(text)))
            .collect();

        // Add a dummy source for the sentinel file ID to avoid errors when
        // diagnostics have fake/default spans (e.g., for errors without location)
        ariadne_sources.insert(FileId::sentinel(), Source::from(String::new()));

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
    /// Compact location-based output for coding agents.
    Agent,
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
    /// Configuration for human CLI output, always colored (Ariadne).
    pub fn cli() -> Self {
        Self {
            format: DiagnosticFormat::Ariadne,
            color: true,
            show_error_codes: true,
        }
    }

    /// Configuration for compact agent output.
    pub fn agent() -> Self {
        Self {
            format: DiagnosticFormat::Agent,
            color: false,
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
        DiagnosticFormat::Ariadne => {
            // Single-diagnostic path builds its own one-shot cache. The batch
            // path (`render_diagnostics`) shares one cache across diagnostics
            // instead; this cache is exactly what that code used to build
            // per-diagnostic, so the rendered bytes are unchanged.
            let mut cache = SourceCache::new(sources.clone(), file_paths.clone());
            render_ariadne(diagnostic, sources, &mut cache, config.color)
        }
        DiagnosticFormat::Agent => {
            let mut path_cache = HashMap::new();
            render_agent(
                diagnostic,
                sources,
                file_paths,
                &mut path_cache,
                config.color,
                config.show_error_codes,
            )
        }
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
    // Build the ariadne `SourceCache` ONCE for the whole batch. Previously
    // each diagnostic reconstructed its own cache inside
    // `render_report_to_string`, and every `SourceCache::new` eagerly clones
    // `sources`/`file_paths` and ariadne then computes a line index over every
    // touched file. On warning-heavy projects that per-diagnostic rebuild was a
    // large fraction of `baml check` wall time. Hoisting it is a pure
    // construction move: the cache handed to `render_ariadne` is byte-for-byte
    // the same object it would have built itself, so rendered output is
    // identical (safe under `BAML_CACHE_VERIFY`).
    let mut ariadne_cache = matches!(config.format, DiagnosticFormat::Ariadne)
        .then(|| SourceCache::new(sources.clone(), file_paths.clone()));
    let mut path_cache = HashMap::new();
    diagnostics
        .iter()
        .map(|d| match (&config.format, &mut ariadne_cache) {
            (DiagnosticFormat::Ariadne, Some(cache)) => {
                render_ariadne(d, sources, cache, config.color)
            }
            (DiagnosticFormat::Agent, _) => render_agent(
                d,
                sources,
                file_paths,
                &mut path_cache,
                config.color,
                config.show_error_codes,
            ),
            _ => render_concise(d, sources, file_paths),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Translate a byte offset into a character offset within `text`.
///
/// Ariadne's `Source` indexes by characters (codepoints), but our `Span`
/// values come from `text_size::TextRange` and use byte offsets. For ASCII
/// the two coincide; for files with multi-byte UTF-8 (e.g. em-dashes) they
/// don't, and ariadne would otherwise render at the wrong line/column or
/// fail to render at all when a byte offset is past the char-count of the
/// source.
fn byte_to_char(text: &str, byte_offset: usize) -> usize {
    let cap = byte_offset.min(text.len());
    text[..cap].chars().count()
}

fn translate_span(span: Span, sources: &HashMap<FileId, String>) -> Span {
    let Some(source) = sources.get(&span.file_id) else {
        return span;
    };
    let start = byte_to_char(source, span.range.start().into());
    let end = byte_to_char(source, span.range.end().into()).max(start);
    Span {
        file_id: span.file_id,
        range: text_size::TextRange::new(
            text_size::TextSize::new(u32::try_from(start).unwrap_or(u32::MAX)),
            text_size::TextSize::new(u32::try_from(end).unwrap_or(u32::MAX)),
        ),
    }
}

/// Render a diagnostic using Ariadne (pretty CLI output).
///
/// Takes a pre-built [`SourceCache`] by `&mut` so batch rendering can share a
/// single line-index computation across all diagnostics instead of rebuilding
/// it per diagnostic. The cache carries the same `sources`/`file_paths` used
/// elsewhere here, so filename display and rendered bytes are unchanged.
fn render_ariadne(
    diagnostic: &Diagnostic,
    sources: &HashMap<FileId, String>,
    cache: &mut SourceCache,
    color: bool,
) -> String {
    // BAML CLI uses lowercase `error:` / `warning:` everywhere (matching
    // cargo, rustc, clap) — ariadne's built-in `ReportKind::Error` /
    // `Warning` render capitalized, so swap to `Custom(name, color)`
    // with the same red/yellow ariadne would have picked anyway.
    //
    // (and yes, ariadne not letting `Custom` participate in
    // `with_color(false)` is genuinely dumb — `Custom(_, color)` is the
    // one match arm that unconditionally returns `Some(color)` regardless
    // of the config flag, so plain-text mode leaks ANSI escapes for the
    // keyword. We work around it by stripping ANSI from the rendered
    // string below when `color = false`. Real fix would be upstreaming a
    // `Custom { name, color: Option<Color> }` shape; until then, strip.)
    let report_kind = match diagnostic.severity {
        Severity::Error => ReportKind::Custom("error", ariadne::Color::Red),
        Severity::Warning => ReportKind::Custom("warning", ariadne::Color::Yellow),
        // `Advice` (info) keeps ariadne's default 147 (a soft blue/purple).
        Severity::Info => ReportKind::Custom("info", ariadne::Color::Fixed(147)),
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
                file_id: FileId::sentinel(),
                range: text_size::TextRange::new(0.into(), 0.into()),
            })
    });
    let primary_span = translate_span(primary_span, sources);

    // Build the report
    let mut builder = Report::build(report_kind, primary_span).with_message(&diagnostic.message);

    // Add labels for each annotation
    for annotation in &diagnostic.annotations {
        let span = translate_span(annotation.span, sources);
        let label = if let Some(msg) = &annotation.message {
            Label::new(span).with_message(msg)
        } else {
            Label::new(span)
        };
        builder = builder.with_label(label);
    }

    // Add note with error code
    builder = builder.with_note(format!("Error code: {}", diagnostic.code()));

    let report = builder
        .with_config(ariadne::Config::default().with_color(color))
        .finish();

    // Render to string using SourceCache for proper filename display.
    let rendered = render_report_to_string(&report, cache);

    // See the rant above the `report_kind` match — ariadne paints the
    // `Custom` keyword unconditionally, so `with_color(false)` doesn't
    // reach it. Strip ANSI from the final string so snapshots / piped
    // output / `NO_COLOR` users get plain text without losing the
    // lowercase keyword we get from `Custom`.
    if color {
        rendered
    } else {
        strip_ansi(&rendered)
    }
}

/// Strip ANSI SGR escape sequences (`\x1b[...m`) from `s`. Inlined to
/// avoid a dep on `strip-ansi-escapes` for ~25 lines — only used in the
/// no-color path of [`render_ariadne`].
///
/// Preserves malformed input:
///   - `\x1b` not followed by `[` is kept verbatim (a stray ESC byte is
///     unusual but shouldn't silently eat the next character).
///   - `\x1b[…` with no terminating `m` (truncated stream) is written
///     back verbatim instead of dropping every character to EOF.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        // Peek — don't consume — so a non-`[` follow-up survives.
        if chars.peek() != Some(&'[') {
            out.push('\x1b');
            continue;
        }
        chars.next(); // commit to consuming the `[`
        // Buffer the body in case the sequence is unterminated; we
        // need to write it back verbatim then. SGR bodies are short
        // (a handful of digits + `;`s), so the allocation is trivial.
        let mut buffered = String::new();
        let mut terminated = false;
        for inner in chars.by_ref() {
            if inner == 'm' {
                terminated = true;
                break;
            }
            buffered.push(inner);
        }
        if !terminated {
            // Truncated escape — surface the bytes rather than swallow
            // the rest of the line.
            out.push('\x1b');
            out.push('[');
            out.push_str(&buffered);
        }
    }
    out
}

/// Render one diagnostic as compact, location-first text for coding agents.
fn render_agent(
    diagnostic: &Diagnostic,
    sources: &HashMap<FileId, String>,
    file_paths: &HashMap<FileId, PathBuf>,
    path_cache: &mut HashMap<FileId, String>,
    color: bool,
    show_error_codes: bool,
) -> String {
    let primary_index = diagnostic
        .annotations
        .iter()
        .position(|annotation| annotation.is_primary)
        .or((!diagnostic.annotations.is_empty()).then_some(0));

    let level = severity_name(diagnostic.severity);
    let label = if show_error_codes {
        format!("{level}[{}]", diagnostic.code())
    } else {
        level.to_string()
    };
    let label = if color {
        label.fg(severity_color(diagnostic.severity)).to_string()
    } else {
        label
    };

    let mut lines =
        Vec::with_capacity(1 + diagnostic.annotations.len() + diagnostic.related_info.len());
    if let Some(index) = primary_index {
        let location = format_span(
            diagnostic.annotations[index].span,
            sources,
            file_paths,
            path_cache,
        );
        lines.push(format!("{location} {label}: {}", diagnostic.message));

        if diagnostic.annotations[index]
            .message
            .as_deref()
            .is_some_and(|message| message != diagnostic.message)
        {
            lines.push(format!(
                "  primary: {}",
                diagnostic.annotations[index]
                    .message
                    .as_deref()
                    .unwrap_or_default()
            ));
        }
    } else {
        lines.push(format!("{label}: {}", diagnostic.message));
    }

    for (index, annotation) in diagnostic.annotations.iter().enumerate() {
        if Some(index) == primary_index {
            continue;
        }
        let kind = if annotation.is_primary {
            "primary"
        } else {
            "secondary"
        };
        let location = format_span(annotation.span, sources, file_paths, path_cache);
        match &annotation.message {
            Some(message) => lines.push(format!("  {kind} {location}: {message}")),
            None => lines.push(format!("  {kind} {location}")),
        }
    }

    for related in &diagnostic.related_info {
        let location = related.file_path.as_ref().map_or_else(
            || format_span(related.span, sources, file_paths, path_cache),
            |path| format_span_with_path(related.span, path, sources),
        );
        lines.push(format!("  related {location}: {}", related.message));
    }

    lines.join("\n")
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

fn severity_color(severity: Severity) -> ariadne::Color {
    match severity {
        Severity::Error => ariadne::Color::Red,
        Severity::Warning => ariadne::Color::Yellow,
        Severity::Info => ariadne::Color::Fixed(147),
    }
}

fn format_span(
    span: Span,
    sources: &HashMap<FileId, String>,
    file_paths: &HashMap<FileId, PathBuf>,
    path_cache: &mut HashMap<FileId, String>,
) -> String {
    let path = path_cache
        .entry(span.file_id)
        .or_insert_with(|| shortest_unique_path(span.file_id, file_paths));
    format_span_with_path(span, path, sources)
}

fn format_span_with_path(span: Span, path: &str, sources: &HashMap<FileId, String>) -> String {
    let Some(source) = sources.get(&span.file_id) else {
        let start: u32 = span.range.start().into();
        let end: u32 = span.range.end().into();
        return format!("{path}:bytes:{start}-{end}");
    };

    let start: usize = span.range.start().into();
    let end: usize = span.range.end().into();
    let start = line_column(source, start);
    let end = line_column(source, end);
    if start == end {
        format!("{path}:{}:{}", start.0, start.1)
    } else {
        format!("{path}:{}:{}-{}:{}", start.0, start.1, end.0, end.1)
    }
}

/// Return a 1-based Unicode-scalar line and column for a byte offset.
fn line_column(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut offset = byte_offset.min(source.len());
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let line_start = prefix.rfind('\n').map_or(0, |position| position + 1);
    let column = source[line_start..offset].chars().count() + 1;
    (line, column)
}

/// Use the shortest path suffix that uniquely identifies this file in the
/// render batch. Most projects get a filename; duplicate filenames retain just
/// enough parent directories to disambiguate them.
fn shortest_unique_path(file_id: FileId, file_paths: &HashMap<FileId, PathBuf>) -> String {
    let Some(path) = file_paths.get(&file_id) else {
        return file_id.to_string();
    };

    let mut suffix = PathBuf::new();
    for component in path.components().rev() {
        suffix = if suffix.as_os_str().is_empty() {
            PathBuf::from(component.as_os_str())
        } else {
            PathBuf::from(component.as_os_str()).join(suffix)
        };
        let unique = file_paths
            .iter()
            .filter(|(other_id, _)| **other_id != file_id)
            .all(|(_, other)| !other.ends_with(&suffix));
        if unique {
            return suffix.display().to_string();
        }
    }
    path.display().to_string()
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
///
/// The `SourceCache` is now supplied by the caller (built once per batch)
/// rather than reconstructed here per diagnostic. `report.write` only reads
/// from the cache (populating ariadne's internal line index lazily), so
/// reusing one cache across reports produces identical bytes.
fn render_report_to_string(report: &Report<'_, Span>, cache: &mut SourceCache) -> String {
    let mut output = Vec::new();

    report.write(cache, &mut output).unwrap_or_else(|_| {
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

    // ── strip_ansi: malformed-input preservation ───────────────────────

    /// Well-formed SGR sequence: stripped, surrounding text intact.
    #[test]
    fn strip_ansi_removes_well_formed_sgr() {
        assert_eq!(
            strip_ansi("hello \x1b[31mred\x1b[0m world"),
            "hello red world"
        );
    }

    /// Bare ESC (no `[` follow-up) is preserved together with the next
    /// character — regression for the earlier bug where `chars.next()`
    /// dropped the byte after ESC unconditionally.
    #[test]
    fn strip_ansi_preserves_bare_escape_and_next_char() {
        assert_eq!(strip_ansi("a\x1bXb"), "a\x1bXb");
        // Trailing bare ESC (no follow-up at all) survives too.
        assert_eq!(strip_ansi("end\x1b"), "end\x1b");
    }

    /// Unterminated `ESC[…` (truncated stream — no `m`) is written back
    /// verbatim instead of swallowing the rest of the input. Without
    /// this the old loop would drop everything past the `[`.
    #[test]
    fn strip_ansi_preserves_unterminated_csi() {
        assert_eq!(strip_ansi("ok \x1b[31;1"), "ok \x1b[31;1");
        assert_eq!(strip_ansi("\x1b[31"), "\x1b[31");
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

    #[test]
    fn agent_render_is_compact_and_does_not_repeat_the_primary_message() {
        let diag = Diagnostic::error(DiagnosticId::DuplicateName, "Duplicate class 'Foo'")
            .with_primary_span(test_span());

        let output = render_diagnostic(
            &diag,
            &make_source(),
            &make_file_paths(),
            &RenderConfig::agent(),
        );

        assert_eq!(
            output,
            "test.baml:1:7-1:10 error[E0011]: Duplicate class 'Foo'"
        );
    }

    #[test]
    fn agent_render_preserves_secondary_and_related_locations() {
        let mut sources = HashMap::new();
        sources.insert(FileId::new(0), "class Foo {}".to_string());
        sources.insert(FileId::new(1), "class Foo {}".to_string());
        let mut paths = HashMap::new();
        paths.insert(FileId::new(0), PathBuf::from("/project/first/main.baml"));
        paths.insert(FileId::new(1), PathBuf::from("/project/second/main.baml"));

        let first = Span {
            file_id: FileId::new(0),
            range: TextRange::new(6.into(), 9.into()),
        };
        let second = Span {
            file_id: FileId::new(1),
            range: TextRange::new(6.into(), 9.into()),
        };
        let diag = Diagnostic::error(DiagnosticId::DuplicateName, "Duplicate class 'Foo'")
            .with_primary_span(second)
            .with_secondary(first, "Conflicts with this declaration")
            .with_related(first, "First defined here");

        let output = render_diagnostic(&diag, &sources, &paths, &RenderConfig::agent());

        assert_eq!(
            output,
            "second/main.baml:1:7-1:10 error[E0011]: Duplicate class 'Foo'\n  secondary first/main.baml:1:7-1:10: Conflicts with this declaration\n  related first/main.baml:1:7-1:10: First defined here"
        );
    }

    #[test]
    fn agent_render_uses_character_columns_for_utf8() {
        let name = "caf\u{e9}";
        let source = format!("let {name} = 1");
        let start = source.find(name).unwrap();
        let end = start + name.len();
        let span = Span {
            file_id: FileId::new(0),
            range: TextRange::new(
                u32::try_from(start).unwrap().into(),
                u32::try_from(end).unwrap().into(),
            ),
        };
        let message = format!("Unknown variable `{name}`");
        let diag =
            Diagnostic::error(DiagnosticId::UnknownVariable, &message).with_primary_span(span);
        let sources = HashMap::from([(FileId::new(0), source)]);

        let output = render_diagnostic(&diag, &sources, &make_file_paths(), &RenderConfig::agent());

        assert_eq!(output, format!("test.baml:1:5-1:9 error[E0003]: {message}"));
    }
}
