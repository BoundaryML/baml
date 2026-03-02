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

use std::{collections::HashMap, sync::Arc};

use baml_compiler_diagnostics::{Diagnostic, ToDiagnostic};
use baml_compiler_hir::{
    self, FunctionBody, HirSourceMap, ItemId, SpanResolutionContext, file_items, file_lowering,
    function_body, function_signature, function_signature_source_map, is_llm_function,
    llm_function_file_offset, llm_function_meta, project_class_field_type_spans,
    project_type_alias_type_spans, project_type_item_spans, template_string_file_offset,
};
use baml_compiler_tir::{self, class_field_types, enum_variants, type_aliases, typing_context};
use baml_db::{FileId, SourceFile, baml_compiler_parser};
use baml_workspace::Project;

use crate::ProjectDatabase;

/// Result of checking a project, containing diagnostics and metadata for rendering.
#[derive(Debug)]
pub struct CheckResult {
    /// The collected diagnostics.
    pub diagnostics: Vec<Diagnostic>,
    /// Maps `FileId` to source text (for Ariadne rendering).
    pub sources: HashMap<FileId, String>,
    /// Maps `FileId` to file path (for URL generation).
    pub file_paths: HashMap<FileId, std::path::PathBuf>,
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

    // Get cached type item spans for error location resolution
    let type_spans = project_type_item_spans(db, project);
    let field_type_spans = project_class_field_type_spans(db, project);
    let type_alias_type_spans = project_type_alias_type_spans(db, project);

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

    // 3.5. Collect TIR validation errors (cycle detection + unknown types)
    // This requires resolved types, so it happens after HIR validation but uses TIR data
    let class_fields_result = class_field_types(db, project);
    let class_fields = class_fields_result.classes(db).clone();
    let type_aliases_result = type_aliases(db, project);
    let type_aliases_map = type_aliases_result.aliases(db).clone();

    // Create a context for type-level errors (no expression source map, no template offset)
    let type_level_ctx = SpanResolutionContext {
        expr_fn_source_map: &HirSourceMap::default(),
        type_spans: &type_spans,
        field_type_spans: &field_type_spans,
        type_alias_type_spans: &type_alias_type_spans,
        jinja_file_id: FileId::default(),
        template_file_offset: None,
    };

    // Collect unknown type errors from class field types
    for error in class_fields_result.errors(db) {
        diagnostics.push(
            error.to_diagnostic(std::string::ToString::to_string, |loc| {
                loc.to_span(&type_level_ctx)
            }),
        );
    }

    // Collect unknown type errors from type aliases
    for error in type_aliases_result.errors(db) {
        diagnostics.push(
            error.to_diagnostic(std::string::ToString::to_string, |loc| {
                loc.to_span(&type_level_ctx)
            }),
        );
    }

    // Collect cycle detection errors
    let alias_cycle_errors = baml_compiler_tir::validate_type_alias_cycles(&type_aliases_map);
    for error in &alias_cycle_errors {
        diagnostics.push(
            error.to_diagnostic(std::string::ToString::to_string, |loc| {
                loc.to_span(&type_level_ctx)
            }),
        );
    }

    let class_cycle_errors =
        baml_compiler_tir::validate_class_cycles(&class_fields, &type_aliases_map);
    for error in &class_cycle_errors {
        diagnostics.push(
            error.to_diagnostic(std::string::ToString::to_string, |loc| {
                loc.to_span(&type_level_ctx)
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
            // Validate template string bodies
            if let ItemId::TemplateString(ts_loc) = item {
                let ts_errors = baml_compiler_tir::validate_template_string_body(db, *ts_loc);

                // Look up the template file offset from the CST for Jinja error resolution
                let template_file_offset = template_string_file_offset(db, *ts_loc);
                let file_id = ts_loc.file(db).file_id(db);

                // Template strings don't have expression IDs, use empty source map
                let ctx = SpanResolutionContext {
                    expr_fn_source_map: &HirSourceMap::default(),
                    type_spans: &type_spans,
                    field_type_spans: &field_type_spans,
                    type_alias_type_spans: &type_alias_type_spans,
                    jinja_file_id: file_id,
                    template_file_offset,
                };

                for type_error in &ts_errors {
                    diagnostics.push(
                        type_error.to_diagnostic(ToString::to_string, |loc| loc.to_span(&ctx)),
                    );
                }
            }

            if let ItemId::Function(func_loc) = item {
                let signature = function_signature(db, *func_loc);
                let sig_source_map = function_signature_source_map(db, *func_loc);
                // For LLM functions, use the original LlmBody for type inference
                // (Jinja validation + declared return type) instead of the synthetic
                // Expr body which is for compilation only.
                let body = if let Some(llm_meta) = llm_function_meta(db, *func_loc) {
                    Arc::new(FunctionBody::Llm((*llm_meta).clone()))
                } else if is_llm_function(db, *func_loc) {
                    // Malformed LLM function (parse errors prevented metadata extraction).
                    // Use Missing to skip type-checking the synthetic body.
                    Arc::new(FunctionBody::Missing)
                } else {
                    function_body(db, *func_loc)
                };

                // Collect body lowering diagnostics (e.g., missing semicolons)
                if let FunctionBody::Expr(expr_body, _) = &*body {
                    for diag in &expr_body.diagnostics {
                        diagnostics.push(diag.to_diagnostic());
                    }
                }

                // Infer types for both expression and LLM functions
                // LLM functions are validated for Jinja template errors
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
                // Both LLM and Expr bodies have source maps (LLM has an empty one)
                let file_id = func_loc.file(db).file_id(db);

                // For LLM functions, look up the prompt's file offset for Jinja error resolution
                let template_file_offset = match &*body {
                    FunctionBody::Llm(_) => llm_function_file_offset(db, *func_loc),
                    _ => None,
                };

                // Create context based on body type
                let empty_source_map = HirSourceMap::default();
                let expr_fn_source_map = match &*body {
                    FunctionBody::Expr(_, source_map) => source_map,
                    _ => &empty_source_map,
                };

                let ctx = SpanResolutionContext {
                    expr_fn_source_map,
                    type_spans: &type_spans,
                    field_type_spans: &field_type_spans,
                    type_alias_type_spans: &type_alias_type_spans,
                    jinja_file_id: file_id,
                    template_file_offset,
                };

                for type_error in &inference_result.errors {
                    diagnostics.push(
                        type_error.to_diagnostic(ToString::to_string, |loc| loc.to_span(&ctx)),
                    );
                }
            }
        }
    }

    // Filter out diagnostics from synthetic stream expansion files.
    // These are generated files (stream_* types) and their errors duplicate
    // diagnostics already reported on the real source files.
    diagnostics.retain(|d| d.file_id().is_none_or(|fid| !fid.is_stream_expansion()));

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

        let source_files: Vec<SourceFile> = self.get_source_files();
        let mut sources: HashMap<FileId, String> = HashMap::new();
        let mut file_paths: HashMap<FileId, std::path::PathBuf> = HashMap::new();

        // Build all maps
        for source_file in &source_files {
            let file_id = source_file.file_id(self);
            let text = source_file.text(self).clone();
            let path = source_file.path(self);

            sources.insert(file_id, text);
            file_paths.insert(file_id, path);

            // Register virtual files from PPIR stream_* expansions
            let synth = baml_compiler_ppir::ppir_expansion_cst(self, *source_file);
            if let Some(synth_file) = synth.source_file(self) {
                let synth_file_id = synth_file.file_id(self);
                sources.insert(synth_file_id, synth_file.text(self).clone());
                file_paths.insert(synth_file_id, synth_file.path(self));
            }
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
