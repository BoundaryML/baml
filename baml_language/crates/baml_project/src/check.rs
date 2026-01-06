//! Centralized diagnostic collection for BAML projects.
//!
//! This module provides the `check()` and `check_file()` methods that collect
//! all diagnostics from a BAML project using the unified `Diagnostic` type.
//!
//! ## Example
//!
//! ```ignore
//! let diagnostics = lsp_db.check();
//! for diag in diagnostics {
//!     println!("{}", diag.display_ariadne(&sources, false));
//! }
//! ```

use std::{collections::HashMap, path::PathBuf};

use baml_db::{
    FileId, RootDatabase, SourceFile,
    baml_hir::{
        self, FunctionBody, ItemId, file_items, file_lowering, function_body, function_signature,
    },
    baml_parser,
    baml_tir::{self, class_field_types, enum_variants, type_aliases, typing_context},
    baml_workspace::Project,
};
use baml_diagnostics::{Diagnostic, LspConversionConfig, ToDiagnostic, compute_line_starts};

use crate::LspDatabase;

/// Result of checking a project, containing diagnostics and metadata for rendering.
#[derive(Debug)]
pub struct CheckResult {
    /// The collected diagnostics.
    pub diagnostics: Vec<Diagnostic>,
    /// Maps FileId to source text (for Ariadne rendering).
    pub sources: HashMap<FileId, String>,
    /// Maps FileId to file path (for LSP URL generation).
    pub file_paths: HashMap<FileId, PathBuf>,
    /// Maps FileId to (source_text, line_starts) for LSP range conversion.
    pub file_sources: HashMap<FileId, (String, Vec<u32>)>,
}

impl CheckResult {
    /// Get an LSP conversion configuration from this result.
    pub fn lsp_config(&self) -> LspConversionConfig<'_> {
        LspConversionConfig {
            file_paths: &self.file_paths,
            file_sources: &self.file_sources,
        }
    }

    /// Convert all diagnostics to LSP diagnostics, grouped by file URL.
    pub fn to_lsp_diagnostics(&self) -> HashMap<lsp_types::Url, Vec<lsp_types::Diagnostic>> {
        let config = self.lsp_config();
        let mut result: HashMap<lsp_types::Url, Vec<lsp_types::Diagnostic>> = HashMap::new();

        // Initialize empty diagnostics for all files (so files with no errors get cleared)
        for path in self.file_paths.values() {
            if let Ok(url) = lsp_types::Url::from_file_path(path) {
                result.entry(url).or_default();
            }
        }

        // Add diagnostics
        for diag in &self.diagnostics {
            if let Some((url, lsp_diag)) = diag.to_lsp(&config) {
                result.entry(url).or_default().push(lsp_diag);
            }
        }

        result
    }
}

impl LspDatabase {
    /// Check the entire project and return all diagnostics.
    ///
    /// This is the centralized entry point for diagnostic collection, replacing
    /// the duplicated logic in the LSP server and test infrastructure.
    ///
    /// Returns a `CheckResult` containing diagnostics and metadata for rendering.
    pub fn check(&self) -> CheckResult {
        let db = self.db();
        let Some(project) = self.project() else {
            return CheckResult {
                diagnostics: Vec::new(),
                sources: HashMap::new(),
                file_paths: HashMap::new(),
                file_sources: HashMap::new(),
            };
        };

        let source_files: Vec<SourceFile> = self.files().collect();
        let mut sources: HashMap<FileId, String> = HashMap::new();
        let mut file_paths: HashMap<FileId, PathBuf> = HashMap::new();
        let mut file_sources: HashMap<FileId, (String, Vec<u32>)> = HashMap::new();

        // Build all maps
        for source_file in &source_files {
            let file_id = source_file.file_id(db);
            let text = source_file.text(db).to_string();
            let path = source_file.path(db);
            let line_starts = compute_line_starts(&text);

            sources.insert(file_id, text.clone());
            file_paths.insert(file_id, path);
            file_sources.insert(file_id, (text, line_starts));
        }

        let diagnostics = self.check_project(db, project, &source_files);

        CheckResult {
            diagnostics,
            sources,
            file_paths,
            file_sources,
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
        let db = self.db();
        let Some(project) = self.project() else {
            return Vec::new();
        };

        let source_files = vec![file];
        self.check_project(db, project, &source_files)
    }

    /// Internal method to collect diagnostics from the given source files.
    fn check_project(
        &self,
        db: &RootDatabase,
        project: Project,
        source_files: &[SourceFile],
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // 1. Collect parse errors
        for source_file in source_files {
            let parse_errors = baml_parser::parse_errors(db, *source_file);
            for error in parse_errors.iter() {
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
        let validation_result = baml_hir::validate_hir(db, project);
        for diag in &validation_result.hir_diagnostics {
            diagnostics.push(diag.to_diagnostic());
        }
        for error in &validation_result.name_errors {
            diagnostics.push(error.to_diagnostic());
        }

        // 4. Collect type errors from function inference
        let globals = typing_context(db, project).functions(db).clone();
        let class_fields = class_field_types(db, project).classes(db).clone();
        let type_aliases_map = type_aliases(db, project).aliases(db).clone();
        let enum_variants_struct = enum_variants(db, project);
        let enum_variants_map = enum_variants_struct.enums(db).clone();

        for source_file in source_files {
            let items_struct = file_items(db, *source_file);
            let items = items_struct.items(db);

            for item in items {
                if let ItemId::Function(func_loc) = item {
                    let signature = function_signature(db, *func_loc);
                    let body = function_body(db, *func_loc);

                    // Only infer types for expression functions (not LLM functions)
                    if matches!(*body, FunctionBody::Expr(_)) {
                        let inference_result = baml_tir::infer_function(
                            db,
                            &signature,
                            &body,
                            Some(globals.clone()),
                            Some(class_fields.clone()),
                            Some(type_aliases_map.clone()),
                            Some(enum_variants_map.clone()),
                            *func_loc,
                        );

                        for type_error in &inference_result.errors {
                            diagnostics.push(type_error.to_diagnostic());
                        }
                    }
                }
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn test_check_empty_project() {
        let mut db = LspDatabase::new();
        db.set_project_root(Path::new("/tmp"));

        let result = db.check();
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn test_check_valid_file() {
        let mut db = LspDatabase::new();
        db.set_project_root(Path::new("/tmp"));
        db.add_or_update_file(Path::new("/tmp/test.baml"), "class Foo {\n  name string\n}");

        let result = db.check();
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn test_check_parse_error() {
        let mut db = LspDatabase::new();
        db.set_project_root(Path::new("/tmp"));
        db.add_or_update_file(Path::new("/tmp/test.baml"), "class Foo {");

        let result = db.check();
        assert!(!result.diagnostics.is_empty());

        // Should be a parse error
        let first = &result.diagnostics[0];
        assert!(first.code().starts_with("E00"));
    }

    #[test]
    fn test_to_lsp_diagnostics() {
        let mut db = LspDatabase::new();
        db.set_project_root(Path::new("/tmp"));
        db.add_or_update_file(Path::new("/tmp/test.baml"), "class Foo {");

        let result = db.check();
        let lsp_diags = result.to_lsp_diagnostics();

        // Should have diagnostics for test.baml
        assert!(!lsp_diags.is_empty());
    }
}
