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
use baml_workspace::{Compiler2ExtraFiles, Project};
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
    /// Compiler2-only extra files (`baml_builtins2` stubs). Held separately so
    /// they are NOT added to `project.files()` — the v1 compiler must not see
    /// them because it cannot parse compiler2-specific syntax.
    compiler2_extra_files: Option<Compiler2ExtraFiles>,
    /// Maps file paths to their `SourceFile` handles (user files + v1 builtins only).
    /// v2 builtin stubs are stored in `compiler2_file_map` instead to prevent them
    /// from appearing in `get_source_files()` which feeds the v1 compiler pipeline.
    file_map: HashMap<std::path::PathBuf, SourceFile>,
    /// Maps file paths to compiler2-only `SourceFile` handles.
    /// These files are NOT returned by `get_source_files()`.
    compiler2_file_map: HashMap<std::path::PathBuf, SourceFile>,
    /// Maps `FileId` to file path for reverse lookup (all files including v2 stubs).
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
impl baml_compiler2_hir::Db for ProjectDatabase {
    fn compiler2_extra_files(&self) -> Option<baml_workspace::Compiler2ExtraFiles> {
        self.compiler2_extra_files
    }
}

#[salsa::db]
impl baml_compiler2_tir::Db for ProjectDatabase {}

#[salsa::db]
impl baml_lsp2_actions::Db for ProjectDatabase {}

