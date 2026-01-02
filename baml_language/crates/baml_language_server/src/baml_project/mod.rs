// TODO: This file has been heavily modified to remove BamlRuntime dependency.
// The original code is commented out below for reference.

use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
};

use baml_lsp_types::{BamlFunction, BamlFunctionTestCasePair, BamlGeneratorConfig, BamlSpan};
use file_utils::gather_files;
use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range, TextDocumentItem};

use crate::{DocumentKey, TextDocument, lsp_db::LspDatabase, server::client::Notifier, version};

pub mod file_utils;
pub mod position_utils;

// --- Helper functions for working with text documents ---

/// Trims a given string by removing non-alphanumeric characters (besides underscores and periods).
pub fn trim_line(s: &str) -> String {
    let res = s
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .to_string();
    res
}

pub struct BamlProject {
    pub root_dir_name: PathBuf,
    // This is the version of the file on disk
    pub files: HashMap<DocumentKey, TextDocument>,
    // This is the version of the file that is currently being edited
    // (unsaved changes)
    pub unsaved_files: HashMap<DocumentKey, TextDocument>,
    // TODO: Salsa database for diagnostics would go here
    // pub db: Option<baml_db::RootDatabase>,
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
            .field("files", &self.files.keys())
            .field("unsaved_files", &self.unsaved_files.keys())
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

    // TODO: Implement using salsa database
    pub fn list_functions(
        &mut self,
        _feature_flags: &[String],
        _filter: Option<baml_lsp_types::FunctionFlavor>,
    ) -> Vec<BamlFunction> {
        // TODO: Implement using salsa database
        vec![]
    }

    // TODO: Implement using salsa database
    pub fn check_version(
        &self,
        _generator_config: &BamlGeneratorConfig,
        _is_diagnostic: bool,
    ) -> Option<String> {
        // TODO: Implement version checking
        None
    }

    // TODO: Commented out - depends on generators_lib
    // pub fn run_generators_native(
    //     &mut self,
    //     no_version_check: Option<bool>,
    //     feature_flags: &[String],
    // ) -> Result<Vec<GenerateOutput>, anyhow::Error> {
    //     todo!()
    // }

    pub fn set_unsaved_file(&mut self, document_key: &DocumentKey, content: Option<String>) {
        tracing::debug!(
            "Setting unsaved file: {}, {}",
            document_key.path().display(),
            content.clone().unwrap_or("None".to_string())
        );
        if let Some(content) = content {
            let text_document = TextDocument::new(content, 0);
            self.unsaved_files
                .insert(document_key.clone(), text_document);
        } else {
            self.unsaved_files.remove(document_key);
        }
    }

    pub fn remove_unsaved_file(&mut self, document_key: &DocumentKey) {
        self.unsaved_files.remove(document_key);
    }

    pub fn save_file(&mut self, document_key: &DocumentKey, content: &str) {
        tracing::debug!(
            "Saving file: {}, {}",
            document_key.path().display(),
            content
        );
        let text_document = TextDocument::new(content.to_string(), 0);
        self.files.insert(document_key.clone(), text_document);
        self.unsaved_files.remove(document_key);
    }

    pub fn update_file(&mut self, document_key: &DocumentKey, content: Option<String>) {
        tracing::debug!(
            "Updating file: {}, {}",
            document_key.path().display(),
            content.clone().unwrap_or("None".to_string())
        );
        if let Some(content) = content {
            let text_document = TextDocument::new(content, 0);
            self.files.insert(document_key.clone(), text_document);
        } else {
            self.files.remove(document_key);
        }
    }

    /// Load files into the current state. Also return the newly loaded files.
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

        let project_files = workspace_files.clone();

        self.files = project_files;
        Ok(workspace_files)
    }

    // TODO: Implement using salsa database
    pub fn list_generators(
        &mut self,
        _feature_flags: &[String],
    ) -> Result<Vec<BamlGeneratorConfig>, &str> {
        // TODO: Implement using salsa database
        Ok(vec![])
    }

    pub fn files(&self) -> Vec<String> {
        let mut all_files = self.files.clone();
        self.unsaved_files.iter().for_each(|(k, v)| {
            all_files.insert(k.clone(), v.clone());
        });
        let formatted_files = all_files
            .iter()
            .map(|(k, v)| format!("{}BAML_PATH_SPLTTER{}", k.unchecked_to_string(), v.contents))
            .collect::<Vec<String>>();
        formatted_files
    }

    /// Get all file contents as a HashMap for diagnostics
    pub fn all_file_contents(&self) -> HashMap<String, String> {
        let mut result = HashMap::new();
        for (key, doc) in &self.files {
            result.insert(key.unchecked_to_string(), doc.contents.clone());
        }
        for (key, doc) in &self.unsaved_files {
            result.insert(key.unchecked_to_string(), doc.contents.clone());
        }
        result
    }
}

/// The Project struct wraps a BAML project and exposes methods for file updates,
/// diagnostics, symbol lookup, and code generation.
pub struct Project {
    pub baml_project: BamlProject,
    /// Salsa-based database for incremental compilation.
    /// This is lazily initialized when diagnostics are first requested.
    lsp_db: LspDatabase,
}

