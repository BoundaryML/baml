//! LSP conversion for BAML diagnostics.
//!
//! This module provides conversion from the unified `Diagnostic` type
//! (from `baml_diagnostics`) to `lsp_types::Diagnostic` for editor integration.
//!
//! Following ty's architecture, this conversion logic lives in the LSP server crate,
//! keeping the diagnostics crate free of LSP dependencies.

use std::{collections::HashMap, path::PathBuf};

use baml_db::FileId;
use baml_diagnostics::{Diagnostic, Severity};
use lsp_types::{DiagnosticRelatedInformation, DiagnosticSeverity, Location, NumberOrString, Url};

/// Configuration for LSP diagnostic conversion.
pub struct LspConversionConfig<'a> {
    /// Maps FileId to file path for URL generation.
    pub file_paths: &'a HashMap<FileId, PathBuf>,
    /// Maps FileId to (source_text, line_starts) for range conversion.
    pub file_sources: &'a HashMap<FileId, (String, Vec<u32>)>,
}

/// Convert a diagnostic to an LSP diagnostic.
///
/// Returns `None` if the primary span's file is not in the provided file maps.
pub fn to_lsp_diagnostic(
    diagnostic: &Diagnostic,
    config: &LspConversionConfig,
) -> Option<(Url, lsp_types::Diagnostic)> {
    let primary_span = diagnostic.primary_span()?;
    let path = config.file_paths.get(&primary_span.file_id)?;
    let url = Url::from_file_path(path).ok()?;

    let (source_text, line_starts) = config.file_sources.get(&primary_span.file_id)?;

    let range = span_to_lsp_range(primary_span.range, source_text, line_starts);

    let severity = match diagnostic.severity {
        Severity::Error => Some(DiagnosticSeverity::ERROR),
        Severity::Warning => Some(DiagnosticSeverity::WARNING),
        Severity::Info => Some(DiagnosticSeverity::INFORMATION),
    };

    // Build related information from secondary annotations and related_info
    let mut related_information: Vec<DiagnosticRelatedInformation> = Vec::new();

    // Add secondary annotations as related info
    for annotation in &diagnostic.annotations {
        if !annotation.is_primary {
            if let Some(path) = config.file_paths.get(&annotation.span.file_id) {
                if let Ok(ann_url) = Url::from_file_path(path) {
                    if let Some((ann_source, ann_line_starts)) =
                        config.file_sources.get(&annotation.span.file_id)
                    {
                        let ann_range =
                            span_to_lsp_range(annotation.span.range, ann_source, ann_line_starts);
                        related_information.push(DiagnosticRelatedInformation {
                            location: Location {
                                uri: ann_url,
                                range: ann_range,
                            },
                            message: annotation
                                .message
                                .clone()
                                .unwrap_or_else(|| "related".to_string()),
                        });
                    }
                }
            }
        }
    }

    // Add explicit related_info
    for info in &diagnostic.related_info {
        if let Some(path) = config.file_paths.get(&info.span.file_id) {
            if let Ok(info_url) = Url::from_file_path(path) {
                if let Some((info_source, info_line_starts)) =
                    config.file_sources.get(&info.span.file_id)
                {
                    let info_range =
                        span_to_lsp_range(info.span.range, info_source, info_line_starts);
                    related_information.push(DiagnosticRelatedInformation {
                        location: Location {
                            uri: info_url,
                            range: info_range,
                        },
                        message: info.message.clone(),
                    });
                }
            }
        }
    }

    let related_information = if related_information.is_empty() {
        None
    } else {
        Some(related_information)
    };

    Some((
        url,
        lsp_types::Diagnostic {
            range,
            severity,
            code: Some(NumberOrString::String(diagnostic.code().to_string())),
            code_description: None,
            source: Some("baml".to_string()),
            message: diagnostic.message.clone(),
            related_information,
            tags: None,
            data: None,
        },
    ))
}

/// Convert a TextRange to an LSP Range.
fn span_to_lsp_range(
    range: text_size::TextRange,
    _source_text: &str,
    line_starts: &[u32],
) -> lsp_types::Range {
    let start_offset: u32 = range.start().into();
    let end_offset: u32 = range.end().into();

    let start = offset_to_position(start_offset, line_starts);
    let end = offset_to_position(end_offset, line_starts);

    lsp_types::Range { start, end }
}

/// Convert a byte offset to an LSP Position.
fn offset_to_position(offset: u32, line_starts: &[u32]) -> lsp_types::Position {
    // Binary search for the line containing this offset
    let line = match line_starts.binary_search(&offset) {
        Ok(line) => line,
        Err(line) => line.saturating_sub(1),
    };

    let line_start = line_starts.get(line).copied().unwrap_or(0);
    let character = offset.saturating_sub(line_start);

    lsp_types::Position {
        line: line as u32,
        character,
    }
}

/// Build line starts for a source file.
pub fn compute_line_starts(source: &str) -> Vec<u32> {
    let mut line_starts = vec![0];
    for (i, c) in source.chars().enumerate() {
        if c == '\n' {
            line_starts.push((i + 1) as u32);
        }
    }
    line_starts
}

#[cfg(test)]
mod tests {
    use baml_db::Span;
    use baml_diagnostics::DiagnosticId;
    use text_size::TextRange;

    use super::*;

    #[test]
    fn test_compute_line_starts() {
        let source = "line1\nline2\nline3";
        let starts = compute_line_starts(source);
        assert_eq!(starts, vec![0, 6, 12]);
    }

    #[test]
    fn test_offset_to_position() {
        let line_starts = vec![0, 10, 20];

        // First character
        let pos = offset_to_position(0, &line_starts);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);

        // Middle of first line
        let pos = offset_to_position(5, &line_starts);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 5);

        // Start of second line
        let pos = offset_to_position(10, &line_starts);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);

        // Middle of second line
        let pos = offset_to_position(15, &line_starts);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn test_diagnostic_to_lsp() {
        let file_id = FileId::new(0);
        let span = Span {
            file_id,
            range: TextRange::new(0.into(), 5.into()),
        };

        let diag =
            Diagnostic::error(DiagnosticId::TypeMismatch, "Type mismatch").with_primary_span(span);

        let mut file_paths = HashMap::new();
        file_paths.insert(file_id, PathBuf::from("/tmp/test.baml"));

        let source = "hello\nworld";
        let line_starts = compute_line_starts(source);
        let mut file_sources = HashMap::new();
        file_sources.insert(file_id, (source.to_string(), line_starts));

        let config = LspConversionConfig {
            file_paths: &file_paths,
            file_sources: &file_sources,
        };

        let result = to_lsp_diagnostic(&diag, &config);
        assert!(result.is_some());

        let (url, lsp_diag) = result.unwrap();
        assert!(url.as_str().contains("test.baml"));
        assert_eq!(lsp_diag.message, "Type mismatch");
        assert_eq!(lsp_diag.severity, Some(DiagnosticSeverity::ERROR));
    }
}
