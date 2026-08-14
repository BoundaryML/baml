//! `ProjectDatabase` - the main database for BAML projects.
//!
//! This module provides `ProjectDatabase`, which owns the Salsa storage directly
//! (following the ty/ruff pattern) and implements all the compiler `Db` traits.
//!
//! Unlike the previous `LspDatabase` which wrapped `RootDatabase`, `ProjectDatabase`
//! has direct ownership of the storage, removing a layer of indirection.

use std::{
    collections::{HashMap, HashSet},
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
    /// Raw `ExprId` index — matched against `node.metadata.sourceExpr` on the TS side.
    /// NOT a CFG `NodeId`. The TS side scans the cached graph for a node whose
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
    /// Function body that owns `source_expr_id` / `source_expr_candidates`.
    ///
    /// This differs from `function_name` at call sites: the token resolves to
    /// the callee, but the expression span belongs to the caller.
    #[serde(default)]
    pub source_expr_function_name: Option<String>,
    pub test_name: Option<String>,
    /// Byte offset of the cursor position in the source file.
    /// Used for cursor ↔ event matching via span containment.
    #[serde(default)]
    pub cursor_offset: Option<u32>,
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
/// Cap on the total node count of a fully-inlined control-flow graph. Callee
/// graphs are copied into every call site, so an uncapped graph grows as
/// `fan_out^depth` on deep call chains; once the budget is reached remaining
/// calls stay plain call nodes.
const CFG_EXPANSION_NODE_BUDGET: usize = 5_000;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CfgExpansionCacheKey {
    callee_name: String,
    active_expansions: Vec<String>,
}

/// State threaded through one top-level [`ProjectDatabase::ast_control_flow_graph`] build.
#[derive(Default)]
struct CfgExpansionCtx {
    /// Functions currently being expanded (cycle guard).
    expanding: HashSet<String>,
    /// Fully-expanded callee graphs keyed by the active expansion context.
    /// `None` records callees with no buildable graph so they are not retried
    /// per equivalent site.
    cache: HashMap<
        CfgExpansionCacheKey,
        Option<std::sync::Arc<baml_compiler2_visualization::control_flow::ControlFlowGraph>>,
    >,
}

type CfgDispatchBindings = HashMap<String, baml_type::Ty>;

enum CfgCallTarget<'db> {
    Function {
        loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
        display_name: String,
        dispatch_bindings: CfgDispatchBindings,
    },
    UnresolvedName(String),
}

impl CfgExpansionCtx {
    fn cache_key(&self, callee_name: String) -> CfgExpansionCacheKey {
        // The recursion guard depends on membership in `expanding`, not call
        // order, so a sorted active set is the safe memoization context.
        let mut active_expansions = self.expanding.iter().cloned().collect::<Vec<_>>();
        active_expansions.sort();
        CfgExpansionCacheKey {
            callee_name,
            active_expansions,
        }
    }
}

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
    /// Per-file throw facts seeded from a previous compile (bytecode cache).
    ///
    /// This is a real `#[salsa::input]` handle, created **once** (empty) in
    /// `ProjectDatabase::new` and thereafter mutated *in place* by
    /// `set_seeded_throw_facts` via its Salsa setter. It is therefore always
    /// `Some` for a `ProjectDatabase`. Keeping the input present from
    /// construction is what makes `throw_inference::file_throw_facts` read the
    /// seed map through a **tracked** dependency: were the handle absent until
    /// the first seed, a query memoized while it was `None` would record no
    /// dependency and a later seed on a reused database would be invisible to
    /// the memo. Mutating via the setter bumps the revision and correctly
    /// invalidates dependents.
    seeded_throw_facts: Option<baml_workspace::SeededThrowFacts>,
    /// Stdlib packages' typed interfaces seeded from a previous compile
    /// (bytecode cache).
    ///
    /// Same present-from-construction discipline as `seeded_throw_facts` above: a
    /// real `#[salsa::input]` handle created **once** (empty) in the constructors
    /// and thereafter mutated in place via its Salsa setter, so it is always
    /// `Some` and `package_interface::package_interface` reads the seed through a
    /// **tracked** dependency (an absent-then-added handle would leave a stale
    /// memo on a reused database, e.g. the LSP's long-lived `ProjectDatabase`).
    seeded_stdlib_interface: Option<baml_workspace::SeededStdlibInterface>,
    /// Per-function `callable_throws` values seeded from a previous compile
    /// (bytecode cache).
    ///
    /// Same present-from-construction discipline as `seeded_throw_facts` and
    /// `seeded_stdlib_interface` above: a real `#[salsa::input]` handle created
    /// **once** (empty) in the constructors and thereafter mutated in place via
    /// its Salsa setter, so it is always `Some` and `callable::callable_throws`
    /// reads the seed through a **tracked** dependency (an absent-then-added
    /// handle would leave a stale memo on a reused database, e.g. the LSP's
    /// long-lived `ProjectDatabase`).
    seeded_callable_throws: Option<baml_workspace::SeededCallableThrows>,
    /// Maps file paths to their `SourceFile` handles (user files only).
    ///
    /// `Arc`-wrapped (with `Arc::make_mut` at the mutation sites) so cloning a
    /// database handle stays O(1): the parallel check and emit drivers mint a
    /// shared-storage handle per work chunk, and a deep per-clone copy of an
    /// N-entry `PathBuf` map made every clone O(files) — quadratic CPU and
    /// peak RSS across a whole compile.
    file_map: Arc<HashMap<std::path::PathBuf, SourceFile>>,
    /// Maps file paths to compiler2-only `SourceFile` handles.
    compiler2_file_map: HashMap<std::path::PathBuf, SourceFile>,
    /// Maps `FileId` to file path for reverse lookup (all files including v2
    /// stubs). `Arc`-wrapped for the same reason as `file_map`.
    file_id_to_path: Arc<HashMap<FileId, std::path::PathBuf>>,
    /// `SourceFile` inputs of removed paths. Salsa never frees inputs, so a
    /// delete/recreate cycle (branch switch, codegen rewriting `.baml`
    /// files) would mint a new immortal input per cycle; instead the input
    /// parks here with empty text (releasing the source string and its
    /// downstream memos) and is revived if the path reappears.
    removed_file_tombstones: HashMap<std::path::PathBuf, SourceFile>,
}

/// Origin-preference order for disambiguating functions that share one
/// declaration span (a declarative LLM function and its `$stream` /
/// `$parse_stream` companions): the user-authored function sorts first.
fn func_origin_rank(origin: baml_compiler2_ast::ast::FunctionOrigin) -> u8 {
    use baml_compiler2_ast::ast::FunctionOrigin;
    match origin {
        FunctionOrigin::UserDefined => 0,
        FunctionOrigin::Companion => 1,
        FunctionOrigin::Internal => 2,
        FunctionOrigin::AutoDerive => 3,
    }
}

#[salsa::db]
impl salsa::Database for ProjectDatabase {}

#[salsa::db]
impl baml_workspace::Db for ProjectDatabase {
    fn project(&self) -> Project {
        self.project
            .expect("project must be set before querying - call set_project_root first")
    }

    fn seeded_throw_facts(&self) -> Option<baml_workspace::SeededThrowFacts> {
        self.seeded_throw_facts
    }

    fn seeded_stdlib_interface(&self) -> Option<baml_workspace::SeededStdlibInterface> {
        self.seeded_stdlib_interface
    }

