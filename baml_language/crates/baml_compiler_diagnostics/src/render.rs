//! Multi-format rendering for diagnostics.
//!
//! This module provides rendering of unified `Diagnostic` types to various formats:
//! - **Human**: Graphical CLI output with colors and source snippets
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
//! let cli_output = render_diagnostic(&diag, &sources, &file_paths, &RenderConfig::default());
//!
//! // Render concise (for tests)
//! let concise = render_diagnostic(&diag, &sources, &file_paths, &RenderConfig::concise());
//! ```

use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use baml_base::{FileId, Span};
use miette::{
    Diagnostic as MietteDiagnosticTrait, GraphicalReportHandler, GraphicalTheme, LabeledSpan,
    MietteError, MietteSpanContents, NamedSource, Severity as MietteSeverity, SourceCode,
    SourceSpan, SpanContents,
    highlighters::{Highlighter, HighlighterState},
};
use owo_colors::{Style as OwoStyle, Styled};

use crate::{
    diagnostic::{Annotation, Diagnostic, Severity},
    highlight::{
        DiagnosticMessageHighlightError, DiagnosticMessageHighlighter, HighlightAttributes,
        HighlightColor, HighlightSpan, HighlightStyle, SourceHighlights,
    },
    message::{DiagnosticIdentifierKind, DiagnosticMessageHighlight, DiagnosticMessageKind},
};

/// Output format for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagnosticFormat {
    /// Full human output with colors and source context.
    #[default]
    Human,
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
            format: DiagnosticFormat::Human,
            color: true,
            show_error_codes: true,
        }
    }
}

