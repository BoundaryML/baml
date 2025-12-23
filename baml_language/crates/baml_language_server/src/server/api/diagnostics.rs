// Diagnostics implementation using the Salsa database.
// Gathers parse errors, type errors, and name errors from the compiler.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use baml_db::{
    FileId, SourceFile,
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
    _feature_flags: &[String],
    session: &Session,
) -> HashMap<Url, Vec<lsp_types::Diagnostic>> {
    let guard = project.lock();

    // Use the Project's LspDatabase for incremental compilation (Salsa caching)
    let lsp_db = guard.lsp_db();
    let db = lsp_db.db();

    // Get the project root from LspDatabase
    let Some(project_root) = lsp_db.project() else {
        tracing::warn!("No project root set in LspDatabase");
        return HashMap::new();
    };

    // Build file info and initialize diagnostics map from LspDatabase files
    // Initialize with empty diagnostics so files with no errors get cleared
    let mut file_info: HashMap<FileId, (PathBuf, String, LineIndex)> = HashMap::new();
    let mut diagnostics_map: HashMap<Url, Vec<lsp_types::Diagnostic>> = HashMap::new();
    let source_files: Vec<SourceFile> = lsp_db.files().collect();

    for source_file in &source_files {
        let path = source_file.path(db);
        let contents = source_file.text(db);
        let line_index = LineIndex::from_source_text(contents);
        let file_id = source_file.file_id(db);
        file_info.insert(file_id, (path.clone(), contents.to_string(), line_index));

        // Initialize empty diagnostics for this file
        if let Ok(url) = Url::from_file_path(&path) {
            diagnostics_map.entry(url).or_default();
        }
    }

    // Helper to add a diagnostic to the map
    let mut add_diagnostic = |file_id: FileId, diag: lsp_types::Diagnostic| {
        if let Some((path, _, _)) = file_info.get(&file_id) {
            if let Ok(url) = Url::from_file_path(path) {
                diagnostics_map.entry(url).or_default().push(diag);
            }
        }
    };

    // 1. Gather parse errors
    for source_file in &source_files {
        let parse_errors = baml_parser::parse_errors(db, *source_file);
        for error in parse_errors {
            if let Some(diag) = parse_error_to_diagnostic(&error, &file_info, session) {
                add_diagnostic(get_parse_error_file_id(&error), diag);
            }
        }
    }

    // 2. Gather name errors (duplicate names)
    let name_errors = baml_hir::validate_duplicate_names(db, project_root);
    for error in name_errors {
        if let Some((diag, file_id)) = name_error_to_diagnostic(&error, &file_info, session) {
            add_diagnostic(file_id, diag);
        }
    }

    // 3. Gather type errors from function inference
    let globals = baml_thir::build_typing_context_from_files(db, &source_files);
    let class_fields = baml_thir::build_class_fields_from_files(db, project_root);

    for source_file in &source_files {
        let items_struct = baml_hir::file_items(db, *source_file);
        let items = items_struct.items(db);

        for item in items {
            if let ItemId::Function(func_loc) = item {
                let signature = baml_hir::function_signature(db, *func_loc);
                let body = baml_hir::function_body(db, *func_loc);

                // Only infer types for expression functions (not LLM functions)
                if matches!(*body, FunctionBody::Expr(_)) {
                    let inference_result = baml_thir::infer_function(
                        db,
                        &signature,
                        &body,
                        Some(globals.clone()),
                        Some(class_fields.clone()),
                        None, // type_aliases
                        None, // enum_variants
                        *func_loc,
                    );

                    for type_error in &inference_result.errors {
                        if let Some(diag) =
                            type_error_to_diagnostic(type_error, &file_info, session)
                        {
                            add_diagnostic(get_type_error_file_id(type_error), diag);
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
    let (message, span, code) = match error {
        ParseError::UnexpectedToken {
            expected,
            found,
            span,
        } => (
            format!("Expected {}, found {}", expected, found),
            span,
            "E0010",
        ),
        ParseError::UnexpectedEof { expected, span } => (
            format!("Unexpected end of file, expected {}", expected),
            span,
            "E0009",
        ),
    };

    let (_, source_text, line_index) = file_info.get(&span.file_id)?;
    let range =
        convert_text_range(span.range).to_range(source_text, line_index, session.position_encoding);

    Some(lsp_types::Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(lsp_types::NumberOrString::String(code.to_string())),
        code_description: None,
        source: Some("baml".to_string()),
        message,
        related_information: None,
        tags: None,
        data: None,
    })
}

/// Convert a NameError to an LSP Diagnostic with related information.
fn name_error_to_diagnostic(
    error: &NameError,
    file_info: &HashMap<FileId, (PathBuf, String, LineIndex)>,
    session: &Session,
) -> Option<(lsp_types::Diagnostic, FileId)> {
    match error {
        NameError::DuplicateName {
            name,
            kind,
            first,
            first_path,
            second,
            second_path: _,
        } => {
            let (_, source_text, line_index) = file_info.get(&second.file_id)?;
            let range = convert_text_range(second.range).to_range(
                source_text,
                line_index,
                session.position_encoding,
            );

            let message = format!(
                "Duplicate {} `{}` (first defined in {})",
                kind, name, first_path
            );

            // Build related information pointing to the first definition
            let related_information = file_info.get(&first.file_id).and_then(
                |(path, first_source_text, first_line_index)| {
                    let first_range = convert_text_range(first.range).to_range(
                        first_source_text,
                        first_line_index,
                        session.position_encoding,
                    );
                    let uri = Url::from_file_path(path).ok()?;
                    Some(vec![lsp_types::DiagnosticRelatedInformation {
                        location: lsp_types::Location {
                            uri,
                            range: first_range,
                        },
                        message: format!("First definition of `{}` here", name),
                    }])
                },
            );

            Some((
                lsp_types::Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(lsp_types::NumberOrString::String("E0011".to_string())),
                    code_description: None,
                    source: Some("baml".to_string()),
                    message,
                    related_information,
                    tags: None,
                    data: None,
                },
                second.file_id,
            ))
        }
    }
}

/// Convert a TypeError to an LSP Diagnostic with specific error codes.
fn type_error_to_diagnostic<T: std::fmt::Display>(
    error: &TypeError<T>,
    file_info: &HashMap<FileId, (PathBuf, String, LineIndex)>,
    session: &Session,
) -> Option<lsp_types::Diagnostic> {
    let (message, span, code) = match error {
        TypeError::TypeMismatch {
            expected,
            found,
            span,
        } => (
            format!("Expected `{}`, found `{}`", expected, found),
            span,
            "E0001",
        ),
        TypeError::UnknownType { name, span } => {
            (format!("Unknown type `{}`", name), span, "E0002")
        }
        TypeError::UnknownVariable { name, span } => {
            (format!("Unknown variable `{}`", name), span, "E0003")
        }
        TypeError::InvalidBinaryOp { op, lhs, rhs, span } => (
            format!("Invalid operation `{}` {} `{}`", lhs, op, rhs),
            span,
            "E0004",
        ),
        TypeError::InvalidUnaryOp { op, operand, span } => (
            format!("Invalid operation {}`{}`", op, operand),
            span,
            "E0004",
        ),
        TypeError::ArgumentCountMismatch {
            expected,
            found,
            span,
        } => (
            format!("Expected {} arguments, found {}", expected, found),
            span,
            "E0005",
        ),
        TypeError::NotCallable { ty, span } => {
            (format!("Type `{}` is not callable", ty), span, "E0006")
        }
        TypeError::NoSuchField { ty, field, span } => (
            format!("Type `{}` has no field `{}`", ty, field),
            span,
            "E0007",
        ),
        TypeError::NotIndexable { ty, span } => {
            (format!("Type `{}` is not indexable", ty), span, "E0008")
        }
        TypeError::NonExhaustiveMatch {
            scrutinee_type,
            missing_cases,
            span,
        } => (
            format!(
                "Non-exhaustive match on `{}`: missing cases {}",
                scrutinee_type,
                missing_cases.join(", ")
            ),
            span,
            "E0012",
        ),
        TypeError::UnreachableArm { span } => ("Unreachable match arm".to_string(), span, "E0013"),
    };

    let (_, source_text, line_index) = file_info.get(&span.file_id)?;
    let range =
        convert_text_range(span.range).to_range(source_text, line_index, session.position_encoding);

    Some(lsp_types::Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(lsp_types::NumberOrString::String(code.to_string())),
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
        TypeError::NonExhaustiveMatch { span, .. } => span.file_id,
        TypeError::UnreachableArm { span, .. } => span.file_id,
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
