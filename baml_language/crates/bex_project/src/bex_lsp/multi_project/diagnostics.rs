use std::collections::HashMap;

use baml_base::FileId;

use crate::bex_lsp::multi_project::wasm_helpers;

/// Configuration for LSP diagnostic conversion.
struct LspConversionConfig<'a> {
    /// Maps `FileId` to file path for URL generation.
    pub file_paths: &'a HashMap<FileId, std::path::PathBuf>,
    /// Maps `FileId` to (`source_text`, `line_starts`) for range conversion.
    pub file_sources: &'a HashMap<FileId, (String, Vec<u32>)>,
}

/// A convenient enumeration for supported text encodings. Can be converted to [`lsp_types::PositionEncodingKind`].
// Please maintain the order from least to greatest priority for the derived `Ord` impl.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum PositionEncoding {
    /// UTF 16 is the encoding supported by all LSP clients.
    #[default]
    UTF16,

    /// Second choice because UTF32 uses a fixed 4 byte encoding for each character (makes conversion relatively easy)
    #[allow(dead_code)]
    UTF32,

    /// BAML's preferred encoding
    UTF8,
}

fn to_lsp_diagnostic(
    diagnostic: baml_compiler_diagnostics::Diagnostic,
    config: &LspConversionConfig,
    encoding: PositionEncoding,
) -> Option<lsp_types::Diagnostic> {
    let primary_span = diagnostic.primary_span()?;
    let (source_text, line_starts) = config.file_sources.get(&primary_span.file_id)?;

    let diagnostic = lsp_types::Diagnostic {
        severity: Some(match diagnostic.severity {
            baml_compiler_diagnostics::Severity::Error => lsp_types::DiagnosticSeverity::ERROR,
            baml_compiler_diagnostics::Severity::Warning => lsp_types::DiagnosticSeverity::WARNING,
            baml_compiler_diagnostics::Severity::Info => lsp_types::DiagnosticSeverity::INFORMATION,
        }),
        range: span_to_lsp_range(primary_span.range, source_text, line_starts, encoding),
        code: Some(lsp_types::NumberOrString::String(
            diagnostic.code().to_string(),
        )),
        message: diagnostic.message,
        code_description: None,
        source: Some("baml".to_string()),
        related_information: Some(
            diagnostic
                .related_info
                .into_iter()
                .filter_map(|r| {
                    let path = config.file_paths.get(&r.span.file_id)?;
                    let (source_text, line_starts) = config.file_sources.get(&r.span.file_id)?;
                    let range = span_to_lsp_range(r.span.range, source_text, line_starts, encoding);
                    Some(lsp_types::DiagnosticRelatedInformation {
                        location: lsp_types::Location {
                            uri: wasm_helpers::from_file_path(path).ok()?,
                            range,
                        },
                        message: r.message,
                    })
                })
                .collect(),
        ),
        tags: None,
        data: None,
    };

    Some(diagnostic)
}

/// Convert a `TextRange` to an LSP Range.
pub(super) fn span_to_lsp_range(
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
                u32::try_from(slice.encode_utf16().count()).unwrap_or(0)
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
                u32::try_from(slice.chars().count()).unwrap_or(0)
            }
        }
    };

    lsp_types::Position {
        line: u32::try_from(line).unwrap_or(0),
        character,
    }
}

pub(super) trait WithDiagnostics {
    /// The position encoding negotiated with the LSP client.
    /// This is essential for correct character position calculation in files
    /// containing multi-byte UTF-8 characters (like 'é' or emoji).
    fn diagnostics_by_file(
        &self,
        position_encoding: PositionEncoding,
    ) -> std::collections::HashMap<std::path::PathBuf, Vec<lsp_types::Diagnostic>>;
}

impl WithDiagnostics for crate::project::BexProject {
    /// Collect diagnostics for all files in the project (compiler2 only).
    fn diagnostics_by_file(
        &self,
        position_encoding: PositionEncoding,
    ) -> std::collections::HashMap<std::path::PathBuf, Vec<lsp_types::Diagnostic>> {
        let Ok(db) = self.db.try_lock() else {
            log::warn!("diagnostics_by_file: db mutex already locked, skipping");
            return HashMap::new();
        };

        let source_files = db.get_source_files();

        let mut file_sources: HashMap<baml_base::FileId, (String, Vec<u32>)> = HashMap::new();
        let mut file_paths: HashMap<baml_base::FileId, std::path::PathBuf> = HashMap::new();
        let mut diags_by_file: Vec<(
            std::path::PathBuf,
            Vec<baml_compiler_diagnostics::Diagnostic>,
        )> = Vec::new();

        for file in &source_files {
            let file_id = file.file_id(&*db);
            let Some(path) = db.file_id_to_path(file_id).cloned() else {
                continue;
            };

            let text = file.text(&*db).clone();
            let line_starts = compute_line_starts(&text);
            file_sources.insert(file_id, (text, line_starts));
            file_paths.insert(file_id, path.clone());

            let diags = baml_lsp2_actions::check_file(&*db, *file);
            diags_by_file.push((path, diags));
        }

        let config = LspConversionConfig {
            file_paths: &file_paths,
            file_sources: &file_sources,
        };

        // Seed every known file with an empty vec so cleared diagnostics
        // get an empty publish (removing stale markers).
        let mut grouped: HashMap<std::path::PathBuf, Vec<lsp_types::Diagnostic>> = file_paths
            .values()
            .map(|p| (p.clone(), Vec::new()))
            .collect();

        let mut total_converted = 0usize;
        let mut total_dropped = 0usize;
        for (path, diags) in diags_by_file {
            for diag in diags {
                if let Some(lsp_diag) = to_lsp_diagnostic(diag, &config, position_encoding) {
                    grouped.entry(path.clone()).or_default().push(lsp_diag);
                    total_converted += 1;
                } else {
                    total_dropped += 1;
                }
            }
        }

        grouped
    }
}

/// Build line starts for a source file.
///
/// Returns byte offsets of each line start. Uses `char_indices()` to get
/// byte positions rather than character indices, which is essential for
/// files containing multi-byte UTF-8 characters.
pub(super) fn compute_line_starts(source: &str) -> Vec<u32> {
    let mut line_starts = vec![0];
    for (byte_offset, c) in source.char_indices() {
        if c == '\n' {
            // The next line starts at byte_offset + 1
            // (newline '\n' is always 1 byte in UTF-8)
            line_starts.push(u32::try_from(byte_offset + 1).unwrap_or(0));
        }
    }
    line_starts
}
