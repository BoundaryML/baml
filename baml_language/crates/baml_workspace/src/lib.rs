//! Workspace and project management.
//!
//! Handles discovering BAML files in a project directory and managing the project structure.
//!
//! This crate provides:
//! - File discovery (`discover_baml_files`)
//! - Project root tracking (`Project` Salsa input)
//! - Source file utilities
//! - The base `Db` trait for project context
//!
//! ## Architecture Note
//!
//! `Project` is defined here (rather than in `baml_project`) because:
//! - Lower-level crates (`baml_hir`, `baml_tir`, `baml_mir`) need the `Project` type
//!   in their query signatures (e.g., `validate_hir(db, project)`)
//! - If `Project` were in `baml_project`, those crates would need to depend on
//!   `baml_project`, creating a circular dependency
//! - This follows the pattern: low-level types here, high-level operations in `baml_project`
//!
//! This is similar to how ty/ruff structures their codebase:
//! - `ruff_db` provides low-level types and the base `Db` trait
//! - `ty_project` provides high-level `ProjectDatabase` and operations
//! - The `Program` singleton (compiler settings) lives in the semantic crate

use std::path::PathBuf;

use baml_base::{FileId, SourceFile};

mod discovery;
pub use discovery::discover_baml_files;

/// Database trait for workspace/project context.
///
/// Provides access to the project being compiled. Extended by downstream
/// crates (`baml_hir::Db`, `baml_tir::Db`, etc.).
#[salsa::db]
pub trait Db: salsa::Database {
    /// Returns the project being analyzed.
    fn project(&self) -> Project;
}

/// Input: the project root configuration
///
/// This tracks both the root path and the list of source files in the project.
/// By storing files as an input field, Salsa can properly track changes to the
/// file list (files added/removed) as well as changes to individual files.
#[salsa::input]
pub struct Project {
    pub root: PathBuf,

    /// The list of source files in this project.
    /// This should be updated whenever files are added or removed.
    #[returns(ref)]
    pub files: Vec<SourceFile>,
}

/// Helper to create a source file in the database
pub fn create_source_file(
    db: &dyn salsa::Database,
    path: PathBuf,
    text: String,
    file_id: FileId,
) -> SourceFile {
    SourceFile::new(db, text, path, file_id)
}
