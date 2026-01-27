//! Centralized diagnostic collection for BAML projects.
//!
//! This module provides the `check()` and `check_file()` methods that collect
//! all diagnostics from a BAML project using the unified `Diagnostic` type.
//!
//! ## Example
//!
//! ```ignore
//! let result = db.check();
//! for diag in &result.diagnostics {
//!     println!("{}", diag.message);
//! }
//! ```

use std::{collections::HashMap, path::PathBuf};

use baml_compiler_diagnostics::{Diagnostic, ToDiagnostic};
use baml_compiler_hir::{
    self, Definition, ErrorLocation, FunctionBody, ItemId, file_items, file_lowering,
    fqn::FullyQualifiedName, function_body, function_signature, function_signature_source_map,
    get_item_name_span, symbol_table,
};
use baml_compiler_tir::{self, class_field_types, enum_variants, type_aliases, typing_context};
use baml_db::{FileId, SourceFile, Span, baml_compiler_parser};
use baml_workspace::Project;
use text_size::TextRange;

use crate::ProjectDatabase;

/// Result of checking a project, containing diagnostics and metadata for rendering.
#[derive(Debug)]
pub struct CheckResult {
    /// The collected diagnostics.
    pub diagnostics: Vec<Diagnostic>,
    /// Maps `FileId` to source text (for Ariadne rendering).
    pub sources: HashMap<FileId, String>,
    /// Maps `FileId` to file path (for URL generation).
    pub file_paths: HashMap<FileId, PathBuf>,
}

/// Resolve an `ErrorLocation` to a Span for type-level items.
///
/// For `ErrorLocation::TypeItem`, this looks up the type alias or class using the cached
/// `SymbolTable` and returns its name span. For other `ErrorLocation` variants, this should
/// not be called.
fn resolve_type_item_location(db: &ProjectDatabase, project: Project, loc: &ErrorLocation) -> Span {
    match loc {
        ErrorLocation::TypeItem(name) => {
            // Use the cached symbol table for efficient lookup
            let symbols = symbol_table(db, project);
            let fqn = FullyQualifiedName::local(name.clone());

            if let Some(def) = symbols.lookup_type(db, &fqn) {
                // Match on the definition type to get the appropriate location
                match def {
                    Definition::TypeAlias(alias_loc) => {
                        let file = alias_loc.file(db);
                        let local_id = alias_loc.id(db);
                        get_item_name_span(db, file, "type alias", name.as_str(), local_id.index())
                            .unwrap_or_else(|| {
                                Span::new(file.file_id(db), TextRange::empty(0.into()))
                            })
                    }
                    Definition::Class(class_loc) => {
                        let file = class_loc.file(db);
                        let local_id = class_loc.id(db);
                        get_item_name_span(db, file, "class", name.as_str(), local_id.index())
                            .unwrap_or_else(|| {
                                Span::new(file.file_id(db), TextRange::empty(0.into()))
                            })
                    }
                    _ => {
                        // Should not happen - cycle errors are only for type aliases and classes
                        Span::new(FileId::default(), TextRange::empty(0.into()))
                    }
                }
            } else {
                // Fallback if not found in symbol table
                Span::new(FileId::default(), TextRange::empty(0.into()))
            }
        }
        _ => panic!("resolve_type_item_location should only be called with TypeItem locations"),
    }
}

