//! Workspace and project management.
//!
//! Handles discovering BAML files in a project directory and managing the project structure.

use std::path::PathBuf;

use baml_base::{FileId, SourceFile};

mod discovery;
pub use discovery::discover_baml_files;

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

/// Get all BAML files in the project.
///
/// This simply returns the files stored in the `ProjectRoot` input.
/// The files list should be maintained by the caller (e.g., the language server
/// or test harness) as files are added/removed from the project.
pub fn project_files(db: &dyn salsa::Database, root: Project) -> Vec<SourceFile> {
    root.files(db).clone()
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