    fn seeded_callable_throws(&self) -> Option<baml_workspace::SeededCallableThrows> {
        self.seeded_callable_throws
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
impl baml_compiler2_mir::Db for ProjectDatabase {}

#[salsa::db]
impl baml_compiler2_emit::Db for ProjectDatabase {
    fn parallel_db_handle(&self) -> Option<Box<dyn baml_compiler2_mir::Db + Send>> {
        // A shared-storage salsa handle (an `Arc` bump — the same handle
        // cloning the parallel check in `check.rs` relies on): the clone is
        // MOVED into an emit worker thread, and all clones share one memo
        // table. `ProjectDatabase` is `Send` but deliberately not `Sync`, so
        // handing out owned handles is the only way workers can read salsa.
        Some(Box::new(self.clone()))
    }
}

#[salsa::db]
impl baml_surface::Db for ProjectDatabase {}

#[salsa::db]
impl baml_lsp2_actions::Db for ProjectDatabase {}

impl ProjectDatabase {
    fn playground_function_name_for_source_file(
        &self,
        source_file: SourceFile,
        name: &baml_db::Name,
    ) -> String {
        let package_info = baml_compiler2_hir::file_package::file_package(self, source_file);
        crate::symbols::playground_function_name(&package_info.namespace_path, name)
    }

    fn function_name_matches_source_name(
        &self,
        source_file: SourceFile,
        name: &baml_db::Name,
        target_name: &str,
    ) -> bool {
        name.as_str() == target_name
            || self.playground_function_name_for_source_file(source_file, name) == target_name
    }

    /// Create a new empty database.
    pub fn new() -> Self {
        Self::from_storage(salsa::Storage::default())
    }

    /// Create a new database with an event callback for tracking query execution.
    ///
    /// The callback will be invoked for various Salsa events, including:
    /// - `WillExecute`: A query is about to be recomputed
    /// - `DidValidateMemoizedValue`: A cached value was reused
    ///
    /// This is useful for tracking incremental compilation behavior.
    pub fn new_with_event_callback(callback: EventCallback) -> Self {
        Self::from_storage(salsa::Storage::new(Some(callback)))
    }

    /// Build a database over `storage`, installing the three seed inputs empty
    /// from construction. Holding each `#[salsa::input]` handle present (not
    /// `None`) from the start is what lets the seed-reading queries record a
    /// *tracked* dependency on the initially-empty seed maps, so a later
    /// `set_seeded_*` on a reused database reliably invalidates their memos; an
    /// empty map means "no seeds" and every file derives honestly. See the
    /// `seeded_*` field docs.
    fn from_storage(storage: salsa::Storage<Self>) -> Self {
        let mut db = Self {
            storage,
            next_file_id: Arc::new(AtomicU32::new(0)),
            project: None,
            compiler2_extra_files: None,
            seeded_throw_facts: None,
            seeded_stdlib_interface: None,
            seeded_callable_throws: None,
            file_map: Arc::new(HashMap::new()),
            compiler2_file_map: HashMap::new(),
            file_id_to_path: Arc::new(HashMap::new()),
            removed_file_tombstones: HashMap::new(),
        };
        db.seeded_throw_facts = Some(baml_workspace::SeededThrowFacts::new(
            &db,
            std::collections::BTreeMap::new(),
        ));
        db.seeded_stdlib_interface = Some(baml_workspace::SeededStdlibInterface::new(
            &db,
            std::collections::BTreeMap::new(),
        ));
        db.seeded_callable_throws = Some(baml_workspace::SeededCallableThrows::new(
            &db,
            std::collections::BTreeMap::new(),
        ));
        db
    }

    /// Get the project, if set.
    pub fn get_project(&self) -> Option<Project> {
        self.project
    }

    /// Seed per-file throw facts from a previous compile of identical file
    /// content (bytecode-cache per-file reuse); keys are full source-file path
    /// strings.
    ///
    /// This mutates the always-present `SeededThrowFacts` input (created in
    /// `new`) through its Salsa setter, so it bumps the revision and correctly
    /// invalidates any already-computed `file_throw_facts` memo — it is safe to
    /// call before *or* after queries have run.
    pub fn set_seeded_throw_facts(
        &mut self,
        by_path: std::collections::BTreeMap<
            String,
            Vec<baml_type::throw_facts::FunctionThrowFacts>,
        >,
    ) {
        let seeds = self
            .seeded_throw_facts
            .expect("SeededThrowFacts input is created in ProjectDatabase::new");
        seeds.set_by_path(self).to(by_path);
    }

    /// Seed the stdlib packages' typed interfaces from a previous compile;
    /// keys are package names, values are `borsh(PackageInterface)`.
    ///
    /// Mutates the always-present `SeededStdlibInterface` input (created in
    /// `new`) through its Salsa setter, so it bumps the revision and correctly
    /// invalidates any already-computed `package_interface` memo — it is safe to
    /// call before *or* after queries have run. Only stdlib package names ever
    /// appear in the map, so user packages are never seeded and always derive
    /// their interface honestly.
    pub fn set_seeded_stdlib_interface(
        &mut self,
        by_package: std::collections::BTreeMap<String, Vec<u8>>,
    ) {
        let seeds = self
            .seeded_stdlib_interface
            .expect("SeededStdlibInterface input is created in ProjectDatabase::new");
        seeds.set_by_package(self).to(by_package);
    }

    /// Seed per-function `callable_throws` values from a previous compile of
    /// identical file content; the outer key is a full source-file path string,
    /// the inner key an item-tree `LocalItemId::as_u32`.
    ///
    /// Mutates the always-present `SeededCallableThrows` input (created in `new`)
    /// through its Salsa setter, so it bumps the revision and correctly
    /// invalidates any already-computed `callable_throws` memo — safe to call
    /// before *or* after queries have run. Only functions the reuse plan proved
    /// clean (unchanged body and unchanged transitive throw contributors) ever
    /// appear, so a dirty or throws-tainted function is never seeded and always
    /// infers honestly.
    pub fn set_seeded_callable_throws(
        &mut self,
        by_path: std::collections::BTreeMap<String, std::collections::BTreeMap<u32, baml_type::Ty>>,
    ) {
        let seeds = self
            .seeded_callable_throws
            .expect("SeededCallableThrows input is created in ProjectDatabase::new");
        seeds.set_by_path(self).to(by_path);
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
        SourceFile::new(self, text.into(), path.into(), file_id, false)
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
            // Revive the tombstoned input if this path existed before —
            // creating a fresh input would leak the old one forever.
            let file = if let Some(file) = self.removed_file_tombstones.remove(&canonical_path) {
                file.set_text(self).to(content.to_string());
                file
            } else {
                self.add_file_internal(&canonical_path, content)
            };
            let file_id = file.file_id(self);

            Arc::make_mut(&mut self.file_map).insert(canonical_path.clone(), file);
            Arc::make_mut(&mut self.file_id_to_path).insert(file_id, canonical_path);

            // Update project files list if project is set
            if let Some(project) = self.project {
                let mut files: Vec<SourceFile> = project.files(self).clone();
                files.push(file);
                project.set_files(self).to(files);
            }

            file
        }
    }

    /// Bulk [`Self::add_or_update_file`]: identical per-file semantics
    /// (canonicalization, tombstone revival, map registration), but the
    /// project file list is written once at the end instead of once per new
    /// file. The per-file path clones and re-sets the whole `files` Vec and
    /// bumps the salsa revision each time — O(files²) copies plus one
    /// revision per file during initial project load.
    pub fn add_or_update_files<'a, I>(&mut self, files: I)
    where
        I: IntoIterator<Item = (&'a std::path::Path, &'a str)>,
    {
        let mut new_files = Vec::new();
        for (path, content) in files {
            let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

            if let Some(&existing_file) = self.file_map.get(&canonical_path) {
                existing_file.set_text(self).to(content.to_string());
                continue;
            }
            let file = if let Some(file) = self.removed_file_tombstones.remove(&canonical_path) {
                file.set_text(self).to(content.to_string());
                file
            } else {
                self.add_file_internal(&canonical_path, content)
            };
            let file_id = file.file_id(self);
            Arc::make_mut(&mut self.file_map).insert(canonical_path.clone(), file);
            Arc::make_mut(&mut self.file_id_to_path).insert(file_id, canonical_path);
            new_files.push(file);
        }
        if !new_files.is_empty()
            && let Some(project) = self.project
        {
            let mut project_files: Vec<SourceFile> = project.files(self).clone();
            project_files.extend(new_files);
            project.set_files(self).to(project_files);
        }
    }

    /// Remove a file from the database.
    ///
    /// Note: Salsa doesn't support true removal. The input is emptied (so its
    /// text and per-file memos can be reclaimed), removed from tracking and
    /// the project's file list, and parked in a tombstone map for reuse if
    /// the same path is re-added later.
    pub fn remove_file(&mut self, path: &std::path::Path) {
        let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        if let Some(file) = Arc::make_mut(&mut self.file_map).remove(&canonical_path) {
            let file_id = file.file_id(self);
            Arc::make_mut(&mut self.file_id_to_path).remove(&file_id);

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

            file.set_text(self).to(String::new());
            self.removed_file_tombstones.insert(canonical_path, file);
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
    /// Media, baml.env, baml.http, baml.sys namespaces, etc.).
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

            Arc::make_mut(&mut self.file_id_to_path).insert(file_id, path.clone());
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

    /// Get the compiled bytecode for the project using the compiler2 pipeline.
    pub fn get_bytecode(
        &self,
    ) -> Result<bex_vm_types::Program, baml_compiler2_emit::LoweringError> {
        // Bytecode generation lowers types through the runtime-conversion
        // boundary (`ResolvedAliases::convert`), which deliberately panics on
        // inference-only `Unknown`/`Error` types. Those are legitimate
        // error-recovery types in a program that does not type-check, so do not
        // attempt codegen on an error-bearing project: surface the failure as a
        // recoverable `LoweringError`. The diagnostics themselves are reported
        // through the normal check path. (CLI commands gate before calling
        // `generate_project_bytecode` directly; this protects the in-process /
        // runtime-eval callers that go through `get_bytecode`.) The error filter
        // matches `testing::assert_no_diagnostic_errors` — user-file errors only.
        let user_file_ids: std::collections::HashSet<_> = self
            .get_source_files()
            .iter()
            .map(|f| f.file_id(self))
            .collect();
        let error_count = crate::check::collect_compiler2_diagnostics(self)
            .iter()
            .filter(|d| matches!(d.severity, baml_compiler_diagnostics::Severity::Error))
            .filter(|d| {
                d.primary_span()
                    .map(|span| user_file_ids.contains(&span.file_id))
                    .unwrap_or(false)
            })
            .count();
        if error_count > 0 {
            return Err(baml_compiler2_emit::LoweringError::ProjectHasErrors { error_count });
        }
        self.get_bytecode_unchecked()
    }

    /// [`Self::get_bytecode`] without the error gate: goes straight to codegen.
    ///
    /// Only for callers that have already run a full-project check (per-file
    /// `check_file` sweep **plus** package-level diagnostics) at the current
    /// revision and found no user-file errors — the gate in `get_bytecode`
    /// would re-derive exactly that result. Calling this on an error-bearing
    /// project can panic in the runtime-conversion boundary (see the gate
    /// comment above).
    pub fn get_bytecode_unchecked(
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
    ///
    /// Callee graphs are inlined at every call site, memoized per callee and
    /// active recursion context, and capped at 5,000 total nodes — the fully-inlined
    /// representation is otherwise exponential in call-chain depth.
    pub fn ast_control_flow_graph(
        &self,
        function_name: &str,
    ) -> Option<baml_compiler2_visualization::control_flow::ControlFlowGraph> {
        let mut ctx = CfgExpansionCtx::default();
        self.ast_control_flow_graph_impl(function_name, &mut ctx)
            .or_else(|| self.ast_test_control_flow_graph_impl(function_name, &mut ctx))
    }

    /// Build a graph for a statically named top-level `test "..." { ... }`
    /// declaration. New-style tests are lowered into lambdas passed to the
    /// per-file `$init_test_*` function, so they do not appear in
    /// `file_functions`. The test registry exposes their canonical names to
    /// the playground (`root[.namespace]::name`); recover the matching lambda
    /// from that synthesized registration and graph its body directly.
    fn ast_test_control_flow_graph_impl(
        &self,
        test_name: &str,
        ctx: &mut CfgExpansionCtx,
    ) -> Option<baml_compiler2_visualization::control_flow::ControlFlowGraph> {
        use baml_compiler2_ast::{Expr, FunctionBodyDef, Item};
        use baml_compiler2_visualization::control_flow::{
            NodeType, build_control_flow_graph_from_expr,
        };
        use baml_type::Literal;

        if !ctx.expanding.insert(test_name.to_string()) {
            return None;
        }

        let mut result = None;
        'files: for source_file in self.file_map.values().copied() {
            let ast = baml_compiler2_hir::file_ast(self, source_file);
            for item in &ast.items {
                let Item::Function(init_function) = item else {
                    continue;
                };
                if !init_function.name.as_str().starts_with("$init_test") {
                    continue;
                }
                let Some(FunctionBodyDef::Expr(registration_body, registration_source_map)) =
                    init_function.body.as_ref()
                else {
                    continue;
                };
                let Some(&init_function_loc) =
                    baml_compiler2_ppir::item_data::file_functions(self, source_file)
                        .iter()
                        .find(|&&loc| {
                            baml_compiler2_ppir::item_data::function_data(self, loc).name
                                == init_function.name
                        })
                else {
                    continue;
                };

                let mut duplicate_counts = HashMap::<String, usize>::new();
                for (_, expr) in registration_body.exprs.iter() {
                    let Expr::Call { callee, args, .. } = expr else {
                        continue;
                    };
                    let Expr::Path(callee_segments) = &registration_body.exprs[*callee] else {
                        continue;
                    };
                    if callee_segments.last().map(AsRef::<str>::as_ref) != Some("register_test_at")
                        || args.len() != 4
                    {
                        continue;
                    }

                    let Expr::Literal(Literal::String(owner)) =
                        &registration_body.exprs[args[0].expr]
                    else {
                        continue;
                    };
                    let Expr::Literal(Literal::String(name)) =
                        &registration_body.exprs[args[1].expr]
                    else {
                        // Runtime-computed test names cannot be identified
                        // statically from the canonical registry name.
                        continue;
                    };
                    let canonical_base = format!("{owner}::{name}");
                    let duplicate_count = duplicate_counts
                        .entry(canonical_base.clone())
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                    let canonical_name = if *duplicate_count == 1 {
                        canonical_base
                    } else {
                        format!("{canonical_base}#{duplicate_count}")
                    };
                    if canonical_name != test_name {
                        continue;
                    }

                    let Expr::Lambda(test_lambda) = &registration_body.exprs[args[2].expr] else {
                        continue;
                    };
                    // The test body is an expression in the registration body's
                    // own arena, so it shares that body's source map.
                    let test_body = registration_body;
                    let mut graph =
                        build_control_flow_graph_from_expr(test_name, test_body, test_lambda.body);
                    self.attach_source_spans_to_graph(
                        &mut graph,
                        source_file,
                        registration_source_map,
                    );

                    let test_name_span =
                        Self::source_map_expr_range(registration_source_map, args[1].expr)
                            .and_then(|range| self.source_span_for_range(source_file, range));
                    if let Some(root) = graph
                        .nodes
                        .values_mut()
                        .find(|node| node.node_type == NodeType::FunctionRoot)
                    {
                        root.source_span = test_name_span
                            .or_else(|| self.source_span_for_range(source_file, test_lambda.span));
                    }

                    self.expand_user_function_calls_in_graph(
                        &mut graph,
                        init_function_loc,
                        test_body,
                        &CfgDispatchBindings::new(),
                        ctx,
                    );
                    result = Some(graph);
                    break 'files;
                }
            }
        }

        ctx.expanding.remove(test_name);
        result
    }

    fn ast_control_flow_graph_impl(
        &self,
        function_name: &str,
        ctx: &mut CfgExpansionCtx,
    ) -> Option<baml_compiler2_visualization::control_flow::ControlFlowGraph> {
        let func_loc = self.find_function_loc(function_name)?;
        self.ast_control_flow_graph_for_loc(
            func_loc,
            function_name,
            &CfgDispatchBindings::new(),
            ctx,
        )
    }

    fn ast_control_flow_graph_for_loc<'db>(
        &'db self,
        func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
        function_name: &str,
        dispatch_bindings: &CfgDispatchBindings,
        ctx: &mut CfgExpansionCtx,
    ) -> Option<baml_compiler2_visualization::control_flow::ControlFlowGraph> {
        use baml_compiler2_visualization::control_flow::{
            build_control_flow_graph_from_ast, build_llm_control_flow_graph,
        };

        let function_identity = self.cfg_function_identity(func_loc);
        if !ctx.expanding.insert(function_identity.clone()) {
            return None;
        }

        let source_file = func_loc.file(self);
        let func_span = baml_compiler2_ppir::item_data::function_source_map(self, func_loc).span;
        let body = baml_compiler2_ppir::function_body(self, func_loc);

        // LLM functions desugar to Expr bodies, so it is `declarative_meta`
        // (surfaced span-free by `function_llm_meta`) — not the body variant —
        // that marks them.
        let result = if let Some(llm_meta) =
            baml_compiler2_ppir::item_data::function_llm_meta(self, func_loc)
        {
            let client_name = llm_meta
                .client_name
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "unknown".to_string());
            let mut graph = build_llm_control_flow_graph(function_name, &client_name);
            if let Some(source_span) = self.source_span_for_range(source_file, func_span) {
                if let Some(node) = graph.nodes.values_mut().next() {
                    node.source_span = Some(source_span);
                }
            }
            Some(graph)
        } else {
            match body.as_ref() {
                baml_compiler2_hir::body::FunctionBody::Expr(expr_body) => {
                    let mut graph = build_control_flow_graph_from_ast(function_name, expr_body);
                    if let Some(source_map) =
                        baml_compiler2_ppir::function_body_source_map(self, func_loc)
                    {
                        self.attach_source_spans_to_graph(&mut graph, source_file, &source_map);
                    }
                    // The FunctionRoot node has no `source_expr`, so
                    // `attach_source_spans_to_graph` skips it. Point it at the
                    // whole function declaration so clicking the root in the
                    // playground selects the function (mirrors the LLM path above).
                    if let Some(root_span) = self.source_span_for_range(source_file, func_span) {
                        if let Some(root) = graph.nodes.values_mut().find(|node| {
                            node.node_type
                                == baml_compiler2_visualization::control_flow::NodeType::FunctionRoot
                        }) {
                            root.source_span.get_or_insert(root_span);
                        }
                    }
                    self.expand_user_function_calls_in_graph(
                        &mut graph,
                        func_loc,
                        expr_body,
                        dispatch_bindings,
                        ctx,
                    );
                    Some(graph)
                }
                baml_compiler2_hir::body::FunctionBody::Builtin(_)
                | baml_compiler2_hir::body::FunctionBody::Missing => None,
            }
        };

        ctx.expanding.remove(&function_identity);
        result
    }

    fn expand_user_function_calls_in_graph<'db>(
        &'db self,
        graph: &mut baml_compiler2_visualization::control_flow::ControlFlowGraph,
        caller: baml_compiler2_hir::loc::FunctionLoc<'db>,
        body: &baml_compiler2_ast::ExprBody,
        dispatch_bindings: &CfgDispatchBindings,
        ctx: &mut CfgExpansionCtx,
    ) {
        use baml_compiler2_visualization::control_flow::NodeType;

        for (call_expr, target) in self.call_sites_by_source_expr(caller, body, dispatch_bindings) {
            let Some((call_node_id, is_return_node)) = graph
                .nodes
                .values()
                .find(|node| node.source_expr == Some(call_expr))
                .map(|node| (node.id, matches!(node.node_type, NodeType::Return)))
            else {
                continue;
            };
            if is_return_node {
                continue;
            }

            let (callee_header, callee_graph) = match target {
                CfgCallTarget::Function {
                    loc,
                    display_name,
                    dispatch_bindings,
                } => {
                    let function_identity = self.cfg_function_identity(loc);
                    if ctx.expanding.contains(&function_identity) {
                        continue;
                    }
                    let key = self.cfg_expansion_key(loc, &dispatch_bindings);
                    let cache_key = ctx.cache_key(key.clone());
                    let graph = if let Some(cached) = ctx.cache.get(&cache_key) {
                        cached.clone()
                    } else {
                        let built = self
                            .ast_control_flow_graph_for_loc(
                                loc,
                                &display_name,
                                &dispatch_bindings,
                                ctx,
                            )
                            .map(std::sync::Arc::new);
                        ctx.cache.insert(cache_key, built.clone());
                        built
                    };
                    (self.function_header_title_for_loc(loc), graph)
                }
                CfgCallTarget::UnresolvedName(callee_name) => {
                    let cache_key = ctx.cache_key(callee_name.clone());
                    let graph = if let Some(cached) = ctx.cache.get(&cache_key) {
                        cached.clone()
                    } else {
                        let built = self
                            .ast_control_flow_graph_impl(&callee_name, ctx)
                            .map(std::sync::Arc::new);
                        ctx.cache.insert(cache_key, built.clone());
                        built
                    };
                    (self.function_header_title(&callee_name), graph)
                }
            };

            // Recursion is cut at the call node rather than cached: a graph
            // truncated by the cycle guard must not be reused at sites where
            // the callee is not part of the active expansion chain.
            let Some(callee_graph) = callee_graph else {
                continue;
            };

            if Self::is_single_llm_graph(&callee_graph) {
                // Calls to LLM functions always render. Mark the call node so
                // the visualization prep keeps it (and styles it as an LLM
                // call) instead of pruning it like a plain function call.
                let client_name = callee_graph
                    .nodes
                    .values()
                    .next()
                    .and_then(|node| node.llm_client.clone());
                if let Some(node) = graph.nodes.get_mut(&call_node_id) {
                    node.llm_client = Some(client_name.unwrap_or_else(|| "unknown".to_string()));
                    if matches!(node.node_type, NodeType::OtherScope) {
                        node.node_type = NodeType::LlmFunction;
                    }
                }
                continue;
            }

            // A `//#` header directly above the callee's declaration names the
            // call node: `//# process stuff` above `function somefunc()` makes
            // every `somefunc()` call render as a "process stuff" node.
            if let Some(title) = callee_header {
                if let Some(node) = graph.nodes.get_mut(&call_node_id) {
                    node.label = title;
                    if matches!(node.node_type, NodeType::OtherScope) {
                        node.node_type = NodeType::HeaderContextEnter;
                    }
                }
            }

            // Even with per-callee memoization the merged output copies the
            // callee graph at every call site, so deep chains still multiply
            // node counts. Stop inlining once the graph reaches the budget;
            // remaining calls render as plain call nodes.
            if graph.nodes.len() + callee_graph.nodes.len() > CFG_EXPANSION_NODE_BUDGET {
                continue;
            }
            Self::merge_callee_graph_under_call_node(graph, call_node_id, &callee_graph);
        }
    }