impl RenderConfig {
    /// Configuration for human CLI output, always colored.
    pub fn cli() -> Self {
        Self {
            format: DiagnosticFormat::Human,
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

    /// Configuration for test output without color.
    pub fn test() -> Self {
        Self {
            format: DiagnosticFormat::Human,
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
        DiagnosticFormat::Human => render_miette(
            diagnostic,
            sources,
            file_paths,
            &SourceHighlights::new(),
            config.color,
            config.show_error_codes,
        ),
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
    render_diagnostics_with_highlights(
        diagnostics,
        sources,
        file_paths,
        &SourceHighlights::new(),
        config,
    )
}

pub fn render_diagnostics_with_highlights(
    diagnostics: &[Diagnostic],
    sources: &HashMap<FileId, String>,
    file_paths: &HashMap<FileId, PathBuf>,
    highlights: &SourceHighlights,
    config: &RenderConfig,
) -> String {
    render_diagnostics_with_highlighters(diagnostics, sources, file_paths, highlights, None, config)
}

pub fn render_diagnostics_with_highlighters(
    diagnostics: &[Diagnostic],
    sources: &HashMap<FileId, String>,
    file_paths: &HashMap<FileId, PathBuf>,
    source_highlights: &SourceHighlights,
    message_highlighter: Option<&dyn DiagnosticMessageHighlighter>,
    config: &RenderConfig,
) -> String {
    let mut path_cache = HashMap::new();
    let highlighter = (config.format == DiagnosticFormat::Human && config.color)
        .then(|| DiagnosticHighlighter::new(source_highlights, file_paths));
    diagnostics
        .iter()
        .map(|diagnostic| match config.format {
            DiagnosticFormat::Human => {
                let handler =
                    miette_handler(highlighter.clone(), config.color, diagnostic.severity);
                match render_miette_with_handler(
                    diagnostic,
                    sources,
                    file_paths,
                    &handler,
                    config.color,
                    config.show_error_codes,
                    message_highlighter,
                ) {
                    Ok(output) => output,
                    Err(DiagnosticMessageHighlightError) => {
                        let handler = miette_handler(None, false, diagnostic.severity);
                        render_miette_with_handler(
                            diagnostic,
                            sources,
                            file_paths,
                            &handler,
                            false,
                            config.show_error_codes,
                            None,
                        )
                        .expect("no-color diagnostics do not invoke the message highlighter")
                    }
                }
            }
            DiagnosticFormat::Agent => render_agent(
                diagnostic,
                sources,
                file_paths,
                &mut path_cache,
                config.color,
                config.show_error_codes,
            ),
            DiagnosticFormat::Concise => render_concise(diagnostic, sources, file_paths),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug)]
struct RenderedDiagnostic {
    message: String,
    code: Option<&'static str>,
    severity: MietteSeverity,
    source: Option<NamedSource<FullLineSource>>,
    labels: Vec<LabeledSpan>,
    related: Vec<RenderedDiagnostic>,
}

impl fmt::Display for RenderedDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RenderedDiagnostic {}

impl MietteDiagnosticTrait for RenderedDiagnostic {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.code
            .map(|code| Box::new(code) as Box<dyn fmt::Display>)
    }

    fn severity(&self) -> Option<MietteSeverity> {
        Some(self.severity)
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        self.source.as_ref().map(|source| source as &dyn SourceCode)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        (!self.labels.is_empty()).then(|| Box::new(self.labels.iter().cloned()) as Box<_>)
    }

    fn related<'a>(
        &'a self,
    ) -> Option<Box<dyn Iterator<Item = &'a dyn MietteDiagnosticTrait> + 'a>> {
        (!self.related.is_empty()).then(|| {
            Box::new(
                self.related
                    .iter()
                    .map(|diagnostic| diagnostic as &dyn MietteDiagnosticTrait),
            ) as Box<_>
        })
    }
}

fn render_miette(
    diagnostic: &Diagnostic,
    sources: &HashMap<FileId, String>,
    file_paths: &HashMap<FileId, PathBuf>,
    highlights: &SourceHighlights,
    color: bool,
    show_error_codes: bool,
) -> String {
    let highlighter = color.then(|| DiagnosticHighlighter::new(highlights, file_paths));
    let handler = miette_handler(highlighter, color, diagnostic.severity);
    render_miette_with_handler(
        diagnostic,
        sources,
        file_paths,
        &handler,
        color,
        show_error_codes,
        None,
    )
    .expect("rendering without a message highlighter cannot fail semantic highlighting")
}

fn miette_handler(
    highlighter: Option<DiagnosticHighlighter>,
    color: bool,
    severity: Severity,
) -> GraphicalReportHandler {
    let mut theme = if color {
        GraphicalTheme::unicode()
    } else {
        GraphicalTheme::unicode_nocolor()
    };
    if color {
        theme.styles.highlights = annotation_styles(severity);
    }
    let mut handler = GraphicalReportHandler::new_themed(theme)
        .with_links(false)
        .with_urls(false)
        .with_show_related_as_nested(true)
        .with_break_words(false)
        .with_context_lines(0);
    if color {
        handler = handler.with_syntax_highlighting(
            highlighter.expect("colored diagnostics have a source highlighter"),
        );
    } else {
        handler = handler.without_syntax_highlighting();
    }
    handler
}

fn render_miette_with_handler(
    diagnostic: &Diagnostic,
    sources: &HashMap<FileId, String>,
    file_paths: &HashMap<FileId, PathBuf>,
    handler: &GraphicalReportHandler,
    color: bool,
    show_error_codes: bool,
    message_highlighter: Option<&dyn DiagnosticMessageHighlighter>,
) -> Result<String, DiagnosticMessageHighlightError> {
    let diagnostic = build_rendered_diagnostic(
        diagnostic,
        sources,
        file_paths,
        color,
        show_error_codes,
        message_highlighter,
    )?;
    let mut output = String::new();
    if handler.render_report(&mut output, &diagnostic).is_err() {
        return Ok("<error rendering diagnostic>".to_string());
    }
    if color {
        Ok(apply_message_styles(&output))
    } else {
        Ok(output)
    }
}

fn build_rendered_diagnostic(
    diagnostic: &Diagnostic,
    sources: &HashMap<FileId, String>,
    file_paths: &HashMap<FileId, PathBuf>,
    color: bool,
    show_error_codes: bool,
    message_highlighter: Option<&dyn DiagnosticMessageHighlighter>,
) -> Result<RenderedDiagnostic, DiagnosticMessageHighlightError> {
    let primary_file = diagnostic
        .primary_span()
        .or_else(|| {
            diagnostic
                .annotations
                .first()
                .map(|annotation| annotation.span)
        })
        .map(|span| span.file_id)
        .filter(|file_id| sources.contains_key(file_id));
    let mut primary_labels = diagnostic
        .annotations
        .iter()
        .filter(|annotation| Some(annotation.span.file_id) == primary_file)
        .collect::<Vec<_>>();
    primary_labels.sort_by_key(|annotation| !annotation.is_primary);

    let message = marked_message(
        &diagnostic.message,
        &diagnostic.message_highlights,
        MessageParentStyle::None,
        color,
        message_highlighter,
    )?;
    let labels = primary_labels
        .into_iter()
        .enumerate()
        .map(|(index, annotation)| {
            let message = annotation
                .message
                .as_ref()
                .filter(|message| message.as_str() != diagnostic.message)
                .map(|message| {
                    marked_message(
                        message,
                        &annotation.message_highlights,
                        MessageParentStyle::Annotation {
                            severity: diagnostic.severity,
                            index,
                        },
                        color,
                        message_highlighter,
                    )
                })
                .transpose()?;
            Ok(labeled_span(
                annotation.span,
                message,
                annotation.is_primary,
            ))
        })
        .collect::<Result<Vec<_>, DiagnosticMessageHighlightError>>()?;

    let mut rendered = RenderedDiagnostic {
        message,
        code: show_error_codes.then(|| diagnostic.code()),
        severity: miette_severity(diagnostic.severity),
        source: primary_file.and_then(|file_id| named_source(file_id, sources, file_paths)),
        labels,
        related: Vec::new(),
    };

    let mut related_files: Vec<(FileId, Vec<&Annotation>)> = Vec::new();
    for annotation in &diagnostic.annotations {
        let file_id = annotation.span.file_id;
        if Some(file_id) == primary_file || !sources.contains_key(&file_id) {
            continue;
        }
        if let Some((_, annotations)) = related_files
            .iter_mut()
            .find(|(related_file, _)| *related_file == file_id)
        {
            annotations.push(annotation);
        } else {
            related_files.push((file_id, vec![annotation]));
        }
    }
    for (file_id, annotations) in related_files {
        let related_message = annotations.iter().find_map(|annotation| {
            annotation
                .message
                .as_ref()
                .map(|message| (message, &annotation.message_highlights))
        });
        let message = match related_message {
            Some((message, highlights)) => marked_message(
                message,
                highlights,
                MessageParentStyle::None,
                color,
                message_highlighter,
            )?,
            None => "related location".to_string(),
        };
        rendered.related.push(RenderedDiagnostic {
            message,
            code: None,
            severity: MietteSeverity::Advice,
            source: named_source(file_id, sources, file_paths),
            labels: annotations
                .into_iter()
                .map(|annotation| labeled_span(annotation.span, None, false))
                .collect(),
            related: Vec::new(),
        });
    }

    for related in &diagnostic.related_info {
        rendered.related.push(RenderedDiagnostic {
            message: marked_message(
                &related.message,
                &related.message_highlights,
                MessageParentStyle::None,
                color,
                message_highlighter,
            )?,
            code: None,
            severity: MietteSeverity::Advice,
            source: named_source(related.span.file_id, sources, file_paths),
            labels: sources
                .contains_key(&related.span.file_id)
                .then(|| labeled_span(related.span, None, true))
                .into_iter()
                .collect(),
            related: Vec::new(),
        });
    }

    Ok(rendered)
}

const MESSAGE_STYLE_START: char = '\u{1d}';
const MESSAGE_STYLE_END: char = '\u{1e}';
const MESSAGE_STYLE_CODE_BASE: u32 = 0xe0100;
const MESSAGE_FOREGROUND_COUNT: u32 = 9;
const MESSAGE_ATTRIBUTE_COUNT: u32 = 16;

#[derive(Clone, Copy)]
enum MessageParentStyle {
    None,
    Annotation { severity: Severity, index: usize },
}

fn annotation_styles(severity: Severity) -> Vec<OwoStyle> {
    vec![
        severity_style(severity).bold(),
        OwoStyle::new().cyan().bold(),
        OwoStyle::new().green().bold(),
        OwoStyle::new().magenta().bold(),
    ]
}

fn marked_message(
    message: &str,
    highlights: &[DiagnosticMessageHighlight],
    parent: MessageParentStyle,
    color: bool,
    highlighter: Option<&dyn DiagnosticMessageHighlighter>,
) -> Result<String, DiagnosticMessageHighlightError> {
    if !color || highlights.is_empty() {
        return Ok(message.to_string());
    }

    let mut highlights = highlights.to_vec();
    highlights.sort_by_key(|highlight| highlight.start);
    let mut resolved = Vec::new();
    let mut cursor = 0;
    for highlight in highlights {
        let start = highlight.start as usize;
        let end = highlight.end as usize;
        if start < cursor
            || start >= end
            || end > message.len()
            || !message.is_char_boundary(start)
            || !message.is_char_boundary(end)
        {
            continue;
        }
        let fragment = &message[start..end];
        if let Some(highlighter) = highlighter {
            append_fragment_highlights(
                &mut resolved,
                start,
                fragment,
                highlighter.highlight(highlight.kind, fragment)?,
            );
        } else {
            resolved.push(ResolvedMessageHighlight {
                start,
                end,
                style: fallback_message_style(highlight.kind),
            });
        }
        cursor = end;
    }

    let mut output = String::with_capacity(message.len() + resolved.len() * 6);
    cursor = 0;
    for highlight in resolved {
        output.push_str(&message[cursor..highlight.start]);
        output.push(MESSAGE_STYLE_START);
        output.push(message_style_code(highlight.style));
        output.push_str(&message[highlight.start..highlight.end]);
        output.push(MESSAGE_STYLE_END);
        output.push(message_style_code(highlight.style));
        output.push(parent_style_id(parent));
        cursor = highlight.end;
    }
    output.push_str(&message[cursor..]);
    Ok(output)
}

#[derive(Clone, Copy)]
struct ResolvedMessageHighlight {
    start: usize,
    end: usize,
    style: HighlightStyle,
}

fn append_fragment_highlights(
    output: &mut Vec<ResolvedMessageHighlight>,
    message_start: usize,
    fragment: &str,
    mut highlights: Vec<HighlightSpan>,
) {
    highlights.sort_by_key(|highlight| highlight.range.start());
    let mut cursor = 0;
    for highlight in highlights {
        let start: usize = highlight.range.start().into();
        let end: usize = highlight.range.end().into();
        if start < cursor
            || start >= end
            || end > fragment.len()
            || !fragment.is_char_boundary(start)
            || !fragment.is_char_boundary(end)
        {
            continue;
        }
        if cursor < start {
            output.push(ResolvedMessageHighlight {
                start: message_start + cursor,
                end: message_start + start,
                style: HighlightStyle::default(),
            });
        }
        output.push(ResolvedMessageHighlight {
            start: message_start + start,
            end: message_start + end,
            style: highlight.style,
        });
        cursor = end;
    }
    if cursor < fragment.len() {
        output.push(ResolvedMessageHighlight {
            start: message_start + cursor,
            end: message_start + fragment.len(),
            style: HighlightStyle::default(),
        });
    }
}

fn fallback_message_style(kind: DiagnosticMessageKind) -> HighlightStyle {
    let foreground = match kind {
        DiagnosticMessageKind::Identifier(DiagnosticIdentifierKind::Type)
        | DiagnosticMessageKind::TypeExpression => HighlightColor::Yellow,
        DiagnosticMessageKind::Identifier(DiagnosticIdentifierKind::Function) => {
            HighlightColor::BrightBlue
        }
        DiagnosticMessageKind::Identifier(DiagnosticIdentifierKind::Field) => HighlightColor::Cyan,
        DiagnosticMessageKind::Identifier(DiagnosticIdentifierKind::Variable)
        | DiagnosticMessageKind::Code => HighlightColor::BrightCyan,
        DiagnosticMessageKind::Identifier(DiagnosticIdentifierKind::EnumVariant) => {
            HighlightColor::BrightYellow
        }
        DiagnosticMessageKind::Identifier(DiagnosticIdentifierKind::Attribute) => {
            HighlightColor::Magenta
        }
    };
    HighlightStyle {
        foreground: Some(foreground),
        attributes: HighlightAttributes::empty(),
    }
}

fn message_style_code(style: HighlightStyle) -> char {
    let foreground = match style.foreground {
        None => 0,
        Some(HighlightColor::Green) => 1,
        Some(HighlightColor::Yellow) => 2,
        Some(HighlightColor::Magenta) => 3,
        Some(HighlightColor::Cyan) => 4,
        Some(HighlightColor::BrightYellow) => 5,
        Some(HighlightColor::BrightBlue) => 6,
        Some(HighlightColor::BrightMagenta) => 7,
        Some(HighlightColor::BrightCyan) => 8,
    };
    let mut attributes = 0;
    if style.attributes.contains(HighlightAttributes::BOLD) {
        attributes |= 1;
    }
    if style.attributes.contains(HighlightAttributes::DIM) {
        attributes |= 2;
    }
    if style.attributes.contains(HighlightAttributes::ITALIC) {
        attributes |= 4;
    }
    if style
        .attributes
        .contains(HighlightAttributes::STRIKETHROUGH)
    {
        attributes |= 8;
    }
    char::from_u32(MESSAGE_STYLE_CODE_BASE + foreground + MESSAGE_FOREGROUND_COUNT * attributes)
        .expect("diagnostic message style code is a valid Unicode variation selector")
}

fn message_style(code: char) -> Option<OwoStyle> {
    let value = u32::from(code).checked_sub(MESSAGE_STYLE_CODE_BASE)?;
    if value >= MESSAGE_FOREGROUND_COUNT * MESSAGE_ATTRIBUTE_COUNT {
        return None;
    }
    let foreground = match value % MESSAGE_FOREGROUND_COUNT {
        0 => None,
        1 => Some(HighlightColor::Green),
        2 => Some(HighlightColor::Yellow),
        3 => Some(HighlightColor::Magenta),
        4 => Some(HighlightColor::Cyan),
        5 => Some(HighlightColor::BrightYellow),
        6 => Some(HighlightColor::BrightBlue),
        7 => Some(HighlightColor::BrightMagenta),
        8 => Some(HighlightColor::BrightCyan),
        _ => unreachable!(),
    };
    let attributes = value / MESSAGE_FOREGROUND_COUNT;
    let mut style = OwoStyle::new();
    style = match foreground {
        Some(HighlightColor::Green) => style.green(),
        Some(HighlightColor::Yellow) => style.yellow(),
        Some(HighlightColor::Magenta) => style.magenta(),
        Some(HighlightColor::Cyan) => style.cyan(),
        Some(HighlightColor::BrightYellow) => style.bright_yellow(),
        Some(HighlightColor::BrightBlue) => style.bright_blue(),
        Some(HighlightColor::BrightMagenta) => style.bright_magenta(),
        Some(HighlightColor::BrightCyan) => style.bright_cyan(),
        None => style,
    };
    if attributes & 1 != 0 {
        style = style.bold();
    }
    if attributes & 2 != 0 {
        style = style.dimmed();
    }
    if attributes & 4 != 0 {
        style = style.italic();
    }
    if attributes & 8 != 0 {
        style = style.strikethrough();
    }
    Some(style)
}

fn parent_style_id(parent: MessageParentStyle) -> char {
    match parent {
        MessageParentStyle::None => '\u{1}',
        MessageParentStyle::Annotation { severity, index } => {
            match index % annotation_styles(severity).len() {
                0 => match severity {
                    Severity::Error => '\u{2}',
                    Severity::Warning => '\u{3}',
                    Severity::Info => '\u{4}',
                },
                1 => '\u{4}',
                2 => '\u{5}',
                _ => '\u{6}',
            }
        }
    }
}

fn parent_style(id: char) -> Option<OwoStyle> {
    match id {
        '\u{2}' => Some(OwoStyle::new().red().bold()),
        '\u{3}' => Some(OwoStyle::new().yellow().bold()),
        '\u{4}' => Some(OwoStyle::new().cyan().bold()),
        '\u{5}' => Some(OwoStyle::new().green().bold()),
        '\u{6}' => Some(OwoStyle::new().magenta().bold()),
        _ => None,
    }
}

fn apply_message_styles(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut active_style = None;

    while let Some(ch) = chars.next() {
        if ch == MESSAGE_STYLE_START {
            let Some(id) = chars.next() else {
                output.push(ch);
                break;
            };
            if let Some(style) = message_style(id) {
                output.push_str("\u{1b}[0m");
                output.push_str(&style.prefix_formatter().to_string());
                active_style = Some(style);
            }
            continue;
        }
        if ch == MESSAGE_STYLE_END {
            let (Some(style_id), Some(parent_id)) = (chars.next(), chars.next()) else {
                output.push(ch);
                break;
            };
            if message_style(style_id).is_some() {
                output.push_str("\u{1b}[0m");
            }
            if let Some(parent) = parent_style(parent_id) {
                output.push_str(&parent.prefix_formatter().to_string());
            }
            active_style = None;
            continue;
        }
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            let mut sequence = String::from("\u{1b}");
            for part in chars.by_ref() {
                sequence.push(part);
                if part == 'm' {
                    break;
                }
            }
            let resets_style = sgr_resets_style(&sequence);
            output.push_str(&sequence);
            if resets_style {
                if let Some(style) = active_style {
                    output.push_str(&style.prefix_formatter().to_string());
                }
            }
            continue;
        }
        output.push(ch);
    }

    output
}

fn sgr_resets_style(sequence: &str) -> bool {
    let Some(parameters) = sequence
        .strip_prefix("\u{1b}[")
        .and_then(|sequence| sequence.strip_suffix('m'))
    else {
        return false;
    };
    parameters.split(';').any(|parameter| {
        matches!(
            parameter,
            "" | "0" | "22" | "23" | "24" | "25" | "27" | "28" | "29" | "39" | "49"
        )
    })
}

fn named_source(
    file_id: FileId,
    sources: &HashMap<FileId, String>,
    file_paths: &HashMap<FileId, PathBuf>,
) -> Option<NamedSource<FullLineSource>> {
    sources.get(&file_id).map(|source| {
        NamedSource::new(
            shortest_unique_path(file_id, file_paths),
            FullLineSource::new(source.clone()),
        )
        .with_language("baml")
    })
}

#[derive(Debug)]
struct FullLineSource {
    source: String,
    line_starts: Vec<usize>,
}

impl FullLineSource {
    fn new(source: String) -> Self {
        let mut line_starts = vec![0];
        let bytes = source.as_bytes();
        let mut offset = 0;
        while offset < bytes.len() {
            match bytes[offset] {
                b'\r' if bytes.get(offset + 1) == Some(&b'\n') => {
                    offset += 2;
                    line_starts.push(offset);
                }
                b'\n' => {
                    offset += 1;
                    line_starts.push(offset);
                }
                _ => offset += 1,
            }
        }
        Self {
            source,
            line_starts,
        }
    }

    fn line_index(&self, offset: usize) -> usize {
        self.line_starts
            .partition_point(|line_start| *line_start <= offset)
            .saturating_sub(1)
    }
}

impl SourceCode for FullLineSource {
    fn read_span<'a>(
        &'a self,
        span: &SourceSpan,
        context_lines_before: usize,
        context_lines_after: usize,
    ) -> Result<Box<dyn SpanContents<'a> + 'a>, MietteError> {
        let span_start = span.offset();
        let span_end = span_start
            .checked_add(span.len())
            .filter(|end| *end <= self.source.len())
            .ok_or(MietteError::OutOfBounds)?;
        if span_start > self.source.len() {
            return Err(MietteError::OutOfBounds);
        }

        let span_start_line = self.line_index(span_start);
        let span_end_line = self.line_index(if span_end > span_start {
            span_end - 1
        } else {
            span_start
        });
        let first_line = span_start_line.saturating_sub(context_lines_before);
        let last_line = span_end_line
            .saturating_add(context_lines_after)
            .min(self.line_starts.len() - 1);
        let data_start = self.line_starts[first_line];
        let data_end = self
            .line_starts
            .get(last_line + 1)
            .copied()
            .unwrap_or(self.source.len());
        let column = if context_lines_before == 0 {
            self.source
                .get(self.line_starts[span_start_line]..span_start)
                .ok_or(MietteError::OutOfBounds)?
                .chars()
                .count()
        } else {
            0
        };
        let line_count =
            last_line - first_line + usize::from(self.line_starts.get(last_line + 1).is_some());
        let contents = MietteSpanContents::new(
            &self.source.as_bytes()[data_start..data_end],
            (data_start, data_end - data_start).into(),
            first_line,
            column,
            line_count,
        );
        Ok(Box::new(contents))
    }
}

fn labeled_span(span: Span, message: Option<String>, primary: bool) -> LabeledSpan {
    let start: usize = span.range.start().into();
    let end: usize = span.range.end().into();
    let source_span = SourceSpan::from((start, end.saturating_sub(start)));
    if primary {
        LabeledSpan::new_primary_with_span(message, source_span)
    } else {
        LabeledSpan::new_with_span(message, source_span)
    }
}

fn miette_severity(severity: Severity) -> MietteSeverity {
    match severity {
        Severity::Error => MietteSeverity::Error,
        Severity::Warning => MietteSeverity::Warning,
        Severity::Info => MietteSeverity::Advice,
    }
}

#[derive(Debug, Clone)]
struct DiagnosticHighlighter {
    by_name: Arc<HashMap<String, Vec<HighlightSpan>>>,
}

impl DiagnosticHighlighter {
    fn new(highlights: &SourceHighlights, file_paths: &HashMap<FileId, PathBuf>) -> Self {
        let by_name = highlights
            .iter()
            .map(|(file_id, spans)| {
                let mut spans = spans.clone();
                spans.sort_by_key(|span| span.range.start());
                (shortest_unique_path(*file_id, file_paths), spans)
            })
            .collect();
        Self {
            by_name: Arc::new(by_name),
        }
    }
}

impl Highlighter for DiagnosticHighlighter {
    fn start_highlighter_state<'h>(
        &'h self,
        source: &dyn SpanContents<'_>,
    ) -> Box<dyn HighlighterState + 'h> {
        let spans = source
            .name()
            .and_then(|name| self.by_name.get(name))
            .map(Vec::as_slice)
            .unwrap_or_default();
        let base = source.span().offset();
        let mut line_starts = vec![base];
        line_starts.extend(
            source
                .data()
                .iter()
                .enumerate()
                .filter(|(_, byte)| **byte == b'\n')
                .map(|(index, _)| base + index + 1),
        );
        Box::new(DiagnosticHighlighterState {
            spans,
            line_starts,
            line_index: 0,
        })
    }
}

