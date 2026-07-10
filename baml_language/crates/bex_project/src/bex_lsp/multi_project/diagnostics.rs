use std::collections::HashMap;

use baml_base::FileId;
use baml_project::position::{LspPositionCodec, PositionEncoding};

use crate::{bex_lsp::multi_project::wasm_helpers, fs::FsPath, project::SourceRevision};

pub(super) enum DiagnosticRead<T> {
    Ready(T),
    Busy,
    Poisoned,
}

pub(super) struct DiagnosticCandidate {
    pub(super) source_revision: SourceRevision,
    pub(super) documents: HashMap<std::path::PathBuf, Vec<lsp_types::Diagnostic>>,
    pub(super) source_texts: HashMap<FsPath, String>,
}

/// Configuration for LSP diagnostic conversion.
struct LspConversionConfig<'a> {
    /// Maps `FileId` to file path for URL generation.
    pub file_paths: &'a HashMap<FileId, std::path::PathBuf>,
    /// Maps `FileId` to source text for range conversion.
    pub file_sources: &'a HashMap<FileId, String>,
}

fn to_lsp_diagnostic(
    diagnostic: baml_compiler_diagnostics::Diagnostic,
    config: &LspConversionConfig,
    encoding: PositionEncoding,
) -> Option<lsp_types::Diagnostic> {
    let primary_span = diagnostic.primary_span()?;
    let source_text = config.file_sources.get(&primary_span.file_id)?;
    let codec = LspPositionCodec::new(source_text, encoding);

    let diagnostic = lsp_types::Diagnostic {
        severity: Some(match diagnostic.severity {
            baml_compiler_diagnostics::Severity::Error => lsp_types::DiagnosticSeverity::ERROR,
            baml_compiler_diagnostics::Severity::Warning => lsp_types::DiagnosticSeverity::WARNING,
            baml_compiler_diagnostics::Severity::Info => lsp_types::DiagnosticSeverity::INFORMATION,
        }),
        range: codec.text_range_to_range(primary_span.range).ok()?,
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
                    let source_text = config.file_sources.get(&r.span.file_id)?;
                    let range = LspPositionCodec::new(source_text, encoding)
                        .text_range_to_range(r.span.range)
                        .ok()?;
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

pub(super) trait WithDiagnostics {
    /// The position encoding negotiated with the LSP client.
    /// This is essential for correct character position calculation in files
    /// containing multi-byte UTF-8 characters (like 'é' or emoji).
    fn diagnostics_by_file(
        &self,
        position_encoding: PositionEncoding,
    ) -> DiagnosticRead<DiagnosticCandidate>;
}

impl WithDiagnostics for crate::project::BexProject {
    /// Collect diagnostics for all files in the project (compiler2 only).
    fn diagnostics_by_file(
        &self,
        position_encoding: PositionEncoding,
    ) -> DiagnosticRead<DiagnosticCandidate> {
        let db = match self.db.try_lock() {
            Ok(db) => db,
            Err(std::sync::TryLockError::WouldBlock) => return DiagnosticRead::Busy,
            Err(std::sync::TryLockError::Poisoned(_)) => return DiagnosticRead::Poisoned,
        };
        let source_snapshot = db.source_snapshot();

        let source_files = db.get_source_files();

        let mut file_sources: HashMap<baml_base::FileId, String> = HashMap::new();
        let mut file_paths: HashMap<baml_base::FileId, std::path::PathBuf> = HashMap::new();
        let mut diags_by_file: Vec<(
            std::path::PathBuf,
            Vec<baml_compiler_diagnostics::Diagnostic>,
        )> = Vec::new();

        for file in &source_files {
            let file_id = file.file_id(&**db);
            let Some(path) = db.file_id_to_path(file_id).cloned() else {
                continue;
            };

            let text = file.text(&**db).clone();
            file_sources.insert(file_id, text);
            file_paths.insert(file_id, path.clone());

            let diags = baml_lsp2_actions::check_file(&**db, *file);
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

        for (path, diags) in diags_by_file {
            for diag in diags {
                if let Some(lsp_diag) = to_lsp_diagnostic(diag, &config, position_encoding) {
                    grouped.entry(path.clone()).or_default().push(lsp_diag);
                }
            }
        }

        let source_texts = file_paths
            .iter()
            .filter_map(|(file_id, path)| {
                file_sources.get(file_id).map(|text| {
                    (
                        FsPath::from_str(path.to_string_lossy().into_owned()),
                        text.clone(),
                    )
                })
            })
            .collect();

        DiagnosticRead::Ready(DiagnosticCandidate {
            source_revision: source_snapshot.revision,
            documents: grouped,
            source_texts,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::project::BexProject;

    fn project() -> Arc<BexProject> {
        Arc::new(BexProject::new(
            &vfs::VfsPath::new(vfs::MemoryFS::new()),
            Arc::new(sys_ops::SysOpsBuilder::new().build()),
        ))
    }

    #[test]
    fn invalid_open_document_produces_revision_and_version_tagged_diagnostics() {
        let project = project();
        let path = FsPath::from_str("/main.baml".to_string());
        let uri = lsp_types::Url::parse("file:///main.baml").expect("valid test URI");
        let text = "function f() -> int {".to_string();
        let revision = project.apply_open_document(path.clone(), uri, 8, text.clone());

        let DiagnosticRead::Ready(candidate) = project.diagnostics_by_file(PositionEncoding::Utf16)
        else {
            panic!("an uncontended invalid document should produce diagnostics");
        };

        assert_eq!(candidate.source_revision, revision);
        assert_eq!(candidate.source_texts.get(&path), Some(&text));
        assert!(
            candidate
                .documents
                .get(std::path::Path::new("/main.baml"))
                .is_some_and(|diagnostics| !diagnostics.is_empty()),
            "the final invalid edit must remain publishable without an engine"
        );
    }

    #[test]
    #[cfg(panic = "unwind")]
    fn diagnostics_distinguish_busy_from_poison() {
        let busy_project = project();
        let source_guard = busy_project.db.lock().expect("source lock");
        assert!(matches!(
            busy_project.diagnostics_by_file(PositionEncoding::Utf16),
            DiagnosticRead::Busy
        ));
        drop(source_guard);

        let poisoned_project = project();
        let poisoner = poisoned_project.clone();
        let poison = std::thread::spawn(move || {
            let _guard = poisoner.db.lock().expect("source lock");
            panic!("poison diagnostics source gate for the unwind-enabled unit test");
        });
        assert!(poison.join().is_err());
        assert!(matches!(
            poisoned_project.diagnostics_by_file(PositionEncoding::Utf16),
            DiagnosticRead::Poisoned
        ));
    }
}
