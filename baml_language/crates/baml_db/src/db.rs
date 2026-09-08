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

use baml_base::{
    Dependency, FileId, Name, SourceFile, SourceRoot, SourceRootKind, SourceRootTable,
};
use baml_compiler2_hir::{
    inputs::{LangRootsInput, SeededCallableThrows, SeededStdlibInterface, SeededThrowFacts},
    package::package_dependency_closure,
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
    pub kind: SourceRootKind,
    /// The package's own name (`[package].name`, or a stdlib package's
    /// language-fixed name); `None` for an unnamed package.
    pub self_name: Option<Name>,
    /// The package's serialized compiler interface when it is served
    /// source-less (a runtime mount, a precompiled stdlib package).
    pub interface: Option<Vec<u8>>,
    /// The package's DECLARED dependency edges. The stdlib prelude is added
    /// by the database to every non-`Stdlib` root; only explicit edges go
    /// here, and every root they name must already be live.
    pub dependencies: Vec<Dependency>,
}

impl SourceRootSpec {
    /// An unnamed, source-backed root with no declared dependencies.
    pub fn new(path: impl Into<PathBuf>, kind: SourceRootKind) -> Self {
        Self {
            path: path.into(),
            kind,
            self_name: None,
            interface: None,
            dependencies: Vec::new(),
        }
    }

    #[must_use]
    pub fn named(mut self, name: Name) -> Self {
        self.self_name = Some(name);
        self
    }

    #[must_use]
    pub fn served_from(mut self, interface: Vec<u8>) -> Self {
        self.interface = Some(interface);
        self
    }

    #[must_use]
    pub fn depending_on(mut self, dependencies: Vec<Dependency>) -> Self {
        self.dependencies = dependencies;
        self
    }
}

