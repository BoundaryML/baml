use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use baml_runtime::InternalRuntimeInterface;
use internal_baml_diagnostics::{SourceFile, Span};
use lsp_server::{ErrorCode, Notification, Request};
use lsp_types::{
    notification::PublishDiagnostics, Diagnostic, DiagnosticSeverity, PublishDiagnosticsParams, Url,
};
use parking_lot::Mutex;

use super::LSPResult;
use crate::{
    baml_project::{self, Project},
    baml_text_size::TextSize,
    server::{api::ResultExt, client::Notifier, Result},
    DocumentKey, Session,
};

pub(super) fn clear_diagnostics(uri: &Url, notifier: &Notifier) -> Result<()> {
    notifier
        .notify::<PublishDiagnostics>(PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics: vec![],
            version: None,
        })
        .with_failure_code(ErrorCode::InternalError)?;
    Ok(())
}

pub fn publish_diagnostics(
    notifier: &Notifier,
    project: Arc<Mutex<Project>>,
    version: Option<i32>,
    feature_flags: &[String],
    session: &Session,
) -> Result<()> {
    tracing::info!(
        "publish_diagnostics called with feature_flags: {:?}",
        feature_flags
    );
    let diagnostics = project_diagnostics(project.clone(), feature_flags, session);
    // Calculate counts *after* all diagnostics (including generator) are collected.
    let error_count = diagnostics
        .iter()
        .filter(|(_, diags)| {
            diags
                .iter()
                .any(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        })
        .count();
    let warning_count = diagnostics
        .iter()
        .filter(|(_, diags)| {
            diags
                .iter()
                .any(|d| d.severity == Some(DiagnosticSeverity::WARNING))
        })
        .count();

    for (uri, diagnostics) in diagnostics.clone() {
        notifier
            .notify::<PublishDiagnostics>(PublishDiagnosticsParams {
                uri: uri.clone(),
                diagnostics,
                version,
            })
            .internal_error()?;
    }

    tracing::info!("sending status bar diagnostics");
    // Update status bar
    notifier
        .0
        .send(lsp_server::Message::Notification(Notification::new(
            "runtime_diagnostics".to_string(),
            serde_json::json!({
                "errors": error_count,
                "warnings": warning_count,
            }),
        )))
        .internal_error()?;

    Ok(())
}

// If any file changed in the workspace, publish new diagnostics for the baml project
// that file belongs to.
pub fn publish_session_lsp_diagnostics(
    notifier: &Notifier,
    session: &mut Session,
    file_url: &Url,
) -> Result<()> {
    // let keys = session.index().documents.keys();
    let path = file_url.to_file_path().unwrap_or_default();
    if !file_url.to_string().contains("baml_src") {
        return Ok(());
    }
    tracing::info!("publishing diagnostics for {}", file_url);
    let project = session
        .get_or_create_project(&path)
        .expect("We just ensured the session is valid.");

    let default_flags = vec!["beta".to_string()];
    let feature_flags = session
        .baml_settings
        .feature_flags
        .as_ref()
        .unwrap_or(&default_flags);
    tracing::info!(
        "publish_diagnostics_for_file: session feature_flags: {:?}",
        feature_flags
    );
    let diagnostics = project_diagnostics(project.clone(), feature_flags, session);
    for (uri, diagnostics) in diagnostics {
        notifier
            .notify::<lsp_types::notification::PublishDiagnostics>(PublishDiagnosticsParams {
                uri: uri.clone(),
                version: None,
                diagnostics,
            })
            .map_err(|e| anyhow::anyhow!("did_change err: {}", e))
            .internal_error()?;
    }
    Ok(())
}

pub fn project_diagnostics(
    project: Arc<Mutex<Project>>,
    feature_flags: &[String],
    session: &Session,
) -> HashMap<Url, Vec<lsp_types::Diagnostic>> {
    tracing::info!(
        "project_diagnostics called with feature_flags: {:?}",
        feature_flags
    );
    let mut guard = project.lock();
    let root_path = PathBuf::from(guard.root_path());
    let fake_env = HashMap::new();
    let baml_diagnostics = match guard.baml_project.runtime(fake_env, feature_flags) {
        Ok(runtime) => {
            runtime.internal().diagnostics().clone()
            // Diagnostics::new(PathBuf::from("/fake1"))
        }
        Err(err) => err,
    };
    tracing::debug!("baml_project_diagnostics: {:?}", baml_diagnostics);

    // Initialize the map with an entry for every file in the project.
    // This is important as we want to CLEAR existing error diagnostics we pushed if errors got fixed.
    let mut diagnostics_map: HashMap<Url, Vec<lsp_types::Diagnostic>> = guard
        .baml_project
        .files // Use the files map from the project
        .keys()
        .filter_map(|doc_key| {
            let path = doc_key.path();
            match Url::from_file_path(path) {
                Ok(url) => Some((url, Vec::new())), // Initialize with empty diagnostics
                Err(_) => {
                    tracing::warn!(
                        "Failed to convert path {:?} to URL for initial diagnostics map",
                        path
                    );
                    None
                }
            }
        })
        .collect();

    // Add regular BAML diagnostics
    for error in baml_diagnostics.errors().iter() {
        let span_path = ensure_absolute(&root_path, &PathBuf::from(error.span().file.path()));
        let url = match Url::from_file_path(&span_path) {
            Ok(url) => url,
            Err(_) => {
                tracing::warn!(
                    "Failed to convert path {:?} to URL for diagnostic",
                    span_path
                );
                continue;
            }
        };
        if let Some(range) = span_to_range(&guard, &root_path, error.span()) {
            let diag = lsp_types::Diagnostic::new(
                range,
                Some(DiagnosticSeverity::ERROR),
                None,
                None,
                error.message().to_string(),
                None,
                None,
            );
            diagnostics_map.entry(url).or_default().push(diag);
        }
    }

    for warning in baml_diagnostics.warnings().iter() {
        let span_path = ensure_absolute(&root_path, &PathBuf::from(warning.span().file.path()));
        let url = match Url::from_file_path(&span_path) {
            Ok(url) => url,
            Err(_) => {
                tracing::warn!(
                    "Failed to convert path {:?} to URL for diagnostic",
                    span_path
                );
                continue;
            }
        };
        if let Some(range) = span_to_range(&guard, &root_path, warning.span()) {
            let diag = lsp_types::Diagnostic::new(
                range,
                Some(DiagnosticSeverity::WARNING),
                None,
                None,
                warning.message().to_string(),
                None,
                None,
            );
            diagnostics_map.entry(url).or_default().push(diag);
        }
    }

    // Add generator version diagnostics
    if let Ok(generators) = guard.baml_project.list_generators(feature_flags) {
        for gen in generators.into_iter() {
            if let Some(message) = guard.baml_project.check_version(&gen, false) {
                if let Some(range) = span_to_range(
                    &guard,
                    &root_path,
                    &Span {
                        file: SourceFile::new_static(PathBuf::from(gen.span.file_path.clone()), ""),
                        start: gen.span.start,
                        end: gen.span.end,
                    },
                ) {
                    let diagnostic = Diagnostic {
                        range,
                        message,
                        severity: Some(DiagnosticSeverity::ERROR),
                        source: Some("baml".to_string()),
                        ..Default::default()
                    };

                    let span_path =
                        ensure_absolute(&root_path, &PathBuf::from(gen.span.file_path.clone()));
                    match Url::from_file_path(span_path) {
                        Ok(uri) => {
                            diagnostics_map.entry(uri).or_default().push(diagnostic);
                        }
                        Err(_) => {
                            tracing::error!(
                                "Failed to parse URI for generator diagnostic: {}",
                                gen.span.file_path
                            );
                        }
                    }
                } else {
                    tracing::warn!(
                        "Could not get range for generator diagnostic span in file {}",
                        gen.span.file_path
                    );
                }
            }
        }
    }

    // Check for generator version mismatch as well.
    if let Err(message) = guard.get_common_generator_version() {
        // Add the diagnostic to all generators
        if let Ok(generators) = guard.list_generators(feature_flags) {
            // Need to list generators again to get their spans
            for gen in &generators {
                if let Some(range) = span_to_range(
                    &guard,
                    &root_path,
                    &Span {
                        file: SourceFile::new_static(PathBuf::from(gen.span.file_path.clone()), ""),
                        start: gen.span.start,
                        end: gen.span.end,
                    },
                ) {
                    let diagnostic = Diagnostic {
                        range,
                        message: message.to_string(),
                        severity: Some(DiagnosticSeverity::ERROR),
                        source: Some("baml".to_string()),
                        ..Default::default()
                    };

                    let span_path =
                        ensure_absolute(&root_path, &PathBuf::from(gen.span.file_path.clone()));

                    match Url::from_file_path(span_path) {
                        Ok(uri) => {
                            diagnostics_map.entry(uri).or_default().push(diagnostic);
                        }
                        Err(_) => {
                            tracing::error!(
                                "Failed to parse URI for version mismatch diagnostic: {}",
                                gen.span.file_path
                            );
                        }
                    }
                }
            }
        }
    }

    diagnostics_map
}

/// Returns diagnostics only for the specified file URL.
pub fn file_diagnostics(
    project: Arc<Mutex<Project>>,
    file_url: &Url,
    feature_flags: &[String],
) -> Vec<lsp_types::Diagnostic> {
    tracing::info!(
        "file_diagnostics called for URL: {} with feature_flags: {:?}",
        file_url,
        feature_flags
    );
    let mut guard = project.lock();
    let root_path = PathBuf::from(guard.root_path());
    let fake_env = HashMap::new();
    let baml_diagnostics = match guard.baml_project.runtime(fake_env, feature_flags) {
        Ok(runtime) => runtime.internal().diagnostics().clone(),
        Err(err) => err,
    };

    let errors = baml_diagnostics
        .errors()
        .iter()
        .filter(|e| matches_target(&root_path, file_url, e.span()))
        .filter_map(|error| {
            Some(lsp_types::Diagnostic::new(
                span_to_range(&guard, &root_path, error.span())?,
                Some(DiagnosticSeverity::ERROR),
                None,
                None,
                error.message().to_string(),
                None,
                None,
            ))
        });

    let warnings = baml_diagnostics
        .warnings()
        .iter()
        .filter(|w| matches_target(&root_path, file_url, w.span()))
        .filter_map(|warning| {
            Some(lsp_types::Diagnostic::new(
                span_to_range(&guard, &root_path, warning.span())?,
                Some(DiagnosticSeverity::WARNING),
                None,
                None,
                warning.message().to_string(),
                None,
                None,
            ))
        });

    errors.chain(warnings).collect()
}

/// Checks if the diagnostic span's file path matches the target URL's path.
fn matches_target(
    project_root: &Path,
    target_url: &Url,
    span: &internal_baml_diagnostics::Span,
) -> bool {
    let absolute_file = DocumentKey::from_url(project_root, target_url);
    let absolute_target = DocumentKey::from_path(project_root, &PathBuf::from(span.file.path()));
    match (&absolute_file, &absolute_target) {
        (Ok(file), Ok(target)) => file.path() == target.path(),
        _ => {
            tracing::error!(
                "Error determining file path: {:?}, or target path: {:?}",
                absolute_file,
                absolute_target
            );
            false
        }
    }
}

/// Convert a baml Span into a lsp_types::Range for use in an `lsp_types::Diagnostic.
/// Params:
///   - project: Pass the baml project, we'll need it for getting the span's
///     document's line index.
///   - project_root: Root of the baml project, needed for augmenting span paths, which
///     seem to sporadically be relative paths.
///   - file_url: The absolute file:/// url of the file whose diagnostics we care about.
///     Spans not related to this URL will be filtered out.
///   - span: The baml span to convert.
fn span_to_range(
    project: &Project,
    project_root: &Path,
    span: &internal_baml_diagnostics::Span,
) -> Option<lsp_types::Range> {
    let span_path = ensure_absolute(project_root, &PathBuf::from(span.file.path()));
    // let span_path_with_prefix = span.file.path();
    // let span_path = span_path_with_prefix.strip_prefix("file://").map_err(|e| {
    //     tracing::warn!("Failed to strip file:// prefix from span path: {}", e);
    //     e
    // })?;

    let doc_key = DocumentKey::from_path(project_root, &span_path)
        .map_err(|e| {
            tracing::warn!("Failed to create DocumentKey: {}", e);
        })
        .ok()?;
    let doc = project
        .baml_project
        .unsaved_files
        .get(&doc_key)
        .or(project.baml_project.files.get(&doc_key))?;
    let line_index = doc.index();

    let start_loc =
        line_index.source_location(TextSize::new(span.start as u32), span.file.as_str());
    let end_loc = line_index.source_location(TextSize::new(span.end as u32), span.file.as_str());

    let (start_line, start_col) = (
        start_loc.row.to_zero_indexed(),
        start_loc.column.to_zero_indexed(),
    );
    let (end_line, end_col) = (
        end_loc.row.to_zero_indexed(),
        end_loc.column.to_zero_indexed(),
    );
    Some(lsp_types::Range {
        start: lsp_types::Position::new(start_line as u32, start_col as u32),
        end: lsp_types::Position::new(end_line as u32, end_col as u32),
    })
}

/// For a project root and a path to a file in that project, return an absolute path
/// to that file.
/// This function is taylored to the quirks of spans coming from baml_runtime, which
/// sometimes include absolute paths to the source files and sometimes include
/// "relative" paths (scare-quotes are used because these paths prefixed with `/`,
/// making them technically absolute).
fn ensure_absolute(project_root: &Path, file_path: &Path) -> PathBuf {
    let file_path_relative = file_path
        .strip_prefix(std::path::MAIN_SEPARATOR_STR)
        .unwrap_or(file_path);

    if file_path
        .to_str()
        .unwrap()
        .starts_with(project_root.to_str().unwrap())
    {
        PathBuf::from(file_path)
    } else {
        project_root.join(file_path_relative)
    }
}