struct DiagnosticHighlighterState<'a> {
    spans: &'a [HighlightSpan],
    line_starts: Vec<usize>,
    line_index: usize,
}

impl HighlighterState for DiagnosticHighlighterState<'_> {
    fn highlight_line<'s>(&mut self, line: &'s str) -> Vec<Styled<&'s str>> {
        let line_start = self
            .line_starts
            .get(self.line_index)
            .copied()
            .unwrap_or_default();
        self.line_index += 1;
        let line_end = line_start + line.len();
        let mut output = Vec::new();
        let mut cursor = 0;
        for span in self.spans {
            let start: usize = span.range.start().into();
            let end: usize = span.range.end().into();
            if end <= line_start {
                continue;
            }
            if start >= line_end {
                break;
            }
            let start = start.max(line_start) - line_start;
            let end = end.min(line_end) - line_start;
            if start < cursor || start >= end {
                continue;
            }
            if !line.is_char_boundary(start) || !line.is_char_boundary(end) {
                continue;
            }
            if cursor < start {
                output.push(OwoStyle::new().style(&line[cursor..start]));
            }
            output.push(owo_style(span.style).style(&line[start..end]));
            cursor = end;
        }
        if cursor < line.len() || output.is_empty() {
            output.push(OwoStyle::new().style(&line[cursor..]));
        }
        output
    }
}

