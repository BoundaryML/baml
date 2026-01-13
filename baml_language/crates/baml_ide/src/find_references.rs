//! Find all references for BAML files.
//!
//! This module provides LSP-agnostic find-references functionality.
//! Given a cursor position, it finds all references to the symbol under the cursor.

use std::path::PathBuf;

use baml_db::{
    Span, FileId, Name,
    baml_compiler_hir::{ExprId, FunctionLoc, FullyQualifiedName, ExprBody},
    baml_compiler_tir::{InferenceResult, ResolvedValue, DefinitionSite},
};
use baml_project::ProjectDatabase;
use text_size::{TextRange, TextSize};

/// A reference location in source code.
#[derive(Debug, Clone)]
pub struct Reference {
    /// The file containing the reference.
    pub file_path: PathBuf,
    /// The span of the reference.
    pub span: Span,
    /// Whether this is the definition (not just a reference).
    pub is_definition: bool,
}

impl Reference {
    /// Create a new reference.
    pub fn new(file_path: PathBuf, span: Span, is_definition: bool) -> Self {
        Self {
            file_path,
            span,
            is_definition,
        }
    }
}

/// Find all references to the symbol at the given position.
///
/// Returns an empty vector if:
/// - No symbol is found at the position
/// - The symbol cannot be resolved
///
/// The returned references include the definition itself (marked with `is_definition: true`).
pub fn find_all_references(
    db: &ProjectDatabase,
    file_id: FileId,
    position: TextSize,
) -> Vec<Reference> {
    // Get the source file
    let source_files = db.get_source_files();
    let Some(source_file) = source_files.iter().find(|f| f.file_id(db) == file_id) else {
        return Vec::new();
    };
    let text = source_file.text(db);

    // Find the word at the cursor position
    let Some(word_range) = crate::goto_definition::find_word_at_offset(&text, position) else {
        return Vec::new();
    };

    // TODO: This is a placeholder implementation.
    // A full implementation would:
    // 1. Find what symbol is at the cursor position (using resolution)
    // 2. Search all files in the project for references to that symbol
    // 3. Use the resolution maps from all functions to find references

    // For now, return empty to allow compilation
    Vec::new()
}

/// Find all references to a specific symbol.
fn find_references_to_symbol(
    db: &ProjectDatabase,
    target: &ResolvedValue,
) -> Vec<Reference> {
    let mut references = Vec::new();

    // Get all files in the project
    let source_files = db.get_source_files();

    for source_file in source_files {
        let file_id = source_file.file_id(db);
        let file_path = match db.file_id_to_path(file_id) {
            Some(path) => path.clone(),
            None => continue,
        };

        // TODO: Get all functions in this file
        // For each function:
        //   1. Get its InferenceResult
        //   2. Search expr_resolutions for matching ResolvedValues
        //   3. Convert matching ExprIds to References

        // This requires:
        // - A way to enumerate functions in a file
        // - A way to get InferenceResult for each function
        // - A way to map ExprIds to text ranges
    }

    references
}

/// Check if two resolved values refer to the same entity.
fn is_same_resolution(a: &ResolvedValue, b: &ResolvedValue) -> bool {
    match (a, b) {
        (
            ResolvedValue::Local { name: n1, definition_site: d1 },
            ResolvedValue::Local { name: n2, definition_site: d2 },
        ) => n1 == n2 && d1 == d2,

        (ResolvedValue::Function(f1), ResolvedValue::Function(f2)) => f1 == f2,
        (ResolvedValue::Class(c1), ResolvedValue::Class(c2)) => c1 == c2,
        (ResolvedValue::Enum(e1), ResolvedValue::Enum(e2)) => e1 == e2,
        (ResolvedValue::TypeAlias(t1), ResolvedValue::TypeAlias(t2)) => t1 == t2,

        (
            ResolvedValue::EnumVariant { enum_fqn: e1, variant: v1 },
            ResolvedValue::EnumVariant { enum_fqn: e2, variant: v2 },
        ) => e1 == e2 && v1 == v2,

        (
            ResolvedValue::Field { class_fqn: c1, field: f1 },
            ResolvedValue::Field { class_fqn: c2, field: f2 },
        ) => c1 == c2 && f1 == f2,

        (
            ResolvedValue::BuiltinFunction { path: p1 },
            ResolvedValue::BuiltinFunction { path: p2 },
        ) => p1 == p2,

        _ => false,
    }
}