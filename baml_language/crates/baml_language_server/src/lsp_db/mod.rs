//! LSP Database wrapper for the new baml_language compiler infrastructure.
//!
//! This module provides an LSP-friendly interface to the Salsa-based `baml_language`
//! compiler, replacing the previous Pest-based `BamlRuntime` integration.

pub mod position;
pub mod symbols;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use baml_db::{FileId, RootDatabase, Setter, SourceFile, baml_workspace::Project};

/// LSP Database wrapper that provides high-level APIs for language server operations.
///
/// This wraps the Salsa-based `RootDatabase` and provides LSP-friendly methods for:
/// - File management (add/update/remove files)
/// - Symbol lookup
/// - Diagnostics collection
/// - Position/span conversion
#[derive(Clone)]
pub struct LspDatabase {
    /// The underlying Salsa database (includes the project).
    db: RootDatabase,
    /// Maps file paths to their SourceFile handles.
    file_map: HashMap<PathBuf, SourceFile>,
    /// Maps FileId to file path for reverse lookup.
    file_id_to_path: HashMap<FileId, PathBuf>,
}

impl LspDatabase {
    /// Create a new empty LSP database.
    pub fn new() -> Self {
        Self {
            db: RootDatabase::new(),
            file_map: HashMap::new(),
            file_id_to_path: HashMap::new(),
        }
    }

    /// Get a reference to the underlying Salsa database.
    pub fn db(&self) -> &RootDatabase {
        &self.db
    }

    /// Get a mutable reference to the underlying Salsa database.
    pub fn db_mut(&mut self) -> &mut RootDatabase {
        &mut self.db
    }

    /// Get the project, if set.
    pub fn project(&self) -> Option<Project> {
        self.db.project
    }

    /// Add or update a file in the database.
    ///
    /// If the file already exists, its content is updated using Salsa's `set_text` method.
    /// Otherwise, a new SourceFile is created.
    ///
    /// Returns the SourceFile handle.
    pub fn add_or_update_file(&mut self, path: &Path, content: &str) -> SourceFile {
        let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        if let Some(&existing_file) = self.file_map.get(&canonical_path) {
            // Update existing file using Salsa's setter
            existing_file.set_text(&mut self.db).to(content.to_string());
            existing_file
        } else {
            // Create new file
            let file = self.db.add_file(&canonical_path, content);
            let file_id = file.file_id(&self.db);

            self.file_map.insert(canonical_path.clone(), file);
            self.file_id_to_path.insert(file_id, canonical_path);

            // Update project files list if project is set
            if let Some(project) = self.db.project {
                let mut files: Vec<SourceFile> = project.files(&self.db).clone();
                files.push(file);
                project.set_files(&mut self.db).to(files);
            }

            file
        }
    }

    /// Remove a file from the database.
    ///
    /// Note: Salsa doesn't support true removal, but we can remove it from our tracking
    /// and the project's file list.
    pub fn remove_file(&mut self, path: &Path) {
        let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        if let Some(file) = self.file_map.remove(&canonical_path) {
            let file_id = file.file_id(&self.db);
            self.file_id_to_path.remove(&file_id);

            // Remove from project files list
            if let Some(project) = self.db.project {
                let files: Vec<SourceFile> = project
                    .files(&self.db)
                    .iter()
                    .copied()
                    .filter(|f| f.file_id(&self.db) != file_id)
                    .collect();
                project.set_files(&mut self.db).to(files);
            }
        }
    }

    /// Set the project root directory.
    ///
    /// This creates a new Project in the database with an empty file list.
    /// Files should be added using `add_or_update_file`.
    pub fn set_project_root(&mut self, root: &Path) {
        let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());

        // Collect existing files that are under this root
        let existing_files: Vec<SourceFile> = self
            .file_map
            .iter()
            .filter(|(p, _)| p.starts_with(&canonical_root))
            .map(|(_, f)| *f)
            .collect();

        // Create and set the project on the underlying RootDatabase
        let project = Project::new(&self.db, canonical_root, existing_files);
        self.db.project = Some(project);
    }

    /// Get all files currently in the database.
    pub fn files(&self) -> impl Iterator<Item = SourceFile> + '_ {
        self.file_map.values().copied()
    }

    /// Get a SourceFile by its path.
    pub fn get_file(&self, path: &Path) -> Option<SourceFile> {
        let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.file_map.get(&canonical_path).copied()
    }

    /// Get the file path for a FileId.
    pub fn get_path(&self, file_id: FileId) -> Option<&Path> {
        self.file_id_to_path.get(&file_id).map(|p| p.as_path())
    }

    /// Get a SourceFile by its FileId.
    pub fn get_file_by_id(&self, file_id: FileId) -> Option<SourceFile> {
        self.file_id_to_path
            .get(&file_id)
            .and_then(|path| self.file_map.get(path).copied())
    }
}

impl Default for LspDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for LspDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LspDatabase")
            .field("file_count", &self.file_map.len())
            .field("has_project", &self.db.project.is_some())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_file() {
        let mut db = LspDatabase::new();
        let path = Path::new("/tmp/test.baml");
        let content = "class Foo { name string }";

        let file = db.add_or_update_file(path, content);
        assert_eq!(file.text(&db.db), content);
    }

    #[test]
    fn test_update_file() {
        let mut db = LspDatabase::new();
        let path = Path::new("/tmp/test.baml");

        let file1 = db.add_or_update_file(path, "class Foo {}");
        let file2 = db.add_or_update_file(path, "class Bar {}");

        // Should be the same file handle
        assert_eq!(file1.file_id(&db.db), file2.file_id(&db.db));
        // Content should be updated
        assert_eq!(file1.text(&db.db), "class Bar {}");
    }
}
