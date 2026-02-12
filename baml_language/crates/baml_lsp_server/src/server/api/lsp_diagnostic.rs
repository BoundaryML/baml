//! LSP conversion for BAML diagnostics.
//!
//! This module provides conversion from the unified `Diagnostic` type
//! (from `baml_compiler_diagnostics`) to `lsp_types::Diagnostic` for editor integration.
//!
//! Following ty's architecture, this conversion logic lives in the LSP server crate,
//! keeping the diagnostics crate free of LSP dependencies.

use std::{collections::HashMap, path::PathBuf};

use baml_db::{
    FileId,
    baml_compiler_diagnostics::{Diagnostic, Severity},
};
use lsp_types::{DiagnosticRelatedInformation, DiagnosticSeverity, Location, NumberOrString, Url};

use crate::edit::PositionEncoding;

/// Configuration for LSP diagnostic conversion.
pub struct LspConversionConfig<'a> {
    /// Maps FileId to file path for URL generation.
    pub file_paths: &'a HashMap<FileId, PathBuf>,
    /// Maps FileId to (source_text, line_starts) for range conversion.
    pub file_sources: &'a HashMap<FileId, (String, Vec<u32>)>,
    /// The position encoding negotiated with the LSP client.
    /// This is essential for correct character position calculation in files
    /// containing multi-byte UTF-8 characters (like 'é' or emoji).
    pub position_encoding: PositionEncoding,
}

/// Convert a diagnostic to LSP diagnostics.
///
/// Returns a vec of `(Url, Diagnostic)` pairs. The first element is the primary
/// diagnostic. Additional hint-severity diagnostics are emitted for each
/// `related_info` span so that editors render squigglies at those locations too
/// (matching rust-analyzer's behaviour).
///
/// Returns an empty vec if the primary span's file is not in the provided file maps.
pub fn to_lsp_diagnostics(
    diagnostic: &Diagnostic,
    config: &LspConversionConfig,
) -> Vec<(Url, lsp_types::Diagnostic)> {
    let Some(primary_span) = diagnostic.primary_span() else {
        return Vec::new();
    };
    let Some(path) = config.file_paths.get(&primary_span.file_id) else {
        return Vec::new();
    };
    let Ok(url) = Url::from_file_path(path) else {
        return Vec::new();
    };
    let Some((source_text, line_starts)) = config.file_sources.get(&primary_span.file_id) else {
        return Vec::new();
    };

    let range = span_to_lsp_range(
        primary_span.range,
        source_text,
        line_starts,
        config.position_encoding,
    );

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
                        let ann_range = span_to_lsp_range(
                            annotation.span.range,
                            ann_source,
                            ann_line_starts,
                            config.position_encoding,
                        );
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
                    let info_range = span_to_lsp_range(
                        info.span.range,
                        info_source,
                        info_line_starts,
                        config.position_encoding,
                    );
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

    let primary_location = Location {
        uri: url.clone(),
        range,
    };

    let mut result = Vec::new();

    result.push((
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
    ));

    // Emit hint-severity diagnostics at each related info location so that
    // editors render squigglies there too (à la rust-analyzer).
    for info in &diagnostic.related_info {
        if let Some(path) = config.file_paths.get(&info.span.file_id) {
            if let Ok(info_url) = Url::from_file_path(path) {
                if let Some((info_source, info_line_starts)) =
                    config.file_sources.get(&info.span.file_id)
                {
                    let info_range = span_to_lsp_range(
                        info.span.range,
                        info_source,
                        info_line_starts,
                        config.position_encoding,
                    );
                    result.push((
                        info_url,
                        lsp_types::Diagnostic {
                            range: info_range,
                            severity: Some(DiagnosticSeverity::HINT),
                            code: Some(NumberOrString::String(diagnostic.code().to_string())),
                            code_description: None,
                            source: Some("baml".to_string()),
                            message: info.message.clone(),
                            related_information: Some(vec![DiagnosticRelatedInformation {
                                location: primary_location.clone(),
                                message: diagnostic.message.clone(),
                            }]),
                            tags: None,
                            data: None,
                        },
                    ));
                }
            }
        }
    }

    result
}

/// Convert a TextRange to an LSP Range.
fn span_to_lsp_range(
    range: text_size::TextRange,
    source_text: &str,
    line_starts: &[u32],
    encoding: PositionEncoding,
) -> lsp_types::Range {
    let start_offset: u32 = range.start().into();
    let end_offset: u32 = range.end().into();

    let start = offset_to_position(start_offset, source_text, line_starts, encoding);
    let end = offset_to_position(end_offset, source_text, line_starts, encoding);

    lsp_types::Range { start, end }
}

