//! `ProjectDatabase` - the main database for BAML projects.
//!
//! This module provides `ProjectDatabase`, which owns the Salsa storage directly
//! (following the ty/ruff pattern) and implements all the compiler `Db` traits.
//!
//! Unlike the previous `LspDatabase` which wrapped `RootDatabase`, `ProjectDatabase`
//! has direct ownership of the storage, removing a layer of indirection.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, atomic::AtomicU32},
};

use baml_compiler_emit::CompileOptions;
use baml_db::{FileId, SourceFile};
use baml_workspace::Project;
use salsa::Setter;

// Note: Builtin BAML files (like llm.baml) are loaded in set_project_root().
// The paths are defined in `baml_builtins::baml_sources::ALL`.

/// Type alias for Salsa event callbacks
pub type EventCallback = Box<dyn Fn(salsa::Event) + Send + Sync + 'static>;

/// The main database for BAML projects.
///
/// `ProjectDatabase` owns the Salsa storage directly and implements all the
/// compiler `Db` traits. It provides high-level APIs for:
/// - File management (add/update/remove files)
/// - Project root management
/// - Diagnostics collection via `check()`
///
/// ## Example
///
/// ```ignore
/// let mut db = ProjectDatabase::new();
/// db.set_project_root(std::path::Path::new("/my/project"));
/// db.add_or_update_file(std::path::Path::new("/my/project/main.baml"), "class Foo {}");
///
/// let result = db.check();
/// for diag in &result.diagnostics {
///     println!("{}", diag.message);
/// }
/// ```
#[salsa::db]
#[derive(Clone)]
pub struct ProjectDatabase {
    /// The Salsa storage - owned directly, not via wrapper.
    storage: salsa::Storage<ProjectDatabase>,
    /// Counter for generating unique `FileId`s.
    next_file_id: Arc<AtomicU32>,
    /// The current project. Set via `set_project_root()`.
    project: Option<Project>,
    /// Maps file paths to their `SourceFile` handles.
    file_map: HashMap<std::path::PathBuf, SourceFile>,
    /// Maps `FileId` to file path for reverse lookup.
    file_id_to_path: HashMap<FileId, std::path::PathBuf>,
}

#[salsa::db]
impl salsa::Database for ProjectDatabase {}

#[salsa::db]
impl baml_workspace::Db for ProjectDatabase {
    fn project(&self) -> Project {
        self.project
            .expect("project must be set before querying - call set_project_root first")
    }
}

#[salsa::db]
impl baml_compiler_hir::Db for ProjectDatabase {}

#[salsa::db]
impl baml_compiler_tir::Db for ProjectDatabase {}

#[salsa::db]
impl baml_compiler_vir::Db for ProjectDatabase {}

#[salsa::db]
impl baml_compiler_mir::Db for ProjectDatabase {}

#[salsa::db]
impl baml_compiler_emit::Db for ProjectDatabase {}

impl ProjectDatabase {
    /// Create a new empty database.
    pub fn new() -> Self {
        Self {
            storage: salsa::Storage::default(),
            next_file_id: Arc::new(AtomicU32::new(0)),
            project: None,
            file_map: HashMap::new(),
            file_id_to_path: HashMap::new(),
        }
    }

    /// Create a new database with an event callback for tracking query execution.
    ///
    /// The callback will be invoked for various Salsa events, including:
    /// - `WillExecute`: A query is about to be recomputed
    /// - `DidValidateMemoizedValue`: A cached value was reused
    ///
    /// This is useful for tracking incremental compilation behavior.
    pub fn new_with_event_callback(callback: EventCallback) -> Self {
        Self {
            storage: salsa::Storage::new(Some(callback)),
            next_file_id: Arc::new(AtomicU32::new(0)),
            project: None,
            file_map: HashMap::new(),
            file_id_to_path: HashMap::new(),
        }
    }

    /// Get the project, if set.
    pub fn get_project(&self) -> Option<Project> {
        self.project
    }

    /// Get the project, if set.
    ///
    /// Alias for `get_project()` for API compatibility with old `LspDatabase`.
    pub fn project(&self) -> Option<Project> {
        self.project
    }

    /// Get a reference to self as the database.
    ///
    /// This method exists for API compatibility with code that previously
    /// called `lsp_db.db()` to get the underlying `RootDatabase`.
    /// Since `ProjectDatabase` IS the database now, this just returns `self`.
    pub fn db(&self) -> &Self {
        self
    }