#[salsa::db]
impl baml_compiler_ppir::Db for ProjectDatabase {}

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
            compiler2_extra_files: None,
            file_map: HashMap::new(),
            compiler2_file_map: HashMap::new(),
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
            compiler2_extra_files: None,
            file_map: HashMap::new(),
            compiler2_file_map: HashMap::new(),
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

        // Load v1 builtin BAML files (for the shared project.files() list).
        // v2 builtin stubs are loaded separately into compiler2_extra_files.
        let (v1_builtin_files, v2_builtin_files) = self.load_builtin_baml_files();

        // Combine user files with v1 builtin files (user first, then builtins)
        let mut all_files = user_files;
        all_files.extend(v1_builtin_files);

        // Create and set the project (v1 compiler only sees this)
        let project = Project::new(self, canonical_root, all_files);
        self.project = Some(project);

        // Create the compiler2 extra files Salsa input (separate from project.files)
        let compiler2_extra = Compiler2ExtraFiles::new(self, v2_builtin_files);
        self.compiler2_extra_files = Some(compiler2_extra);

        project
    }

    /// Load builtin BAML source files into the database.
    ///
    /// Returns two lists:
    /// - `(v1_files, v2_files)` where `v1_files` are for the shared `project.files()`
    ///   (visible to both compilers) and `v2_files` are compiler2-only stubs that must
    ///   NOT be added to `project.files()` because the v1 parser cannot handle
    ///   compiler2-specific syntax (generic type parameters, `$rust_type`, etc.).
    ///
    /// ## Note on goto-definition
    ///
    /// Builtin files use virtual paths like `<builtin>/baml/llm.baml`. These paths
    /// are embedded in the compiler binary, not present on the user's filesystem.
    /// As a result, goto-definition to builtins won't work in editors.
    fn load_builtin_baml_files(&mut self) -> (Vec<SourceFile>, Vec<SourceFile>) {
        let mut v1_builtin_files = Vec::new();

        // Load all v1 builtin BAML sources (disk read on native, embedded on WASM)
        for builtin_source in baml_builtins::baml_sources() {
            let path = PathBuf::from(builtin_source.path);
            let file = self.add_file_internal(&path, builtin_source.source());
            let file_id = file.file_id(self);

            // Register in file_id_to_path for diagnostic filename display
            // and in file_map so builtins are included in check() diagnostics.
            self.file_id_to_path.insert(file_id, path.clone());
            self.file_map.insert(path, file);

            v1_builtin_files.push(file);
        }

        // Load compiler2-only builtin stub files (Array<T>, Map<K,V>, String, Media, etc.)
        // These flow through the compiler2 HIR pipeline: package_items(db, "baml")
        // will contain Array, Map, String, Media, and the baml.env / baml.http /
        // baml.math / baml.sys namespaces.
        //
        // IMPORTANT: These are stored in `compiler2_file_map` (NOT `file_map`) so
        // that `get_source_files()` does NOT return them and they are never passed
        // to the v1 parser. The v1 parser cannot handle compiler2-specific syntax:
        // generic type parameters, `$rust_type`, void functions without explicit
        // return types, `root.sys.xxx` qualified calls, etc.
        let mut v2_builtin_files = Vec::new();
        for builtin in baml_builtins2::ALL {
            // Use the BuiltinFile's virtual_path() to get the correct path.
            // Root files: "<builtin>/baml/containers.baml"
            // Namespaced files: "<builtin>/baml/env/env.baml"
            let virtual_path = builtin.virtual_path();
            let path = PathBuf::from(&virtual_path);
            let file = self.add_file_internal(&path, builtin.contents);
            let file_id = file.file_id(self);

            // Register in file_id_to_path for diagnostic filename display
            // but in compiler2_file_map (not file_map!) so the v1 compiler
            // never sees these files.
            self.file_id_to_path.insert(file_id, path.clone());
            self.compiler2_file_map.insert(path, file);

            v2_builtin_files.push(file);
        }

        (v1_builtin_files, v2_builtin_files)
    }

    /// Register synthetic stream-expansion files in the reverse-lookup maps.
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
        self.file_id_to_path.get(&file_id).and_then(|path| {
            self.file_map
                .get(path)
                .or_else(|| self.compiler2_file_map.get(path))
                .copied()
        })
    }

    /// Build the control flow visualization graph for a function.
    ///
    /// Returns `None` if the function is not found, is compiler-generated
    /// (`render_prompt`, `build_request`, `client_resolve`), or has errors that
    /// prevent VIR lowering.
    pub fn control_flow_graph(
        &self,
        function_name: &str,
    ) -> Option<baml_compiler_vir::control_flow::ControlFlowGraph> {
        use baml_compiler_hir::{
            FunctionBody, ItemId, file_item_tree, file_items, function_body, function_signature,
            function_signature_source_map,
        };
        use baml_compiler_tir::{
            class_field_types, enum_variants, infer_function, type_aliases, typing_context,
        };
        use baml_compiler_vir::control_flow::{
            build_control_flow_graph, build_llm_control_flow_graph,
        };

        let project = self.project?;
        let files = project.files(self);

        // Build typing context lazily (only if we find an expr function)
        let mut typing_ctx = None;

        for source_file in files {
            let items_struct = file_items(self, *source_file);
            for item in items_struct.items(self) {
                let ItemId::Function(func_loc) = item else {
                    continue;
                };
                let item_tree = file_item_tree(self, func_loc.file(self));
                let func = &item_tree[func_loc.id(self)];

                // Skip compiler-generated functions
                if let Some(ref cg) = func.compiler_generated {
                    use baml_compiler_hir::CompilerGenerated;
                    match cg {
                        CompilerGenerated::ClientResolve { .. }
                        | CompilerGenerated::LlmRenderPrompt { .. }
                        | CompilerGenerated::LlmBuildRequest { .. } => continue,
                        CompilerGenerated::LlmCall { .. } => {
                            // LlmCall functions have an expr body that wraps the LLM call.
                            // We can still build a control flow graph for them.
                        }
                    }
                }

                let sig = function_signature(self, *func_loc);
                if sig.name != function_name {
                    continue;
                }

                // Found the function — check body type
                let body = function_body(self, *func_loc);
                match body.as_ref() {
                    FunctionBody::Llm(llm_body) => {
                        return Some(build_llm_control_flow_graph(
                            function_name,
                            llm_body.client.as_ref(),
                        ));
                    }
                    FunctionBody::Expr(_, _) => {
                        // Lazy-init typing context
                        let ctx = typing_ctx.get_or_insert_with(|| {
                            let globals = typing_context(self, project).functions(self).clone();
                            let class_fields =
                                class_field_types(self, project).classes(self).clone();
                            let ta = type_aliases(self, project).aliases(self).clone();
                            let recursive = baml_compiler_tir::find_recursive_aliases(&ta);
                            let ev = enum_variants(self, project).enums(self).clone();
                            let resolution_ctx =
                                baml_compiler_tir::TypeResolutionContext::new(self, project);
                            (globals, class_fields, ta, recursive, ev, resolution_ctx)
                        });

                        let sig_source_map = function_signature_source_map(self, *func_loc);
                        let inference = infer_function(
                            self,
                            &sig,
                            Some(&sig_source_map),
                            &body,
                            Some(ctx.0.clone()),
                            Some(ctx.1.clone()),
                            Some(ctx.2.clone()),
                            Some(ctx.4.clone()),
                            *func_loc,
                        );

                        match baml_compiler_vir::lower_from_hir(
                            &body, &inference, &ctx.5, &ctx.2, &ctx.3,
                        ) {
                            Ok(vir_body) => {
                                return Some(build_control_flow_graph(function_name, &vir_body));
                            }
                            Err(baml_compiler_vir::LoweringError::LlmFunction) => {
                                // Shouldn't happen since we check FunctionBody first,
                                // but handle gracefully
                                return None;
                            }
                            Err(_) => return None,
                        }
                    }
                    FunctionBody::Missing => return None,
                }
            }
        }

        None
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