/// Convert a byte offset to an LSP Position.
///
/// The character position is computed based on the position encoding negotiated
/// with the LSP client:
/// - UTF-8: character offset is byte offset from line start
/// - UTF-16: character offset is UTF-16 code unit count from line start
/// - UTF-32: character offset is character (codepoint) count from line start
fn offset_to_position(
    offset: u32,
    source_text: &str,
    line_starts: &[u32],
    encoding: PositionEncoding,
) -> lsp_types::Position {
    // Binary search for the line containing this offset
    let line = match line_starts.binary_search(&offset) {
        Ok(line) => line,
        Err(line) => line.saturating_sub(1),
    };

    let line_start = line_starts.get(line).copied().unwrap_or(0);

    // Calculate character position based on encoding
    let character = match encoding {
        PositionEncoding::UTF8 => {
            // UTF-8 encoding: character offset equals byte offset
            offset.saturating_sub(line_start)
        }
        PositionEncoding::UTF16 => {
            // UTF-16 encoding: count UTF-16 code units
            let line_start_usize = line_start as usize;
            let offset_usize = offset as usize;
            if offset_usize <= line_start_usize || offset_usize > source_text.len() {
                0
            } else {
                // Get the slice from line start to offset
                let slice = &source_text[line_start_usize..offset_usize.min(source_text.len())];
                // Count UTF-16 code units
                slice.encode_utf16().count() as u32
            }
        }
        PositionEncoding::UTF32 => {
            // UTF-32 encoding: count Unicode codepoints (characters)
            let line_start_usize = line_start as usize;
            let offset_usize = offset as usize;
            if offset_usize <= line_start_usize || offset_usize > source_text.len() {
                0
            } else {
                // Get the slice from line start to offset
                let slice = &source_text[line_start_usize..offset_usize.min(source_text.len())];
                // Count characters
                slice.chars().count() as u32
            }
        }
    };

    lsp_types::Position {
        line: line as u32,
        character,
    }
}

/// Build line starts for a source file.
///
/// Returns byte offsets of each line start. Uses `char_indices()` to get
/// byte positions rather than character indices, which is essential for
/// files containing multi-byte UTF-8 characters.
pub fn compute_line_starts(source: &str) -> Vec<u32> {
    let mut line_starts = vec![0];
    for (byte_offset, c) in source.char_indices() {
        if c == '\n' {
            // The next line starts at byte_offset + 1
            // (newline '\n' is always 1 byte in UTF-8)
            line_starts.push((byte_offset + 1) as u32);
        }
    }
    line_starts
}

#[cfg(test)]
mod tests {
    use baml_db::{Span, baml_compiler_diagnostics::DiagnosticId};
    use text_size::TextRange;

    use super::*;

    #[test]
    fn test_compute_line_starts() {
        let source = "line1\nline2\nline3";
        let starts = compute_line_starts(source);
        assert_eq!(starts, vec![0, 6, 12]);
    }

    #[test]
    fn test_compute_line_starts_multibyte_utf8() {
        // Test with multi-byte UTF-8 characters
        // "héllo" has 'é' which is 2 bytes (0xC3 0xA9), so:
        // h=1byte, é=2bytes, l=1byte, l=1byte, o=1byte = 6 bytes total
        // Then \n at byte 6, so next line starts at byte 7
        let source = "héllo\nworld";
        let starts = compute_line_starts(source);
        // "héllo" is 6 bytes (h=1, é=2, l=1, l=1, o=1), newline at byte 6
        // "world" starts at byte 7
        assert_eq!(starts, vec![0, 7]);

        // UTF-8 encoding: byte offsets are used as character positions
        // 'w' in "world" is at byte offset 7
        let pos = offset_to_position(7, source, &starts, PositionEncoding::UTF8);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);