/// Why [`ProjectDatabase::add_source_root`] or
/// [`ProjectDatabase::add_dependency`] refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SourceRootError {
    /// A live root already sits at this (canonical) path.
    #[error("a source root already exists at this path")]
    PathTaken(SourceRoot),
    /// The edge name is one no package may declare: a stdlib package's name
    /// (already an implicit edge of every package) or a source-level
    /// qualifier (`root`, `env`).
    #[error("dependency name `{name}` is reserved")]
    ReservedDependencyName { name: Name },
    /// The root already has an edge under this name.
    #[error("dependency `{name}` is declared twice")]
    DuplicateDependencyName { name: Name },
    /// The edge names a root that is not live in this database.
    #[error("dependency `{name}` names a source root that does not exist")]
    UnknownDependencyRoot { name: Name },
    /// The edge would make the dependency graph cyclic.
    #[error("dependency `{name}` would form a dependency cycle")]
    DependencyCycle { name: Name },
    /// The interface bytes are not a valid `PackageInterface` artifact.
    #[error("invalid package interface: {message}")]
    InvalidInterface { message: String },
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
/// let root = db.add_source_root(SourceRootSpec::new("/my/project", SourceRootKind::Workspace))?;
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

    /// Where the language packages live (`baml`, `reflect`, ...): the one
    /// place a package is found by its manifest name. `None` until the
    /// stdlib is installed; created once by [`Self::install_stdlib`] and
    /// re-pointed through its Salsa setter on every later install.
    lang_roots: Option<LangRootsInput>,
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
    /// The stdlib packages every non-`Stdlib` root implicitly depends on,
    /// under their own names — the `[package] prelude = true` packages of
    /// the embedded stdlib manifests. Filled by the stdlib installer and
    /// added to every root's edges (see [`Self::add_source_root`]); empty
    /// until the stdlib is installed, after which existing roots are
    /// retrofitted so the invariant holds whatever the installation order.
    stdlib_prelude: Arc<[Dependency]>,
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

    fn lang_roots_input(&self) -> Option<LangRootsInput> {
        self.lang_roots
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
/// spelled (lexically normalized).
///
/// Exported so path producers (the LSP boundary) apply the identical rule
/// instead of maintaining a copy that could drift.
pub fn canonicalize_lossy(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    // The tail beyond the existing prefix can only be normalized lexically:
    // resolve `.`/`..` up front so the ancestor walk never meets a `..`
    // component (whose `file_name()` is `None` — appending it verbatim would
    // silently drop it from the key).
    let normalized = lexically_normalize(path);
    let mut ancestor = normalized.as_path();
    let mut remainder = Vec::new();
    while let Some(parent) = ancestor.parent() {
        if let Some(name) = ancestor.file_name() {
            remainder.push(name.to_os_string());
        }
        // Stop before the bare root/prefix: canonicalizing it resolves no
        // symlinks (on unix `/` is its own canonical form), and on Windows it
        // SUCCEEDS by resolving to the current drive — grafting a
        // never-existing unix-spelled path (`/fixture/…`) onto
        // `\\?\C:\fixture\…` and changing the stored spelling per platform.
        // A path with no existing named ancestor is kept exactly as spelled.
        if parent.file_name().is_none() {
            break;
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
    normalized
}

/// Drop `.` components and resolve `..` lexically (popping at a root or
/// prefix is a no-op, matching `..` at `/`).
///
/// A path with nothing to normalize is returned byte-for-byte as spelled:
/// rebuilding it from components would rejoin them with the platform
/// separator, and on Windows that rewrites the virtual `<builtin>/pkg/…`
/// paths to `<builtin>\pkg\…` — breaking every consumer of the `<builtin>/`
/// prefix contract (the compiler's builtin-syntax gate, emit's builtin
/// filter, `bex_vm_types::link`), which must see the same spelling on every
/// platform.
fn lexically_normalize(path: &Path) -> PathBuf {
    let needs_normalization = path.components().any(|component| {
        matches!(
            component,
            std::path::Component::CurDir | std::path::Component::ParentDir
        )
    });
    if !needs_normalization {
        return path.to_path_buf();
    }
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::Prefix(_)
            | std::path::Component::RootDir
            | std::path::Component::Normal(_) => {
                out.push(component.as_os_str());
            }
        }
    }
    out
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
    /// the seed inputs empty from construction. Holding each
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
            lang_roots: None,
            seeded_stdlib_interface: None,
            seeded_callable_throws: None,
            stdlib_prelude: Arc::from(Vec::new()),
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
    /// [`baml_builtins2::ALL`], with the dependency edges its own `baml.toml`
    /// declares.
    ///
    /// Idempotent: a stdlib root that already exists is left in place and its
    /// files are (re)synchronized to the embedded contents, so calling this
    /// on a database that already has the stdlib is a no-op revision-wise.
    pub fn ensure_stdlib_sources(&mut self) {
        self.install_stdlib(|package| StdlibProvenance::Source {
            files: baml_builtins2::ALL
                .iter()
                .filter(|builtin| builtin.package == package)
                .map(|builtin| (PathBuf::from(builtin.virtual_path()), builtin.contents))
                .collect(),
        });
    }

    /// Install one `Stdlib` root per embedded builtin package served from its
    /// compiler-built interface (`borsh(PackageInterface)`, keyed by package
    /// name) instead of from source — the runtime compiler's shape, where the
    /// stdlib arrives precompiled and no builtin source is materialized. The
    /// dependency edges come from the embedded manifests exactly as for
    /// source roots.
    ///
    /// # Panics
    ///
    /// Panics if `interfaces` lacks a stdlib package: a precompiled stdlib is
    /// all-or-nothing.
    pub fn ensure_precompiled_stdlib(&mut self, interfaces: &BTreeMap<String, Vec<u8>>) {
        self.install_stdlib(|package| StdlibProvenance::Interface {
            bytes: interfaces
                .get(package)
                .unwrap_or_else(|| panic!("precompiled stdlib is missing package `{package}`"))
                .clone(),
        });
    }

    /// The one stdlib installer: create (or reuse) the roots of
    /// [`stdlib_layout`] in dependency order, each with its manifest-declared
    /// edges, record the prelude, and retrofit the prelude onto every
    /// non-`Stdlib` root added before the stdlib was.
    fn install_stdlib(&mut self, provenance: impl Fn(&str) -> StdlibProvenance) {
        let layout = stdlib_layout();
        let mut roots: HashMap<&'static str, SourceRoot> = HashMap::new();
        for package in &layout.packages {
            let path = PathBuf::from(format!("<builtin>/{}", package.name));
            let dependencies: Vec<Dependency> = package
                .dependencies
                .iter()
                .map(|(name, target)| Dependency {
                    name: name.clone(),
                    root: roots[target],
                })
                .collect();
            let root = match self.roots_by_path.get(&path) {
                Some(&root) => root,
                None => {
                    let mut spec = SourceRootSpec::new(path, SourceRootKind::Stdlib)
                        .named(Name::new(package.name))
                        .depending_on(dependencies);
                    if let StdlibProvenance::Interface { bytes } = provenance(package.name) {
                        spec = spec.served_from(bytes);
                    }
                    self.add_source_root(spec).unwrap_or_else(|err| {
                        panic!(
                            "cannot install the stdlib source root for `{}`: {err}",
                            package.name
                        )
                    })
                }
            };
            if let StdlibProvenance::Source { files } = provenance(package.name) {
                self.add_or_update_files_in(
                    root,
                    files
                        .iter()
                        .map(|(path, contents)| (path.as_path(), *contents)),
                );
            }
            roots.insert(package.name, root);
        }
        let lang = baml_base::LangPackage::ALL
            .into_iter()
            .filter_map(|package| {
                roots
                    .get(package.manifest_name())
                    .map(|&root| (package, root))
            })
            .fold(baml_base::LangRoots::default(), |lang, (package, root)| {
                lang.with(package, root)
            });
        match self.lang_roots {
            Some(input) => {
                input.set_roots(self).to(lang);
            }
            None => self.lang_roots = Some(LangRootsInput::new(self, lang)),
        }
        let prelude: Vec<Dependency> = layout
            .packages
            .iter()
            .filter(|package| package.prelude)
            .map(|package| Dependency {
                name: Name::new(package.name),
                root: roots[package.name],
            })
            .collect();
        self.stdlib_prelude = Arc::from(prelude);
        // A root added before the stdlib got no prelude edges; give it them
        // now so "every non-stdlib package reaches the prelude" holds
        // regardless of installation order.
        let live: Vec<SourceRoot> = self.roots_by_path.values().copied().collect();
        for root in live {
            if root.kind(self) == SourceRootKind::Stdlib {
                continue;
            }
            let current = root.dependencies(self);
            let missing: Vec<Dependency> = self
                .stdlib_prelude
                .iter()
                .filter(|edge| !current.iter().any(|have| have.name == edge.name))
                .cloned()
                .collect();
            if !missing.is_empty() {
                let mut edges = missing;
                edges.extend(current.iter().cloned());
                root.set_dependencies(self).to(edges);
            }
        }
    }

    /// The prelude edges every non-`Stdlib` root carries (empty until the
    /// stdlib is installed).
    pub fn stdlib_prelude(&self) -> &[Dependency] {
        &self.stdlib_prelude
    }

    /// Add a source root, or revive the tombstoned root at the same path.
    ///
    /// Invariants enforced (each is a [`SourceRootError`]):
    /// - at most one live root per canonical path;
    /// - one live root per wire name (`self_name`, else the unnamed default)
    ///   — the interim consequence of name-spelled type heads;
    /// - every declared dependency names a live root, under a name that is
    ///   neither reserved nor declared twice;
    /// - a served-from-interface root's bytes decode as a `PackageInterface`.
    ///
    /// Every non-`Stdlib` root also receives the stdlib prelude edges ahead
    /// of its declared ones (see [`Self::stdlib_prelude`]).
    ///
    /// The root is inserted into its kind's bucket of the table (`Stdlib` <
    /// `Dependency` < `Workspace` < `Dynamic`, [`SourceRootKind`]'s order),
    /// preserving the order invariant `compiler2_all_files` relies on.
    pub fn add_source_root(&mut self, spec: SourceRootSpec) -> Result<SourceRoot, SourceRootError> {
        let SourceRootSpec {
            path,
            kind,
            self_name,
            interface,
            dependencies,
        } = spec;
        let path = canonicalize_lossy(&path);

        if let Some(&existing) = self.roots_by_path.get(&path) {
            return Err(SourceRootError::PathTaken(existing));
        }
        let wire_interface = interface
            .as_deref()
            .map(|bytes| Self::decode_interface(kind, bytes))
            .transpose()?;
        let dependencies = self.complete_dependencies(kind, dependencies)?;

        // Revive the tombstoned input if a root lived at this path before —
        // creating a fresh input would leak the old one forever.
        let root = match self.removed_root_tombstones.get(&path).copied() {
            Some(root) => {
                Arc::make_mut(&mut self.removed_root_tombstones).remove(&path);
                debug_assert!(
                    root.files(self).is_empty(),
                    "a tombstoned root must have had its files emptied on removal"
                );
                root.set_kind(self).to(kind);
                root.set_self_name(self).to(self_name);
                root.set_interface(self).to(interface);
                root.set_dependencies(self).to(dependencies);
                root
            }
            None => SourceRoot::new(
                self,
                path.clone(),
                kind,
                self_name,
                Vec::new(),
                interface,
                dependencies,
            ),
        };

        // Insert at the end of this kind's bucket. The table is sorted by
        // kind rank, so `partition_point` finds the boundary.
        let table = self.table();
        let mut roots = table.roots(self).clone();
        let at = roots.partition_point(|root| root.kind(self) <= kind);
        roots.insert(at, root);
        table.set_roots(self).to(roots);
        Arc::make_mut(&mut self.roots_by_path).insert(path, root);

        // A served-from-interface root's blob must resolve from the root it
        // now is: every head it spells names the root itself or a package
        // reached by one of its edges. A blob that names anything else is
        // not mountable here, and the root does not stay.
        if let Some(wire) = wire_interface
            && let Err(error) =
                baml_compiler2_hir_ty::package_interface::import_interface(self, root, &wire)
        {
            self.remove_source_root(root);
            return Err(SourceRootError::InvalidInterface {
                message: error.to_string(),
            });
        }
        Ok(root)
    }

    /// Add one dependency edge to a live root.
    ///
    /// Rejects a reserved or already-declared name, a target that is not
    /// live, and an edge that would close a cycle.
    pub fn add_dependency(
        &mut self,
        from: SourceRoot,
        dependency: Dependency,
    ) -> Result<(), SourceRootError> {
        self.validate_dependency(from.kind(self), from.dependencies(self), &dependency)?;
        if dependency.root == from
            || package_dependency_closure(self, dependency.root).contains(&from)
        {
            return Err(SourceRootError::DependencyCycle {
                name: dependency.name,
            });
        }
        let mut edges = from.dependencies(self).clone();
        edges.push(dependency);
        from.set_dependencies(self).to(edges);
        Ok(())
    }

    /// The full edge list of a new root of `kind`: the stdlib prelude (for
    /// every kind but `Stdlib`, which IS the stdlib) followed by the declared
    /// edges, each validated.
    fn complete_dependencies(
        &self,
        kind: SourceRootKind,
        declared: Vec<Dependency>,
    ) -> Result<Vec<Dependency>, SourceRootError> {
        let mut edges: Vec<Dependency> = if kind == SourceRootKind::Stdlib {
            Vec::new()
        } else {
            self.stdlib_prelude.iter().cloned().collect()
        };
        for dependency in declared {
            self.validate_dependency(kind, &edges, &dependency)?;
            edges.push(dependency);
        }
        Ok(edges)
    }

    /// One edge's admissibility against the edges a root of `kind` already
    /// has. A stdlib package may name its stdlib siblings (that is how the
    /// stdlib graph is declared); every other package reaches them through
    /// the implicit prelude, so for it those names are reserved.
    fn validate_dependency(
        &self,
        kind: SourceRootKind,
        existing: &[Dependency],
        dependency: &Dependency,
    ) -> Result<(), SourceRootError> {
        let name = dependency.name.as_str();
        let reserved = if kind == SourceRootKind::Stdlib {
            matches!(name, "root" | "env")
        } else {
            baml_builtins2::reserved_edge_names().contains(&name)
        };
        if reserved {
            return Err(SourceRootError::ReservedDependencyName {
                name: dependency.name.clone(),
            });
        }
        if existing.iter().any(|edge| edge.name == dependency.name) {
            return Err(SourceRootError::DuplicateDependencyName {
                name: dependency.name.clone(),
            });
        }
        if !self
            .roots_by_path
            .values()
            .any(|root| *root == dependency.root)
        {
            return Err(SourceRootError::UnknownDependencyRoot {
                name: dependency.name.clone(),
            });
        }
        Ok(())
    }

    /// A served-from-interface root's bytes must decode: a precompiled stdlib
    /// package is raw `borsh(PackageInterface)`, anything else a versioned
    /// `baml_artifact`.
    fn decode_interface(
        kind: SourceRootKind,
        bytes: &[u8],
    ) -> Result<
        baml_compiler2_hir_ty::package_interface::PackageInterface<baml_type::TypeName>,
        SourceRootError,
    > {
        use baml_compiler2_hir_ty::package_interface::PackageInterface;
        let decoded = if kind == SourceRootKind::Stdlib {
            borsh::from_slice::<PackageInterface<baml_type::TypeName>>(bytes)
                .map_err(|error| error.to_string())
        } else {
            baml_artifact::decode::<PackageInterface<baml_type::TypeName>>(
                baml_artifact::ArtifactKind::PackageInterface,
                bytes,
            )
            .map_err(|error| error.to_string())
        };
        decoded.map_err(|message| SourceRootError::InvalidInterface { message })
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

        // No live root may keep an edge to a root that is gone.
        let live: Vec<SourceRoot> = self.roots_by_path.values().copied().collect();
        for other in live {
            if other
                .dependencies(self)
                .iter()
                .any(|edge| edge.root == root)
            {
                let remaining: Vec<Dependency> = other
                    .dependencies(self)
                    .iter()
                    .filter(|edge| edge.root != root)
                    .cloned()
                    .collect();
                other.set_dependencies(self).to(remaining);
            }
        }

        for file in root.files(self).clone() {
            self.park_file(file);
        }
        root.set_files(self).to(Vec::new());
        root.set_dependencies(self).to(Vec::new());

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
            // This is the ordinary-file path: a file that was previously a
            // session submission (the flag is set AFTER upsert by
            // `add_session_file`) must not keep the stale flag.
            if existing.is_session_submission(self) {
                existing.set_is_session_submission(self).to(false);
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
                // A revived tombstone may have been a session submission in
                // its previous life; this path creates ordinary files.
                if file.is_session_submission(self) {
                    file.set_is_session_submission(self).to(false);
                }
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
        for file in &previous {
            if !kept.contains(file) {
                self.park_file(*file);
            }
        }
        // A no-op replacement must not bump the `files` revision: dependents
        // of the file *set* would re-run for a change that isn't one.
        if next != previous {
            root.set_files(self).to(next);
        }
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
        by_path: BTreeMap<
            String,
            Vec<baml_type::throw_facts::FunctionThrowFacts<baml_type::TypeName>>,
        >,
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
        by_path: BTreeMap<String, BTreeMap<u32, baml_type::Ty<baml_type::TypeName>>>,
    ) {
        let seeds = self.seeded_callable_throws.unwrap_or_else(|| {
            unreachable!("SeededCallableThrows input is created in ProjectDatabase::new")
        });
        seeds.set_by_path(self).to(by_path);
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
        baml_compiler2_emit::generate_project_bytecode(self)
    }
}

/// How a stdlib package's contents arrive in the database.
enum StdlibProvenance {
    /// The embedded sources.
    Source { files: Vec<(PathBuf, &'static str)> },
    /// A compiler-built `borsh(PackageInterface)`.
    Interface { bytes: Vec<u8> },
}

/// One embedded stdlib package as its manifest declares it.
struct StdlibPackage {
    name: &'static str,
    /// `[package] prelude = true`: implicitly reachable from every package.
    prelude: bool,
    /// `[dependencies]`: the edge name and the stdlib package it reaches,
    /// resolved from the manifest's `path`.
    dependencies: Vec<(Name, &'static str)>,
}

/// The embedded stdlib's package graph, in dependency order (every package
/// after the packages it depends on).
struct StdlibLayout {
    packages: Vec<StdlibPackage>,
}

/// Parse the embedded stdlib manifests once per process.
///
/// # Panics
///
/// The stdlib ships with the compiler, so a manifest that does not parse, a
/// `path` that leaves the stdlib, or a dependency cycle is a build defect,
/// not a runtime condition: each panics with the offending package.
fn stdlib_layout() -> &'static StdlibLayout {
    static LAYOUT: std::sync::OnceLock<StdlibLayout> = std::sync::OnceLock::new();
    LAYOUT.get_or_init(|| {
        let known: Vec<&'static str> = baml_builtins2::MANIFESTS
            .iter()
            .map(|manifest| manifest.package)
            .collect();
        let mut packages: Vec<StdlibPackage> = baml_builtins2::MANIFESTS
            .iter()
            .map(|manifest| {
                let parsed = crate::manifest::parse(manifest.contents).unwrap_or_else(|error| {
                    panic!("stdlib package `{}` has an invalid baml.toml: {error}", manifest.package)
                });
                let package = parsed.package.as_ref().unwrap_or_else(|| {
                    panic!("stdlib package `{}` declares no [package]", manifest.package)
                });
                assert_eq!(
                    package.name.as_deref(),
                    Some(manifest.package),
                    "stdlib package `{}` must be named after its directory",
                    manifest.package
                );
                let dependencies = parsed
                    .dependencies
                    .iter()
                    .map(|(edge, spec)| {
                        let target = lexically_normalize(
                            &PathBuf::from(format!("<builtin>/{}", manifest.package))
                                .join(&spec.path),
                        );
                        let target = target
                            .strip_prefix("<builtin>")
                            .ok()
                            .and_then(|rest| rest.to_str())
                            .and_then(|rest| known.iter().copied().find(|name| *name == rest))
                            .unwrap_or_else(|| {
                                panic!(
                                    "stdlib package `{}` depends on `{}` at `{}`, which is not an embedded stdlib package",
                                    manifest.package, edge, spec.path
                                )
                            });
                        (Name::new(edge.as_str()), target)
                    })
                    .collect();
                StdlibPackage {
                    name: manifest.package,
                    prelude: package.prelude,
                    dependencies,
                }
            })
            .collect();

        // Dependency order (Kahn), keeping manifest order among the ready set
        // so the table order — and every index space built from it — is the
        // manifest's, not process-dependent.
        let mut ordered: Vec<StdlibPackage> = Vec::with_capacity(packages.len());
        let mut placed: Vec<&'static str> = Vec::new();
        while !packages.is_empty() {
            let Some(index) = packages.iter().position(|package| {
                package
                    .dependencies
                    .iter()
                    .all(|(_, target)| placed.contains(target))
            }) else {
                panic!(
                    "stdlib dependency cycle among {:?}",
                    packages.iter().map(|p| p.name).collect::<Vec<_>>()
                );
            };
            let package = packages.remove(index);
            placed.push(package.name);
            ordered.push(package);
        }
        StdlibLayout { packages: ordered }
    })
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
        SourceRootSpec::new(path, SourceRootKind::Workspace)
    }

    fn dependency_spec(path: &str, package: &str) -> SourceRootSpec {
        SourceRootSpec::new(path, SourceRootKind::Dependency).named(Name::new(package))
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
            "function a(input: string) -> string {\n  input\n}\n",
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
                &format!("function a(input: string) -> string {{\n  //# v{i}\n  input\n}}\n"),
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
            assert_eq!(root.self_name(&db).as_ref().map(Name::as_str), Some(*name));
            let expected = baml_builtins2::ALL
                .iter()
                .filter(|b| b.package == *name)
                .count();
            assert_eq!(db.root_files(root).len(), expected);
        }
        assert_eq!(db.workspace_root(), Some(workspace));
        // The workspace root, added before the stdlib, was retrofitted with
        // the prelude edges.
        let prelude: Vec<&str> = workspace
            .dependencies(&db)
            .iter()
            .map(|edge| edge.name.as_str())
            .collect();
        assert!(prelude.contains(&"baml") && prelude.contains(&"reflect"));
    }

    #[test]
    fn stdlib_graph_comes_from_the_stdlib_manifests() {
        let mut db = ProjectDatabase::new();
        db.ensure_stdlib_sources();
        let root_of = |name: &str| {
            db.source_root_for_path(Path::new(&format!("<builtin>/{name}")))
                .expect("stdlib root")
        };
        let edges = |name: &str| -> Vec<(String, SourceRoot)> {
            root_of(name)
                .dependencies(&db)
                .iter()
                .map(|edge| (edge.name.to_string(), edge.root))
                .collect()
        };
        assert_eq!(edges("log"), vec![]);
        assert_eq!(edges("baml"), vec![("log".to_string(), root_of("log"))]);
        assert_eq!(
            edges("testing"),
            vec![("baml".to_string(), root_of("baml"))]
        );
        assert!(edges("openai").iter().any(|(name, _)| name == "ai"));
        // Stdlib roots carry no prelude edges of their own.
        assert!(!edges("log").iter().any(|(name, _)| name == "reflect"));
        // Dependency order: every package after its dependencies.
        let roots = db.source_roots();
        let index = |root: SourceRoot| roots.iter().position(|r| *r == root).unwrap();
        for root in &roots {
            for edge in root.dependencies(&db) {
                assert!(index(edge.root) < index(*root));
            }
        }
    }

    /// The stdlib must compile with no errors under the dependency graph its
    /// own manifests declare: a stdlib package that reaches a sibling it does
    /// not list, or lists a sibling that does not exist, fails here with the
    /// offending file and message rather than deep inside runtime lowering.
    #[test]
    fn stdlib_compiles_cleanly_under_its_declared_graph() {
        let mut db = ProjectDatabase::new();
        db.ensure_stdlib_sources();
        let errors: Vec<String> = crate::check::collect_diagnostics(&db)
            .into_iter()
            .filter(|diagnostic| {
                matches!(
                    diagnostic.severity,
                    baml_compiler_diagnostics::Severity::Error
                )
            })
            .map(|diagnostic| {
                let at = diagnostic
                    .primary_span()
                    .and_then(|span| db.file_id_to_path(span.file_id).cloned())
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
                format!("{at}: {}", diagnostic.message)
            })
            .collect();
        assert!(
            errors.is_empty(),
            "the embedded stdlib has errors under its declared graph:\n{}",
            errors.join("\n")
        );
    }

    /// Every package the compiler's `"provider/model"` shorthand table names
    /// must be reachable from every package: a literal `client
    /// "bedrock/..."` lowers to `aws.BedrockClient.new(...)` in the user's own
    /// package, and the runtime half is a lambda synthesized there too.
    #[test]
    fn every_shorthand_provider_package_is_in_the_prelude() {
        let mut db = ProjectDatabase::new();
        db.ensure_stdlib_sources();
        let prelude: Vec<&str> = db
            .stdlib_prelude()
            .iter()
            .map(|edge| edge.name.as_str())
            .collect();
        for (prefix, package, _) in baml_compiler2_ast::SHORTHAND_PROVIDERS {
            assert!(
                prelude.contains(package),
                "shorthand `{prefix}/…` constructs a client from `{package}`, which is not a prelude package"
            );
        }
    }

    #[test]
    fn fixture_paths_with_no_existing_ancestor_stay_as_spelled() {
        // Test fixtures use absolute unix-spelled paths under a root that
        // exists on no platform. Their only "existing ancestor" is the bare
        // filesystem root, which the ancestor walk must not canonicalize:
        // on Windows that succeeds by resolving to the current drive and
        // grafts the path onto `\\?\C:\…`, so relative rendering (describe
        // listings, `relative_source_path`) diverges from every other
        // platform's snapshot.
        let fixture = Path::new("/no-such-fixture-root/ns_x/file.baml");
        assert_eq!(canonicalize_lossy(fixture).as_os_str(), fixture.as_os_str());
    }

    #[test]
    fn virtual_paths_are_stored_byte_for_byte_as_spelled() {
        // The `<builtin>/` prefix is a wire contract read by string
        // comparison (the compiler's builtin-syntax gate, emit's builtin
        // filter, `bex_vm_types::link`), so the stored spelling must keep its
        // forward slashes on every platform. Rebuilding the path from
        // components would rejoin with `\` on Windows; the byte-level
        // assertions here are what a component-wise `Path` comparison would
        // miss.
        let virtual_path = Path::new("<builtin>/baml/string.baml");
        assert_eq!(
            canonicalize_lossy(virtual_path).as_os_str(),
            virtual_path.as_os_str()
        );

        let mut db = ProjectDatabase::new();
        db.ensure_stdlib_sources();
        for root in db.source_roots() {
            for file in db.root_files(root) {
                let path = file.path(&db);
                assert!(
                    path.to_string_lossy().starts_with("<builtin>/"),
                    "stdlib path lost its wire spelling: {}",
                    path.display()
                );
                assert!(
                    !path.to_string_lossy().contains('\\'),
                    "stdlib path grew a platform separator: {}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn root_invariants_are_enforced() {
        let mut db = ProjectDatabase::new();
        let workspace = db.add_source_root(workspace_spec("/ws")).unwrap();
        assert_eq!(
            db.add_source_root(workspace_spec("/ws")),
            Err(SourceRootError::PathTaken(workspace))
        );
        // Spellings are not an identity: two roots may share one, and the
        // boundary that needs the spelling table injective reports it.
        let dep = db.add_source_root(dependency_spec("/dep", "dep")).unwrap();
        let dep2 = db.add_source_root(dependency_spec("/dep2", "dep")).unwrap();
        assert_eq!(
            baml_compiler2_hir::package::spelling(&db).collisions(),
            &[baml_compiler2_hir::package::SpellingCollision::SharedName {
                name: Name::new("dep"),
                roots: vec![dep, dep2],
            }]
        );
        // Dependency roots sort before the workspace root; Dynamic roots
        // (runtime-loaded) sort after it, whatever the insertion order.
        let dynamic = db
            .add_source_root(
                SourceRootSpec::new("<builtin>/mount", SourceRootKind::Dynamic)
                    .named(Name::new("mount")),
            )
            .unwrap();
        assert_eq!(db.source_roots(), vec![dep, dep2, workspace, dynamic]);

        // Edges: the target must be live, the name unique per root, and the
        // graph acyclic.
        let edge = |name: &str, root: SourceRoot| Dependency {
            name: Name::new(name),
            root,
        };
        db.add_dependency(workspace, edge("dep", dep)).unwrap();
        assert_eq!(
            db.add_dependency(workspace, edge("dep", dynamic)),
            Err(SourceRootError::DuplicateDependencyName {
                name: Name::new("dep")
            })
        );
        assert_eq!(
            db.add_dependency(dep, edge("ws", workspace)),
            Err(SourceRootError::DependencyCycle {
                name: Name::new("ws")
            })
        );
        assert_eq!(
            db.add_dependency(dep, edge("me", dep)),
            Err(SourceRootError::DependencyCycle {
                name: Name::new("me")
            })
        );
        db.remove_source_root(dep);
        assert_eq!(
            db.add_dependency(workspace, edge("gone", dep)),
            Err(SourceRootError::UnknownDependencyRoot {
                name: Name::new("gone")
            })
        );
        // Removing a root strips the edges that reached it.
        assert!(workspace.dependencies(&db).is_empty());

        // A served-from-interface root's bytes must decode.
        assert!(matches!(
            db.add_source_root(
                SourceRootSpec::new("<mount>/bad", SourceRootKind::Dynamic)
                    .named(Name::new("bad"))
                    .served_from(vec![1, 2, 3]),
            ),
            Err(SourceRootError::InvalidInterface { .. })
        ));
        let blob = baml_artifact::encode(
            baml_artifact::ArtifactKind::PackageInterface,
            &baml_compiler2_hir_ty::package_interface::PackageInterface::<baml_type::TypeName> {
                types: std::iter::empty().collect(),
                functions: std::iter::empty().collect(),
                throw_sets: baml_compiler2_hir_ty::package_interface::FunctionThrowSets::default(),
                namespaces: std::collections::BTreeSet::default(),
                impls: Vec::default(),
            },
        )
        .unwrap();
        db.add_source_root(
            SourceRootSpec::new("<mount>/good", SourceRootKind::Dynamic)
                .named(Name::new("good"))
                .served_from(blob),
        )
        .unwrap();
    }

    #[test]
    fn reserved_edge_names_are_rejected_for_user_packages() {
        let mut db = ProjectDatabase::new();
        db.ensure_stdlib_sources();
        let baml = db
            .source_root_for_path(Path::new("<builtin>/baml"))
            .expect("stdlib root");
        let workspace = db.add_source_root(workspace_spec("/ws")).unwrap();
        // The prelude already reaches `baml`; a declared edge may not reuse
        // a stdlib name, nor a source-level qualifier.
        for name in ["baml", "root", "env"] {
            assert_eq!(
                db.add_dependency(
                    workspace,
                    Dependency {
                        name: Name::new(name),
                        root: baml
                    }
                ),
                Err(SourceRootError::ReservedDependencyName {
                    name: Name::new(name)
                })
            );
        }
        // Every non-stdlib root gets the prelude, in stdlib table order.
        let prelude: Vec<&str> = workspace
            .dependencies(&db)
            .iter()
            .map(|edge| edge.name.as_str())
            .collect();
        assert_eq!(
            prelude,
            db.stdlib_prelude()
                .iter()
                .map(|edge| edge.name.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn upserts_clear_stale_session_submission_flags() {
        let mut db = ProjectDatabase::new();
        let workspace = db.add_source_root(workspace_spec("/ws")).unwrap();

        // A live session file re-upserted as an ordinary file loses the flag
        // (the flag is set AFTER upsert by `add_session_file`, so an
        // ordinary upsert is the only way the file re-enters the maps).
        let session = db.add_session_file("/ws/s.baml", "let a = 1;");
        assert!(session.is_session_submission(&db));
        let plain = db.add_or_update_file_in(workspace, Path::new("/ws/s.baml"), "class A {}");
        assert_eq!(plain, session, "same input, updated in place");
        assert!(!plain.is_session_submission(&db));

        // A tombstoned session file revived as an ordinary file loses the
        // flag too.
        let session = db.add_session_file("/ws/t.baml", "let b = 2;");
        assert!(session.is_session_submission(&db));
        db.remove_file(Path::new("/ws/t.baml"));
        let revived = db.add_or_update_file_in(workspace, Path::new("/ws/t.baml"), "enum E { X }");
        assert_eq!(revived, session, "tombstone revived, not recreated");
        assert!(!revived.is_session_submission(&db));
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
        assert_eq!(
            revived.self_name(&db).as_ref().map(Name::as_str),
            Some("dep2")
        );
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
