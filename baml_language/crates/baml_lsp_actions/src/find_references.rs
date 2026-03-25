//! Find all references for BAML files.
//!
//! NOTE: Stubbed — pending compiler2 LSP action reimplementation.

use std::path::PathBuf;

use baml_db::{FileId, Span};
use baml_project::ProjectDatabase;
use text_size::TextSize;

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
/// NOTE: Stubbed — returns empty. Will be reimplemented with compiler2 HIR.
#[allow(unused_variables)]
pub fn find_all_references(
    db: &ProjectDatabase,
    file_id: FileId,
    position: TextSize,
) -> Vec<Reference> {
    Vec::new()
}
