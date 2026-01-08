//! Diagnostics implementation using the centralized `LspDatabase::check()` method.
//!
//! This replaces the previous manual diagnostic collection with the unified
//! `Diagnostic` type from `baml_compiler_diagnostics`, eliminating code duplication
//! with the test infrastructure (`baml_ide_tests`).

use std::{collections::HashMap, sync::Arc};

use lsp_types::{
    DiagnosticSeverity, PublishDiagnosticsParams, Url, notification::PublishDiagnostics,
};
use parking_lot::Mutex;

use super::{
    ResultExt,
    lsp_diagnostic::{LspConversionConfig, compute_line_starts, to_lsp_diagnostic},
};
use crate::{
    Session,
    baml_project::Project,
    server::{Result, client::Notifier},
};

pub(super) fn publish_diagnostics(
    notifier: &Notifier,
    project: Arc<Mutex<Project>>,
    version: Option<i32>,
    _feature_flags: &[String],
    session: &Session,
) -> Result<()> {
    let diagnostics = project_diagnostics(project, session.position_encoding);

    // Calculate counts for status bar
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

    // Publish diagnostics for each file
    for (uri, diagnostics) in diagnostics.clone() {
        notifier
            .notify::<PublishDiagnostics>(PublishDiagnosticsParams {
                uri,
                diagnostics,
                version,
            })
            .internal_error()?;
    }

    tracing::info!("sending status bar diagnostics");
    // Update status bar
    notifier
        .notify_raw(
            "runtime_diagnostics".to_string(),
            serde_json::json!({
                "errors": error_count,
                "warnings": warning_count,
            }),
        )
        .internal_error()?;

    Ok(())
}

/// If any file changed in the workspace, publish new diagnostics for the baml project
/// that file belongs to.
pub fn publish_session_lsp_diagnostics(
    notifier: &Notifier,
    session: &mut Session,
    file_url: &Url,
) -> Result<()> {
    let path = file_url.to_file_path().unwrap_or_default();
    let Ok(project) = session.get_or_create_project(&path) else {
        tracing::info!(
            "BAML file not in baml_src directory, not publishing diagnostics: {}",
            file_url
        );
        return Ok(());
    };

    let diagnostics = project_diagnostics(project, session.position_encoding);
    for (uri, diagnostics) in diagnostics {
        notifier
            .notify::<lsp_types::notification::PublishDiagnostics>(PublishDiagnosticsParams {
                uri,
                version: None,
                diagnostics,
            })
            .map_err(|e| anyhow::anyhow!("did_change err: {}", e))
            .internal_error()?;
    }
    Ok(())
}

/// Collect diagnostics for all files in the project using the centralized check() method.
///
/// This is the main entry point for diagnostics collection. It uses `LspDatabase::check()`
/// which centralizes all diagnostic collection (parse errors, HIR diagnostics, type errors)
/// in one place, eliminating code duplication with the test infrastructure.
fn project_diagnostics(
    project: Arc<Mutex<Project>>,
    position_encoding: crate::edit::PositionEncoding,
) -> HashMap<Url, Vec<lsp_types::Diagnostic>> {
    let guard = project.lock();
    let lsp_db = guard.lsp_db();

    // Use the centralized check() method - this replaces ~150 lines of manual collection!
    let check_result = lsp_db.check();

    // Build file_sources map with line_starts for LSP conversion
    let file_sources: HashMap<baml_db::FileId, (String, Vec<u32>)> = check_result
        .sources
        .iter()
        .map(|(file_id, text)| {
            let line_starts = compute_line_starts(text);
            (*file_id, (text.clone(), line_starts))
        })
        .collect();

    let config = LspConversionConfig {
        file_paths: &check_result.file_paths,
        file_sources: &file_sources,
        position_encoding,
    };

    // Initialize empty diagnostics for all files (so files with no errors get cleared)
    let mut result: HashMap<Url, Vec<lsp_types::Diagnostic>> = HashMap::new();
    for path in check_result.file_paths.values() {
        if let Ok(url) = Url::from_file_path(path) {
            result.entry(url).or_default();
        }
    }

    // Convert and add diagnostics
    for diag in &check_result.diagnostics {
        if let Some((url, lsp_diag)) = to_lsp_diagnostic(diag, &config) {
            result.entry(url).or_default().push(lsp_diag);
        }
    }

    result
}

/// Returns diagnostics only for the specified file URL.
pub fn file_diagnostics(
    _project: Arc<Mutex<Project>>,
    file_url: &Url,
    _feature_flags: &[String],
) -> Vec<lsp_types::Diagnostic> {
    tracing::info!(
        "file_diagnostics called for URL: {} with feature_flags: {:?}",
        file_url,
        _feature_flags
    );

    // TODO: Implement per-file diagnostics using check_file() for better performance
    vec![]
}

/// Creates an error diagnostic for BAML files outside baml_src directories
pub fn not_in_baml_src_diagnostic(file_url: &Url) -> lsp_types::PublishDiagnosticsParams {
    let range = lsp_types::Range::new(
        lsp_types::Position::new(0, 0),
        // Choose a position reasonably likely to be either at or past the end of the file.
        // IDEs should correctly defend against this, ideally clamping it to the end of the file.
        lsp_types::Position::new(10_000, 0),
    );

    lsp_types::PublishDiagnosticsParams {
        uri: file_url.clone(),
        diagnostics: vec![lsp_types::Diagnostic::new(
            range,
            Some(lsp_types::DiagnosticSeverity::ERROR),
            None,
            None,
            "BAML files must be placed in a baml_src/ directory, see https://docs.boundaryml.com/guide/introduction/baml_src.".to_string(),
            None,
            None,
        )],
        version: None,
    }
}
