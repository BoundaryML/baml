// Diagnostics implementation using the Salsa database.
// Gathers parse errors, type errors, and name errors from the compiler.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use baml_db::{
    FileId, RootDatabase, Setter, SourceFile,
    baml_diagnostics::{NameError, ParseError, TypeError},
    baml_hir::{self, FunctionBody, ItemId},
    baml_parser, baml_thir,
};
use lsp_server::ErrorCode;
use lsp_types::{
    DiagnosticSeverity, PublishDiagnosticsParams, Url, notification::PublishDiagnostics,
};
use parking_lot::Mutex;

use super::LSPResult;
use crate::{
    Session,
    baml_project::Project,
    baml_source_file::LineIndex,
    baml_text_size::{TextRange, TextSize},
    edit::ToRangeExt,
    server::{Result, api::ResultExt, client::Notifier},
};

/// Convert a text_size::TextRange (from baml_base/Span) to our local TextRange
fn convert_text_range(range: text_size::TextRange) -> TextRange {
    TextRange::new(
        TextSize::new(range.start().into()),
        TextSize::new(range.end().into()),
    )
}

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

pub(super) fn publish_diagnostics(
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

// If any file changed in the workspace, publish new diagnostics for the baml project
// that file belongs to.
pub fn publish_session_lsp_diagnostics(
    notifier: &Notifier,
    session: &mut Session,
    file_url: &Url,
) -> Result<()> {
    // let keys = session.index().documents.keys();
    let path = file_url.to_file_path().unwrap_or_default();
    let Ok(project) = session.get_or_create_project(&path) else {
        tracing::info!(
            "BAML file not in baml_src directory, not publishing diagnostics: {}",
            file_url
        );
        return Ok(());
    };

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

pub(super) fn project_diagnostics(
    project: Arc<Mutex<Project>>,
    feature_flags: &[String],
    session: &Session,
) -> HashMap<Url, Vec<lsp_types::Diagnostic>> {
    tracing::info!(
        "project_diagnostics called with feature_flags: {:?}",
        feature_flags
    );
    let guard = project.lock();
    let root_path = PathBuf::from(guard.root_path());

    // Initialize the map with an entry for every file in the project.
    // This is important as we want to CLEAR existing error diagnostics we pushed if errors got fixed.
    let mut diagnostics_map: HashMap<Url, Vec<lsp_types::Diagnostic>> = guard
        .baml_project
        .files
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

    // Create a RootDatabase and add all files
    let mut db = RootDatabase::new();

    // Build a mapping from FileId -> (PathBuf, source text, LineIndex)
    // We need this to convert Spans to LSP Ranges later
    let mut file_info: HashMap<FileId, (PathBuf, String, LineIndex)> = HashMap::new();
    let mut source_files: Vec<SourceFile> = Vec::new();

    // Merge files and unsaved_files, with unsaved_files taking precedence
    let mut all_files = guard.baml_project.files.clone();
    for (key, doc) in &guard.baml_project.unsaved_files {
        all_files.insert(key.clone(), doc.clone());
    }

    for (doc_key, text_doc) in &all_files {
        let path = doc_key.path();
        let contents = text_doc.contents.clone();
        let line_index = LineIndex::from_source_text(&contents);

        let source_file = db.add_file(path, &contents);
        let file_id = source_file.file_id(&db);
        file_info.insert(file_id, (path.to_path_buf(), contents, line_index));
        source_files.push(source_file);
    }

    // Create the project root and set the files
    let project_root = db.set_project_root(&root_path);
    project_root.set_files(&mut db).to(source_files.clone());

    // 1. Gather parse errors
    for source_file in &source_files {
        let parse_errors = baml_parser::parse_errors(&db, *source_file);
        for error in parse_errors {
            if let Some(diag) = parse_error_to_diagnostic(&error, &file_info, session) {
                let file_id = get_parse_error_file_id(&error);
                if let Some((path, _, _)) = file_info.get(&file_id) {
                    if let Ok(url) = Url::from_file_path(path) {
                        diagnostics_map.entry(url).or_default().push(diag);
                    }
                }
            }
        }
    }

    // 2. Gather name errors (duplicate names)
    let name_errors = baml_hir::validate_duplicate_names(&db, project_root);
    for error in name_errors {
        if let Some((diag, file_id)) = name_error_to_diagnostic(&error, &file_info, session) {
            if let Some((path, _, _)) = file_info.get(&file_id) {
                if let Ok(url) = Url::from_file_path(path) {
                    diagnostics_map.entry(url).or_default().push(diag);
                }
            }
        }
    }

    // 3. Gather type errors from function inference
    // Build typing context for all functions
    let globals = baml_thir::build_typing_context_from_files(&db, &source_files);
    let class_fields = baml_thir::build_class_fields_from_files(&db, project_root);

    for source_file in &source_files {
        let _file_id = source_file.file_id(&db);
        let items_struct = baml_hir::file_items(&db, *source_file);
        let items = items_struct.items(&db);

        for item in items {
            if let ItemId::Function(func_loc) = item {
                let signature = baml_hir::function_signature(&db, *func_loc);
                let body = baml_hir::function_body(&db, *func_loc);

                // Only infer types for expression functions (not LLM functions)
                if matches!(*body, FunctionBody::Expr(_)) {
                    let inference_result = baml_thir::infer_function(
                        &db,
                        &signature,
                        &body,
                        Some(globals.clone()),
                        Some(class_fields.clone()),
                    );

                    for type_error in &inference_result.errors {
                        if let Some(diag) =
                            type_error_to_diagnostic(type_error, &file_info, session)
                        {
                            let error_file_id = get_type_error_file_id(type_error);
                            if let Some((path, _, _)) = file_info.get(&error_file_id) {
                                if let Ok(url) = Url::from_file_path(path) {
                                    diagnostics_map.entry(url).or_default().push(diag);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    diagnostics_map
}

/// Convert a ParseError to an LSP Diagnostic
fn parse_error_to_diagnostic(
    error: &ParseError,
    file_info: &HashMap<FileId, (PathBuf, String, LineIndex)>,
    session: &Session,
) -> Option<lsp_types::Diagnostic> {
    let (message, span) = match error {
        ParseError::UnexpectedToken {
            expected,
            found,
            span,
        } => (format!("Expected {}, found {}", expected, found), span),
        ParseError::UnexpectedEof { expected, span } => {
            (format!("Unexpected end of file, expected {}", expected), span)
        }
    };

    let (_, source_text, line_index) = file_info.get(&span.file_id)?;
    let range = convert_text_range(span.range)
        .to_range(source_text, line_index, session.position_encoding);

    Some(lsp_types::Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(lsp_types::NumberOrString::String("parse-error".to_string())),
        code_description: None,
        source: Some("baml".to_string()),
        message,
        related_information: None,
        tags: None,
        data: None,
    })
}

/// Convert a NameError to an LSP Diagnostic
fn name_error_to_diagnostic(
    error: &NameError,
    file_info: &HashMap<FileId, (PathBuf, String, LineIndex)>,
    session: &Session,
) -> Option<(lsp_types::Diagnostic, FileId)> {
    match error {
        NameError::DuplicateName {
            name,
            kind,
            first: _,
            first_path,
            second,
            second_path: _,
        } => {
            let (_, source_text, line_index) = file_info.get(&second.file_id)?;
            let range = convert_text_range(second.range)
                .to_range(source_text, line_index, session.position_encoding);

            let message = format!(
                "Duplicate {} '{}' (first defined in {})",
                kind, name, first_path
            );

            Some((
                lsp_types::Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(lsp_types::NumberOrString::String("name-error".to_string())),
                    code_description: None,
                    source: Some("baml".to_string()),
                    message,
                    related_information: None,
                    tags: None,
                    data: None,
                },
                second.file_id,
            ))
        }
    }
}

/// Convert a TypeError to an LSP Diagnostic
fn type_error_to_diagnostic<T: std::fmt::Display>(
    error: &TypeError<T>,
    file_info: &HashMap<FileId, (PathBuf, String, LineIndex)>,
    session: &Session,
) -> Option<lsp_types::Diagnostic> {
    let (message, span) = match error {
        TypeError::TypeMismatch {
            expected,
            found,
            span,
        } => (
            format!("Type mismatch: expected {}, found {}", expected, found),
            span,
        ),
        TypeError::UnknownType { name, span } => (format!("Unknown type: {}", name), span),
        TypeError::UnknownVariable { name, span } => (format!("Unknown variable: {}", name), span),
        TypeError::InvalidBinaryOp { op, lhs, rhs, span } => (
            format!("Invalid binary operation '{}' on {} and {}", op, lhs, rhs),
            span,
        ),
        TypeError::InvalidUnaryOp { op, operand, span } => {
            (format!("Invalid unary operation '{}' on {}", op, operand), span)
        }
        TypeError::ArgumentCountMismatch {
            expected,
            found,
            span,
        } => (
            format!(
                "Argument count mismatch: expected {}, found {}",
                expected, found
            ),
            span,
        ),
        TypeError::NotCallable { ty, span } => (format!("Type {} is not callable", ty), span),
        TypeError::NoSuchField { ty, field, span } => {
            (format!("No field '{}' on type {}", field, ty), span)
        }
        TypeError::NotIndexable { ty, span } => (format!("Type {} is not indexable", ty), span),
    };

    let (_, source_text, line_index) = file_info.get(&span.file_id)?;
    let range = convert_text_range(span.range)
        .to_range(source_text, line_index, session.position_encoding);

    Some(lsp_types::Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(lsp_types::NumberOrString::String("type-error".to_string())),
        code_description: None,
        source: Some("baml".to_string()),
        message,
        related_information: None,
        tags: None,
        data: None,
    })
}

/// Get the FileId from a ParseError
fn get_parse_error_file_id(error: &ParseError) -> FileId {
    match error {
        ParseError::UnexpectedToken { span, .. } => span.file_id,
        ParseError::UnexpectedEof { span, .. } => span.file_id,
    }
}

/// Get the FileId from a TypeError
fn get_type_error_file_id<T>(error: &TypeError<T>) -> FileId {
    match error {
        TypeError::TypeMismatch { span, .. } => span.file_id,
        TypeError::UnknownType { span, .. } => span.file_id,
        TypeError::UnknownVariable { span, .. } => span.file_id,
        TypeError::InvalidBinaryOp { span, .. } => span.file_id,
        TypeError::InvalidUnaryOp { span, .. } => span.file_id,
        TypeError::ArgumentCountMismatch { span, .. } => span.file_id,
        TypeError::NotCallable { span, .. } => span.file_id,
        TypeError::NoSuchField { span, .. } => span.file_id,
        TypeError::NotIndexable { span, .. } => span.file_id,
    }
}

/// Returns diagnostics only for the specified file URL.
pub fn file_diagnostics(
    _project: Arc<Mutex<Project>>,
    file_url: &Url,
    feature_flags: &[String],
) -> Vec<lsp_types::Diagnostic> {
    tracing::info!(
        "file_diagnostics called for URL: {} with feature_flags: {:?}",
        file_url,
        feature_flags
    );

    // TODO: Implement actual diagnostics using salsa database
    // For now, return empty diagnostics
    vec![]
}

/// For a project root and a path to a file in that project, return an absolute path
/// to that file.
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