impl std::fmt::Debug for Project {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Project")
    }
}

impl Project {
    /// Creates a new `Project` instance.
    pub fn new(baml_project: BamlProject) -> Self {
        let mut lsp_db = LspDatabase::new();
        lsp_db.set_project_root(&baml_project.root_dir_name);
        Self {
            baml_project,
            lsp_db,
        }
    }

    /// Returns a reference to the LspDatabase.
    pub fn lsp_db(&self) -> &LspDatabase {
        &self.lsp_db
    }

    /// Returns a mutable reference to the LspDatabase.
    pub fn lsp_db_mut(&mut self) -> &mut LspDatabase {
        &mut self.lsp_db
    }

    /// Syncs all files from the BamlProject to the LspDatabase.
    /// This should be called after loading/reloading files.
    pub fn sync_files_to_lsp_db(&mut self) {
        // Merge files and unsaved_files, with unsaved_files taking precedence
        let mut all_files = self.baml_project.files.clone();
        for (key, doc) in &self.baml_project.unsaved_files {
            all_files.insert(key.clone(), doc.clone());
        }

        for (doc_key, text_doc) in &all_files {
            let path = doc_key.path();
            self.lsp_db.add_or_update_file(path, &text_doc.contents);
        }
    }

    /// Updates a single file in the LspDatabase.
    pub fn update_file_in_lsp_db(&mut self, path: &Path, content: &str) {
        self.lsp_db.add_or_update_file(path, content);
    }

    /// Checks the version of a given generator.
    pub fn check_version(
        &self,
        generator: &BamlGeneratorConfig,
        _is_diagnostic: bool,
    ) -> Option<String> {
        Some(generator.version.clone())
    }

    /// Iterates over all generators and prints error messages if version mismatches are found.
    pub fn check_version_on_save(&self, _feature_flags: &[String]) -> Option<String> {
        // TODO: Implement version checking
        None
    }

    /// Returns true if any generator produces TypeScript output.
    pub fn is_typescript_generator_present(&self, _feature_flags: &[String]) -> bool {
        // TODO: Implement using salsa database
        false
    }

    /// Updates the runtime/diagnostics.
    pub fn update_runtime(
        &mut self,
        _runtime_notifier: Option<Notifier>,
        _feature_flags: &[String],
    ) -> anyhow::Result<()> {
        // Sync files to the LspDatabase for incremental compilation
        self.sync_files_to_lsp_db();
        tracing::debug!("update_runtime: synced files to LspDatabase");
        Ok(())
    }

    /// Returns a map of file URIs to their content.
    pub fn files(&self) -> HashMap<String, String> {
        self.baml_project.all_file_contents()
    }

    /// Replaces the current BAML project with a new one.
    pub fn replace_all_files(&mut self, project: BamlProject) {
        self.baml_project = project;
    }

    /// Reads a file and converts it into a text document.
    pub fn get_file(&self, uri: &str) -> io::Result<TextDocumentItem> {
        let path = Path::new(uri);
        file_utils::convert_to_text_document(path)
    }

    // TODO: Implement using salsa database
    // pub fn handle_hover_request(
    //     &mut self,
    //     doc: &TextDocumentItem,
    //     position: &Position,
    //     notifier: Notifier,
    //     feature_flags: &[String],
    // ) -> anyhow::Result<Option<Hover>> {
    //     todo!()
    // }

    /// Returns a list of functions from the project.
    pub fn list_functions(&self) -> Result<Vec<BamlFunction>, &str> {
        // TODO: Implement using salsa database
        Ok(vec![])
    }

    /// Returns a list of expr functions from the project.
    pub fn list_expr_fns(&self) -> Result<Vec<BamlFunction>, &str> {
        // TODO: Implement using salsa database
        Ok(vec![])
    }

    /// Returns a list of test cases from the project.
    pub fn list_function_test_pairs(&self) -> Result<Vec<BamlFunctionTestCasePair>, &str> {
        // TODO: Implement using salsa database
        Ok(vec![])
    }

    /// Returns a list of generator configurations.
    pub fn list_generators(
        &self,
        _feature_flags: &[String],
    ) -> Result<Vec<BamlGeneratorConfig>, &str> {
        // TODO: Implement using salsa database
        Ok(vec![])
    }

    /// Returns the root path of this project.
    pub fn root_path(&self) -> &Path {
        &self.baml_project.root_dir_name
    }

    // TODO: Generators are disabled for now
    // pub fn run_generators_without_debounce<F, E>(
    //     &mut self,
    //     feature_flags: &[String],
    //     on_success: F,
    //     on_error: E,
    // ) where
    //     F: Fn(String) + Send,
    //     E: Fn(String) + Send,
    // {
    //     todo!()
    // }

    /// Checks if all generators use the same major.minor version.
    pub fn get_common_generator_version(&self) -> anyhow::Result<Option<String>> {
        // TODO: Implement using salsa database
        Ok(None)
    }
}
