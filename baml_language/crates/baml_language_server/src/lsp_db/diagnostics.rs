//! Diagnostics collection and conversion for LSP.
//!
//! This module collects errors from all compiler phases and converts them
//! to LSP diagnostics format.

use std::{collections::HashMap, path::PathBuf};

use baml_db::{
    FileId, SourceFile, Span,
    baml_diagnostics::{NameError, ParseError, TypeError},
    baml_hir::{ItemId, file_items, project_items, validate_duplicate_names},
    baml_parser::parse_errors,
};
use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Url};
use text_size::TextRange;

use super::{
    LspDatabase,
    position::{LineIndex, span_to_lsp_range},
};

/// An LSP diagnostic with file association.
#[derive(Debug, Clone)]
pub struct LspDiagnostic {
    /// The file path this diagnostic belongs to.
    pub file_path: PathBuf,
    /// The diagnostic itself.
    pub diagnostic: Diagnostic,
}

impl LspDatabase {
    /// Collect all diagnostics from the project.
    ///
    /// This aggregates errors from:
    /// - Parse errors (per file)
    /// - Name resolution errors (project-wide)
    /// - Type errors (per function) - TODO: implement when THIR is integrated
    pub fn collect_diagnostics(&self) -> Vec<LspDiagnostic> {
        let mut diagnostics = Vec::new();

        // Collect parse errors for all files
        for file in self.files() {
            let errors = parse_errors(&self.db, file);
            let text = file.text(&self.db);
            let file_path = file.path(&self.db);
            let line_index = LineIndex::new(text);

            for error in errors {
                let diag = convert_parse_error(&error, &line_index);
                diagnostics.push(LspDiagnostic {
                    file_path: file_path.clone(),
                    diagnostic: diag,
                });
            }
        }

        // Collect name resolution errors if we have a project
        if let Some(project) = self.project() {
            let name_errors = validate_duplicate_names(&self.db, project);

            for error in name_errors {
                // Name errors may reference multiple files
                let diags = convert_name_error(&error, self);
                diagnostics.extend(diags);
            }
        }

        // TODO: Collect type errors when THIR integration is complete
        // This would iterate over all functions and collect type inference errors

        diagnostics
    }

    /// Collect diagnostics grouped by file URL.
    ///
    /// This is the format expected by LSP for publishing diagnostics.
    pub fn diagnostics_by_file(&self) -> HashMap<Url, Vec<Diagnostic>> {
        let all_diagnostics = self.collect_diagnostics();

        let mut by_file: HashMap<Url, Vec<Diagnostic>> = HashMap::new();

        // Initialize empty diagnostic lists for all tracked files
        // This ensures files with no errors get their diagnostics cleared
        for file in self.files() {
            let path = file.path(&self.db);
            if let Ok(url) = Url::from_file_path(&path) {
                by_file.entry(url).or_default();
            }
        }

        // Add diagnostics to their respective files
        for diag in all_diagnostics {
            if let Ok(url) = Url::from_file_path(&diag.file_path) {
                by_file.entry(url).or_default().push(diag.diagnostic);
            }
        }

        by_file
    }

    /// Get diagnostics for a single file.
    pub fn file_diagnostics(&self, file: SourceFile) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Parse errors
        let errors = parse_errors(&self.db, file);
        let text = file.text(&self.db);
        let line_index = LineIndex::new(text);

        for error in errors {
            diagnostics.push(convert_parse_error(&error, &line_index));
        }

        diagnostics
    }
}

/// Convert a parse error to an LSP diagnostic.
fn convert_parse_error(error: &ParseError, line_index: &LineIndex) -> Diagnostic {
    match error {
        ParseError::UnexpectedToken {
            expected,
            found,
            span,
        } => {
            let range = span_to_range_with_index(line_index, span);
            Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("E0010".to_string())),
                code_description: None,
                source: Some("baml".to_string()),
                message: format!("Expected {expected}, found {found}"),
                related_information: None,
                tags: None,
                data: None,
            }
        }
        ParseError::UnexpectedEof { expected, span } => {
            let range = span_to_range_with_index(line_index, span);
            Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("E0009".to_string())),
                code_description: None,
                source: Some("baml".to_string()),
                message: format!("Unexpected end of file, expected {expected}"),
                related_information: None,
                tags: None,
                data: None,
            }
        }
    }
}

