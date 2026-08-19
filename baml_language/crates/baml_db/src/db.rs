//! `ProjectDatabase` — the concrete Salsa database of the BAML compiler
//! (rust-analyzer's `RootDatabase` analog).
//!
//! `ProjectDatabase` owns the Salsa storage directly (the ty/ruff pattern) and
//! implements every compiler `Db` trait. Its public surface is the
//! source-root model: files are grouped into [`SourceRoot`]s (one package
//! each), the roots live in a single [`SourceRootTable`] input, and every
//! file-level mutation goes through the root that owns the file.

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use baml_base::{FileId, Name, SourceFile, SourceRoot, SourceRootKind, SourceRootTable};
use baml_compiler2_hir::inputs::{
    MountedPackages, SeededCallableThrows, SeededStdlibInterface, SeededThrowFacts,
};
use salsa::Setter;

/// Type alias for Salsa event callbacks.
pub type EventCallback = Box<dyn Fn(salsa::Event) + Send + Sync + 'static>;

/// What a caller asks [`ProjectDatabase::add_source_root`] to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRootSpec {
    /// Root directory (canonicalized when it exists on disk; virtual paths
    /// such as `<builtin>/baml` are kept verbatim).
    pub path: PathBuf,
    /// The package every file under the root belongs to.
    pub package: Name,
    pub kind: SourceRootKind,
}

/// Why [`ProjectDatabase::add_source_root`] refused a spec.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SourceRootError {
    /// Another live root already carries this package name — one root per
    /// package, in every kind (two `Stdlib` roots may not share a name
    /// either).
    #[error("package name `{name}` is already taken by another source root")]
    PackageNameTaken { name: Name, by: SourceRoot },
    /// A live root already sits at this (canonical) path.
    #[error("a source root already exists at this path")]
    PathTaken(SourceRoot),
    /// A `Workspace` root already exists. The compiler is single-world (impl
    /// resolution, `definition_of`, and `Ty`'s `Package::Local` carry no
    /// viewpoint), so a database holds at most one `Workspace` root until the
    /// world-viewpoint rework lands.
    #[error("the database already has a workspace source root")]
    SecondWorkspaceRoot(SourceRoot),
}

/// The main database for BAML projects.
///
/// `ProjectDatabase` owns the Salsa storage directly and implements all the
/// compiler `Db` traits. It provides high-level APIs for:
/// - Source-root management (add/remove roots, longest-prefix lookup)
/// - File management within a root (add/update/remove files)
/// - Diagnostics collection via `check()`
///
/// ## Example
///
/// ```ignore
/// let mut db = ProjectDatabase::new();
/// db.ensure_stdlib_sources();
/// let root = db.add_source_root(SourceRootSpec {
///     path: PathBuf::from("/my/project"),
///     package: Name::new("user"),
///     kind: SourceRootKind::Workspace,
/// })?;
/// db.add_or_update_file_in(root, Path::new("/my/project/main.baml"), "class Foo {}");
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
    /// The ordered set of source roots.
    ///
    /// A real `#[salsa::input]` handle, created **once** (empty) in
    /// [`Self::from_storage`] and thereafter mutated in place through its
    /// Salsa setter, so it is always `Some`. Present-from-construction is what
    /// lets table-reading queries record a *tracked* dependency on the
    /// initially-empty root list; were the handle absent until the first root,
    /// a query memoized while it was `None` would record no dependency and a
    /// later root on a reused database would be invisible to the memo.
    source_roots: Option<SourceRootTable>,
    /// Per-file throw facts seeded from a previous compile (bytecode cache).
    ///
    /// Same present-from-construction discipline as `source_roots`: a real
    /// `#[salsa::input]` handle created **once** (empty) in
    /// [`Self::from_storage`] and thereafter mutated *in place* by
    /// [`Self::set_seeded_throw_facts`] via its Salsa setter, so it is always
    /// `Some`. That is what makes `throw_inference::file_throw_facts` read the
    /// seed map through a **tracked** dependency: mutating via the setter bumps
    /// the revision and correctly invalidates dependents.
    seeded_throw_facts: Option<SeededThrowFacts>,
    /// Stdlib packages' typed interfaces seeded from a previous compile
    /// (bytecode cache). Same present-from-construction discipline as
    /// `seeded_throw_facts`, so `package_interface::package_interface` reads
    /// the seed through a **tracked** dependency (an absent-then-added handle
    /// would leave a stale memo on a reused database, e.g. the LSP's
    /// long-lived `ProjectDatabase`).
    seeded_stdlib_interface: Option<SeededStdlibInterface>,
    /// Per-function `callable_throws` values seeded from a previous compile
    /// (bytecode cache). Same present-from-construction discipline as the
    /// other seeds, so `callable::callable_throws` reads the seed through a
    /// **tracked** dependency.
    seeded_callable_throws: Option<SeededCallableThrows>,
    /// Source-less dependency packages, present from database construction so
    /// Salsa queries always track the mount map even while it is empty.
    mounted_packages: Option<MountedPackages>,
    /// Maps canonical file paths to their live `SourceFile` handles (every
    /// root's files, stdlib included).
    ///
    /// `Arc`-wrapped (with `Arc::make_mut` at the mutation sites) so cloning a
    /// database handle stays O(1): the parallel check and emit drivers mint a
    /// shared-storage handle per work chunk, and a deep per-clone copy of an
    /// N-entry `PathBuf` map made every clone O(files) — quadratic CPU and
    /// peak RSS across a whole compile.
    file_map: Arc<HashMap<PathBuf, SourceFile>>,
    /// Maps `FileId` to canonical file path for reverse lookup (live files
    /// only). `Arc`-wrapped for the same reason as `file_map`.
    file_id_to_path: Arc<HashMap<FileId, PathBuf>>,
    /// `SourceFile` inputs of removed paths. Salsa never frees inputs, so a
    /// delete/recreate cycle (branch switch, codegen rewriting `.baml`
    /// files) would mint a new immortal input per cycle; instead the input
    /// parks here with empty text (releasing the source string and its
    /// downstream memos) and is revived if the path reappears — under
    /// whichever root then owns it. `Arc`-wrapped for the same reason as
    /// `file_map`.
    removed_file_tombstones: Arc<HashMap<PathBuf, SourceFile>>,
    /// Live roots keyed by canonical path, for the longest-prefix lookup in
    /// [`Self::source_root_for_path`]. `Arc`-wrapped for the same reason as
    /// `file_map`.
    roots_by_path: Arc<BTreeMap<PathBuf, SourceRoot>>,
    /// `SourceRoot` inputs of removed roots, parked by canonical path (their
    /// files emptied and tombstoned) and revived if a root is re-added at the
    /// same path — the same immortal-input discipline as
    /// `removed_file_tombstones`.
    removed_root_tombstones: Arc<HashMap<PathBuf, SourceRoot>>,
}

