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

use baml_db::{FileId, SourceFile};
use baml_workspace::{Compiler2ExtraFiles, Project};
use salsa::Setter;

/// Context about what the cursor is pointing at, for playground navigation.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorContext {
    pub function_name: Option<String>,
    pub is_workflow: bool,
    pub workflow_memberships: Vec<String>,
    /// Raw ExprId index — matched against node.metadata.sourceExpr on the TS side.
    /// NOT a CFG NodeId. The TS side scans the cached graph for a node whose
    /// sourceExpr matches this value.
    pub source_expr_id: Option<u32>,
    /// Ordered list of expression IDs containing the cursor, from most
    /// specific (smallest span) to least specific (largest span). The TS
    /// side tries each one in order, highlighting the first that matches a
    /// CFG node. This gives "closest ancestor" behavior — e.g. cursor on a
    /// variable inside a call highlights the call; cursor on `if` keyword
    /// highlights the branch group.
    #[serde(default)]
    pub source_expr_candidates: Vec<u32>,
    pub test_name: Option<String>,
}

// Note: Builtin BAML files (like llm.baml) are loaded in set_project_root().
// The paths are defined in `baml_builtins2`.

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
    /// they are NOT added to `project.files()`.
    compiler2_extra_files: Option<Compiler2ExtraFiles>,
    /// Maps file paths to their `SourceFile` handles (user files only).
    file_map: HashMap<std::path::PathBuf, SourceFile>,
    /// Maps file paths to compiler2-only `SourceFile` handles.
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
impl baml_compiler2_ppir::Db for ProjectDatabase {}

#[salsa::db]
impl baml_compiler2_tir::Db for ProjectDatabase {}

#[salsa::db]
impl baml_compiler2_mir::Db for ProjectDatabase {}

#[salsa::db]
impl baml_compiler2_emit::Db for ProjectDatabase {}