    /// Get a mutable reference to self as the database.
    ///
    /// This method exists for API compatibility with code that previously
    /// called `lsp_db.db_mut()` to get the underlying `RootDatabase`.
    pub fn db_mut(&mut self) -> &mut Self {
        self
    }

    /// Get all source files in the database, sorted by `FileId` for deterministic ordering.
    pub fn get_source_files(&self) -> Vec<SourceFile> {
        let mut files: Vec<SourceFile> = self.file_map.values().copied().collect();
        files.sort_by_key(|f| f.file_id(self).as_u32());
        files
    }

    /// Get the file path for a `FileId`.
    pub fn file_id_to_path(&self, file_id: FileId) -> Option<&std::path::PathBuf> {
        self.file_id_to_path.get(&file_id)
    }

    /// Add a file to the database (internal helper).
    fn add_file_internal(
        &mut self,
        path: impl Into<std::path::PathBuf>,
        text: impl Into<String>,
    ) -> SourceFile {
        let file_id = FileId::new(
            self.next_file_id
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        );

        // Create a new SourceFile input
        SourceFile::new(self, text.into(), path.into(), file_id)
    }

    /// Add or update a file in the database.
    ///
    /// If the file already exists, its content is updated using Salsa's `set_text` method.
    /// Otherwise, a new `SourceFile` is created.
    ///
    /// Returns the `SourceFile` handle.
    pub fn add_or_update_file(&mut self, path: &std::path::Path, content: &str) -> SourceFile {
        let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        if let Some(&existing_file) = self.file_map.get(&canonical_path) {
            // Update existing file using Salsa's setter
            existing_file.set_text(self).to(content.to_string());
            existing_file
        } else {
            // Create new file
            let file = self.add_file_internal(&canonical_path, content);
            let file_id = file.file_id(self);

            self.file_map.insert(canonical_path.clone(), file);
            self.file_id_to_path.insert(file_id, canonical_path);

            // Update project files list if project is set
            if let Some(project) = self.project {
                let mut files: Vec<SourceFile> = project.files(self).clone();
                files.push(file);
                project.set_files(self).to(files);
            }

            file
        }
    }

    /// Remove a file from the database.
    ///
    /// Note: Salsa doesn't support true removal, but we can remove it from our tracking
    /// and the project's file list.
    pub fn remove_file(&mut self, path: &std::path::Path) {
        let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        if let Some(file) = self.file_map.remove(&canonical_path) {
            let file_id = file.file_id(self);
            self.file_id_to_path.remove(&file_id);

            // Remove from project files list
            if let Some(project) = self.project {
                let files: Vec<SourceFile> = project
                    .files(self)
                    .iter()
                    .copied()
                    .filter(|f| f.file_id(self) != file_id)
                    .collect();
                project.set_files(self).to(files);
            }
        }
    }

    /// Set the project root directory.
    ///
    /// This creates a new Project in the database with an empty file list.
    /// Files should be added using `add_file` or `add_or_update_file`.
    ///
    /// This also loads builtin BAML files (like `llm.baml`) into the project.
    /// Builtin files are available from the start of the compilation pipeline.
    ///
    /// Returns the created `Project`.
    pub fn set_project_root(&mut self, root: &std::path::Path) -> Project {
        let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());

        // Collect existing user files that are under this root
        let user_files: Vec<SourceFile> = self
            .file_map
            .iter()
            .filter(|(p, _)| p.starts_with(&canonical_root))
            .map(|(_, f)| *f)
            .collect();

        // Load builtin BAML files after user files (matches production order)
        let builtin_files = self.load_builtin_baml_files();

        // Combine user files with builtin files (user first, then builtins)
        let mut all_files = user_files;
        all_files.extend(builtin_files);