fn owo_style(style: HighlightStyle) -> OwoStyle {
    let mut output = match style.foreground {
        Some(HighlightColor::Green) => OwoStyle::new().green(),
        Some(HighlightColor::Yellow) => OwoStyle::new().yellow(),
        Some(HighlightColor::Magenta) => OwoStyle::new().magenta(),
        Some(HighlightColor::Cyan) => OwoStyle::new().cyan(),
        Some(HighlightColor::BrightYellow) => OwoStyle::new().bright_yellow(),
        Some(HighlightColor::BrightBlue) => OwoStyle::new().bright_blue(),
        Some(HighlightColor::BrightMagenta) => OwoStyle::new().bright_magenta(),
        Some(HighlightColor::BrightCyan) => OwoStyle::new().bright_cyan(),
        None => OwoStyle::new(),
    };
    if style.attributes.contains(HighlightAttributes::BOLD) {
        output = output.bold();
    }
    if style.attributes.contains(HighlightAttributes::DIM) {
        output = output.dimmed();
    }
    if style.attributes.contains(HighlightAttributes::ITALIC) {
        output = output.italic();
    }
    if style
        .attributes
        .contains(HighlightAttributes::STRIKETHROUGH)
    {
        output = output.strikethrough();
    }
    output
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
        severity_style(diagnostic.severity).style(label).to_string()
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

fn severity_style(severity: Severity) -> OwoStyle {
    match severity {
        Severity::Error => OwoStyle::new().red(),
        Severity::Warning => OwoStyle::new().yellow(),
        Severity::Info => OwoStyle::new().cyan(),
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
            return display_agent_path(&suffix);
        }
    }
    display_agent_path(path)
}

