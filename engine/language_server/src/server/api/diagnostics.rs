use baml_runtime::InternalRuntimeInterface;
use lsp_server::ErrorCode;
use lsp_types::DiagnosticSeverity;
use lsp_types::{notification::PublishDiagnostics, PublishDiagnosticsParams, Url};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::baml_project::Project;
use crate::baml_text_size::TextSize;
use crate::server::client::Notifier;
use crate::server::Result;
use crate::{DocumentKey, Session};

use super::LSPResult;

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

pub fn session_lsp_diagnostics(
    session: &mut Session,
    file_url: &Url,
) -> Vec<lsp_types::Diagnostic> {
    // let keys = session.index().documents.keys();
    let path = file_url.to_file_path().unwrap_or(PathBuf::new());
    let _ = session
        .ensure_project_db_for_baml_file(file_url)
        .map_err(|e| {
            tracing::error!("Failed to ensure project db for baml file: {}", e);
        });
    let project = session
        .project_db_for_path_mut(path)
        .expect("We just ensured the session is valid");

    project_diagnostics(project, Some(file_url))
}

pub fn project_diagnostics(
    project: Arc<Mutex<Project>>,
    file_url: Option<&Url>,
) -> Vec<lsp_types::Diagnostic> {
    let guard = project.lock().unwrap();
    let root_path = PathBuf::from(guard.root_path());
    let fake_env = HashMap::new();
    let baml_diagnostics = match guard.baml_project.runtime(fake_env) {
        Ok(runtime) => {
            runtime.internal().diagnostics().clone()
            // Diagnostics::new(PathBuf::from("/fake1"))
        }
        Err(err) => err,
    };

    let errors = baml_diagnostics
        .errors()
        .iter()
        .filter(|e| file_url.map_or(true, |url| matches_target(&root_path, &url, &e.span())))
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
        .filter(|w| file_url.map_or(true, |url| matches_target(&root_path, &url, &w.span())))
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

fn matches_target(
    project_root: &Path,
    target: &Url,
    span: &internal_baml_diagnostics::Span,
) -> bool {
    let absolute_file = DocumentKey::from_url(project_root, target);
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

    let doc_key = DocumentKey::from_path(project_root, &PathBuf::from(span_path))
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
