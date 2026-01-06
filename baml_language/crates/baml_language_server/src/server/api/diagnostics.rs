// Diagnostics implementation using the Salsa database.
// Gathers parse errors, type errors, and name errors from the compiler.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use baml_db::{
    FileId, SourceFile,
    baml_diagnostics::{HirDiagnostic, NameError, ParseError, TypeError},
    baml_hir::{self, FunctionBody, ItemId, file_lowering},
    baml_parser, baml_tir,
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

    // 2. Gather HIR lowering diagnostics (duplicate fields, attributes, etc.)
    for source_file in &source_files {
        let lowering_result = file_lowering(db, *source_file);
        for error in lowering_result.diagnostics(db) {
            if let Some(diag) = hir_diagnostic_to_lsp_diagnostic(error, &file_info, session) {
                add_diagnostic(get_hir_diagnostic_file_id(error), diag);
            }
        }
    }

    // 3. Gather validation errors (duplicate names, reserved names)
    let validation_result = baml_hir::validate_hir(db, project_root);
    for diag in &validation_result.hir_diagnostics {
        if let Some(lsp_diag) = hir_diagnostic_to_lsp_diagnostic(diag, &file_info, session) {
            add_diagnostic(get_hir_diagnostic_file_id(diag), lsp_diag);
        }
    }
    for error in validation_result.name_errors {
        if let Some((diag, file_id)) = name_error_to_diagnostic(&error, &file_info, session) {
            add_diagnostic(file_id, diag);
        }
    }

    // 3. Gather type errors from function inference
    let globals = baml_tir::typing_context(db, project_root);
    let class_fields = baml_tir::class_field_types(db, project_root);
    let type_aliases = baml_tir::type_aliases(db, project_root);
    let enum_variants_map = baml_tir::enum_variants(db, project_root);
    let enum_variants = enum_variants_map.enums(db).clone();

    for source_file in &source_files {
        let items_struct = baml_hir::file_items(db, *source_file);
        let items = items_struct.items(db);

        for item in items {
            if let ItemId::Function(func_loc) = item {
                let signature = baml_hir::function_signature(db, *func_loc);
                let body = baml_hir::function_body(db, *func_loc);

                // Only infer types for expression functions (not LLM functions)
                if matches!(*body, FunctionBody::Expr(_)) {
                    let inference_result = baml_tir::infer_function(
                        db,
                        &signature,
                        &body,
                        Some(globals.clone()),
                        Some(class_fields.clone()),
                        Some(type_aliases.clone()),
                        Some(enum_variants.clone()),
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
        ParseError::InvalidSyntax { message, span } => (message.clone(), span, "E0010"),
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
        NameError::DuplicateTestForFunction {
            test_name,
            function_name,
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
                "Duplicate test `{}` for function `{}` (first defined in {})",
                test_name, function_name, first_path
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
                        message: format!(
                            "First definition of test `{}` for `{}` here",
                            test_name, function_name
                        ),
                    }])
                },
            );

            Some((
                lsp_types::Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(lsp_types::NumberOrString::String("E0012".to_string())),
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
        TypeError::UnknownEnumVariant {
            enum_name,
            variant_name,
            span,
        } => (
            format!("Unknown variant `{variant_name}` for enum `{enum_name}`"),
            span,
            "E0064",
        ),
        TypeError::WatchOnNonVariable { span } => (
            "$watch can only be used on simple variable expressions".to_string(),
            span,
            "E0065",
        ),
        TypeError::WatchOnUnwatchedVariable { name, span } => (
            format!("Cannot use $watch on `{name}`: variable must be declared with `watch let`"),
            span,
            "E0066",
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

/// Get the FileId from a ParseError
fn get_parse_error_file_id(error: &ParseError) -> FileId {
    match error {
        ParseError::UnexpectedToken { span, .. } => span.file_id,
        ParseError::UnexpectedEof { span, .. } => span.file_id,
        ParseError::InvalidSyntax { span, .. } => span.file_id,
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
        TypeError::UnknownEnumVariant { span, .. } => span.file_id,
        TypeError::WatchOnNonVariable { span, .. } => span.file_id,
        TypeError::WatchOnUnwatchedVariable { span, .. } => span.file_id,
    }
}

/// Convert an HirDiagnostic to an LSP Diagnostic with related information
fn hir_diagnostic_to_lsp_diagnostic(
    error: &HirDiagnostic,
    file_info: &HashMap<FileId, (PathBuf, String, LineIndex)>,
    session: &Session,
) -> Option<lsp_types::Diagnostic> {
    // Extract message, main span, first span (for related info), and code
    let (message, span, first_span, code, related_msg) = match error {
        HirDiagnostic::DuplicateField {
            class_name,
            field_name,
            first_span,
            second_span,
        } => (
            format!("Duplicate field '{}' in class '{}'", field_name, class_name),
            second_span,
            Some(first_span),
            "E0012",
            format!("'{}' first defined here", field_name),
        ),
        HirDiagnostic::DuplicateVariant {
            enum_name,
            variant_name,
            first_span,
            second_span,
        } => (
            format!(
                "Duplicate variant '{}' in enum '{}'",
                variant_name, enum_name
            ),
            second_span,
            Some(first_span),
            "E0013",
            format!("'{}' first defined here", variant_name),
        ),
        HirDiagnostic::DuplicateBlockAttribute {
            item_kind,
            item_name,
            attr_name,
            first_span,
            second_span,
        } => (
            format!(
                "Attribute '{}' can only be defined once on {} '{}'",
                attr_name, item_kind, item_name
            ),
            second_span,
            Some(first_span),
            "E0014",
            format!("'{}' first defined here", attr_name),
        ),
        HirDiagnostic::DuplicateFieldAttribute {
            container_kind,
            container_name,
            field_name,
            attr_name,
            first_span,
            second_span,
        } => (
            format!(
                "Attribute '{}' can only be defined once on field '{}' in {} '{}'",
                attr_name, field_name, container_kind, container_name
            ),
            second_span,
            Some(first_span),
            "E0014",
            format!("'{}' first defined here", attr_name),
        ),
        HirDiagnostic::UnknownAttribute {
            attr_name,
            span,
            valid_attributes,
        } => {
            let suggestions = if valid_attributes.is_empty() {
                String::new()
            } else {
                format!(". Valid attributes: {}", valid_attributes.join(", "))
            };
            (
                format!("Unknown attribute '{}'{}", attr_name, suggestions),
                span,
                None,
                "E0015",
                String::new(),
            )
        }
        HirDiagnostic::InvalidAttributeContext {
            attr_name,
            context,
            allowed_contexts,
            span,
        } => (
            format!(
                "Attribute '{}' is not valid on {}. Allowed on: {}",
                attr_name, context, allowed_contexts
            ),
            span,
            None,
            "E0016",
            String::new(),
        ),
        HirDiagnostic::UnknownGeneratorProperty {
            generator_name,
            property_name,
            span,
            valid_properties,
        } => (
            format!(
                "Unknown property '{}' in generator '{}'. Valid properties: {}",
                property_name,
                generator_name,
                valid_properties.join(", ")
            ),
            span,
            None,
            "E0017",
            String::new(),
        ),
        HirDiagnostic::MissingGeneratorProperty {
            generator_name,
            property_name,
            span,
        } => (
            format!(
                "Generator '{}' is missing required property '{}'",
                generator_name, property_name
            ),
            span,
            None,
            "E0018",
            String::new(),
        ),
        HirDiagnostic::InvalidGeneratorPropertyValue {
            generator_name,
            property_name,
            value,
            span,
            valid_values,
            help,
        } => {
            let mut msg = format!(
                "Invalid value '{}' for property '{}' in generator '{}'",
                value, property_name, generator_name
            );
            if let Some(valid) = valid_values {
                msg.push_str(&format!(". Valid values: {}", valid.join(", ")));
            }
            if let Some(h) = help {
                msg.push_str(&format!(". {}", h));
            }
            (msg, span, None, "E0019", String::new())
        }
        HirDiagnostic::ReservedFieldName {
            item_kind,
            item_name,
            field_name,
            span,
            target_languages,
        } => (
            format!(
                "Field '{}' in {} '{}' is a reserved keyword in {}",
                field_name,
                item_kind,
                item_name,
                target_languages.join(", ")
            ),
            span,
            None,
            "E0020",
            String::new(),
        ),
        HirDiagnostic::FieldNameMatchesTypeName {
            class_name,
            field_name,
            type_name,
            span,
        } => (
            format!(
                "Field '{}' in class '{}' has the same name as its type '{}', which is not supported in generated Python code.",
                field_name, class_name, type_name
            ),
            span,
            None,
            "E0021",
            String::new(),
        ),
        HirDiagnostic::InvalidClientResponseType {
            client_name: _,
            value,
            span,
            valid_values,
        } => (
            format!(
                "client_response_type must be one of {}. Got: {}",
                valid_values.join(", "),
                value
            ),
            span,
            None,
            "E0022",
            String::new(),
        ),
        HirDiagnostic::HttpConfigNotBlock {
            client_name: _,
            span,
        } => (
            "http must be a configuration block with timeout settings".to_string(),
            span,
            None,
            "E0023",
            String::new(),
        ),
        HirDiagnostic::UnknownHttpConfigField {
            client_name: _,
            field_name,
            span,
            suggestion,
            is_composite,
        } => {
            let valid_fields = if *is_composite {
                "total_timeout_ms"
            } else {
                "connect_timeout_ms, request_timeout_ms, time_to_first_token_timeout_ms, idle_timeout_ms"
            };

            let mut msg = format!(
                "Unrecognized field '{}' in http configuration block.",
                field_name
            );

            if let Some(suggested) = suggestion {
                msg.push_str(&format!(" Did you mean '{}'?", suggested));
            }

            if *is_composite {
                msg.push_str(&format!(
                    " Composite clients (fallback/round-robin) only support: {}",
                    valid_fields
                ));
            } else if field_name == "total_timeout_ms" {
                msg.push_str(&format!(
                    " 'total_timeout_ms' is only available for composite clients (fallback/round-robin). For regular clients, use: {}",
                    valid_fields
                ));
            }

            (msg, span, None, "E0024", String::new())
        }
        HirDiagnostic::NegativeTimeout {
            client_name: _,
            field_name,
            value,
            span,
        } => (
            format!("{} must be non-negative, got: {}ms", field_name, value),
            span,
            None,
            "E0025",
            String::new(),
        ),
        HirDiagnostic::MissingProvider {
            client_name: _,
            span,
        } => (
            "Missing `provider` field in client. e.g. `provider openai`".to_string(),
            span,
            None,
            "E0026",
            String::new(),
        ),
        HirDiagnostic::UnknownClientProperty {
            client_name: _,
            field_name,
            span,
        } => (
            format!(
                "Unknown field `{}` in client. Only `provider` and `options` are supported.",
                field_name
            ),
            span,
            None,
            "E0027",
            String::new(),
        ),
    };

    let (path, source_text, line_index) = file_info.get(&span.file_id)?;
    let range =
        convert_text_range(span.range).to_range(source_text, line_index, session.position_encoding);

    // Build related information if we have a first span (for duplicates)
    let related_information = first_span.and_then(|first| {
        let (first_path, first_source_text, first_line_index) = file_info.get(&first.file_id)?;
        let first_range = convert_text_range(first.range).to_range(
            first_source_text,
            first_line_index,
            session.position_encoding,
        );
        let uri = Url::from_file_path(first_path).ok()?;
        Some(vec![lsp_types::DiagnosticRelatedInformation {
            location: lsp_types::Location {
                uri,
                range: first_range,
            },
            message: related_msg.clone(),
        }])
    });

    Some(lsp_types::Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(lsp_types::NumberOrString::String(code.to_string())),
        code_description: None,
        source: Some("baml".to_string()),
        message,
        related_information,
        tags: None,
        data: None,
    })
}

/// Get the FileId from an HirDiagnostic
fn get_hir_diagnostic_file_id(error: &HirDiagnostic) -> FileId {
    match error {
        HirDiagnostic::DuplicateField { second_span, .. } => second_span.file_id,
        HirDiagnostic::DuplicateVariant { second_span, .. } => second_span.file_id,
        HirDiagnostic::DuplicateBlockAttribute { second_span, .. } => second_span.file_id,
        HirDiagnostic::DuplicateFieldAttribute { second_span, .. } => second_span.file_id,
        HirDiagnostic::UnknownAttribute { span, .. } => span.file_id,
        HirDiagnostic::InvalidAttributeContext { span, .. } => span.file_id,
        HirDiagnostic::UnknownGeneratorProperty { span, .. } => span.file_id,
        HirDiagnostic::MissingGeneratorProperty { span, .. } => span.file_id,
        HirDiagnostic::InvalidGeneratorPropertyValue { span, .. } => span.file_id,
        HirDiagnostic::ReservedFieldName { span, .. } => span.file_id,
        HirDiagnostic::FieldNameMatchesTypeName { span, .. } => span.file_id,
        HirDiagnostic::InvalidClientResponseType { span, .. } => span.file_id,
        HirDiagnostic::HttpConfigNotBlock { span, .. } => span.file_id,
        HirDiagnostic::UnknownHttpConfigField { span, .. } => span.file_id,
        HirDiagnostic::NegativeTimeout { span, .. } => span.file_id,
        HirDiagnostic::MissingProvider { span, .. } => span.file_id,
        HirDiagnostic::UnknownClientProperty { span, .. } => span.file_id,
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
