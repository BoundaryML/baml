//! LSP project management for BAML.
//!
//! This module provides the LSP-specific layer on top of `ProjectDatabase`:
//! - `BamlProject`: Manages file state (disk files vs unsaved editor buffers)
//! - `Project`: Combines file management with the Salsa-based compiler

use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
};

use baml_lsp_types::{BamlFunction, BamlFunctionTestCasePair, BamlGeneratorConfig};
use baml_project::ProjectDatabase;
use file_utils::gather_files;
use lsp_types::TextDocumentItem;

use crate::{DocumentKey, TextDocument, server::client::Notifier};

pub mod file_utils;
pub mod position_utils;

/// Trims a given string by removing non-alphanumeric characters (besides underscores and periods).
pub fn trim_line(s: &str) -> String {
    s.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .to_string()
}

/// Manages LSP file state: disk files vs unsaved editor buffers.
///
/// This is the LSP-specific layer that tracks which files have unsaved changes
/// in the editor. The merged view (disk + unsaved) is synced to `ProjectDatabase`
/// for compilation.
pub struct BamlProject {
    pub root_dir_name: PathBuf,
    /// Files loaded from disk.
    files: HashMap<DocumentKey, TextDocument>,
    /// Files with unsaved changes (takes precedence over disk versions).
    unsaved_files: HashMap<DocumentKey, TextDocument>,
}

impl Drop for BamlProject {
    fn drop(&mut self) {
        tracing::debug!("Dropping BamlProject");
    }
}

impl std::fmt::Debug for BamlProject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BamlProject")
            .field("root_dir_name", &self.root_dir_name)
            .field("files", &self.files.len())
            .field("unsaved_files", &self.unsaved_files.len())
            .finish()
    }
}

impl BamlProject {
    pub fn new(root_dir: PathBuf) -> Self {
        tracing::debug!("Creating BamlProject for {}", root_dir.display());
        Self {
            root_dir_name: root_dir,
            files: HashMap::new(),
            unsaved_files: HashMap::new(),
        }
    }

    /// Set or clear an unsaved file (editor buffer).
    pub fn set_unsaved_file(&mut self, document_key: &DocumentKey, content: Option<String>) {
        if let Some(content) = content {
            tracing::debug!("Setting unsaved file: {}", document_key.path().display());
            let text_document = TextDocument::new(content, 0);
            self.unsaved_files
                .insert(document_key.clone(), text_document);
        } else {
            self.unsaved_files.remove(document_key);
        }
    }

    /// Remove an unsaved file (e.g., when editor closes without saving).
    pub fn remove_unsaved_file(&mut self, document_key: &DocumentKey) {
        self.unsaved_files.remove(document_key);
    }

    /// Mark a file as saved (moves from unsaved to disk state).
    pub fn save_file(&mut self, document_key: &DocumentKey, content: &str) {
        tracing::debug!("Saving file: {}", document_key.path().display());
        let text_document = TextDocument::new(content.to_string(), 0);
        self.files.insert(document_key.clone(), text_document);
        self.unsaved_files.remove(document_key);
    }

    /// Update a disk file's content.
    pub fn update_file(&mut self, document_key: &DocumentKey, content: Option<String>) {
        if let Some(content) = content {
            tracing::debug!("Updating file: {}", document_key.path().display());
            let text_document = TextDocument::new(content, 0);
            self.files.insert(document_key.clone(), text_document);
        } else {
            self.files.remove(document_key);
        }
    }

    /// Load all .baml files from disk into this project.
    pub fn load_files(&mut self) -> anyhow::Result<HashMap<DocumentKey, TextDocument>> {
        let workspace_file_paths = gather_files(&self.root_dir_name, false).map_err(|e| {
            anyhow::anyhow!(
                "Failed to gather files from directory {}: {}",
                self.root_dir_name.display(),
                e
            )
        })?;

        let workspace_files = workspace_file_paths
            .into_iter()
            .map(|file_path| {
                let document_key = DocumentKey::from_path(&self.root_dir_name, &file_path)
                    .map_err(|e| {
                        anyhow::anyhow!(
                            "Failed to create document key for file {}: {}",
                            file_path.display(),
                            e
                        )
                    })?;
                let contents = std::fs::read_to_string(&file_path).map_err(|e| {
                    anyhow::anyhow!("Failed to read file {}: {}", file_path.display(), e)
                })?;
                let text_document = TextDocument::new(contents, 0);
                Ok((document_key, text_document))
            })
            .collect::<anyhow::Result<HashMap<_, _>>>()?;

        self.files = workspace_files.clone();
        Ok(workspace_files)
    }

    /// Get merged view of all files (disk + unsaved, with unsaved taking precedence).
    pub fn all_files(&self) -> impl Iterator<Item = (&DocumentKey, &TextDocument)> {
        // Chain disk files with unsaved files, unsaved will shadow disk versions
        // when iterated (though caller should use HashMap for proper shadowing)
        self.files.iter().chain(self.unsaved_files.iter())
    }

    /// Get all file contents as a HashMap (unsaved takes precedence).
    pub fn all_file_contents(&self) -> HashMap<String, String> {
        let mut result = HashMap::new();
        for (key, doc) in &self.files {
            result.insert(key.unchecked_to_string(), doc.contents.clone());
        }
        // Unsaved files override disk files
        for (key, doc) in &self.unsaved_files {
            result.insert(key.unchecked_to_string(), doc.contents.clone());
        }
        result
    }