/// Convert a name error to LSP diagnostics.
///
/// Name errors may produce multiple diagnostics (one for each location).
fn convert_name_error(error: &NameError, db: &LspDatabase) -> Vec<LspDiagnostic> {
    match error {
        NameError::DuplicateName {
            name,
            kind,
            first,
            first_path,
            second,
            second_path,
        } => {
            let mut diagnostics = Vec::new();

            // Get line indices for both files
            let first_line_index = db
                .get_file(first_path.as_ref())
                .map(|f| LineIndex::new(f.text(db.db())));
            let second_line_index = db
                .get_file(second_path.as_ref())
                .map(|f| LineIndex::new(f.text(db.db())));

            // Diagnostic at second location (the duplicate)
            if let Some(line_index) = second_line_index {
                let range = span_to_range_with_index(&line_index, second);
                diagnostics.push(LspDiagnostic {
                    file_path: PathBuf::from(second_path),
                    diagnostic: Diagnostic {
                        range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: Some(NumberOrString::String("E0011".to_string())),
                        code_description: None,
                        source: Some("baml".to_string()),
                        message: format!(
                            "Duplicate {kind} `{name}` (first defined in {first_path})"
                        ),
                        related_information: Some(vec![lsp_types::DiagnosticRelatedInformation {
                            location: lsp_types::Location {
                                uri: Url::from_file_path(first_path)
                                    .unwrap_or_else(|_| Url::parse("file:///unknown").unwrap()),
                                range: first_line_index
                                    .as_ref()
                                    .map(|li| span_to_range_with_index(li, first))
                                    .unwrap_or_default(),
                            },
                            message: format!("First definition of `{name}` here"),
                        }]),
                        tags: None,
                        data: None,
                    },
                });
            }

            // Also add a hint at the first location
            if let Some(line_index) = first_line_index {
                let range = span_to_range_with_index(&line_index, first);
                diagnostics.push(LspDiagnostic {
                    file_path: PathBuf::from(first_path),
                    diagnostic: Diagnostic {
                        range,
                        severity: Some(DiagnosticSeverity::HINT),
                        code: Some(NumberOrString::String("E0011".to_string())),
                        code_description: None,
                        source: Some("baml".to_string()),
                        message: format!("`{name}` first defined here"),
                        related_information: None,
                        tags: None,
                        data: None,
                    },
                });
            }

            diagnostics
        }
    }
}

/// Convert a type error to an LSP diagnostic.
///
/// The type parameter is converted to string for display.
fn convert_type_error<T: std::fmt::Display>(
    error: &TypeError<T>,
    line_index: &LineIndex,
) -> Diagnostic {
    let (message, span, code) = match error {
        TypeError::TypeMismatch {
            expected,
            found,
            span,
        } => (
            format!("Expected `{expected}`, found `{found}`"),
            span,
            "E0001",
        ),
        TypeError::UnknownType { name, span } => (format!("Unknown type `{name}`"), span, "E0002"),
        TypeError::UnknownVariable { name, span } => {
            (format!("Unknown variable `{name}`"), span, "E0003")
        }
        TypeError::InvalidBinaryOp { op, lhs, rhs, span } => (
            format!("Invalid operation `{lhs}` {op} `{rhs}`"),
            span,
            "E0004",
        ),
        TypeError::InvalidUnaryOp { op, operand, span } => {
            (format!("Invalid operation {op}`{operand}`"), span, "E0004")
        }
        TypeError::ArgumentCountMismatch {
            expected,
            found,
            span,
        } => (
            format!("Expected {expected} arguments, found {found}"),
            span,
            "E0005",
        ),
        TypeError::NotCallable { ty, span } => {
            (format!("Type `{ty}` is not callable"), span, "E0006")
        }
        TypeError::NoSuchField { ty, field, span } => {
            (format!("Type `{ty}` has no field `{field}`"), span, "E0007")
        }
        TypeError::NotIndexable { ty, span } => {
            (format!("Type `{ty}` is not indexable"), span, "E0008")
        }
        TypeError::NonExhaustiveMatch {
            scrutinee_type,
            missing_cases,
            span,
        } => (
            format!(
                "Non-exhaustive match on `{scrutinee_type}`: missing cases {}",
                missing_cases.join(", ")
            ),
            span,
            "E0012",
        ),
        TypeError::UnreachableArm { span } => ("Unreachable match arm".to_string(), span, "E0013"),
    };

    let range = span_to_range_with_index(line_index, span);

    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String(code.to_string())),
        code_description: None,
        source: Some("baml".to_string()),
        message,
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Helper to convert span to range using a pre-built line index.
fn span_to_range_with_index(line_index: &LineIndex, span: &Span) -> lsp_types::Range {
    use super::position::span_to_lsp_range_with_index;
    span_to_lsp_range_with_index(line_index, span)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn test_parse_error_conversion() {
        let text = "class Foo {\n  broken\n}";
        let line_index = LineIndex::new(text);

        let error = ParseError::UnexpectedToken {
            expected: "type annotation".to_string(),
            found: "}".to_string(),
            span: Span::new(FileId::new(0), TextRange::new(22.into(), 23.into())),
        };

        let diag = convert_parse_error(&error, &line_index);

        assert_eq!(diag.severity, Some(DiagnosticSeverity::ERROR));
        assert!(diag.message.contains("Expected"));
        assert!(diag.message.contains("type annotation"));
    }
}