#[salsa::db]
impl baml_lsp2_actions::Db for ProjectDatabase {}

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
    /// This also loads compiler2 builtin BAML files into the compiler2 extra files slot.
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

        // Load compiler2 builtin stub files.
        let v2_builtin_files = self.load_builtin_baml_files();

        // Create and set the project (user files only, no builtins in project.files())
        let project = Project::new(self, canonical_root, user_files);
        self.project = Some(project);

        // Create the compiler2 extra files Salsa input (separate from project.files)
        let compiler2_extra = Compiler2ExtraFiles::new(self, v2_builtin_files);
        self.compiler2_extra_files = Some(compiler2_extra);

        project
    }

    /// Load compiler2 builtin BAML source files into the database.
    ///
    /// Returns the list of compiler2 builtin stub files (Array<T>, Map<K,V>, String,
    /// Media, baml.env, baml.http, baml.math, baml.sys namespaces, etc.).
    ///
    /// These are stored in `compiler2_file_map` (NOT `file_map`) so that
    /// `get_source_files()` does NOT return them.
    fn load_builtin_baml_files(&mut self) -> Vec<SourceFile> {
        let mut v2_builtin_files = Vec::new();
        for builtin in baml_builtins2::ALL {
            let virtual_path = builtin.virtual_path();
            let path = PathBuf::from(&virtual_path);
            let file = self.add_file_internal(&path, builtin.contents);
            let file_id = file.file_id(self);

            self.file_id_to_path.insert(file_id, path.clone());
            self.compiler2_file_map.insert(path, file);

            v2_builtin_files.push(file);
        }

        v2_builtin_files
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
            .filter(|path| !path.starts_with("<builtin>"))
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

    /// Get the compiled bytecode for the project using the compiler2 pipeline.
    pub fn get_bytecode(
        &self,
    ) -> Result<bex_vm_types::Program, baml_compiler2_emit::LoweringError> {
        let opts = baml_compiler2_emit::CompileOptions {
            emit_test_cases: false,
        };
        baml_compiler2_emit::generate_project_bytecode(self, &opts)
    }

    /// Build a control flow graph for the given function using the compiler2 AST builder.
    ///
    /// More error-resilient than `control_flow_graph()` — works even when code has type errors,
    /// because it builds directly from the AST using `Missing` sentinels for unresolved nodes.
    /// Suitable for the playground which must function during editing.
    pub fn ast_control_flow_graph(
        &self,
        function_name: &str,
    ) -> Option<baml_compiler2_visualization::control_flow::ControlFlowGraph> {
        use baml_compiler2_visualization::control_flow::{
            build_control_flow_graph_from_ast, build_llm_control_flow_graph,
        };

        for source_file in self.file_map.values().copied() {
            let index = baml_compiler2_ppir::file_semantic_index(self, source_file);
            for (local_id, func_data) in index.item_tree.functions.iter() {
                let func_name = func_data.name.to_string();
                if func_name != function_name {
                    continue;
                }

                let func_loc =
                    baml_compiler2_hir::loc::FunctionLoc::new(self, source_file, *local_id);
                let body = baml_compiler2_hir::body::function_body(self, func_loc);

                // Check if this is an LLM function via declarative_meta (not body variant,
                // since compiler2 desugars LLM functions to Expr bodies).
                let is_llm = matches!(
                    func_data.declarative_meta,
                    Some(baml_compiler2_ast::ast::DeclarativeMeta::Llm(_))
                );

                return if is_llm {
                    let client_name =
                        if let Some(baml_compiler2_ast::ast::DeclarativeMeta::Llm(ref llm)) =
                            func_data.declarative_meta
                        {
                            llm.client
                                .as_ref()
                                .map(|c: &baml_db::Name| c.to_string())
                                .unwrap_or_else(|| "unknown".to_string())
                        } else {
                            "unknown".to_string()
                        };
                    Some(build_llm_control_flow_graph(function_name, &client_name))
                } else {
                    match body.as_ref() {
                        baml_compiler2_hir::body::FunctionBody::Expr(expr_body) => {
                            let graph = build_control_flow_graph_from_ast(function_name, expr_body);
                            Some(graph)
                        }
                        baml_compiler2_hir::body::FunctionBody::Builtin(_)
                        | baml_compiler2_hir::body::FunctionBody::Missing => None,
                    }
                };
            }
        }

        None
    }

    /// Given a file path and byte offset, return context about what entity
    /// the cursor is on — used by the playground for navigation.
    pub fn playground_cursor_context(&self, file_path: &str, byte_offset: u32) -> CursorContext {
        let empty = CursorContext {
            function_name: None,
            is_workflow: false,
            workflow_memberships: vec![],
            source_expr_id: None,
            source_expr_candidates: vec![],
            test_name: None,
        };

        // 1. Find the SourceFile matching file_path
        let source_file = match self.find_source_file(file_path) {
            Some(sf) => sf,
            None => return empty,
        };

        let offset = text_size::TextSize::from(byte_offset);

        // 2. Find CST token at offset
        let token = match baml_lsp2_actions::utils::find_token_at_offset(self, source_file, offset)
        {
            Some(tok) => tok,
            None => return empty,
        };

        use baml_db::baml_compiler_syntax::SyntaxKind;

        // 3. Check if cursor is inside a HEADER_COMMENT node — these need
        //    special handling since their tokens don't resolve via name lookup.
        if let Some(parent) = token.parent() {
            if parent
                .ancestors()
                .any(|n| n.kind() == SyntaxKind::HEADER_COMMENT)
            {
                return self.cursor_context_positional(source_file, offset);
            }
        }

        // 4. For WORD tokens, try name resolution first (handles function
        //    definitions, call sites, local variables).
        if token.kind() == SyntaxKind::WORD {
            let name = baml_db::Name::from(token.text().to_string());

            let resolved =
                baml_compiler2_tir::resolve::resolve_name_at(self, source_file, offset, &name);

            match resolved {
                baml_compiler2_tir::resolve::ResolvedName::Item(def)
                | baml_compiler2_tir::resolve::ResolvedName::Builtin(def) => {
                    use baml_compiler2_hir::contributions::Definition;
                    match &def {
                        Definition::Function(_) => {
                            return self.cursor_context_for_definition(source_file, offset, def);
                        }
                        _ => {
                            // Non-function items (class, enum, etc.) used inside
                            // a function body — fall through to positional so we
                            // can still highlight the enclosing graph node.
                        }
                    }
                }
                baml_compiler2_tir::resolve::ResolvedName::Local { .. } => {
                    return self.cursor_context_for_local(source_file, offset);
                }
                baml_compiler2_tir::resolve::ResolvedName::Unknown => {
                    // Fall through to positional fallback below
                }
            }
        }

        // 5. Positional fallback for non-WORD tokens (keywords like `if`,
        //    `match`, `return`, operators, punctuation), unresolved WORDs,
        //    and non-function item references (class names in return types, etc.).
        self.cursor_context_positional(source_file, offset)
    }

    /// Build cursor context purely from position — no name resolution.
    /// Used for keywords, operators, punctuation, header comments, and
    /// any token that doesn't resolve through the name-lookup path.
    fn cursor_context_positional(
        &self,
        source_file: SourceFile,
        offset: text_size::TextSize,
    ) -> CursorContext {
        let (func_name, is_workflow) = match self.find_enclosing_function(source_file, offset) {
            Some((name, workflow)) => (Some(name), workflow),
            None => (None, false),
        };

        let workflow_memberships = func_name
            .as_ref()
            .map(|n| self.find_workflow_memberships(n))
            .unwrap_or_default();

        let (source_expr_id, source_expr_candidates) =
            self.find_source_expr_ids_at(source_file, offset);

        CursorContext {
            function_name: func_name,
            is_workflow,
            workflow_memberships,
            source_expr_id,
            source_expr_candidates,
            test_name: None,
        }
    }

    /// Build cursor context when the cursor resolved to a top-level Definition.
    fn cursor_context_for_definition(
        &self,
        source_file: SourceFile,
        offset: text_size::TextSize,
        def: baml_compiler2_hir::contributions::Definition<'_>,
    ) -> CursorContext {
        use baml_compiler2_hir::contributions::Definition;

        match def {
            Definition::Function(func_loc) => {
                let sig = baml_compiler2_hir::signature::function_signature(self, func_loc);
                let func_name = sig.name.to_string();
                let body = baml_compiler2_hir::body::function_body(self, func_loc);
                let is_workflow = matches!(
                    body.as_ref(),
                    baml_compiler2_hir::body::FunctionBody::Expr(_)
                );

                // Find which workflows call this function
                let workflow_memberships = self.find_workflow_memberships(&func_name);

                // Find source_expr_id if cursor is inside a function body
                let (source_expr_id, source_expr_candidates) =
                    self.find_source_expr_ids_at(source_file, offset);

                CursorContext {
                    function_name: Some(func_name),
                    is_workflow,
                    workflow_memberships,
                    source_expr_id,
                    source_expr_candidates,
                    test_name: None,
                }
            }
            _ => {
                // For classes, enums, etc. - no meaningful playground navigation
                CursorContext {
                    function_name: None,
                    is_workflow: false,
                    workflow_memberships: vec![],
                    source_expr_id: None,
                    source_expr_candidates: vec![],
                    test_name: None,
                }
            }
        }
    }

    /// Build cursor context when the cursor resolved to a local variable.
    /// We look up the enclosing function to provide context.
    fn cursor_context_for_local(
        &self,
        source_file: SourceFile,
        offset: text_size::TextSize,
    ) -> CursorContext {
        let (func_name, is_workflow) = match self.find_enclosing_function(source_file, offset) {
            Some((name, workflow)) => (Some(name), workflow),
            None => (None, false),
        };

        let workflow_memberships = func_name
            .as_ref()
            .map(|n| self.find_workflow_memberships(n))
            .unwrap_or_default();

        let (source_expr_id, source_expr_candidates) =
            self.find_source_expr_ids_at(source_file, offset);

        CursorContext {
            function_name: func_name,
            is_workflow,
            workflow_memberships,
            source_expr_id,
            source_expr_candidates,
            test_name: None,
        }
    }

    /// Find a SourceFile by file path (matches by suffix to handle different path formats).
    pub fn find_source_file(&self, file_path: &str) -> Option<SourceFile> {
        // Try exact match first
        let path = PathBuf::from(file_path);
        if let Some(&sf) = self.file_map.get(&path) {
            return Some(sf);
        }
        // Also check compiler2_file_map
        if let Some(&sf) = self.compiler2_file_map.get(&path) {
            return Some(sf);
        }
        // Fallback: match by file name suffix (handles Monaco's relative paths)
        for (&ref stored_path, &sf) in &self.file_map {
            if stored_path.ends_with(file_path)
                || file_path.ends_with(stored_path.to_string_lossy().as_ref())
            {
                return Some(sf);
            }
        }
        None
    }

    /// Find the enclosing function name and whether it's a workflow, given a cursor position.
    fn find_enclosing_function(
        &self,
        source_file: SourceFile,
        offset: text_size::TextSize,
    ) -> Option<(String, bool)> {
        use baml_compiler2_hir::scope::ScopeKind;

        let index = baml_compiler2_ppir::file_semantic_index(self, source_file);
        let scope_id = index.scope_at_offset(offset, None);
        let ancestors = index.ancestor_scopes(scope_id);

        // Find the innermost Function scope
        let func_scope_id = ancestors.iter().find(|&&ancestor_id| {
            let scope = &index.scopes[ancestor_id.index() as usize];
            matches!(scope.kind, ScopeKind::Function)
        })?;

        let func_scope_range = index.scopes[func_scope_id.index() as usize].range;

        // Match against item tree functions by span
        let item_tree = &index.item_tree;
        for (local_id, func_data) in item_tree.functions.iter() {
            if func_data.span == func_scope_range {
                let func_loc =
                    baml_compiler2_hir::loc::FunctionLoc::new(self, source_file, *local_id);
                let sig = baml_compiler2_hir::signature::function_signature(self, func_loc);
                let body = baml_compiler2_hir::body::function_body(self, func_loc);
                let is_workflow = matches!(
                    body.as_ref(),
                    baml_compiler2_hir::body::FunctionBody::Expr(_)
                );
                return Some((sig.name.to_string(), is_workflow));
            }
        }
        None
    }

    /// Find workflows that call the given function by scanning all function bodies.
    fn find_workflow_memberships(&self, target_function_name: &str) -> Vec<String> {
        let mut memberships = Vec::new();

        for source_file in self.file_map.values().copied() {
            let index = baml_compiler2_ppir::file_semantic_index(self, source_file);
            for (local_id, func_data) in index.item_tree.functions.iter() {
                let func_name = func_data.name.to_string();
                if func_name == target_function_name {
                    continue; // Skip self
                }

                let func_loc =
                    baml_compiler2_hir::loc::FunctionLoc::new(self, source_file, *local_id);
                let body = baml_compiler2_hir::body::function_body(self, func_loc);

                // Only workflow (Expr) functions can call other functions
                if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() {
                    if self.expr_body_calls_function(expr_body, target_function_name) {
                        memberships.push(func_name);
                    }
                }
            }
        }

        memberships
    }

    /// Check if an expression body contains a call to a function with the given name.
    fn expr_body_calls_function(
        &self,
        body: &baml_compiler2_ast::ExprBody,
        target_name: &str,
    ) -> bool {
        use baml_compiler2_ast::Expr;
        for (_id, expr) in body.exprs.iter() {
            if let Expr::Call { callee, .. } = expr {
                // Check if the callee is a Path containing the target name
                if let Expr::Path(segments) = &body.exprs[*callee] {
                    if segments.len() == 1 && segments[0].as_str() == target_name {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// O(1) lookup of an ExprId's span in the source map. Returns the raw
    /// index and span length as a `(u32, TextSize)` pair for the candidate list.
    fn expr_span_entry(
        &self,
        source_map: &baml_compiler2_ast::AstSourceMap,
        expr_id: baml_compiler2_ast::ExprId,
    ) -> Option<(u32, text_size::TextSize)> {
        let raw = expr_id.into_raw();
        let idx = raw.into_u32() as usize;
        if idx >= source_map.expr_spans.len() {
            return None;
        }
        let span_idx = la_arena::Idx::<text_size::TextRange>::from_raw(raw);
        let range = &source_map.expr_spans[span_idx];
        Some((raw.into_u32(), range.len()))
    }

    /// Find source expression candidates at the cursor offset.
    ///
    /// Returns `(best, candidates)` where `best` is backward-compatible
    /// (the smallest expression) and `candidates` is the full list of
    /// containing expression IDs sorted smallest-first. Headers (tagged
    /// with the high bit) are inserted at the front when present.
    fn find_source_expr_ids_at(
        &self,
        source_file: SourceFile,
        offset: text_size::TextSize,
    ) -> (Option<u32>, Vec<u32>) {
        use baml_compiler2_hir::scope::ScopeKind;

        let index = baml_compiler2_ppir::file_semantic_index(self, source_file);
        let scope_id = index.scope_at_offset(offset, None);
        let ancestors = index.ancestor_scopes(scope_id);

        let func_scope_id = match ancestors.iter().find(|&&ancestor_id| {
            let scope = &index.scopes[ancestor_id.index() as usize];
            matches!(scope.kind, ScopeKind::Function)
        }) {
            Some(id) => id,
            None => return (None, vec![]),
        };

        let func_scope_range = index.scopes[func_scope_id.index() as usize].range;

        let item_tree = &index.item_tree;
        for (local_id, func_data) in item_tree.functions.iter() {
            if func_data.span != func_scope_range {
                continue;
            }

            let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(self, source_file, *local_id);
            let source_map =
                match baml_compiler2_hir::body::function_body_source_map(self, func_loc) {
                    Some(sm) => sm,
                    None => return (None, vec![]),
                };
            let body = baml_compiler2_hir::body::function_body(self, func_loc);
            let expr_body = match body.as_ref() {
                baml_compiler2_hir::body::FunctionBody::Expr(eb) => Some(eb),
                _ => None,
            };

            // Collect ALL expression spans containing the cursor.
            let mut containing: Vec<(u32, text_size::TextSize)> = Vec::new();
            for (idx, (_id, range)) in source_map.expr_spans.iter().enumerate() {
                if range.contains(offset) || range.end() == offset {
                    containing.push((idx as u32, range.len()));
                }
            }

            // For each statement span containing the cursor, inject the
            // statement's "graph-relevant expression" into the candidate list.
            // This maps the whole `let x = Call(...)` or `return Obj {...}`
            // line to the expression the graph node uses as source_expr.
            if let Some(eb) = expr_body {
                for (idx, (_id, range)) in source_map.stmt_spans.iter().enumerate() {
                    if !(range.contains(offset) || range.end() == offset) {
                        continue;
                    }
                    let stmt_id = la_arena::Idx::<baml_compiler2_ast::Stmt>::from_raw(
                        la_arena::RawIdx::from_u32(idx as u32),
                    );
                    // Look up the expression the graph node uses as source_expr
                    // for this statement, and inject it into the candidate list.
                    let injected_expr = match &eb.stmts[stmt_id] {
                        baml_compiler2_ast::Stmt::HeaderComment { .. } => {
                            let tagged =
                                baml_compiler2_visualization::control_flow::STMT_SOURCE_EXPR_TAG
                                    | idx as u32;
                            Some((tagged, range.len()))
                        }
                        baml_compiler2_ast::Stmt::Let {
                            initializer: Some(init),
                            ..
                        } => self.expr_span_entry(&source_map, *init),
                        baml_compiler2_ast::Stmt::Return(Some(expr_id)) => {
                            self.expr_span_entry(&source_map, *expr_id)
                        }
                        baml_compiler2_ast::Stmt::Expr(expr_id) => {
                            self.expr_span_entry(&source_map, *expr_id)
                        }
                        _ => None,
                    };
                    if let Some(entry) = injected_expr {
                        containing.push(entry);
                    }
                }
            }

            // Sort smallest-first and deduplicate so the TS side tries
            // the most specific expression first.
            containing.sort_by_key(|&(_, len)| len);
            let mut seen = std::collections::HashSet::new();
            let candidates: Vec<u32> = containing
                .iter()
                .filter_map(|&(id, _)| if seen.insert(id) { Some(id) } else { None })
                .collect();

            let best = candidates.first().copied();
            return (best, candidates);
        }

        (None, vec![])
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
    fn test_ast_control_flow_graph_with_headers() {
        use baml_compiler2_visualization::control_flow::NodeType;

        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        // Use header comments (//#) inside the function body — these produce
        // HeaderContextEnter nodes which survive the flattening pipeline.
        db.add_or_update_file(
            std::path::Path::new("/tmp/workflow.baml"),
            r#"
function Workflow(input: string) -> string {
    //# Prepare
    let x = input;
    //# Process
    if (true) { x } else { "fallback" }
}
"#,
        );

        let graph = db.ast_control_flow_graph("Workflow");
        assert!(
            graph.is_some(),
            "ast_control_flow_graph should return a graph for a known function"
        );
        let graph = graph.unwrap();

        // Should have at least: FunctionRoot + two HeaderContextEnter nodes.
        assert!(
            graph.nodes.len() >= 3,
            "expected at least 3 nodes (root + 2 headers), got {}",
            graph.nodes.len()
        );

        // The root node should have FunctionRoot type.
        let root = graph.nodes.values().next().unwrap();
        assert!(
            matches!(root.node_type, NodeType::FunctionRoot),
            "first node should be FunctionRoot, got {:?}",
            root.node_type
        );

        // There should be at least two HeaderContextEnter nodes.
        let header_count = graph
            .nodes
            .values()
            .filter(|n| matches!(n.node_type, NodeType::HeaderContextEnter))
            .count();
        assert!(
            header_count >= 2,
            "expected at least 2 HeaderContextEnter nodes, got {header_count}"
        );

        // Edges should be non-empty.
        assert!(!graph.edges_by_src.is_empty(), "graph should have edges");
    }

    #[test]
    fn test_ast_control_flow_graph_not_found() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        db.add_or_update_file(
            std::path::Path::new("/tmp/test.baml"),
            r#"function Simple(x: int) -> int { x + 1 }"#,
        );

        // Non-existent function should return None.
        let graph = db.ast_control_flow_graph("DoesNotExist");
        assert!(graph.is_none(), "should return None for unknown function");
    }

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