/// Collect all diagnostics from a project.
///
/// This is the single source of truth for diagnostic collection, used by all
/// consumers (LSP, onionskin TUI, tests).
///
/// ## Example
///
/// ```ignore
/// let diagnostics = collect_diagnostics(&db, project, &source_files);
/// for diag in &diagnostics {
///     println!("[{}] {}", diag.phase.name(), diag.message);
/// }
/// ```
pub fn collect_diagnostics(
    db: &ProjectDatabase,
    project: Project,
    source_files: &[SourceFile],
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // 1. Collect parse errors
    for source_file in source_files {
        let parse_errors = baml_compiler_parser::parse_errors(db, *source_file);
        for error in &parse_errors {
            diagnostics.push(error.to_diagnostic());
        }
    }

    // 2. Collect HIR lowering diagnostics (per-file validation)
    for source_file in source_files {
        let lowering_result = file_lowering(db, *source_file);
        for diag in lowering_result.diagnostics(db) {
            diagnostics.push(diag.to_diagnostic());
        }
    }

    // 3. Collect validation errors (duplicates across files, reserved names)
    let validation_result = baml_compiler_hir::validate_hir(db, project);
    for diag in &validation_result.hir_diagnostics {
        diagnostics.push(diag.to_diagnostic());
    }
    for error in &validation_result.name_errors {
        diagnostics.push(error.to_diagnostic());
    }

    // 3.5. Collect TIR validation errors (cycle detection)
    // This requires resolved types, so it happens after HIR validation but uses TIR data
    let class_fields = class_field_types(db, project).classes(db).clone();
    let type_aliases_map = type_aliases(db, project).aliases(db).clone();

    let alias_cycle_errors = baml_compiler_tir::validate_type_alias_cycles(&type_aliases_map);
    for error in &alias_cycle_errors {
        diagnostics.push(
            error.to_diagnostic(std::string::ToString::to_string, |loc| {
                resolve_type_item_location(db, project, loc)
            }),
        );
    }

    let class_cycle_errors =
        baml_compiler_tir::validate_class_cycles(&class_fields, &type_aliases_map);
    for error in &class_cycle_errors {
        diagnostics.push(
            error.to_diagnostic(std::string::ToString::to_string, |loc| {
                resolve_type_item_location(db, project, loc)
            }),
        );
    }

    // 4. Collect type errors from function inference
    let globals = typing_context(db, project).functions(db).clone();
    let enum_variants_struct = enum_variants(db, project);
    let enum_variants_map = enum_variants_struct.enums(db).clone();

    for source_file in source_files {
        let items_struct = file_items(db, *source_file);
        let items = items_struct.items(db);

        for item in items {
            if let ItemId::Function(func_loc) = item {
                let signature = function_signature(db, *func_loc);
                let sig_source_map = function_signature_source_map(db, *func_loc);
                let body = function_body(db, *func_loc);

                // Only infer types for expression functions (not LLM functions)
                if let FunctionBody::Expr(expr_body, hir_source_map) = &*body {
                    // Collect body lowering diagnostics (e.g., missing semicolons)
                    for diag in &expr_body.diagnostics {
                        diagnostics.push(diag.to_diagnostic());
                    }

                    let inference_result = baml_compiler_tir::infer_function(
                        db,
                        &signature,
                        Some(&sig_source_map),
                        &body,
                        Some(globals.clone()),
                        Some(class_fields.clone()),
                        Some(type_aliases_map.clone()),
                        Some(enum_variants_map.clone()),
                        *func_loc,
                    );

                    // Convert TIR type errors (with ErrorLocation) to span-based diagnostics
                    for type_error in &inference_result.errors {
                        diagnostics.push(
                            type_error.to_diagnostic(ToString::to_string, |loc| {
                                loc.to_span(hir_source_map)
                            }),
                        );
                    }
                }
            }
        }
    }

    diagnostics
}

impl ProjectDatabase {
    /// Check the entire project and return all diagnostics.
    ///
    /// This is the centralized entry point for diagnostic collection, replacing
    /// the duplicated logic in the LSP server and test infrastructure.
    ///
    /// Returns a `CheckResult` containing diagnostics and metadata for rendering.
    pub fn check(&self) -> CheckResult {
        let Some(project) = self.get_project() else {
            return CheckResult {
                diagnostics: Vec::new(),
                sources: HashMap::new(),
                file_paths: HashMap::new(),
            };
        };

        let source_files: Vec<SourceFile> = self.files().collect();
        let mut sources: HashMap<FileId, String> = HashMap::new();
        let mut file_paths: HashMap<FileId, PathBuf> = HashMap::new();

        // Build all maps
        for source_file in &source_files {
            let file_id = source_file.file_id(self);
            let text = source_file.text(self).clone();
            let path = source_file.path(self);

            sources.insert(file_id, text);
            file_paths.insert(file_id, path);
        }

        // Use the shared collect_diagnostics function
        let diagnostics = collect_diagnostics(self, project, &source_files);

        CheckResult {
            diagnostics,
            sources,
            file_paths,
        }
    }

    /// Legacy check method for backwards compatibility.
    /// Returns (diagnostics, sources) tuple.
    pub fn check_legacy(&self) -> (Vec<Diagnostic>, HashMap<FileId, String>) {
        let result = self.check();
        (result.diagnostics, result.sources)
    }

    /// Check a single file and return diagnostics for that file only.
    ///
    /// Note: This still requires the full project context for type checking.
    pub fn check_file(&self, file: SourceFile) -> Vec<Diagnostic> {
        let Some(project) = self.get_project() else {
            return Vec::new();
        };

        let source_files = vec![file];
        collect_diagnostics(self, project, &source_files)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn test_check_empty_project() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("/tmp"));

        let result = db.check();
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn test_check_valid_file() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("/tmp"));
        db.add_or_update_file(Path::new("/tmp/test.baml"), "class Foo {\n  name string\n}");

        let result = db.check();
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn test_check_parse_error() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("/tmp"));
        db.add_or_update_file(Path::new("/tmp/test.baml"), "class Foo {");

        let result = db.check();
        assert!(!result.diagnostics.is_empty());

        // Should be a parse error
        let first = &result.diagnostics[0];
        assert!(first.code().starts_with("E00"));
    }
}