        // Create and set the project
        let project = Project::new(self, canonical_root, all_files);
        self.project = Some(project);
        project
    }

    /// Load builtin BAML source files into the database.
    ///
    /// These files provide implementations for builtin namespaces like `baml.llm`.
    /// They are loaded once when the project is set up and included in the
    /// compilation pipeline from the start.
    ///
    /// Builtin files use the normal `FileId` allocation just like user files.
    /// They are registered in both `file_id_to_path` (for diagnostic filename
    /// display) and `file_map` (so builtins are included in `files()` iteration
    /// and `check()` diagnostics).
    ///
    /// ## Note on goto-definition
    ///
    /// Builtin files use virtual paths like `<builtin>/baml/llm.baml`. These paths
    /// are embedded in the compiler binary, not present on the user's filesystem.
    /// As a result, goto-definition to builtins won't work in editors.
    ///
    /// Future enhancement: To support goto-definition for builtins, we could:
    /// 1. Extract builtin files to a cache directory (e.g., `~/.cache/baml/builtins/`)
    /// 2. Register the real filesystem paths instead of virtual paths
    /// 3. Ensure the cache is updated when the compiler version changes
    fn load_builtin_baml_files(&mut self) -> Vec<SourceFile> {
        let mut builtin_files = Vec::new();

        // Load all builtin BAML sources (disk read on native, embedded on WASM)
        for builtin_source in baml_builtins::baml_sources() {
            let path = PathBuf::from(builtin_source.path);
            let file = self.add_file_internal(&path, builtin_source.source());
            let file_id = file.file_id(self);

            // Register in file_id_to_path for diagnostic filename display
            // and in file_map so builtins are included in check() diagnostics.
            // Builtin signatures use fully qualified type names (e.g., baml.http.Request)
            // which resolve through the builtin class_names registry.
            self.file_id_to_path.insert(file_id, path.clone());
            self.file_map.insert(path, file);

            builtin_files.push(file);
        }

        builtin_files
    }

    /// Add a file to the database.
    ///
    /// This is an alias for `add_or_update_file` for API compatibility.
    pub fn add_file(&mut self, path: impl AsRef<std::path::Path>, content: &str) -> SourceFile {
        self.add_or_update_file(path.as_ref(), content)
    }

    /// Get all files currently in the database.
    pub fn files(&self) -> impl Iterator<Item = SourceFile> + '_ {
        self.file_map.values().copied()
    }

    /// Get all file paths currently tracked by the database.
    pub fn non_builtin_file_paths(&self) -> impl Iterator<Item = std::path::PathBuf> {
        self.file_map
            .keys()
            .filter(|path| !path.starts_with(baml_builtins::BUILTIN_PATH_PREFIX))
            .cloned()
    }

    /// Get a `SourceFile` by its path.
    pub fn get_file(&self, path: &std::path::Path) -> Option<SourceFile> {
        let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.file_map.get(&canonical_path).copied()
    }

    /// Get a `FileId` by its path.
    pub fn path_to_file_id(&self, path: &std::path::Path) -> Option<FileId> {
        self.get_file(path).map(|file| file.file_id(self))
    }

    /// Get the file path for a `FileId`.
    pub fn get_path(&self, file_id: FileId) -> Option<&std::path::Path> {
        self.file_id_to_path
            .get(&file_id)
            .map(std::path::PathBuf::as_path)
    }

    /// Get a `SourceFile` by its `FileId`.
    pub fn get_file_by_id(&self, file_id: FileId) -> Option<SourceFile> {
        self.file_id_to_path
            .get(&file_id)
            .and_then(|path| self.file_map.get(path).copied())
    }

    /// Get the compiled bytecode for the project.
    pub fn get_bytecode(&self) -> Result<bex_vm_types::Program, baml_compiler_emit::LoweringError> {
        // First ensure no diagnostics errors are present
        let diagnostics = self.check();
        if diagnostics
            .diagnostics
            .iter()
            .any(|diag| diag.severity == baml_compiler_diagnostics::Severity::Error)
        {
            return Err(baml_compiler_emit::LoweringError::HasDiagnosticsErrors);
        }
        let opts = CompileOptions {
            emit_test_cases: false,
        };
        baml_compiler_emit::generate_project_bytecode(self, &opts)
    }
}

impl Default for ProjectDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ProjectDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectDatabase")
            .field("file_count", &self.file_map.len())
            .field("has_project", &self.project.is_some())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_file() {
        let mut db = ProjectDatabase::new();
        let path = std::path::Path::new("/tmp/test.baml");
        let content = "class Foo { name string }";

        let file = db.add_or_update_file(path, content);
        assert_eq!(file.text(&db), content);
    }

    #[test]
    fn test_update_file() {
        let mut db = ProjectDatabase::new();
        let path = std::path::Path::new("/tmp/test.baml");

        let file1 = db.add_or_update_file(path, "class Foo {}");
        let file2 = db.add_or_update_file(path, "class Bar {}");

        // Should be the same file handle
        assert_eq!(file1.file_id(&db), file2.file_id(&db));
        // Content should be updated
        assert_eq!(file1.text(&db), "class Bar {}");
    }

    #[test]
    fn test_set_project_root() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));

        assert!(db.get_project().is_some());
    }
}