    /// Find the `//#` header comment immediately above a function declaration,
    /// if any. Blank lines and regular `//` comments between the header and
    /// the declaration are skipped; any other code stops the search. If multiple
    /// same-named declarations have different headers, do not guess which one a
    /// name-only call resolved to.
    fn function_header_title(&self, function_name: &str) -> Option<String> {
        let mut unique_title = None;
        for source_file in self.file_map.values().copied() {
            for &func_loc in baml_compiler2_ppir::item_data::file_functions(self, source_file) {
                let func_data = baml_compiler2_ppir::item_data::function_data(self, func_loc);
                if !self.function_name_matches_source_name(
                    source_file,
                    &func_data.name,
                    function_name,
                ) {
                    continue;
                }
                let func_span =
                    baml_compiler2_ppir::item_data::function_source_map(self, func_loc).span;
                let text = source_file.text(self);
                let start = usize::from(func_span.start()).min(text.len());
                if let Some(title) = header_title_above(&text[..start]) {
                    match &unique_title {
                        Some(existing) if existing != &title => return None,
                        Some(_) => {}
                        None => unique_title = Some(title),
                    }
                }
            }
        }
        unique_title
    }

    fn function_header_title_for_loc(
        &self,
        func_loc: baml_compiler2_hir::loc::FunctionLoc<'_>,
    ) -> Option<String> {
        let source_file = func_loc.file(self);
        let func_span = baml_compiler2_ppir::item_data::function_source_map(self, func_loc).span;
        let text = source_file.text(self);
        let start = usize::from(func_span.start()).min(text.len());
        header_title_above(&text[..start])
    }

