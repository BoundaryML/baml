//! Conversion of owned [`DiagnosticCandidate`]s into publishable LSP
//! diagnostics.
//!
//! Diagnostics are computed into fully owned, revision-tagged candidates
//! under the source gate ([`crate::project::collect_diagnostic_candidate`]);
//! this module converts them with the negotiated position codec at publish
//! time, with no database access. The candidate carries every file's text so
//! related-information spans in other files convert without a lock.

use std::collections::HashMap;

use baml_base::FileId;

use crate::{
    bex_lsp::{
        multi_project::wasm_helpers,
        position_codec::{PositionCodec, PositionEncoding},
    },
    project::DiagnosticCandidate,
};

/// One publish-ready document: the exact open-document version the text was
/// checked at (`None` for closed/disk-only files) plus converted diagnostics.
/// Every source file is represented, so stale editor markers clear.
pub(super) struct PublishableDocument {
    pub path: std::path::PathBuf,
    pub version: Option<i32>,
    pub diagnostics: Vec<lsp_types::Diagnostic>,
}

/// Convert an owned candidate into publish-ready documents using the
/// negotiated encoding. Pure conversion: no locks, no database.
pub(super) fn candidate_to_publishable(
    candidate: &DiagnosticCandidate,
    encoding: PositionEncoding,
) -> Vec<PublishableDocument> {
    let codecs: HashMap<FileId, PositionCodec<'_>> = candidate
        .file_texts
        .iter()
        .map(|(file_id, (_, text))| (*file_id, PositionCodec::new(text, encoding)))
        .collect();
    let paths: HashMap<FileId, &std::path::PathBuf> = candidate
        .file_texts
        .iter()
        .map(|(file_id, (path, _))| (*file_id, path))
        .collect();

    candidate
        .documents
        .iter()
        .map(|doc| PublishableDocument {
            path: doc.path.clone(),
            version: doc.version,
            diagnostics: doc
                .diagnostics
                .iter()
                .filter_map(|d| to_lsp_diagnostic(d, &codecs, &paths))
                .collect(),
        })
        .collect()
}

fn to_lsp_diagnostic(
    diagnostic: &baml_compiler_diagnostics::Diagnostic,
    codecs: &HashMap<FileId, PositionCodec<'_>>,
    paths: &HashMap<FileId, &std::path::PathBuf>,
) -> Option<lsp_types::Diagnostic> {
    let primary_span = diagnostic.primary_span()?;
    let codec = codecs.get(&primary_span.file_id)?;

    Some(lsp_types::Diagnostic {
        severity: Some(match diagnostic.severity {
            baml_compiler_diagnostics::Severity::Error => lsp_types::DiagnosticSeverity::ERROR,
            baml_compiler_diagnostics::Severity::Warning => lsp_types::DiagnosticSeverity::WARNING,
            baml_compiler_diagnostics::Severity::Info => lsp_types::DiagnosticSeverity::INFORMATION,
        }),
        range: codec.byte_range_to_lsp(primary_span.range),
        code: Some(lsp_types::NumberOrString::String(
            diagnostic.code().to_string(),
        )),
        message: diagnostic.message_with_primary_label().into_owned(),
        code_description: None,
        source: Some("baml".to_string()),
        related_information: Some(
            diagnostic
                .related_info
                .iter()
                .filter_map(|r| {
                    let path = paths.get(&r.span.file_id)?;
                    let codec = codecs.get(&r.span.file_id)?;
                    Some(lsp_types::DiagnosticRelatedInformation {
                        location: lsp_types::Location {
                            uri: wasm_helpers::from_file_path(path).ok()?,
                            range: codec.byte_range_to_lsp(r.span.range),
                        },
                        message: r.message.clone(),
                    })
                })
                .collect(),
        ),
        tags: None,
        data: None,
    })
}