#[salsa::db]
impl salsa::Database for ProjectDatabase {}

#[salsa::db]
impl baml_compiler2_hir::Db for ProjectDatabase {
    fn source_roots(&self) -> SourceRootTable {
        self.table()
    }

    fn seeded_throw_facts(&self) -> Option<SeededThrowFacts> {
        self.seeded_throw_facts
    }

    fn seeded_stdlib_interface(&self) -> Option<SeededStdlibInterface> {
        self.seeded_stdlib_interface
    }

    fn seeded_callable_throws(&self) -> Option<SeededCallableThrows> {
        self.seeded_callable_throws
    }

    fn mounted_packages(&self) -> Option<MountedPackages> {
        self.mounted_packages
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

/// Canonicalize the longest existing ancestor of `path` and re-append the
/// rest verbatim; keep the whole path as spelled when nothing exists.
///
/// Applying one rule to roots and files alike is what keeps the root
/// prefix lookup coherent: an existing root under a symlinked directory
/// (`/tmp` → `/private/tmp` on macOS) and a not-yet-written file beneath it
/// (an unsaved editor buffer) must normalize to the same prefix. Virtual
/// paths (`<builtin>/...`, test fixtures that never touch the filesystem)
/// have no existing ancestor and are stored and looked up exactly as
/// spelled.
fn canonicalize_lossy(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    let mut ancestor = path;
    let mut remainder = Vec::new();
    while let Some(parent) = ancestor.parent() {
        if let Some(name) = ancestor.file_name() {
            remainder.push(name);
        }
        if let Ok(canonical) = parent.canonicalize() {
            let mut out = canonical;
            for name in remainder.into_iter().rev() {
                out.push(name);
            }
            return out;
        }
        ancestor = parent;
    }
    path.to_path_buf()
}

impl ProjectDatabase {
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

    /// Build a database over `storage`, installing the source-root table and
    /// the seed/mount inputs empty from construction. Holding each
    /// `#[salsa::input]` handle present (not `None`) from the start is what
    /// lets the reading queries record a *tracked* dependency on the
    /// initially-empty values, so a later mutation on a reused database
    /// reliably invalidates their memos; an empty map means "no seeds" and
    /// every file derives honestly. See the field docs.
    fn from_storage(storage: salsa::Storage<Self>) -> Self {
        let mut db = Self {
            storage,
            next_file_id: Arc::new(AtomicU32::new(0)),
            source_roots: None,
            seeded_throw_facts: None,
            seeded_stdlib_interface: None,
            seeded_callable_throws: None,
            mounted_packages: None,
            file_map: Arc::new(HashMap::new()),
            file_id_to_path: Arc::new(HashMap::new()),
            removed_file_tombstones: Arc::new(HashMap::new()),
            roots_by_path: Arc::new(BTreeMap::new()),
            removed_root_tombstones: Arc::new(HashMap::new()),
        };
        db.source_roots = Some(SourceRootTable::new(&db, Vec::new()));
        db.seeded_throw_facts = Some(SeededThrowFacts::new(&db, BTreeMap::new()));
        db.seeded_stdlib_interface = Some(SeededStdlibInterface::new(&db, BTreeMap::new()));
        db.seeded_callable_throws = Some(SeededCallableThrows::new(&db, BTreeMap::new()));
        db.mounted_packages = Some(MountedPackages::new(
            &db,
            BTreeMap::new(),
            std::collections::BTreeSet::new(),
        ));
        db
    }

    /// The always-present source-root table (see the `source_roots` field).
    fn table(&self) -> SourceRootTable {
        self.source_roots
            .unwrap_or_else(|| unreachable!("SourceRootTable is created in ProjectDatabase::new"))
    }

    // ── Source roots ─────────────────────────────────────────────────────────

    /// Install one `Stdlib` root per embedded builtin package (path
    /// `<builtin>/<pkg>`), holding that package's files from
    /// [`baml_builtins2::ALL`].
    ///
    /// Idempotent: a stdlib root that already exists is left in place and its
    /// files are (re)synchronized to the embedded contents, so calling this
    /// on a database that already has the stdlib is a no-op revision-wise.
    ///
    /// # Panics
    ///
    /// Panics if a non-stdlib root already claims a builtin package name —
    /// a caller bug, since those names are reserved
    /// (`baml_compiler2_hir::package::is_reserved_package_name`).
    pub fn ensure_stdlib_sources(&mut self) {
        // Group by package in `ALL` order (a `Vec` of buckets, not a hash
        // map, so root creation order — and therefore table order among the
        // stdlib roots — is the manifest's order, not process-dependent).
        let mut buckets: Vec<(&'static str, Vec<&'static baml_builtins2::BuiltinFile>)> =
            Vec::new();
        for builtin in baml_builtins2::ALL {
            match buckets.iter_mut().find(|(pkg, _)| *pkg == builtin.package) {
                Some((_, files)) => files.push(builtin),
                None => buckets.push((builtin.package, vec![builtin])),
            }
        }
        for (package, builtins) in buckets {
            let path = PathBuf::from(format!("<builtin>/{package}"));
            let root = match self.roots_by_path.get(&path) {
                Some(&root) => root,
                None => self
                    .add_source_root(SourceRootSpec {
                        path,
                        package: Name::new(package),
                        kind: SourceRootKind::Stdlib,
                    })
                    .unwrap_or_else(|err| {
                        panic!("cannot install the stdlib source root for `{package}`: {err}")
                    }),
            };
            let files: Vec<(PathBuf, &'static str)> = builtins
                .into_iter()
                .map(|builtin| (PathBuf::from(builtin.virtual_path()), builtin.contents))
                .collect();
            self.add_or_update_files_in(
                root,
                files
                    .iter()
                    .map(|(path, contents)| (path.as_path(), *contents)),
            );
        }
    }

    /// Add a source root, or revive the tombstoned root at the same path.
    ///
    /// Invariants enforced (each is a [`SourceRootError`]):
    /// - at most one live root per canonical path;
    /// - at most one `Workspace` root per database (the compiler is
    ///   single-world);
    /// - one live root per package name, across every kind. A mounted
    ///   source-less package (an interface blob) may share a name with a
    ///   source root: the runtime compiler mounts a dependency's interface
    ///   and adds a stub source root for the same package, and the compiler's
    ///   source-vs-blob contract decides which is authoritative.
    ///
    /// The root is inserted into its kind's bucket of the table (`Stdlib` <
    /// `Dependency` < `Workspace` < `Dynamic`, [`SourceRootKind`]'s order),
    /// preserving the order invariant `compiler2_all_files` relies on.
    pub fn add_source_root(&mut self, spec: SourceRootSpec) -> Result<SourceRoot, SourceRootError> {
        let SourceRootSpec {
            path,
            package,
            kind,
        } = spec;
        let path = canonicalize_lossy(&path);

        if let Some(&existing) = self.roots_by_path.get(&path) {
            return Err(SourceRootError::PathTaken(existing));
        }
        if kind == SourceRootKind::Workspace
            && let Some(existing) = self.workspace_root()
        {
            return Err(SourceRootError::SecondWorkspaceRoot(existing));
        }
        if let Some(&existing) = self
            .roots_by_path
            .values()
            .find(|root| root.package(self) == package)
        {
            return Err(SourceRootError::PackageNameTaken {
                name: package,
                by: existing,
            });
        }
        // Revive the tombstoned input if a root lived at this path before —
        // creating a fresh input would leak the old one forever.
        let root = match self.removed_root_tombstones.get(&path).copied() {
            Some(root) => {
                Arc::make_mut(&mut self.removed_root_tombstones).remove(&path);
                debug_assert!(
                    root.files(self).is_empty(),
                    "a tombstoned root must have had its files emptied on removal"
                );
                root.set_package(self).to(package);
                root.set_kind(self).to(kind);
                root
            }
            None => SourceRoot::new(self, path.clone(), package, kind, Vec::new()),
        };

        // Insert at the end of this kind's bucket. The table is sorted by
        // kind rank, so `partition_point` finds the boundary.
        let table = self.table();
        let mut roots = table.roots(self).clone();
        let at = roots.partition_point(|root| root.kind(self) <= kind);
        roots.insert(at, root);
        table.set_roots(self).to(roots);
        Arc::make_mut(&mut self.roots_by_path).insert(path, root);
        Ok(root)
    }

    /// Remove a source root: its files are tombstoned (see
    /// [`Self::remove_file`]), the root's `files` is emptied, and the root
    /// itself is parked by path for revival by a later
    /// [`Self::add_source_root`] at the same path.
    ///
    /// Removing a root that is not live is a no-op (and a debug-mode
    /// assertion failure).
    pub fn remove_source_root(&mut self, root: SourceRoot) {
        let table = self.table();
        let position = table.roots(self).iter().position(|r| *r == root);
        debug_assert!(
            position.is_some(),
            "remove_source_root called with a root that is not in the table"
        );
        let Some(position) = position else {
            return;
        };
        let mut roots = table.roots(self).clone();
        roots.remove(position);
        table.set_roots(self).to(roots);

        for file in root.files(self).clone() {
            self.park_file(file);
        }
        root.set_files(self).to(Vec::new());

        let path = root.path(self).clone();
        Arc::make_mut(&mut self.roots_by_path).remove(&path);
        Arc::make_mut(&mut self.removed_root_tombstones).insert(path, root);
    }

    /// All live source roots, in table order (`Stdlib` < `Dependency` <
    /// `Workspace`).
    pub fn source_roots(&self) -> Vec<SourceRoot> {
        self.table().roots(self).clone()
    }

    /// The sole `Workspace` root, if one has been added.
    pub fn workspace_root(&self) -> Option<SourceRoot> {
        self.table()
            .roots(self)
            .iter()
            .copied()
            .find(|root| root.kind(self) == SourceRootKind::Workspace)
    }

    /// The live root whose path is the longest prefix of `path` (after
    /// canonicalization), if any.
    pub fn source_root_for_path(&self, path: &Path) -> Option<SourceRoot> {
        let path = canonicalize_lossy(path);
        // Every prefix of `path` sorts at or before `path` in `PathBuf`'s
        // component-wise order, and among prefixes of one path the longer
        // sorts later — so the first prefix met walking backwards from
        // `path` is the longest one.
        self.roots_by_path
            .range::<Path, _>((
                std::ops::Bound::Unbounded,
                std::ops::Bound::Included(path.as_path()),
            ))
            .rev()
            .find(|(root_path, _)| path.starts_with(root_path))
            .map(|(_, root)| *root)
    }

    // ── Files ────────────────────────────────────────────────────────────────

    /// Mint a fresh `SourceFile` input owned by `root`.
    fn new_file(&mut self, root: SourceRoot, path: PathBuf, text: &str) -> SourceFile {
        let file_id = FileId::new(self.next_file_id.fetch_add(1, Ordering::SeqCst));
        SourceFile::new(self, text.to_owned(), path, file_id, false, root)
    }

    /// Create, revive, or update the file at `path` so that it is owned by
    /// `root` with content `text`, registering it in the path maps.
    ///
    /// Returns the file and whether it is *newly* owned by `root` — in which
    /// case the caller must append it to `root`'s `files` (callers batch
    /// that write). A live file whose current owner is a different root is
    /// moved: removed from the old root's `files` here and re-pointed at
    /// `root`, so a root added or removed under an existing file never
    /// leaves the file attributed to the wrong package.
    fn upsert_file(&mut self, root: SourceRoot, path: &Path, text: &str) -> (SourceFile, bool) {
        let path = canonicalize_lossy(path);

        if let Some(&existing) = self.file_map.get(&path) {
            // Skip the setter when the text is unchanged: a Salsa set always
            // bumps the revision, and re-parsing an identical file to reach
            // early cutoff is wasted work.
            if existing.text(self) != text {
                existing.set_text(self).to(text.to_owned());
            }
            let current_root = existing.source_root(self);
            if current_root == root {
                return (existing, false);
            }
            let remaining: Vec<SourceFile> = current_root
                .files(self)
                .iter()
                .copied()
                .filter(|file| *file != existing)
                .collect();
            current_root.set_files(self).to(remaining);
            existing.set_source_root(self).to(root);
            return (existing, true);
        }

        // Revive the tombstoned input if this path existed before — creating
        // a fresh input would leak the old one forever. The owning root may
        // have changed since, so `source_root` is always re-set.
        let file = match self.removed_file_tombstones.get(&path).copied() {
            Some(file) => {
                Arc::make_mut(&mut self.removed_file_tombstones).remove(&path);
                file.set_text(self).to(text.to_owned());
                file.set_source_root(self).to(root);
                file
            }
            None => self.new_file(root, path.clone(), text),
        };
        let file_id = file.file_id(self);
        Arc::make_mut(&mut self.file_map).insert(path.clone(), file);
        Arc::make_mut(&mut self.file_id_to_path).insert(file_id, path);
        (file, true)
    }

    /// Detach `file` from the path maps and park it as a tombstone with empty
    /// text (releasing the source string and its downstream memos). Does not
    /// touch the owning root's `files` — callers batch that write.
    fn park_file(&mut self, file: SourceFile) {
        let path = file.path(self);
        let file_id = file.file_id(self);
        Arc::make_mut(&mut self.file_map).remove(&path);
        Arc::make_mut(&mut self.file_id_to_path).remove(&file_id);
        file.set_text(self).to(String::new());
        Arc::make_mut(&mut self.removed_file_tombstones).insert(path, file);
    }

    /// Add or update a file in `root`.
    ///
    /// If the file already exists, its content is updated using Salsa's
    /// `set_text` method; otherwise a `SourceFile` is created (or a
    /// tombstoned one revived — with `source_root` re-set to `root`).
    ///
    /// Returns the `SourceFile` handle.
    pub fn add_or_update_file_in(
        &mut self,
        root: SourceRoot,
        path: &Path,
        text: &str,
    ) -> SourceFile {
        let (file, newly_owned) = self.upsert_file(root, path, text);
        if newly_owned {
            let mut files = root.files(self).clone();
            files.push(file);
            root.set_files(self).to(files);
        }
        file
    }

    /// Bulk [`Self::add_or_update_file_in`]: identical per-file semantics
    /// (canonicalization, tombstone revival, map registration), but the
    /// root's `files` list is written once at the end instead of once per
    /// new file. The per-file path clones and re-sets the whole `files` Vec
    /// and bumps the salsa revision each time — O(files²) copies plus one
    /// revision per file during initial project load.
    pub fn add_or_update_files_in<'a, I>(&mut self, root: SourceRoot, files: I)
    where
        I: IntoIterator<Item = (&'a Path, &'a str)>,
    {
        let mut added: Vec<SourceFile> = Vec::new();
        for (path, text) in files {
            let (file, newly_owned) = self.upsert_file(root, path, text);
            if newly_owned {
                added.push(file);
            }
        }
        if !added.is_empty() {
            let mut root_files = root.files(self).clone();
            root_files.extend(added);
            root.set_files(self).to(root_files);
        }
    }

    /// Replace `root`'s file set: every listed file is added or updated (in
    /// the given order), and every file currently in the root that is not
    /// listed is tombstoned. One `files` write on the root.
    pub fn set_root_files<'a, I>(&mut self, root: SourceRoot, files: I)
    where
        I: IntoIterator<Item = (&'a Path, &'a str)>,
    {
        let previous: Vec<SourceFile> = root.files(self).clone();
        let mut next: Vec<SourceFile> = Vec::new();
        let mut kept: std::collections::HashSet<SourceFile> = std::collections::HashSet::new();
        for (path, text) in files {
            let (file, _) = self.upsert_file(root, path, text);
            if kept.insert(file) {
                next.push(file);
            }
        }
        for file in previous {
            if !kept.contains(&file) {
                self.park_file(file);
            }
        }
        root.set_files(self).to(next);
    }

    /// Remove a file from the database.
    ///
    /// Note: Salsa doesn't support true removal. The input is emptied (so its
    /// text and per-file memos can be reclaimed), removed from tracking and
    /// its root's file list, and parked in a tombstone map for reuse if the
    /// same path is re-added later.
    pub fn remove_file(&mut self, path: &Path) {
        let path = canonicalize_lossy(path);
        let Some(&file) = self.file_map.get(&path) else {
            return;
        };
        let root = file.source_root(self);
        let remaining: Vec<SourceFile> = root
            .files(self)
            .iter()
            .copied()
            .filter(|f| *f != file)
            .collect();
        root.set_files(self).to(remaining);
        self.park_file(file);
    }

    /// The files of `root`, in insertion order.
    pub fn root_files(&self, root: SourceRoot) -> Vec<SourceFile> {
        root.files(self).clone()
    }

    /// The files of every `Workspace` root, in table order.
    pub fn workspace_files(&self) -> Vec<SourceFile> {
        self.table()
            .roots(self)
            .iter()
            .filter(|root| root.kind(self) == SourceRootKind::Workspace)
            .flat_map(|root| root.files(self).iter().copied())
            .collect()
    }

    /// Get a `SourceFile` by its path.
    pub fn get_file(&self, path: &Path) -> Option<SourceFile> {
        self.file_map.get(&canonicalize_lossy(path)).copied()
    }

    /// Get a `FileId` by its path.
    pub fn path_to_file_id(&self, path: &Path) -> Option<FileId> {
        self.get_file(path).map(|file| file.file_id(self))
    }

    /// Get the file path for a `FileId`.
    pub fn file_id_to_path(&self, file_id: FileId) -> Option<&PathBuf> {
        self.file_id_to_path.get(&file_id)
    }

    /// Find a [`SourceFile`] by file path (matches by suffix to handle
    /// different path formats).
    pub fn find_source_file(&self, file_path: &str) -> Option<SourceFile> {
        // Try exact match first
        if let Some(&file) = self.file_map.get(Path::new(file_path)) {
            return Some(file);
        }
        // Fallback: match by file name suffix (handles editors' relative paths)
        self.file_map
            .iter()
            .find(|(stored_path, _)| {
                stored_path.ends_with(file_path)
                    || file_path.ends_with(stored_path.to_string_lossy().as_ref())
            })
            .map(|(_, file)| *file)
    }

    /// Add compiler-generated source for a `Session.eval` submission to the
    /// workspace root. Session files use the dedicated CST→AST lowering mode
    /// that admits persistent root bindings; ordinary source files remain
    /// unchanged.
    ///
    /// # Panics
    ///
    /// Panics if the database has no `Workspace` root — a session is
    /// workspace-bound by construction.
    pub fn add_session_file(&mut self, path: impl AsRef<Path>, content: &str) -> SourceFile {
        let Some(root) = self.workspace_root() else {
            panic!("add_session_file requires a Workspace source root");
        };
        let file = self.add_or_update_file_in(root, path.as_ref(), content);
        file.set_is_session_submission(self).to(true);
        file
    }

    // ── Seeds and mounts ─────────────────────────────────────────────────────

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
        by_path: BTreeMap<String, Vec<baml_type::throw_facts::FunctionThrowFacts>>,
    ) {
        let seeds = self.seeded_throw_facts.unwrap_or_else(|| {
            unreachable!("SeededThrowFacts input is created in ProjectDatabase::new")
        });
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
    pub fn set_seeded_stdlib_interface(&mut self, by_package: BTreeMap<String, Vec<u8>>) {
        let seeds = self.seeded_stdlib_interface.unwrap_or_else(|| {
            unreachable!("SeededStdlibInterface input is created in ProjectDatabase::new")
        });
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
        by_path: BTreeMap<String, BTreeMap<u32, baml_type::Ty>>,
    ) {
        let seeds = self.seeded_callable_throws.unwrap_or_else(|| {
            unreachable!("SeededCallableThrows input is created in ProjectDatabase::new")
        });
        seeds.set_by_path(self).to(by_path);
    }

    /// The always-present mounted-package input (see the `mounted_packages`
    /// field).
    fn mounts(&self) -> MountedPackages {
        self.mounted_packages.unwrap_or_else(|| {
            unreachable!("MountedPackages input is created in ProjectDatabase::new")
        })
    }

    /// Replace the mounted source-less package map and invalidate all tracked
    /// package/interface lookups that read it.
    pub fn set_mounted_packages(&mut self, by_package: BTreeMap<String, Vec<u8>>) {
        let mounts = self.mounts();
        mounts.set_by_package(self).to(by_package);
        mounts
            .set_immutable_precompiled(self)
            .to(std::collections::BTreeSet::new());
    }

    /// Install compiler-built stdlib interfaces into the mounted-package
    /// transport and mark them image-immutable.
    ///
    /// Only embedded stdlib names are accepted. Ordinary runtime mounts remain
    /// replaceable and keep the conservative mounted impl-facts shape; these
    /// rows are build artifacts from this exact compiler and can therefore be
    /// re-hydrated like source-backed facts instead of being retained in every
    /// impl-cache entry.
    pub fn set_precompiled_stdlib_packages(&mut self, by_package: BTreeMap<String, Vec<u8>>) {
        let mounts = self.mounts();
        let stdlib_names = baml_builtins2::stdlib_package_names();
        let mut merged = mounts.by_package(self).clone();
        let mut immutable = std::collections::BTreeSet::new();
        for (name, bytes) in by_package {
            if stdlib_names.contains(&name.as_str()) {
                immutable.insert(name.clone());
                merged.insert(name, bytes);
            }
        }
        mounts.set_by_package(self).to(merged);
        mounts.set_immutable_precompiled(self).to(immutable);
    }

    // ── Bytecode ─────────────────────────────────────────────────────────────

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
        // matches `testing::assert_no_diagnostic_errors` — workspace-file
        // errors only.
        let user_file_ids: std::collections::HashSet<FileId> = self
            .workspace_files()
            .iter()
            .map(|f| f.file_id(self))
            .collect();
        let error_count = crate::check::collect_compiler2_diagnostics(self)
            .iter()
            .filter(|d| matches!(d.severity, baml_compiler_diagnostics::Severity::Error))
            .filter(|d| {
                d.primary_span()
                    .is_some_and(|span| user_file_ids.contains(&span.file_id))
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
    /// revision and found no workspace-file errors — the gate in
    /// `get_bytecode` would re-derive exactly that result. Calling this on an
    /// error-bearing project can panic in the runtime-conversion boundary (see
    /// the gate comment above).
    pub fn get_bytecode_unchecked(
        &self,
    ) -> Result<bex_vm_types::Program, baml_compiler2_emit::LoweringError> {
        let opts = baml_compiler2_emit::CompileOptions {
            emit_test_cases: false,
        };
        baml_compiler2_emit::generate_project_bytecode(self, &opts)
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
            .field("root_count", &self.roots_by_path.len())
            .field("file_count", &self.file_map.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_spec(path: &str) -> SourceRootSpec {
        SourceRootSpec {
            path: PathBuf::from(path),
            package: Name::new("user"),
            kind: SourceRootKind::Workspace,
        }
    }

    fn dependency_spec(path: &str, package: &str) -> SourceRootSpec {
        SourceRootSpec {
            path: PathBuf::from(path),
            package: Name::new(package),
            kind: SourceRootKind::Dependency,
        }
    }

    #[test]
    fn add_and_update_file_in_root() {
        let mut db = ProjectDatabase::new();
        let root = db.add_source_root(workspace_spec("/virt")).unwrap();
        let path = Path::new("/virt/test.baml");

        let file1 = db.add_or_update_file_in(root, path, "class Foo {}");
        assert_eq!(file1.text(&db), "class Foo {}");
        assert_eq!(file1.source_root(&db), root);
        assert_eq!(db.root_files(root), vec![file1]);
        assert_eq!(db.get_file(path), Some(file1));
        assert_eq!(
            db.file_id_to_path(file1.file_id(&db)),
            Some(&path.to_path_buf())
        );

        let file2 = db.add_or_update_file_in(root, path, "class Bar {}");
        assert_eq!(file1.file_id(&db), file2.file_id(&db));
        assert_eq!(file1.text(&db), "class Bar {}");
        assert_eq!(db.root_files(root).len(), 1);
    }

    #[test]
    fn removed_files_are_revived_not_recreated() {
        let mut db = ProjectDatabase::new();
        db.ensure_stdlib_sources();
        let root = db.add_source_root(workspace_spec("/churn")).unwrap();
        let path = Path::new("/churn/churn.baml");
        let original = db.add_or_update_file_in(
            root,
            path,
            "function A(input: string) -> string {\n  input\n}\n",
        );
        let original_id = original.file_id(&db);
        let baseline = crate::check::collect_compiler2_diagnostics(&db).len();

        // Branch switches and codegen delete and recreate files; each cycle
        // must revive the tombstoned salsa input instead of minting a new
        // immortal one.
        for i in 0..3 {
            db.remove_file(path);
            assert!(db.get_file(path).is_none());
            assert!(db.root_files(root).is_empty());
            assert!(
                crate::check::collect_compiler2_diagnostics(&db).len() >= baseline,
                "diagnostics must still compute while the file is removed"
            );
            let revived = db.add_or_update_file_in(
                root,
                path,
                &format!("function A(input: string) -> string {{\n  //# v{i}\n  input\n}}\n"),
            );
            assert_eq!(
                revived.file_id(&db),
                original_id,
                "re-adding a removed path must reuse its SourceFile input"
            );
            assert_eq!(revived.source_root(&db), root);
        }

        assert_eq!(
            crate::check::collect_compiler2_diagnostics(&db).len(),
            baseline
        );
    }

    #[test]
    fn stdlib_sources_are_idempotent_and_first() {
        let mut db = ProjectDatabase::new();
        let workspace = db.add_source_root(workspace_spec("/ws")).unwrap();
        db.ensure_stdlib_sources();
        let roots_before = db.source_roots();
        let file_count = db.file_map.len();
        db.ensure_stdlib_sources();
        assert_eq!(db.source_roots(), roots_before);
        assert_eq!(db.file_map.len(), file_count);

        // Stdlib roots precede the workspace root regardless of insertion order.
        assert_eq!(db.source_roots().last().copied(), Some(workspace));
        assert!(
            db.source_roots()
                .iter()
                .take(roots_before.len() - 1)
                .all(|root| root.kind(&db) == SourceRootKind::Stdlib)
        );
        // One stdlib root per builtin package, holding all of its files.
        for name in baml_builtins2::stdlib_package_names() {
            let root = db
                .source_root_for_path(Path::new(&format!("<builtin>/{name}")))
                .expect("stdlib root");
            assert_eq!(root.package(&db).as_str(), *name);
            let expected = baml_builtins2::ALL
                .iter()
                .filter(|b| b.package == *name)
                .count();
            assert_eq!(db.root_files(root).len(), expected);
        }
        assert_eq!(db.workspace_root(), Some(workspace));
    }

    #[test]
    fn root_invariants_are_enforced() {
        let mut db = ProjectDatabase::new();
        let workspace = db.add_source_root(workspace_spec("/ws")).unwrap();
        assert_eq!(
            db.add_source_root(workspace_spec("/ws")),
            Err(SourceRootError::PathTaken(workspace))
        );
        assert_eq!(
            db.add_source_root(workspace_spec("/ws2")),
            Err(SourceRootError::SecondWorkspaceRoot(workspace))
        );
        assert_eq!(
            db.add_source_root(dependency_spec("/dep", "user")),
            Err(SourceRootError::PackageNameTaken {
                name: Name::new("user"),
                by: workspace
            })
        );
        // A mounted interface blob and a source root may share a name: the
        // runtime compiler mounts a dependency's interface (the semantic
        // authority) and adds a stub source root for the same package (emit
        // slots); the compiler's source-vs-blob contract decides precedence.
        db.set_mounted_packages(BTreeMap::from([("dep".to_owned(), Vec::new())]));
        let dep = db.add_source_root(dependency_spec("/dep", "dep")).unwrap();
        // Dependency roots sort before the workspace root; Dynamic roots
        // (runtime-loaded) sort after it, whatever the insertion order.
        let dynamic = db
            .add_source_root(SourceRootSpec {
                path: PathBuf::from("<builtin>/mount"),
                package: Name::new("mount"),
                kind: SourceRootKind::Dynamic,
            })
            .unwrap();
        assert_eq!(db.source_roots(), vec![dep, workspace, dynamic]);
    }

    #[test]
    fn removed_roots_are_revived_and_files_tombstoned() {
        let mut db = ProjectDatabase::new();
        let dep = db.add_source_root(dependency_spec("/dep", "dep")).unwrap();
        let path = Path::new("/dep/a.baml");
        let file = db.add_or_update_file_in(dep, path, "class A {}");
        let file_id = file.file_id(&db);

        db.remove_source_root(dep);
        assert!(db.source_roots().is_empty());
        assert!(db.get_file(path).is_none());
        assert!(db.source_root_for_path(path).is_none());
        assert_eq!(file.text(&db), "");

        // Re-adding at the same path revives the root; re-adding the file
        // revives the file and re-points it at the revived root.
        let revived = db.add_source_root(dependency_spec("/dep", "dep2")).unwrap();
        assert_eq!(revived, dep);
        assert_eq!(revived.package(&db).as_str(), "dep2");
        let file_again = db.add_or_update_file_in(revived, path, "class B {}");
        assert_eq!(file_again.file_id(&db), file_id);
        assert_eq!(file_again.source_root(&db), revived);
        assert_eq!(db.root_files(revived), vec![file_again]);
    }

    #[test]
    fn source_root_for_path_is_longest_prefix() {
        let mut db = ProjectDatabase::new();
        let outer = db.add_source_root(workspace_spec("/proj")).unwrap();
        let inner = db
            .add_source_root(dependency_spec("/proj/vendor/dep", "dep"))
            .unwrap();
        let sibling = db
            .add_source_root(dependency_spec("/proj/vendor/depz", "depz"))
            .unwrap();
        assert_eq!(
            db.source_root_for_path(Path::new("/proj/a.baml")),
            Some(outer)
        );
        assert_eq!(
            db.source_root_for_path(Path::new("/proj/vendor/dep/x/y.baml")),
            Some(inner)
        );
        assert_eq!(
            db.source_root_for_path(Path::new("/proj/vendor/depz/y.baml")),
            Some(sibling)
        );
        assert_eq!(
            db.source_root_for_path(Path::new("/proj/vendor/other.baml")),
            Some(outer)
        );
        assert_eq!(
            db.source_root_for_path(Path::new("/elsewhere/z.baml")),
            None
        );
    }

    #[test]
    fn set_root_files_replaces_and_tombstones() {
        let mut db = ProjectDatabase::new();
        let root = db.add_source_root(workspace_spec("/ws")).unwrap();
        let a = db.add_or_update_file_in(root, Path::new("/ws/a.baml"), "class A {}");
        let b = db.add_or_update_file_in(root, Path::new("/ws/b.baml"), "class B {}");

        db.set_root_files(
            root,
            [
                (Path::new("/ws/c.baml"), "class C {}"),
                (Path::new("/ws/b.baml"), "class B2 {}"),
            ],
        );
        let files = db.root_files(root);
        assert_eq!(files.len(), 2);
        assert_eq!(files[1], b);
        assert_eq!(b.text(&db), "class B2 {}");
        assert!(db.get_file(Path::new("/ws/a.baml")).is_none());
        assert_eq!(a.text(&db), "");
        assert_eq!(db.workspace_files(), files);
    }

    #[test]
    fn upsert_moves_file_between_roots() {
        let mut db = ProjectDatabase::new();
        let ws = db.add_source_root(workspace_spec("/ws")).unwrap();
        let path = Path::new("/ws/vendor/dep/a.baml");
        let file = db.add_or_update_file_in(ws, path, "class A {}");
        let dep = db
            .add_source_root(dependency_spec("/ws/vendor/dep", "dep"))
            .unwrap();
        assert_eq!(db.source_root_for_path(path), Some(dep));

        let moved = db.add_or_update_file_in(dep, path, "class A {}");
        assert_eq!(moved, file);
        assert_eq!(moved.source_root(&db), dep);
        assert!(db.root_files(ws).is_empty());
        assert_eq!(db.root_files(dep), vec![moved]);
    }
}