        // 'o' in "héllo" is at byte offset 5 (h=1 + é=2 + l=1 + l=1 = 5)
        let pos = offset_to_position(5, source, &starts, PositionEncoding::UTF8);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn test_compute_line_starts_emoji() {
        // Emoji like 🦀 is 4 bytes
        let source = "🦀\nrust";
        let starts = compute_line_starts(source);
        // 🦀 = 4 bytes, \n at byte 4, "rust" starts at byte 5
        assert_eq!(starts, vec![0, 5]);

        let pos = offset_to_position(5, source, &starts, PositionEncoding::UTF8);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);
    }

    #[test]
    fn test_offset_to_position_utf8() {
        let source = "0123456789\n0123456789\n0123456789";
        let line_starts = vec![0, 11, 22];

        // First character
        let pos = offset_to_position(0, source, &line_starts, PositionEncoding::UTF8);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);

        // Middle of first line
        let pos = offset_to_position(5, source, &line_starts, PositionEncoding::UTF8);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 5);

        // Start of second line (after newline at position 10)
        let pos = offset_to_position(11, source, &line_starts, PositionEncoding::UTF8);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);

        // Middle of second line
        let pos = offset_to_position(16, source, &line_starts, PositionEncoding::UTF8);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn test_offset_to_position_utf16_multibyte() {
        // Test UTF-16 encoding with multi-byte UTF-8 characters
        // "héllo" has 'é' which is 2 bytes in UTF-8 but 1 code unit in UTF-16
        let source = "héllo\nworld";
        let starts = compute_line_starts(source);
        assert_eq!(starts, vec![0, 7]); // "héllo" = 6 bytes + newline

        // UTF-16: 'o' in "héllo" at byte offset 5 should be character 4
        // h=char0, é=char1, l=char2, l=char3, o=char4
        let pos = offset_to_position(5, source, &starts, PositionEncoding::UTF16);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 4); // 4 UTF-16 code units (h, é, l, l)

        // UTF-8: same position would be character 5 (byte offset)
        let pos = offset_to_position(5, source, &starts, PositionEncoding::UTF8);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 5); // 5 bytes from line start
    }

    #[test]
    fn test_offset_to_position_utf16_emoji() {
        // Test UTF-16 encoding with emoji (surrogate pairs)
        // 🦀 is 4 bytes in UTF-8 but 2 code units in UTF-16 (surrogate pair)
        let source = "🦀ab\nrust";
        let starts = compute_line_starts(source);
        // 🦀 = 4 bytes, a = 1 byte, b = 1 byte, \n at byte 6
        assert_eq!(starts, vec![0, 7]);

        // 'a' is at byte offset 4 (after 🦀)
        // UTF-16: 🦀 = 2 code units, so 'a' is at character 2
        let pos = offset_to_position(4, source, &starts, PositionEncoding::UTF16);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 2);

        // 'b' is at byte offset 5
        // UTF-16: 🦀 = 2 code units + a = 1 code unit = 3
        let pos = offset_to_position(5, source, &starts, PositionEncoding::UTF16);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 3);
    }

    #[test]
    fn test_offset_to_position_utf32() {
        // Test UTF-32 encoding (character/codepoint count)
        // 🦀 is 4 bytes in UTF-8, 2 code units in UTF-16, but 1 codepoint in UTF-32
        let source = "🦀ab\nrust";
        let starts = compute_line_starts(source);
        assert_eq!(starts, vec![0, 7]);

        // 'a' is at byte offset 4 (after 🦀)
        // UTF-32: 🦀 = 1 codepoint, so 'a' is at character 1
        let pos = offset_to_position(4, source, &starts, PositionEncoding::UTF32);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 1);

        // 'b' is at byte offset 5
        // UTF-32: 🦀 = 1 codepoint + a = 1 codepoint = 2
        let pos = offset_to_position(5, source, &starts, PositionEncoding::UTF32);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 2);
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

        // Use a cross-platform path (temp_dir works on all platforms)
        let mut test_path = std::env::temp_dir();
        test_path.push("test.baml");

        let mut file_paths = HashMap::new();
        file_paths.insert(file_id, test_path);

        let source = "hello\nworld";
        let line_starts = compute_line_starts(source);
        let mut file_sources = HashMap::new();
        file_sources.insert(file_id, (source.to_string(), line_starts));

        let config = LspConversionConfig {
            file_paths: &file_paths,
            file_sources: &file_sources,
            position_encoding: PositionEncoding::UTF16, // Default LSP encoding
        };

        let result = to_lsp_diagnostics(&diag, &config);
        assert_eq!(result.len(), 1);

        let (url, lsp_diag) = &result[0];
        assert!(url.as_str().contains("test.baml"));
        assert_eq!(lsp_diag.message, "Type mismatch");
        assert_eq!(lsp_diag.severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn test_diagnostic_to_lsp_with_multibyte_utf16() {
        // Test that diagnostics with multi-byte characters use correct UTF-16 positions
        let file_id = FileId::new(0);
        // Source: "héllo" where 'é' is 2 bytes in UTF-8
        // We want a diagnostic pointing to 'o' at byte offset 5
        // In UTF-16, 'o' should be at character 4 (h=0, é=1, l=2, l=3, o=4)
        let span = Span {
            file_id,
            range: TextRange::new(5.into(), 6.into()), // 'o' character
        };

        let diag =
            Diagnostic::error(DiagnosticId::TypeMismatch, "Error at 'o'").with_primary_span(span);

        let mut test_path = std::env::temp_dir();
        test_path.push("multibyte.baml");

        let mut file_paths = HashMap::new();
        file_paths.insert(file_id, test_path);

        let source = "héllo\nworld";
        let line_starts = compute_line_starts(source);
        let mut file_sources = HashMap::new();
        file_sources.insert(file_id, (source.to_string(), line_starts));

        let config = LspConversionConfig {
            file_paths: &file_paths,
            file_sources: &file_sources,
            position_encoding: PositionEncoding::UTF16,
        };

        let result = to_lsp_diagnostics(&diag, &config);
        assert_eq!(result.len(), 1);

        let (_, lsp_diag) = &result[0];
        // UTF-16: 'o' is at character 4 (not byte offset 5)
        assert_eq!(lsp_diag.range.start.line, 0);
        assert_eq!(lsp_diag.range.start.character, 4);
        assert_eq!(lsp_diag.range.end.character, 5); // 'o' ends at character 5
    }
}