fn display_agent_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
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
        diagnostic.message_with_primary_label()
    )
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
    fn concise_renderer_preserves_primary_label() {
        let diag = Diagnostic::error(DiagnosticId::TypeMismatch, "mismatched types")
            .with_primary(test_span(), "expected `int`, found `string`");

        let output = render_diagnostic(
            &diag,
            &make_source(),
            &make_file_paths(),
            &RenderConfig::concise(),
        );

        assert!(output.contains("mismatched types: expected `int`, found `string`"));
    }

    #[test]
    fn test_render_human() {
        let diag = Diagnostic::error(DiagnosticId::TypeMismatch, "Expected int, found string")
            .with_primary_span(test_span());

        let sources = make_source();
        let file_paths = make_file_paths();
        let output = render_diagnostic(&diag, &sources, &file_paths, &RenderConfig::test());

        assert!(output.contains("Expected int, found string"));
        assert!(output.contains("E0001"));
        assert!(output.contains("test.baml")); // Should show filename
    }

    #[test]
    fn test_render_human_shows_filename() {
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
    fn human_renderer_preserves_source_before_the_span() {
        let source = "function Foo() -> int {\n  1 + \"foo\"\n}\n".to_string();
        let start = source.find("1 + \"foo\"").unwrap();
        let end = start + "1 + \"foo\"".len();
        let span = Span {
            file_id: FileId::new(0),
            range: TextRange::new(
                u32::try_from(start).unwrap().into(),
                u32::try_from(end).unwrap().into(),
            ),
        };
        let diag =
            Diagnostic::error(DiagnosticId::TypeMismatch, "Test error").with_primary_span(span);
        let sources = HashMap::from([(FileId::new(0), source)]);

        let output = render_diagnostic(&diag, &sources, &make_file_paths(), &RenderConfig::test());

        assert!(output.contains("[test.baml:2:3]"), "{output}");
        assert!(output.contains(" 2 │   1 + \"foo\""), "{output}");
    }

    #[test]
    fn human_renderer_uses_red_for_the_primary_diagnostic_span() {
        let marker = "PRIMARY_COLOR_MARKER";
        let secondary_marker = "SECONDARY_COLOR_MARKER";
        let secondary_span = Span {
            file_id: FileId::new(0),
            range: TextRange::new(14.into(), 18.into()),
        };
        let diag = Diagnostic::error(DiagnosticId::TypeMismatch, "Test error")
            .with_secondary(secondary_span, secondary_marker)
            .with_primary(test_span(), marker);
        let output = render_diagnostic(
            &diag,
            &make_source(),
            &make_file_paths(),
            &RenderConfig::cli(),
        );
        let styled_marker = OwoStyle::new().red().bold().style(marker).to_string();
        let red_prefix = styled_marker.split_once(marker).unwrap().0;
        let styled_marker = OwoStyle::new().magenta().bold().style(marker).to_string();
        let magenta_prefix = styled_marker.split_once(marker).unwrap().0;
        let label_line = output
            .lines()
            .find(|line| line.contains(marker) && line.contains(red_prefix))
            .unwrap_or_else(|| panic!("missing red primary label in {output:?}"));
        let styled_secondary = OwoStyle::new()
            .cyan()
            .bold()
            .style(secondary_marker)
            .to_string();
        let secondary_prefix = styled_secondary.split_once(secondary_marker).unwrap().0;

        assert!(!label_line.contains(magenta_prefix), "{output:?}");
        assert!(
            output
                .lines()
                .any(|line| { line.contains(secondary_marker) && line.contains(secondary_prefix) }),
            "{output:?}"
        );
    }

    #[test]
    fn human_renderer_uses_yellow_for_warning_primary_spans() {
        let marker = "WARNING_COLOR_MARKER";
        let diagnostic = Diagnostic::warning(DiagnosticId::UnreachableArm, "Test warning")
            .with_primary(test_span(), marker);
        let output = render_diagnostic(
            &diagnostic,
            &make_source(),
            &make_file_paths(),
            &RenderConfig::cli(),
        );
        let styled_marker = OwoStyle::new().yellow().bold().style(marker).to_string();
        let yellow_prefix = styled_marker.split_once(marker).unwrap().0;

        assert!(
            output
                .lines()
                .any(|line| line.contains(marker) && line.contains(yellow_prefix)),
            "{output:?}"
        );
    }

    #[test]
    fn human_renderer_does_not_repeat_an_unlabeled_primary_message() {
        let diagnostic = Diagnostic::error(DiagnosticId::TypeMismatch, "Test error")
            .with_primary_span(test_span());
        let output = render_diagnostic(
            &diagnostic,
            &make_source(),
            &make_file_paths(),
            &RenderConfig::test(),
        );

        assert_eq!(output.matches("Test error").count(), 1, "{output}");
    }

    #[test]
    fn human_renderer_highlights_code_in_messages() {
        let message = crate::DiagnosticText::new()
            .text("unknown type ")
            .identifier("User", DiagnosticIdentifierKind::Type);
        let diagnostic =
            Diagnostic::error(DiagnosticId::UnknownType, message).with_primary_span(test_span());
        let output = render_diagnostic(
            &diagnostic,
            &make_source(),
            &make_file_paths(),
            &RenderConfig::cli(),
        );

        assert!(output.contains("\u{1b}[33mUser"), "{output:?}");
        assert!(!output.contains(MESSAGE_STYLE_START), "{output:?}");
        assert!(!output.contains(MESSAGE_STYLE_END), "{output:?}");
    }

    #[test]
    fn human_renderer_uses_supplied_message_highlights() {
        struct GreenHighlighter;

        impl DiagnosticMessageHighlighter for GreenHighlighter {
            fn highlight(
                &self,
                _kind: DiagnosticMessageKind,
                text: &str,
            ) -> Result<Vec<HighlightSpan>, DiagnosticMessageHighlightError> {
                Ok(vec![HighlightSpan {
                    range: TextRange::new(
                        0.into(),
                        u32::try_from(text.len())
                            .expect("test diagnostic message fits in u32")
                            .into(),
                    ),
                    style: HighlightStyle {
                        foreground: Some(HighlightColor::Green),
                        attributes: HighlightAttributes::empty(),
                    },
                }])
            }
        }

        let message = crate::DiagnosticText::new()
            .text("found ")
            .type_expr("\"not an int\"");
        let diagnostic =
            Diagnostic::error(DiagnosticId::TypeMismatch, message).with_primary_span(test_span());
        let output = render_diagnostics_with_highlighters(
            &[diagnostic],
            &make_source(),
            &make_file_paths(),
            &SourceHighlights::new(),
            Some(&GreenHighlighter),
            &RenderConfig::cli(),
        );

        assert!(output.contains("\u{1b}[32m\"not an int\""), "{output:?}");
        assert!(
            !output.chars().any(
                |ch| (MESSAGE_STYLE_CODE_BASE..MESSAGE_STYLE_CODE_BASE + 256)
                    .contains(&u32::from(ch))
            ),
            "{output:?}"
        );
    }

    #[test]
    fn human_renderer_retries_failed_message_highlighting_with_pretty_nocolor_theme() {
        struct FailingHighlighter;

        impl DiagnosticMessageHighlighter for FailingHighlighter {
            fn highlight(
                &self,
                _kind: DiagnosticMessageKind,
                _text: &str,
            ) -> Result<Vec<HighlightSpan>, DiagnosticMessageHighlightError> {
                Err(DiagnosticMessageHighlightError)
            }
        }

        let message = crate::DiagnosticText::new()
            .text("invalid fragment ")
            .code("${...}");
        let diagnostic =
            Diagnostic::error(DiagnosticId::TypeMismatch, message).with_primary_span(test_span());
        let output = render_diagnostics_with_highlighters(
            &[diagnostic],
            &make_source(),
            &make_file_paths(),
            &SourceHighlights::new(),
            Some(&FailingHighlighter),
            &RenderConfig::cli(),
        );

        assert!(output.contains("E0001"), "{output:?}");
        assert!(output.contains("╭─[test.baml:1:7]"), "{output:?}");
        assert!(output.contains("class Foo"), "{output:?}");
        assert!(output.contains("invalid fragment `${...}`"), "{output:?}");
        assert!(
            !output.contains('\u{1b}'),
            "the fallback diagnostic should contain no ANSI styling: {output:?}"
        );
    }

    #[test]
    fn full_line_source_preserves_utf8_indentation_and_crlf() {
        let source = FullLineSource::new("header\n  caf\u{e9} + 1\r\nfooter".to_string());
        let span = SourceSpan::from((17, 1));

        let contents = source.read_span(&span, 0, 0).unwrap();

        assert_eq!(
            std::str::from_utf8(contents.data()).unwrap(),
            "  caf\u{e9} + 1\r\n"
        );
        assert_eq!(contents.span(), &SourceSpan::from((7, 13)));
        assert_eq!(contents.line(), 1);
        assert_eq!(contents.column(), 9);
        assert_eq!(contents.line_count(), 1);
    }

    #[test]
    fn human_renderer_applies_source_highlights() {
        let diag = Diagnostic::error(DiagnosticId::TypeMismatch, "Test error")
            .with_primary_span(test_span());
        let mut highlights = SourceHighlights::new();
        highlights.insert(
            FileId::new(0),
            vec![HighlightSpan {
                range: TextRange::new(6.into(), 9.into()),
                style: HighlightStyle {
                    foreground: Some(HighlightColor::Yellow),
                    attributes: HighlightAttributes::empty(),
                },
            }],
        );
        let output = render_diagnostics_with_highlights(
            &[diag],
            &make_source(),
            &make_file_paths(),
            &highlights,
            &RenderConfig::cli(),
        );
        assert!(output.contains("\x1b[33mFoo"), "{output:?}");
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
    fn agent_render_preserves_distinct_primary_label() {
        let diag = Diagnostic::error(DiagnosticId::TypeMismatch, "mismatched types")
            .with_primary(test_span(), "expected `int`, found `string`");

        let output = render_diagnostic(
            &diag,
            &make_source(),
            &make_file_paths(),
            &RenderConfig::agent(),
        );

        assert_eq!(
            output,
            "test.baml:1:7-1:10 error[E0001]: mismatched types\n  primary: expected `int`, found `string`"
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