    /// Check if a file exists on disk (in the files map).
    pub fn has_file(&self, key: &DocumentKey) -> bool {
        self.files.contains_key(key)
    }

    /// Check if a file has unsaved changes.
    pub fn has_unsaved_file(&self, key: &DocumentKey) -> bool {
        self.unsaved_files.contains_key(key)
    }

    /// Insert an unsaved file directly.
    pub fn insert_unsaved(&mut self, key: DocumentKey, doc: TextDocument) {
        self.unsaved_files.insert(key, doc);
    }

    /// Clear all unsaved files.
    pub fn clear_unsaved_files(&mut self) {
        self.unsaved_files.clear();
    }
}

/// Combines LSP file management with the Salsa-based compiler.
///
/// This is the main entry point for LSP operations. It wraps `BamlProject`
/// (for file state management) and `ProjectDatabase` (for compilation).
pub struct Project {
    /// LSP file state (disk files + unsaved buffers).
    pub baml_project: BamlProject,
    /// Salsa-based database for incremental compilation.
    db: ProjectDatabase,
}

impl std::fmt::Debug for Project {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Project")
            .field("root", &self.baml_project.root_dir_name)
            .finish()
    }
}

impl Project {
    /// Creates a new Project from a BamlProject.
    pub fn new(baml_project: BamlProject) -> Self {
        let mut db = ProjectDatabase::new();
        db.set_project_root(&baml_project.root_dir_name);
        Self { baml_project, db }
    }

    /// Returns a reference to the ProjectDatabase.
    pub fn db(&self) -> &ProjectDatabase {
        &self.db
    }

    /// Returns a mutable reference to the ProjectDatabase.
    pub fn db_mut(&mut self) -> &mut ProjectDatabase {
        &mut self.db
    }

    // Keep old names as aliases for compatibility
    #[doc(hidden)]
    pub fn lsp_db(&self) -> &ProjectDatabase {
        &self.db
    }
    #[doc(hidden)]
    pub fn lsp_db_mut(&mut self) -> &mut ProjectDatabase {
        &mut self.db
    }

    /// Syncs all files from BamlProject to ProjectDatabase.
    ///
    /// This merges disk files with unsaved editor buffers and updates
    /// the Salsa database for incremental recompilation.
    pub fn sync_files_to_db(&mut self) {
        for (uri, content) in self.baml_project.all_file_contents() {
            let path = Path::new(&uri);
            self.db.add_or_update_file(path, &content);
        }
    }

    /// Updates a single file in the ProjectDatabase.
    pub fn update_file_in_db(&mut self, path: &Path, content: &str) {
        self.db.add_or_update_file(path, content);
    }

    /// Syncs files and prepares for diagnostics/compilation.
    pub fn update_runtime(
        &mut self,
        _runtime_notifier: Option<Notifier>,
        _feature_flags: &[String],
    ) -> anyhow::Result<()> {
        self.sync_files_to_db();
        tracing::debug!("update_runtime: synced files to ProjectDatabase");
        Ok(())
    }

    /// Returns a map of file URIs to their content.
    pub fn files(&self) -> HashMap<String, String> {
        self.baml_project.all_file_contents()
    }

    /// Replaces the file state with a new BamlProject.
    pub fn replace_all_files(&mut self, project: BamlProject) {
        self.baml_project = project;
    }

    /// Reads a file from disk as a TextDocumentItem.
    pub fn get_file(&self, uri: &str) -> io::Result<TextDocumentItem> {
        file_utils::convert_to_text_document(Path::new(uri))
    }

    /// Returns the root path of this project.
    pub fn root_path(&self) -> &Path {
        &self.baml_project.root_dir_name
    }

    // --- Symbol listing (delegates to baml_project::symbols) ---
    // TODO: These should call through to ProjectDatabase once implemented

    /// Returns a list of functions from the project.
    pub fn list_functions(&self) -> Result<Vec<BamlFunction>, &str> {
        // TODO: Use baml_project::list_functions(&self.db)
        Ok(vec![])
    }

    /// Returns a list of test cases from the project.
    pub fn list_function_test_pairs(&self) -> Result<Vec<BamlFunctionTestCasePair>, &str> {
        // TODO: Use baml_project::list_tests(&self.db)
        Ok(vec![])
    }

    /// Returns a list of generator configurations.
    pub fn list_generators(
        &self,
        _feature_flags: &[String],
    ) -> Result<Vec<BamlGeneratorConfig>, &str> {
        // TODO: Use baml_project::list_generators(&self.db)
        Ok(vec![])
    }

    // --- Version checking ---

    /// Checks the version of a given generator.
    pub fn check_version(
        &self,
        generator: &BamlGeneratorConfig,
        _is_diagnostic: bool,
    ) -> Option<String> {
        Some(generator.version.clone())
    }

    /// Checks versions on save.
    pub fn check_version_on_save(&self, _feature_flags: &[String]) -> Option<String> {
        None
    }

    /// Returns true if any generator produces TypeScript output.
    pub fn is_typescript_generator_present(&self, _feature_flags: &[String]) -> bool {
        false
    }

    /// Returns common generator version if all match.
    pub fn get_common_generator_version(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
}