    fn find_function_loc<'db>(
        &'db self,
        function_name: &str,
    ) -> Option<baml_compiler2_hir::loc::FunctionLoc<'db>> {
        for source_file in self.file_map.values().copied() {
            for &func_loc in baml_compiler2_ppir::item_data::file_functions(self, source_file) {
                let func_data = baml_compiler2_ppir::item_data::function_data(self, func_loc);
                if self.function_name_matches_source_name(
                    source_file,
                    &func_data.name,
                    function_name,
                ) {
                    return Some(func_loc);
                }
            }
        }
        None
    }

    fn function_display_name(&self, func_loc: baml_compiler2_hir::loc::FunctionLoc<'_>) -> String {
        use baml_compiler2_ppir::item_data::MethodOwner;

        let data = baml_compiler2_ppir::item_data::function_data(self, func_loc);
        match baml_compiler2_ppir::item_data::method_owner(self, func_loc) {
            Some(MethodOwner::Class(class_loc)) => {
                let class = baml_compiler2_ppir::item_data::class_data(self, class_loc);
                format!("{}.{}", class.name, data.name)
            }
            Some(MethodOwner::Interface(iface_loc)) => {
                let iface = baml_compiler2_ppir::item_data::interface_data(self, iface_loc);
                format!("{}.{}", iface.name, data.name)
            }
            Some(MethodOwner::FreeImpl(_)) | None => {
                self.playground_function_name_for_source_file(func_loc.file(self), &data.name)
            }
        }
    }

    fn cfg_expansion_key(
        &self,
        func_loc: baml_compiler2_hir::loc::FunctionLoc<'_>,
        dispatch_bindings: &CfgDispatchBindings,
    ) -> String {
        let mut bindings = dispatch_bindings
            .iter()
            .map(|(name, ty)| format!("{name}={ty:?}"))
            .collect::<Vec<_>>();
        bindings.sort();
        format!(
            "{}<{}>",
            self.cfg_function_identity(func_loc),
            bindings.join(",")
        )
    }

    fn cfg_function_identity(&self, func_loc: baml_compiler2_hir::loc::FunctionLoc<'_>) -> String {
        format!(
            "{}#{}",
            func_loc.file(self).path(self).display(),
            func_loc.id(self).as_u32()
        )
    }

    fn call_sites_by_source_expr<'db>(
        &'db self,
        caller: baml_compiler2_hir::loc::FunctionLoc<'db>,
        body: &baml_compiler2_ast::ExprBody,
        dispatch_bindings: &CfgDispatchBindings,
    ) -> Vec<(u32, CfgCallTarget<'db>)> {
        use baml_compiler2_ast::Expr;

        let inference = Some(baml_compiler2_hir_ty::infer::infer_body(
            self,
            baml_compiler2_hir::body::BodyOwnerId::Function(caller),
        ));
        let mut calls = Vec::new();
        for (expr_id, expr) in body.exprs.iter() {
            let (callee, args) = match expr {
                Expr::Call { callee, args, .. } | Expr::OptionalCall { callee, args } => {
                    (*callee, args)
                }
                _ => continue,
            };

            if let Some(inference) = inference {
                if let Some(loc) =
                    self.resolved_call_function(inference, body, callee, dispatch_bindings)
                {
                    calls.push((
                        expr_id.into_raw().into_u32(),
                        CfgCallTarget::Function {
                            loc,
                            display_name: self.function_display_name(loc),
                            dispatch_bindings: self
                                .dispatch_bindings_for_call(inference, body, expr_id, args, loc),
                        },
                    ));
                    continue;
                }
            }

            let Expr::Path(segments) = &body.exprs[callee] else {
                continue;
            };

            if let Some(loc) = self.resolve_path_function(caller.file(self), segments) {
                calls.push((
                    expr_id.into_raw().into_u32(),
                    CfgCallTarget::Function {
                        loc,
                        display_name: self.function_display_name(loc),
                        dispatch_bindings: inference
                            .map(|inference| {
                                self.dispatch_bindings_for_call(inference, body, expr_id, args, loc)
                            })
                            .unwrap_or_default(),
                    },
                ));
                continue;
            }

            let callee_name = segments
                .iter()
                .map(AsRef::<str>::as_ref)
                .collect::<Vec<_>>()
                .join(".");
            calls.push((
                expr_id.into_raw().into_u32(),
                CfgCallTarget::UnresolvedName(callee_name),
            ));
        }
        calls
    }

    fn resolve_path_function<'db>(
        &'db self,
        caller_file: SourceFile,
        callee_path: &[baml_db::Name],
    ) -> Option<baml_compiler2_hir::loc::FunctionLoc<'db>> {
        use baml_compiler2_hir::{contributions::Definition, file_package, package::PackageId};
        use baml_compiler2_hir_ty::package_interface::ResolvedSource;

        let caller_package = file_package::file_package(self, caller_file);
        let package_id = PackageId::new(self, caller_package.package.clone());
        let resolution =
            baml_compiler2_hir_ty::package_interface::package_resolution_context(self, package_id);
        match resolution.resolve_value(self, callee_path, &caller_package.namespace_path) {
            Some((ResolvedSource::Item, Definition::Function(function))) => Some(function),
            _ => None,
        }
    }

    fn resolved_call_function<'db>(
        &'db self,
        inference: &baml_compiler2_hir_ty::infer::InferenceResult<'db>,
        body: &baml_compiler2_ast::ExprBody,
        callee: baml_compiler2_ast::ExprId,
        dispatch_bindings: &CfgDispatchBindings,
    ) -> Option<baml_compiler2_hir::loc::FunctionLoc<'db>> {
        use baml_compiler2_ast::Expr;
        use baml_compiler2_hir_ty::infer::MemberResolution;

        let resolution = inference.member_resolutions.get(&callee).or_else(|| {
            inference
                .path_resolutions
                .get(&callee)
                .and_then(|path| path.segments.last())
                .and_then(|segment| segment.resolution.as_ref())
        });

        match resolution {
            Some(
                MemberResolution::Free { func }
                | MemberResolution::BoundMethod { func, .. }
                | MemberResolution::UnboundMethod { func, .. }
                | MemberResolution::InterfaceConcreteMethod { func, .. },
            ) => Some(*func),
            Some(MemberResolution::InterfaceVirtualMethod { interface, method }) => {
                let receiver = match &body.exprs[callee] {
                    Expr::MemberAccess { base, .. } | Expr::OptionalMemberAccess { base, .. } => {
                        match &body.exprs[*base] {
                            Expr::Path(segments) if segments.len() == 1 => {
                                Some(segments[0].as_str())
                            }
                            _ => None,
                        }
                    }
                    Expr::Path(segments) if segments.len() >= 2 => {
                        segments.first().map(baml_db::Name::as_str)
                    }
                    _ => None,
                }?;
                let concrete = dispatch_bindings.get(receiver)?;
                self.interface_method_impl_loc(concrete, *interface, method)
            }
            Some(
                MemberResolution::Field { .. }
                | MemberResolution::Variant { .. }
                | MemberResolution::InterfaceVirtualField { .. },
            )
            | None => None,
        }
    }

    fn dispatch_bindings_for_call(
        &self,
        inference: &baml_compiler2_hir_ty::infer::InferenceResult<'_>,
        body: &baml_compiler2_ast::ExprBody,
        call_expr: baml_compiler2_ast::ExprId,
        args: &[baml_compiler2_ast::CallArg],
        callee: baml_compiler2_hir::loc::FunctionLoc<'_>,
    ) -> CfgDispatchBindings {
        use baml_compiler2_ast::Expr;
        use baml_compiler2_hir_ty::infer::MemberResolution;

        let params = &baml_compiler2_ppir::item_data::function_data(self, callee).params;
        let callee_expr = match &body.exprs[call_expr] {
            Expr::Call { callee, .. } | Expr::OptionalCall { callee, .. } => Some(*callee),
            _ => None,
        };
        let resolution = callee_expr.and_then(|callee_expr| {
            inference.member_resolutions.get(&callee_expr).or_else(|| {
                inference
                    .path_resolutions
                    .get(&callee_expr)
                    .and_then(|path| path.segments.last())
                    .and_then(|segment| segment.resolution.as_ref())
            })
        });
        // Call plans index only the arguments provided by the caller. A bound
        // method's declared `self` parameter is implicit, so shift those
        // indices back into the declaration's full parameter list.
        let implicit_self = usize::from(matches!(
            resolution,
            Some(
                MemberResolution::BoundMethod { .. }
                    | MemberResolution::InterfaceConcreteMethod { .. }
                    | MemberResolution::InterfaceVirtualMethod { .. }
            )
        ));
        let mut bindings = CfgDispatchBindings::new();
        let mut record = |param_index: usize, arg_expr: baml_compiler2_ast::ExprId| {
            let Some(param) = params.get(param_index) else {
                return;
            };
            let Some(concrete) = inference.type_of_expr.get(&arg_expr) else {
                return;
            };
            bindings.insert(param.name.to_string(), concrete.to_plain());
        };

        if let Some(plan) = inference.call_plans.get(&call_expr) {
            for binding in &plan.bindings {
                let baml_compiler2_hir_ty::infer::ParamBinding::Provided { param_index, arg } =
                    binding
                else {
                    continue;
                };
                record(param_index + implicit_self, *arg);
            }
        } else {
            for (position, arg) in args.iter().enumerate() {
                let param_index = arg
                    .label
                    .as_ref()
                    .and_then(|label| params.iter().position(|param| &param.name == label))
                    .unwrap_or(position + implicit_self);
                record(param_index, arg.expr);
            }
        }
        bindings
    }

    fn interface_method_impl_loc<'db>(
        &'db self,
        concrete: &baml_type::Ty,
        iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
        method_name: &baml_db::Name,
    ) -> Option<baml_compiler2_hir::loc::FunctionLoc<'db>> {
        let interned = baml_compiler2_hir_ty::impls::try_interned_ty(concrete)?;
        let method_of = |func_loc: &baml_compiler2_hir::loc::FunctionLoc<'db>| {
            baml_compiler2_ppir::item_data::function_data(self, *func_loc).name == *method_name
        };
        let mut methods = baml_compiler2_hir_ty::impls::impls_for_type(self, &interned)
            .into_iter()
            .filter(|resolved| {
                baml_compiler2_hir_ty::interfaces::impl_data(self, resolved.block)
                    .as_ref()
                    .is_ok_and(|data| data.interface == iface_loc)
            })
            .filter_map(|resolved| {
                // The impl's own override wins; an inherited interface
                // default method fills the slot otherwise.
                baml_compiler2_hir_ty::interfaces::impl_data(self, resolved.block)
                    .as_ref()
                    .ok()
                    .and_then(|data| data.methods.iter().find(|loc| method_of(loc)).copied())
                    .or_else(|| {
                        baml_compiler2_ppir::item_data::interface_data(self, iface_loc)
                            .default_methods
                            .iter()
                            .find(|loc| method_of(loc))
                            .copied()
                    })
            });
        let method = methods.next()?;
        if methods.next().is_some() {
            return None;
        }
        Some(method)
    }

    fn is_single_llm_graph(
        graph: &baml_compiler2_visualization::control_flow::ControlFlowGraph,
    ) -> bool {
        graph.nodes.len() == 1
            && graph.nodes.values().any(|node| {
                matches!(
                    node.node_type,
                    baml_compiler2_visualization::control_flow::NodeType::LlmFunction
                )
            })
    }

    fn merge_callee_graph_under_call_node(
        graph: &mut baml_compiler2_visualization::control_flow::ControlFlowGraph,
        call_node_id: baml_compiler2_visualization::control_flow::NodeId,
        callee_graph: &baml_compiler2_visualization::control_flow::ControlFlowGraph,
    ) {
        use baml_compiler2_visualization::control_flow::{Edge, NodeId};

        let Some(root_id) = callee_graph
            .nodes
            .values()
            .find(|node| node.parent_node_id.is_none())
            .map(|node| node.id)
        else {
            return;
        };

        let mut next_raw = graph
            .nodes
            .keys()
            .map(NodeId::raw)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let mut remap: HashMap<NodeId, NodeId> = HashMap::new();
        for node in callee_graph.nodes.values() {
            if node.id == root_id {
                continue;
            }
            let new_id = NodeId::new(next_raw);
            next_raw = next_raw.saturating_add(1);
            remap.insert(node.id, new_id);
        }

        if remap.is_empty() {
            return;
        }

        for node in callee_graph.nodes.values() {
            if node.id == root_id {
                continue;
            }

            let mut node = node.clone();
            node.id = remap[&node.id];
            node.parent_node_id = match node.parent_node_id {
                Some(parent) if parent == root_id => Some(call_node_id),
                Some(parent) => remap.get(&parent).copied().or(Some(call_node_id)),
                None => Some(call_node_id),
            };
            graph.nodes.insert(node.id, node);
        }

        for edges in callee_graph.edges_by_src.values() {
            for edge in edges {
                let src = if edge.src == root_id {
                    call_node_id
                } else if let Some(src) = remap.get(&edge.src).copied() {
                    src
                } else {
                    continue;
                };
                let Some(dst) = (if edge.dst == root_id {
                    None
                } else {
                    remap.get(&edge.dst).copied()
                }) else {
                    continue;
                };

                graph.edges_by_src.entry(src).or_default().push(Edge {
                    src,
                    dst,
                    label: edge.label.clone(),
                });
            }
        }
    }

    fn attach_source_spans_to_graph(
        &self,
        graph: &mut baml_compiler2_visualization::control_flow::ControlFlowGraph,
        source_file: SourceFile,
        source_map: &baml_compiler2_ast::AstSourceMap,
    ) {
        for node in graph.nodes.values_mut() {
            let Some(source_expr) = node.source_expr else {
                continue;
            };
            if let Some(source_span) =
                self.source_span_for_source_expr(source_file, source_map, source_expr)
            {
                node.source_span = Some(source_span);
            }
        }
    }

    fn source_span_for_source_expr(
        &self,
        source_file: SourceFile,
        source_map: &baml_compiler2_ast::AstSourceMap,
        source_expr: u32,
    ) -> Option<baml_compiler2_visualization::control_flow::SourceSpan> {
        let tag = baml_compiler2_visualization::control_flow::STMT_SOURCE_EXPR_TAG;
        let (raw, spans) = if source_expr & tag != 0 {
            (source_expr & !tag, &source_map.stmt_spans)
        } else {
            (source_expr, &source_map.expr_spans)
        };

        let idx = raw as usize;
        if idx >= spans.len() {
            return None;
        }

        let span_idx =
            la_arena::Idx::<text_size::TextRange>::from_raw(la_arena::RawIdx::from_u32(raw));
        self.source_span_for_range(source_file, spans[span_idx])
    }

    fn source_map_expr_range(
        source_map: &baml_compiler2_ast::AstSourceMap,
        expr_id: baml_compiler2_ast::ExprId,
    ) -> Option<text_size::TextRange> {
        let raw = expr_id.into_raw();
        if raw.into_u32() as usize >= source_map.expr_spans.len() {
            return None;
        }
        let span_idx = la_arena::Idx::<text_size::TextRange>::from_raw(raw);
        Some(source_map.expr_spans[span_idx])
    }

    fn source_span_for_range(
        &self,
        source_file: SourceFile,
        range: text_size::TextRange,
    ) -> Option<baml_compiler2_visualization::control_flow::SourceSpan> {
        let text = source_file.text(self);
        let len = u32::try_from(text.len()).ok()?;
        let start_offset: u32 = range.start().into();
        if start_offset > len {
            return None;
        }
        let end_offset: u32 = range.end().into();
        let end_offset = end_offset.min(len);
        let line_index = crate::position::LineIndex::new(text);
        let start = line_index.offset_to_position(start_offset)?;
        let end = line_index.offset_to_position(end_offset).unwrap_or(start);

        Some(baml_compiler2_visualization::control_flow::SourceSpan {
            file_id: source_file.file_id(self).as_u32(),
            file_path: source_file.path(self).to_string_lossy().into_owned(),
            start_offset,
            end_offset,
            line: start.line,
            column: start.character,
            end_line: end.line,
            end_column: end.character,
        })
    }

    /// Given a file path and byte offset, return context about what entity
    /// the cursor is on — used by the playground for navigation.
    pub fn playground_cursor_context(&self, file_path: &str, byte_offset: u32) -> CursorContext {
        use baml_db::baml_compiler_syntax::SyntaxKind;

        let empty = CursorContext {
            function_name: None,
            is_workflow: false,
            workflow_memberships: vec![],
            source_expr_id: None,
            source_expr_candidates: vec![],
            source_expr_function_name: None,
            test_name: None,
            cursor_offset: Some(byte_offset),
        };

        // 1. Find the SourceFile matching file_path
        let Some(source_file) = self.find_source_file(file_path) else {
            return empty;
        };

        let offset = text_size::TextSize::from(byte_offset);

        // 2. Find CST token at offset
        let Some(token) = baml_lsp2_actions::find_token_at_offset(self, source_file, offset) else {
            return empty;
        };

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
                baml_compiler2_ppir::resolve::resolve_name_at(self, source_file, offset, &name);

            match resolved {
                baml_compiler2_ppir::resolve::ResolvedName::Item(def)
                | baml_compiler2_ppir::resolve::ResolvedName::Builtin(def) => {
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
                baml_compiler2_ppir::resolve::ResolvedName::Local { .. } => {
                    return self.cursor_context_for_local(source_file, offset);
                }
                baml_compiler2_ppir::resolve::ResolvedName::Unknown => {
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
            function_name: func_name.clone(),
            is_workflow,
            workflow_memberships,
            source_expr_id,
            source_expr_candidates,
            source_expr_function_name: func_name,
            test_name: None,
            cursor_offset: Some(u32::from(offset)),
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
                let sig = baml_compiler2_ppir::function_signature(self, func_loc);
                let body = baml_compiler2_ppir::function_body(self, func_loc);
                let is_workflow = matches!(
                    body.as_ref(),
                    baml_compiler2_hir::body::FunctionBody::Expr(_)
                );

                let func_name =
                    self.playground_function_name_for_source_file(func_loc.file(self), &sig.name);
                let workflow_memberships = self.find_workflow_memberships(&func_name);

                // Find source_expr_id if cursor is inside a function body
                let (source_expr_id, source_expr_candidates) =
                    self.find_source_expr_ids_at(source_file, offset);
                let source_expr_function_name = self
                    .find_enclosing_function(source_file, offset)
                    .map(|(name, _)| name);

                CursorContext {
                    function_name: Some(func_name),
                    is_workflow,
                    workflow_memberships,
                    source_expr_id,
                    source_expr_candidates,
                    source_expr_function_name,
                    test_name: None,
                    cursor_offset: Some(u32::from(offset)),
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
                    source_expr_function_name: None,
                    test_name: None,
                    cursor_offset: Some(u32::from(offset)),
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
            function_name: func_name.clone(),
            is_workflow,
            workflow_memberships,
            source_expr_id,
            source_expr_candidates,
            source_expr_function_name: func_name,
            test_name: None,
            cursor_offset: Some(u32::from(offset)),
        }
    }

    /// Find a [`SourceFile`] by file path (matches by suffix to handle different path formats).
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
        for (stored_path, sf) in self.file_map.iter() {
            if stored_path.ends_with(file_path)
                || file_path.ends_with(stored_path.to_string_lossy().as_ref())
            {
                return Some(*sf);
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

        // A declarative LLM function and its `$stream`/`$parse_stream` companions
        // share one declaration span, hence one scope range — so multiple
        // functions match here. Prefer the user-authored one (origin order).
        let func_loc = baml_compiler2_ppir::item_data::file_functions(self, source_file)
            .iter()
            .copied()
            .filter(|&loc| {
                baml_compiler2_ppir::item_data::function_source_map(self, loc).span
                    == func_scope_range
            })
            .min_by_key(|&loc| {
                func_origin_rank(
                    baml_compiler2_ppir::item_data::function_data(self, loc)
                        .metadata
                        .origin,
                )
            })?;
        let sig = baml_compiler2_ppir::function_signature(self, func_loc);
        let body = baml_compiler2_ppir::function_body(self, func_loc);
        let is_workflow = matches!(
            body.as_ref(),
            baml_compiler2_hir::body::FunctionBody::Expr(_)
        );
        Some((
            self.playground_function_name_for_source_file(source_file, &sig.name),
            is_workflow,
        ))
    }

    /// Find workflows that call the given function by scanning all function bodies.
    fn find_workflow_memberships(&self, target_function_name: &str) -> Vec<String> {
        let mut memberships = Vec::new();

        for source_file in self.file_map.values().copied() {
            for &func_loc in baml_compiler2_ppir::item_data::file_functions(self, source_file) {
                let func_data = baml_compiler2_ppir::item_data::function_data(self, func_loc);
                let func_name =
                    self.playground_function_name_for_source_file(source_file, &func_data.name);
                if func_data.name.as_str() == target_function_name
                    || func_name == target_function_name
                {
                    continue; // Skip self
                }

                let body = baml_compiler2_ppir::function_body(self, func_loc);

                // Only workflow (Expr) functions can call other functions
                if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() {
                    if Self::expr_body_calls_function(expr_body, target_function_name) {
                        memberships.push(func_name);
                    }
                }
            }
        }

        memberships
    }

    /// Check if an expression body contains a call to a function with the given name.
    fn expr_body_calls_function(body: &baml_compiler2_ast::ExprBody, target_name: &str) -> bool {
        use baml_compiler2_ast::Expr;
        let target_leaf_name = target_name.rsplit('.').next().unwrap_or(target_name);
        for (_id, expr) in body.exprs.iter() {
            if let Expr::Call { callee, .. } = expr {
                // Check if the callee is a Path containing the target name
                if let Expr::Path(segments) = &body.exprs[*callee] {
                    let callee_name = segments
                        .iter()
                        .map(AsRef::<str>::as_ref)
                        .collect::<Vec<_>>()
                        .join(".");
                    if callee_name == target_name
                        || (segments.len() == 1 && segments[0].as_str() == target_leaf_name)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// O(1) lookup of an [`ExprId`](baml_compiler2_ast::ExprId)'s span in the source map. Returns
    /// the raw index and span length as a `(u32, TextSize)` pair for the candidate list.
    fn expr_span_entry(
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

        let Some(func_scope_id) = ancestors.iter().find(|&&ancestor_id| {
            let scope = &index.scopes[ancestor_id.index() as usize];
            matches!(scope.kind, ScopeKind::Function)
        }) else {
            return (None, vec![]);
        };

        let func_scope_range = index.scopes[func_scope_id.index() as usize].range;
        if let Some(func_loc) = baml_compiler2_ppir::item_data::file_functions(self, source_file)
            .iter()
            .copied()
            .filter(|&loc| {
                baml_compiler2_ppir::item_data::function_source_map(self, loc).span
                    == func_scope_range
            })
            .min_by_key(|&loc| {
                func_origin_rank(
                    baml_compiler2_ppir::item_data::function_data(self, loc)
                        .metadata
                        .origin,
                )
            })
        {
            let Some(source_map) = baml_compiler2_ppir::function_body_source_map(self, func_loc)
            else {
                return (None, vec![]);
            };
            let body = baml_compiler2_ppir::function_body(self, func_loc);
            let expr_body = match body.as_ref() {
                baml_compiler2_hir::body::FunctionBody::Expr(eb) => Some(eb),
                _ => None,
            };

            // Collect ALL expression spans containing the cursor.
            let mut containing: Vec<(u32, text_size::TextSize)> = Vec::new();
            #[allow(clippy::cast_possible_truncation)] // arena indices never exceed u32
            for (idx, (_id, range)) in source_map.expr_spans.iter().enumerate() {
                if range.contains(offset) || range.end() == offset {
                    containing.push((idx as u32, range.len()));
                }
            }

            // For each statement span containing the cursor, inject the
            // statement's "graph-relevant expression" into the candidate list.
            // This maps the whole `let x = Call(...)` or `return Obj {...}`
            // line to the expression the graph node uses as source_expr.
            #[allow(clippy::cast_possible_truncation)] // arena indices never exceed u32
            if let Some(eb) = expr_body {
                for (idx, (_id, range)) in source_map.stmt_spans.iter().enumerate() {
                    if !(range.contains(offset) || range.end() == offset) {
                        continue;
                    }
                    let idx_u32 = idx as u32;
                    let stmt_id = la_arena::Idx::<baml_compiler2_ast::Stmt>::from_raw(
                        la_arena::RawIdx::from_u32(idx_u32),
                    );
                    // Look up the expression the graph node uses as source_expr
                    // for this statement, and inject it into the candidate list.
                    let injected_expr = match &eb.stmts[stmt_id] {
                        baml_compiler2_ast::Stmt::HeaderComment { .. } => {
                            let tagged =
                                baml_compiler2_visualization::control_flow::STMT_SOURCE_EXPR_TAG
                                    | idx_u32;
                            Some((tagged, range.len()))
                        }
                        baml_compiler2_ast::Stmt::Let {
                            initializer: Some(init),
                            ..
                        } => Self::expr_span_entry(&source_map, *init),
                        baml_compiler2_ast::Stmt::Return(Some(expr_id)) => {
                            Self::expr_span_entry(&source_map, *expr_id)
                        }
                        baml_compiler2_ast::Stmt::Expr(expr_id) => {
                            Self::expr_span_entry(&source_map, *expr_id)
                        }
                        _ => None,
                    };
                    if let Some(entry) = injected_expr {
                        containing.push(entry);
                    }
                }
            }

            // Region governance for `//#` headers: a header owns the lines from
            // its own line until the next header in the same block, or the end
            // of that block. Clicking anywhere in that region (e.g. inside an
            // `if` that is not itself a rendered node) should be able to select
            // the header node. So find the nearest preceding header whose block
            // still contains the cursor and inject it as the LEAST-specific
            // candidate — a max length sorts it last, behind any real node.
            #[allow(clippy::cast_possible_truncation)] // arena indices never exceed u32
            if let Some(eb) = expr_body {
                let mut governing: Option<(u32, text_size::TextSize)> = None;
                for (idx, (_id, range)) in source_map.stmt_spans.iter().enumerate() {
                    let idx_u32 = idx as u32;
                    let stmt_id = la_arena::Idx::<baml_compiler2_ast::Stmt>::from_raw(
                        la_arena::RawIdx::from_u32(idx_u32),
                    );
                    if !matches!(
                        &eb.stmts[stmt_id],
                        baml_compiler2_ast::Stmt::HeaderComment { .. }
                    ) {
                        continue;
                    }
                    // The header must begin at or before the cursor.
                    if range.start() > offset {
                        continue;
                    }
                    // ...and its own block must still contain the cursor —
                    // otherwise the header's region ended when that block closed.
                    let header_scope = index.scope_at_offset(range.start(), None);
                    let header_scope_range = index.scopes[header_scope.index() as usize].range;
                    if !(header_scope_range.contains(offset) || header_scope_range.end() == offset)
                    {
                        continue;
                    }
                    // Nearest preceding header wins (the next header supersedes).
                    let take = match governing {
                        None => true,
                        Some((_, start)) => range.start() > start,
                    };
                    if take {
                        governing = Some((idx_u32, range.start()));
                    }
                }
                if let Some((idx_u32, _)) = governing {
                    let tagged =
                        baml_compiler2_visualization::control_flow::STMT_SOURCE_EXPR_TAG | idx_u32;
                    // Max length → sorts last, so any real expression node under
                    // the cursor is still preferred over the governing header.
                    containing.push((tagged, func_scope_range.len()));
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

/// Scan backwards through the source text that precedes a declaration and
/// return the title of the nearest `//#` header comment, if it is separated
/// from the declaration only by blank lines and regular `//` comments.
fn header_title_above(before: &str) -> Option<String> {
    for line in before.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("//") else {
            // Reached code — no header directly above the declaration.
            return None;
        };
        if let Some(title) = rest.strip_prefix('#') {
            let title = title.trim_start_matches('#').trim();
            if title.is_empty() {
                return None;
            }
            return Some(title.to_string());
        }
        // Regular `//` or `///` comment between the header and the
        // declaration — keep scanning upwards.
    }
    None
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
    fn removed_files_are_revived_not_recreated() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        let path = std::path::Path::new("/tmp/churn.baml");
        let original =
            db.add_or_update_file(path, "function A(input: string) -> string {\n  input\n}\n");
        let original_id = original.file_id(&db);
        let baseline = crate::collect_compiler2_diagnostics(&db).len();

        // Branch switches and codegen delete and recreate files; each cycle
        // must revive the tombstoned salsa input instead of minting a new
        // immortal one.
        for i in 0..3 {
            db.remove_file(path);
            assert!(
                crate::collect_compiler2_diagnostics(&db).len() >= baseline,
                "diagnostics must still compute while the file is removed"
            );
            let revived = db.add_or_update_file(
                path,
                &format!("function A(input: string) -> string {{\n  //# v{i}\n  input\n}}\n"),
            );
            assert_eq!(
                revived.file_id(&db),
                original_id,
                "re-adding a removed path must reuse its SourceFile input"
            );
        }

        assert_eq!(crate::collect_compiler2_diagnostics(&db).len(), baseline);
        assert!(db.ast_control_flow_graph("A").is_some());
    }

    #[test]
    fn callee_graphs_are_still_inlined_per_call_site() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        db.add_or_update_file(
            std::path::Path::new("/tmp/diamond.baml"),
            r#"
function Leaf(input: string) -> string {
  //# leaf work
  let a = input;
  a
}

function Mid(input: string) -> string {
  let a = Leaf(input);
  let b = Leaf(a);
  b
}

function Top(input: string) -> string {
  let a = Mid(input);
  let b = Mid(a);
  b
}
"#,
        );
        let leaf = db.ast_control_flow_graph("Leaf").unwrap();
        let mid = db.ast_control_flow_graph("Mid").unwrap();
        let top = db.ast_control_flow_graph("Top").unwrap();
        // Memoization must not change the inlined-output shape: every call
        // site still receives its own copy of the callee graph.
        assert!(
            mid.nodes.len() > leaf.nodes.len(),
            "Mid should contain inlined copies of Leaf ({} vs {})",
            mid.nodes.len(),
            leaf.nodes.len()
        );
        assert!(
            top.nodes.len() > mid.nodes.len(),
            "Top should contain inlined copies of Mid ({} vs {})",
            top.nodes.len(),
            mid.nodes.len()
        );
    }

    #[test]
    fn method_calls_inline_concrete_runner_graphs_through_generic_dispatch() {
        use baml_compiler2_visualization::control_flow::{
            NodeType, prepare_control_flow_graph_for_visualization,
        };

        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        db.add_or_update_file(
            std::path::Path::new("/tmp/runner.baml"),
            r#"
interface Runner<Input> {
  function run(self, input: Input) -> string throws never
}

class Task {
  function run<R extends Runner<Task>>(
    self,
    runner: R,
  ) -> string throws never {
    //# Dispatch the task to its runner
    runner.run(self)
  }
}

class Agent {
  implements Runner<Task> {
    function run(self, input: Task) -> string throws never {
      //# Initialize the agent
      let steps = 0;
      //# Run agent steps until completion
      while (steps < 1) {
        //## Advance one agent step
        steps = steps + 1;
      }
      "done"
    }
  }
}

function observe_an_agent() -> string throws never {
  let task = Task {};
  task.run(runner = Agent {})
}
"#,
        );

        let graph = db
            .ast_control_flow_graph("observe_an_agent")
            .expect("expected graph for observe_an_agent");
        let labels = graph
            .nodes
            .values()
            .map(|node| node.label.as_str())
            .collect::<Vec<_>>();

        assert!(
            labels.contains(&"Dispatch the task to its runner"),
            "Task.run should be inlined into the entry graph; got {labels:?}"
        );
        assert!(
            labels.contains(&"Run agent steps until completion"),
            "the concrete Agent.run body should be inlined through Runner.run; got {labels:?}"
        );
        assert!(
            graph
                .nodes
                .values()
                .any(|node| node.node_type == NodeType::Loop),
            "the concrete Agent.run loop should be visible; got {labels:?}"
        );
        let prepared = prepare_control_flow_graph_for_visualization(&graph);
        assert!(
            prepared.nodes.values().any(|node| {
                node.label == "Run agent steps until completion" || node.node_type == NodeType::Loop
            }),
            "the rendered graph should retain the concrete agent loop"
        );
    }

    #[test]
    fn recursive_callee_cache_is_scoped_by_active_expansions() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        db.add_or_update_file(
            std::path::Path::new("/tmp/recursive-cache.baml"),
            r#"
//# a step
function A(input: string) -> string {
  let next = B(input);
  next
}

//# b step
function B(input: string) -> string {
  let looped = A(input);
  let done = Leaf(looped);
  done
}

//# leaf step
function Leaf(input: string) -> string {
  input
}

function Top(input: string) -> string {
  let first = A(input);
  let second = B(first);
  second
}
"#,
        );

        let graph = db.ast_control_flow_graph("Top").unwrap();
        let a_step_count = graph
            .nodes
            .values()
            .filter(|node| node.label == "a step")
            .count();

        assert!(
            a_step_count >= 2,
            "direct B expansion must not reuse a B graph truncated under A recursion; got {a_step_count} A call node(s)"
        );
    }

    #[test]
    fn deep_call_chains_are_capped_not_exponential() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        // Depth 12, fan-out 3: fully inlined and uncapped this is 3^12 ≈ 531k
        // nodes per top-level function (and exponential build time without
        // per-callee memoization).
        let mut src = String::from(
            "function F12(input: string) -> string {\n  //# leaf\n  let a = input;\n  a\n}\n",
        );
        for i in (0..12).rev() {
            use std::fmt::Write as _;
            let callee = i + 1;
            let _ = write!(
                src,
                "function F{i}(input: string) -> string {{\n  let v0 = F{callee}(input);\n  let v1 = F{callee}(v0);\n  let v2 = F{callee}(v1);\n  v2\n}}\n"
            );
        }
        db.add_or_update_file(std::path::Path::new("/tmp/chain.baml"), &src);

        let graph = db.ast_control_flow_graph("F0").unwrap();
        assert!(
            graph.nodes.len() <= CFG_EXPANSION_NODE_BUDGET,
            "inlined graph must respect the node budget, got {}",
            graph.nodes.len()
        );
    }

    #[test]
    fn header_above_if_keeps_all_branch_arms() {
        use baml_compiler2_visualization::control_flow::{
            NodeType, prepare_control_flow_graph_for_visualization,
        };
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        db.add_or_update_file(
            std::path::Path::new("/tmp/wf.baml"),
            r#"
function classify(text: string) -> string {
  let t = text.to_lower_case();
  //# check sentiment
  if (t.includes("love")) { "positive" } else { "negative" }
}
"#,
        );
        let graph = db.ast_control_flow_graph("classify").unwrap();
        let prepared = prepare_control_flow_graph_for_visualization(&graph);
        // The `//# check sentiment` header sits directly above the if, so the
        // whole branch group and both arms must survive pruning, even though
        // neither arm holds its own anchor (header / LLM call).
        let arms = prepared
            .nodes
            .values()
            .filter(|n| matches!(n.node_type, NodeType::BranchArm))
            .count();
        assert!(
            arms >= 2,
            "a header directly above an if should keep all its branch arms; got {arms}"
        );
        assert!(
            prepared
                .nodes
                .values()
                .any(|n| matches!(n.node_type, NodeType::BranchGroup)),
            "the annotated branch group should survive pruning"
        );
    }

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
        // The root carries the function's declaration span so clicking it in
        // the playground selects the function (it has no `source_expr`, so this
        // is attached explicitly rather than via the source map).
        let root_span = root
            .source_span
            .as_ref()
            .expect("FunctionRoot should have a source span");
        assert!(
            root_span.end_offset > root_span.start_offset,
            "FunctionRoot span should be non-empty"
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
        assert!(
            graph
                .nodes
                .values()
                .filter(|n| matches!(n.node_type, NodeType::HeaderContextEnter))
                .all(|n| n.source_span.is_some()),
            "header graph nodes should include source spans"
        );

        // Edges should be non-empty.
        assert!(!graph.edges_by_src.is_empty(), "graph should have edges");
    }

    #[test]
    fn graph_source_spans_use_vscode_utf16_columns() {
        use baml_compiler2_visualization::control_flow::NodeType;

        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        let src = r#"function Workflow() -> string { let rocket = "🚀"; Summarize(rocket) }"#;
        db.add_or_update_file(std::path::Path::new("/tmp/workflow.baml"), src);

        let graph = db.ast_control_flow_graph("Workflow").unwrap();
        let call_span = graph
            .nodes
            .values()
            .find(|node| {
                matches!(node.node_type, NodeType::OtherScope) && node.label == "Summarize(rocket)"
            })
            .and_then(|node| node.source_span.as_ref())
            .expect("call graph node should have a source span");

        let byte_start = src.find("Summarize(rocket)").unwrap();
        let byte_end = byte_start + "Summarize(rocket)".len();
        assert_eq!(call_span.start_offset, u32::try_from(byte_start).unwrap());
        assert_eq!(call_span.end_offset, u32::try_from(byte_end).unwrap());
        assert_eq!(
            call_span.column,
            u32::try_from(src[..byte_start].encode_utf16().count()).unwrap()
        );
        assert_eq!(
            call_span.end_column,
            u32::try_from(src[..byte_end].encode_utf16().count()).unwrap()
        );
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)] // tiny test fixtures fit in u32
    fn cursor_in_header_region_selects_governing_header() {
        use baml_compiler2_visualization::control_flow::{NodeType, STMT_SOURCE_EXPR_TAG};

        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        let src = r#"
function Workflow(input: string) -> string {
    //# Prepare
    let x = input;
    //# Process
    if (input == "go") {
        "yes"
    } else {
        "no"
    }
}
"#;
        db.add_or_update_file(std::path::Path::new("/tmp/wf.baml"), src);

        // The "Process" header node's tagged source_expr.
        let graph = db.ast_control_flow_graph("Workflow").unwrap();
        let process_expr = graph
            .nodes
            .values()
            .find(|n| matches!(n.node_type, NodeType::HeaderContextEnter) && n.label == "Process")
            .and_then(|n| n.source_expr)
            .expect("Process header should exist with a source_expr");
        assert!(process_expr & STMT_SOURCE_EXPR_TAG != 0);

        // Cursor inside the if-arm (`"yes"`) — that arm is not itself a rendered
        // node, but it lives in the region governed by "//# Process".
        let offset = (src.find("\"yes\"").unwrap() as u32) + 1;
        let ctx = db.playground_cursor_context("/tmp/wf.baml", offset);
        assert!(
            ctx.source_expr_candidates.contains(&process_expr),
            "cursor inside the Process region should offer the Process header; got {:?}",
            ctx.source_expr_candidates
        );

        // Cursor on the `let x = input;` line is governed by "//# Prepare", not
        // "//# Process" (the later header only governs from its own line down).
        let prepare_expr = graph
            .nodes
            .values()
            .find(|n| matches!(n.node_type, NodeType::HeaderContextEnter) && n.label == "Prepare")
            .and_then(|n| n.source_expr)
            .expect("Prepare header should exist with a source_expr");
        let let_offset = (src.find("let x = input;").unwrap() as u32) + 4;
        let let_ctx = db.playground_cursor_context("/tmp/wf.baml", let_offset);
        assert!(
            let_ctx.source_expr_candidates.contains(&prepare_expr),
            "cursor on the let line should offer the Prepare header; got {:?}",
            let_ctx.source_expr_candidates
        );
        assert!(
            !let_ctx.source_expr_candidates.contains(&process_expr),
            "the later Process header must not govern lines above it"
        );
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
    fn test_ast_control_flow_graph_accepts_playground_qualified_namespace_name() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        db.add_or_update_file(
            std::path::Path::new("/tmp/ns_demo/workflow.baml"),
            r#"
function Workflow(input: string) -> string {
    input
}
"#,
        );

        assert!(
            db.ast_control_flow_graph("Workflow").is_some(),
            "legacy bare lookup should keep working"
        );
        assert!(
            db.ast_control_flow_graph("demo.Workflow").is_some(),
            "playground-qualified lookup should resolve the namespaced function"
        );
    }

    #[test]
    fn test_ast_control_flow_graph_llm_is_single_semantic_node() {
        use baml_compiler2_visualization::control_flow::NodeType;

        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        db.add_or_update_file(
            std::path::Path::new("/tmp/llm.baml"),
            r##"
function Summarize(input: string) -> string {
    client: GPT4
    prompt: `Summarize ${input}`
}
"##,
        );

        let graph = db
            .ast_control_flow_graph("Summarize")
            .expect("expected graph for LLM function");

        assert_eq!(graph.nodes.len(), 1);
        let node = graph.nodes.values().next().unwrap();
        assert!(matches!(node.node_type, NodeType::LlmFunction));
        assert_eq!(node.label, "Summarize");
        assert_eq!(node.llm_client.as_deref(), Some("GPT4"));
        let source_span = node.source_span.as_ref().expect("LLM node has source span");
        assert_eq!(source_span.file_path, "/tmp/llm.baml");
        assert!(source_span.end_offset > source_span.start_offset);
        assert!(graph.edges_by_src.is_empty());
    }

    #[test]
    fn test_ast_control_flow_graph_expands_user_function_match_at_call_site() {
        use baml_compiler2_visualization::control_flow::{
            NodeType, prepare_control_flow_graph_for_visualization,
        };

        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        let path = std::path::Path::new("/tmp/game.baml");
        let source = r#"
function ScoreGuess(outcome: string) -> string {
    match (outcome) {
        "hit" => {
            //# matched correct
            "correct"
        }
        _ => {
            //# matched default
            "wrong"
        }
    }
}

function GuessingGame() -> string {
    let outcome = "hit";
    ScoreGuess(outcome)
}
"#;
        db.add_or_update_file(path, source);

        let call_offset =
            u32::try_from(source.rfind("ScoreGuess(outcome)").expect("call exists")).unwrap();
        let call_ctx = db.playground_cursor_context(path.to_str().unwrap(), call_offset);
        assert_eq!(call_ctx.function_name.as_deref(), Some("ScoreGuess"));
        assert_eq!(
            call_ctx.source_expr_function_name.as_deref(),
            Some("GuessingGame"),
            "call-site expression ids should be owned by the caller"
        );

        let match_offset = u32::try_from(source.find("match (outcome)").expect("match exists"))
            .expect("offset fits");
        let match_ctx = db.playground_cursor_context(path.to_str().unwrap(), match_offset);
        assert_eq!(match_ctx.function_name.as_deref(), Some("ScoreGuess"));
        assert_eq!(
            match_ctx.source_expr_function_name.as_deref(),
            Some("ScoreGuess"),
            "callee body expression ids should be owned by the callee"
        );

        let graph = db
            .ast_control_flow_graph("GuessingGame")
            .expect("expected graph for GuessingGame");

        let call_node = graph
            .nodes
            .values()
            .find(|node| node.label == "ScoreGuess(outcome)")
            .expect("caller graph should contain the ScoreGuess call node");
        let match_node = graph
            .nodes
            .values()
            .find(|node| {
                matches!(node.node_type, NodeType::BranchGroup)
                    && node.label == "match (outcome)"
                    && node.log_filter_key.starts_with("ScoreGuess|")
            })
            .expect("callee match should be expanded under the call node");
        assert_eq!(match_node.parent_node_id, Some(call_node.id));
        assert!(
            graph.nodes.values().any(|node| {
                matches!(node.node_type, NodeType::HeaderContextEnter)
                    && node.label == "matched correct"
                    && node.log_filter_key.starts_with("ScoreGuess|")
            }),
            "expanded match arms should keep their branch header nodes"
        );

        let prepared = prepare_control_flow_graph_for_visualization(&graph);
        let call_node = prepared
            .nodes
            .get(&call_node.id)
            .expect("call node should remain visible");
        assert!(
            call_node.is_container,
            "call node should become a visualization container for the expanded callee graph"
        );
        let match_node = prepared
            .nodes
            .values()
            .find(|node| {
                matches!(node.node_type, NodeType::BranchGroup)
                    && node.label == "match (outcome)"
                    && node.log_filter_key.starts_with("ScoreGuess|")
            })
            .expect("prepared graph should keep the expanded match group");
        let edge_labels: Vec<_> = prepared
            .edges_by_src
            .get(&match_node.id)
            .map(|edges| {
                edges
                    .iter()
                    .filter_map(|edge| edge.label.as_deref())
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            edge_labels.contains(&r#""hit""#) && edge_labels.contains(&"default"),
            "prepared match fan-out edges should preserve match arm labels, got {edge_labels:?}"
        );
    }

    #[test]
    fn test_llm_call_node_is_marked_and_always_rendered() {
        use baml_compiler2_visualization::control_flow::{
            NodeType, prepare_control_flow_graph_for_visualization,
        };

        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        db.add_or_update_file(
            std::path::Path::new("/tmp/wf.baml"),
            r##"
function Summarize(input: string) -> string {
    client: GPT4
    prompt: `Summarize ${input}`
}

function Workflow(input: string) -> string {
    let x = Summarize(input);
    x
}
"##,
        );

        let graph = db
            .ast_control_flow_graph("Workflow")
            .expect("expected graph for Workflow");
        let call_node = graph
            .nodes
            .values()
            .find(|n| n.label == "Summarize(input)")
            .expect("caller graph should contain the Summarize call node");
        assert!(
            matches!(call_node.node_type, NodeType::LlmFunction),
            "LLM call node should be marked as LlmFunction, got {:?}",
            call_node.node_type
        );
        assert_eq!(call_node.llm_client.as_deref(), Some("GPT4"));

        // Even with no //# headers anywhere, the LLM call must survive
        // visualization prep.
        let prepared = prepare_control_flow_graph_for_visualization(&graph);
        assert!(
            prepared.nodes.contains_key(&call_node.id),
            "LLM call must always render"
        );
    }

    #[test]
    fn test_cross_namespace_llm_call_node_is_marked_and_rendered() {
        use baml_compiler2_visualization::control_flow::{
            NodeType, prepare_control_flow_graph_for_visualization,
        };

        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        db.add_or_update_file(
            std::path::Path::new("/tmp/ns_workflows/ns_prompts/summarize.baml"),
            r##"
function Summarize(input: string) -> string {
    client: GPT4
    prompt: `Summarize ${input}`
}
"##,
        );
        db.add_or_update_file(
            std::path::Path::new("/tmp/ns_workflows/workflow.baml"),
            r#"
function Workflow(input: string) -> string {
    prompts.Summarize(input)
}
"#,
        );

        let graph = db
            .ast_control_flow_graph("workflows.Workflow")
            .expect("expected graph for workflows.Workflow");
        let call_node = graph
            .nodes
            .values()
            .find(|node| node.label == "prompts.Summarize(input)")
            .expect("caller graph should contain the cross-namespace LLM call node");
        assert!(
            matches!(call_node.node_type, NodeType::LlmFunction),
            "cross-namespace LLM call node should be marked as LlmFunction, got {:?}",
            call_node.node_type
        );
        assert_eq!(call_node.llm_client.as_deref(), Some("GPT4"));

        let prepared = prepare_control_flow_graph_for_visualization(&graph);
        assert!(
            prepared.nodes.contains_key(&call_node.id),
            "cross-namespace LLM call must survive visualization preparation"
        );
    }

    #[test]
    fn test_dependency_call_does_not_expand_same_named_user_function() {
        use baml_compiler2_visualization::control_flow::NodeType;

        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        db.add_or_update_file(
            std::path::Path::new("/tmp/ns_http/fetch.baml"),
            r##"
function fetch(input: string) -> string {
    client: UserClient
    prompt: `User fetch ${input}`
}
"##,
        );
        db.add_or_update_file(
            std::path::Path::new("/tmp/workflow.baml"),
            r#"
function Workflow() -> int {
    let response = baml.http.fetch("https://example.com");
    response.status
}
"#,
        );

        let graph = db
            .ast_control_flow_graph("Workflow")
            .expect("expected graph for Workflow");
        let call_node = graph
            .nodes
            .values()
            .find(|node| node.label.contains("baml.http.fetch"))
            .expect("caller graph should contain the dependency call node");
        assert!(
            matches!(call_node.node_type, NodeType::OtherScope),
            "dependency call must not be marked from the same-named user function, got {:?}",
            call_node.node_type
        );
        assert_eq!(call_node.llm_client, None);
    }

    #[test]
    fn test_function_level_header_labels_call_nodes() {
        use baml_compiler2_visualization::control_flow::{
            NodeType, prepare_control_flow_graph_for_visualization,
        };

        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        db.add_or_update_file(
            std::path::Path::new("/tmp/wf.baml"),
            r#"
//# process stuff
function somefunc(x: int) -> int {
    //# inner step
    x + 1
}

//# do the thing

// a regular note between the header and the function is fine
function plain(x: int) -> int { x + 1 }

function Caller(x: int) -> int {
    let a = somefunc(x);
    let b = plain(a);
    b
}
"#,
        );

        let graph = db
            .ast_control_flow_graph("Caller")
            .expect("expected graph for Caller");

        let somefunc_node = graph
            .nodes
            .values()
            .find(|n| n.label == "process stuff")
            .expect("somefunc call should be relabeled from its function-level header");
        assert!(matches!(
            somefunc_node.node_type,
            NodeType::HeaderContextEnter
        ));
        // The callee's body headers nest under the relabeled call node.
        assert!(
            graph
                .nodes
                .values()
                .any(|n| n.label == "inner step" && n.log_filter_key.starts_with("somefunc|")),
            "callee body headers should be expanded under the call node"
        );

        let plain_node = graph
            .nodes
            .values()
            .find(|n| n.label == "do the thing")
            .expect("plain call should be relabeled from its function-level header");

        let prepared = prepare_control_flow_graph_for_visualization(&graph);
        assert!(
            prepared.nodes.contains_key(&somefunc_node.id),
            "annotated function call must render"
        );
        assert!(
            prepared.nodes.contains_key(&plain_node.id),
            "function-level header alone is enough to render the call node"
        );
    }

    #[test]
    fn test_function_header_title_keeps_searching_after_missing_header() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        db.add_or_update_file(
            std::path::Path::new("/tmp/dupe.baml"),
            r#"
function helper(x: int) -> int { x }

//# titled helper
function helper(x: int) -> int { x + 1 }
"#,
        );

        assert_eq!(
            db.function_header_title("helper"),
            Some("titled helper".to_string())
        );
    }

    #[test]
    fn test_function_header_title_ignores_ambiguous_same_name_headers() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        db.add_or_update_file(
            std::path::Path::new("/tmp/dupe.baml"),
            r#"
//# first helper
function helper(x: int) -> int { x }

//# second helper
function helper(x: int) -> int { x + 1 }
"#,
        );

        assert_eq!(db.function_header_title("helper"), None);
    }

    #[test]
    fn test_early_return_renders_as_terminal_node() {
        use baml_compiler2_visualization::control_flow::{
            NodeType, prepare_control_flow_graph_for_visualization,
        };

        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        db.add_or_update_file(
            std::path::Path::new("/tmp/early.baml"),
            r#"
function Early(x: int) -> string {
    //# Validate
    if (x < 0) {
        //# bail out
        return "neg";
    }
    //# Continue
    "ok"
}
"#,
        );

        let graph = db
            .ast_control_flow_graph("Early")
            .expect("expected graph for Early");
        let ret_node = graph
            .nodes
            .values()
            .find(|n| matches!(n.node_type, NodeType::Return))
            .expect("early return should create a Return node");
        assert!(ret_node.label.starts_with("return"));

        let prepared = prepare_control_flow_graph_for_visualization(&graph);
        let prepared_ret = prepared
            .nodes
            .get(&ret_node.id)
            .expect("return inside annotated branch should render");
        let outgoing = prepared
            .edges_by_src
            .get(&prepared_ret.id)
            .map(Vec::len)
            .unwrap_or(0);
        assert_eq!(
            outgoing, 0,
            "return node must not be connected to later nodes"
        );
    }

    #[test]
    fn test_return_call_is_not_expanded_under_return_node() {
        use baml_compiler2_visualization::control_flow::NodeType;

        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        db.add_or_update_file(
            std::path::Path::new("/tmp/return-call.baml"),
            r#"
function Helper() -> string {
    //# helper body
    "ok"
}

function Early(x: int) -> string {
    if (x < 0) {
        return Helper();
    }

    "later"
}
"#,
        );

        let graph = db
            .ast_control_flow_graph("Early")
            .expect("expected graph for Early");
        let ret_node = graph
            .nodes
            .values()
            .find(|n| matches!(n.node_type, NodeType::Return))
            .expect("return call should create a Return node");

        assert_eq!(ret_node.label, "return Helper()");
        assert!(
            graph
                .edges_by_src
                .get(&ret_node.id)
                .is_none_or(Vec::is_empty),
            "return-call node must stay terminal instead of owning callee edges"
        );
        assert!(
            graph.nodes.values().all(|n| n.label != "helper body"),
            "callee body headers should not be expanded below a terminal return"
        );
    }

    #[test]
    fn ast_control_flow_graph_builds_new_style_test_bodies() {
        use baml_compiler2_visualization::control_flow::NodeType;

        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        let src = r#"
function Workflow(input: int) -> int {
  //# Choose result
  input + 1
}

test "renders workflow" {
  let result = Workflow(41)
  assert.equal(result, 42)
}
"#;
        db.add_or_update_file(std::path::Path::new("/tmp/tests.baml"), src);

        let workflow_graph = db
            .ast_control_flow_graph("Workflow")
            .expect("workflow should have a graph");
        assert!(
            workflow_graph
                .nodes
                .values()
                .any(|node| node.node_type == NodeType::HeaderContextEnter),
            "fixture workflow must have control flow: {:#?}",
            workflow_graph.nodes
        );
        let graph = db
            .ast_control_flow_graph("root::renders workflow")
            .expect("new-style test should have a graph");
        let root = graph
            .nodes
            .values()
            .find(|node| node.node_type == NodeType::FunctionRoot)
            .expect("test graph should have a root");
        let root_span = root
            .source_span
            .as_ref()
            .expect("test graph root should navigate to its declaration");
        let name_start = src.find("\"renders workflow\"").unwrap();
        assert_eq!(root_span.start_offset as usize, name_start);
        assert_eq!(
            root_span.end_offset as usize,
            name_start + "\"renders workflow\"".len()
        );
        assert!(
            graph
                .nodes
                .values()
                .any(|node| node.label.contains("Workflow")),
            "the test body's workflow call should be represented"
        );
        let prepared =
            baml_compiler2_visualization::control_flow::prepare_control_flow_graph_for_visualization(
                &graph,
            );
        assert!(
            prepared.nodes.values().any(|node| {
                node.node_type == NodeType::HeaderContextEnter && node.label == "Choose result"
            }),
            "the selected test should render the called workflow's control flow; raw={:#?}; prepared={:#?}",
            graph.nodes,
            prepared.nodes
        );
    }

    #[test]
    fn test_header_title_above() {
        assert_eq!(
            header_title_above("//# process stuff\n"),
            Some("process stuff".to_string())
        );
        assert_eq!(
            header_title_above("//# process stuff\n\n// note\n/// docs\n"),
            Some("process stuff".to_string()),
            "blank lines and comments between header and declaration are skipped"
        );
        assert_eq!(
            header_title_above("//## nested level\n"),
            Some("nested level".to_string())
        );
        assert_eq!(
            header_title_above("//# old header\n}\n"),
            None,
            "code between the header and the declaration stops the search"
        );
        assert_eq!(header_title_above("// just a comment\n"), None);
        assert_eq!(header_title_above(""), None);
    }

    #[test]
    fn test_playground_cursor_context_inside_llm_prefers_parent_function() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        let path = std::path::Path::new("/tmp/llm.baml");
        let source = r##"
function Summarize(input: string) -> string {
    client: GPT4
    prompt: `Summarize ${input}`
}
"##;
        db.add_or_update_file(path, source);

        for needle in ["client", "GPT4", "prompt", "Summarize ${input}"] {
            let offset = u32::try_from(source.find(needle).expect("needle exists")).unwrap();
            let ctx = db.playground_cursor_context(path.to_str().unwrap(), offset);

            assert_eq!(
                ctx.function_name.as_deref(),
                Some("Summarize"),
                "cursor on {needle:?} should select the top-level LLM function"
            );
        }
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
