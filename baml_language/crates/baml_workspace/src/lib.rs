//! Workspace and project management.
//!
//! Handles discovering BAML files in a project directory and managing the project structure.

use std::path::PathBuf;

use baml_base::{FileId, SourceFile};

mod discovery;
pub use discovery::discover_baml_files;

/// A BAML project (collection of files that compile together)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Project {
    pub root_path: PathBuf,
}

/// Input: the project root configuration
#[salsa::input]
pub struct ProjectRoot {
    pub path: PathBuf,
}

/// Tracked: discover all BAML files in the project
#[salsa::tracked]
pub fn project_files(db: &dyn salsa::Database, root: ProjectRoot) -> Vec<SourceFile> {
    #[allow(unused_variables)]
    let paths = discovery::discover_baml_files(&root.path(db));

    // Create SourceFile inputs for each discovered file
    // For now, return empty vec as we need database context to create inputs
    // In real implementation, this would read files and create SourceFile inputs
    vec![]
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
