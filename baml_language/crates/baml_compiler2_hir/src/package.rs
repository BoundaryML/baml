//! Package-level cross-file symbol aggregation.
//!
//! A package IS a [`SourceRoot`]: package identity is the root's input id,
//! every package-level query is keyed on it, and a package reaches another
//! only through an edge in its own [`SourceRoot::dependencies`] list, under
//! the name that edge carries. `package_items` merges all `namespace_items`
//! within a package into a single lookup structure — the top-level
//! cross-file query used by the TIR layer for name resolution.

use baml_base::{LangPackage, LangRoots, Name, SourceRoot, SourceRootKind, Span};
use baml_compiler_diagnostics::diagnostic::{Diagnostic, DiagnosticId, DiagnosticPhase};
use baml_type::{DeclName, RESERVED_USER_PACKAGE, TypeName};
use indexmap::IndexMap;

use crate::{
    contributions::{Definition, DefinitionKind},
    namespace::{NameConflict, NamespaceId, NamespaceItems, namespace_items},
};

/// A namespace name that shadows a root-level declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceShadow<'db> {
    /// The namespace name that shadows (e.g., "foo" from `ns_foo/`).
    pub ns_name: Name,
    /// The full namespace path (e.g., `["foo"]` or `["foo", "bar"]`).
    pub ns_path: Vec<Name>,
    /// The root-level definition being shadowed.
    pub shadowed_def: Definition<'db>,
}

impl<'db> NamespaceShadow<'db> {
    /// Convert to a `Diagnostic` warning with the shadowed definition's span.
    pub fn to_diagnostic(&self, db: &'db dyn crate::Db) -> Diagnostic {
        let def = self.shadowed_def;
        let file = def.file(db);
        let file_id = file.file_id(db);

        // Look up the name span from file contributions.
        let contribs = crate::file_symbol_contributions(db, file);
        let name_span = contribs
            .types
            .iter()
            .chain(contribs.values.iter())
            .find(|(_, c)| c.definition == def)
            .map(|(_, c)| c.name_span);

        let message = format!(
            "namespace `{}` (from `ns_{}/`) shadows root-level {} `{}`",
            self.ns_name,
            self.ns_name,
            def.source_kind_name(db),
            self.ns_name
        );

        let mut diag = Diagnostic::warning(DiagnosticId::NamespaceShadow, message);

        if let Some(range) = name_span {
            diag = diag.with_primary(
                Span { file_id, range },
                format!(
                    "this {} is shadowed by namespace `{}`",
                    def.source_kind_name(db),
                    self.ns_name
                ),
            );
        }

        diag.with_phase(DiagnosticPhase::Validation)
    }
}

// ── Roots as packages ────────────────────────────────────────────────────────

/// The `Workspace`-kind source roots, in table order.
#[salsa::tracked(returns(ref))]
pub fn workspace_roots(db: &dyn crate::Db) -> Vec<SourceRoot> {
    db.source_roots()
        .roots(db)
        .iter()
        .copied()
        .filter(|root| root.kind(db) == SourceRootKind::Workspace)
        .collect()
}

/// The packages `viewer` can see: itself, then its dependency closure in
/// deterministic order. Every "which impls (or items) exist?" question is
/// asked from a package and answered over exactly this set — never the
/// whole database, which also holds packages `viewer` cannot name.
#[salsa::tracked(returns(ref))]
pub fn visible_packages(db: &dyn crate::Db, viewer: SourceRoot) -> Vec<SourceRoot> {
    std::iter::once(viewer)
        .chain(package_dependency_closure(db, viewer).iter().copied())
        .collect()
}

/// The dependency edge of `root` named `name`, if it has one.
///
/// The ONE place a source-level package spelling becomes a package: `baml`
/// in `baml.Array` resolves through the spelling package's own edge list and
/// nowhere else. A package that lists no edge under a name cannot reach it,
/// whatever else the database holds.
pub fn dependency_named(db: &dyn crate::Db, root: SourceRoot, name: &Name) -> Option<SourceRoot> {
    root.dependencies(db)
        .iter()
        .find(|dependency| dependency.name == *name)
        .map(|dependency| dependency.root)
}

/// The package `root` spells as `name`: itself, when `name` is its own
/// declared name (a named package may qualify its own items by it), else the
/// dependency reached by the edge named `name`. Anything else is invisible.
/// An unnamed package has no name to spell itself by besides `root`.
pub fn accessible_package(db: &dyn crate::Db, root: SourceRoot, name: &Name) -> Option<SourceRoot> {
    if root.self_name(db).is_some_and(|own| own == *name) {
        return Some(root);
    }
    dependency_named(db, root, name)
}

/// Where the language packages are installed in this database.
pub fn lang_roots(db: &dyn crate::Db) -> LangRoots {
    db.lang_roots_input()
        .map(|input| input.roots(db))
        .unwrap_or_default()
}

/// The full transitive dependency closure of `root` (excluding itself), in
/// deterministic breadth-first order with duplicates removed.
///
/// What coherence and membership checks need: every package whose impls
/// could be visible from `root`. The walk is cycle-safe (a `seen` set), though
/// the dependency graph is a DAG by construction.
#[salsa::tracked(returns(ref))]
pub fn package_dependency_closure(db: &dyn crate::Db, root: SourceRoot) -> Vec<SourceRoot> {
    let mut seen: std::collections::HashSet<SourceRoot> = std::collections::HashSet::new();
    let mut order: Vec<SourceRoot> = Vec::new();
    let mut queue: std::collections::VecDeque<SourceRoot> = root
        .dependencies(db)
        .iter()
        .map(|dependency| dependency.root)
        .collect();
    while let Some(dep) = queue.pop_front() {
        if dep == root || !seen.insert(dep) {
            continue;
        }
        order.push(dep);
        queue.extend(
            dep.dependencies(db)
                .iter()
                .map(|dependency| dependency.root),
        );
    }
    order
}

/// Whether `root` is served from its serialized compiler interface rather
/// than from source (a runtime mount, or a precompiled stdlib package in a
/// runtime compile). Such a package has no source rows: its `files`, if any,
/// are link-only stubs, and its interface is the semantic authority.
pub fn is_served_from_interface(db: &dyn crate::Db, root: SourceRoot) -> bool {
    root.interface(db).is_some()
}

/// Whether `root` is a compiler-built, image-immutable stdlib package served
/// from its interface. Ordinary runtime mounts remain replaceable and keep
/// the conservative mounted impl-facts shape; these rows are build artifacts
/// from this exact compiler and can be re-hydrated like source-backed facts.
pub fn is_precompiled_stdlib(db: &dyn crate::Db, root: SourceRoot) -> bool {
    root.kind(db) == SourceRootKind::Stdlib && root.interface(db).is_some()
}

// ── Spelling ─────────────────────────────────────────────────────────────────

/// How every root in the database is spelled on the wire: its own name if it
/// declares one, else the one name every dependency edge reaching it uses,
/// else — for a nameless root nothing depends on, the primary package of a
/// project with no manifest — the default [`RESERVED_USER_PACKAGE`].
///
/// This is the boundary's table. Compile-time heads carry the root itself
/// ([`DeclName`]) and never a spelling; the spelling is consulted exactly
/// where a head leaves the session — emit, package-interface export,
/// describe/export output, canonical dumps — and where one enters it (a
/// package-interface blob, a throw-fact seed). Today every root in the
/// database is compiled into one program, so the table is database-wide and
/// viewpoint-free; when emit compiles a root's closure alone it becomes
/// per-closure.
///
/// A spelling two roots share, or a root reached under two names, is a
/// [`collision`](Self::collisions): a boundary that needs the table injective
/// reports it; a renderer may still spell the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spelling {
    by_root: IndexMap<SourceRoot, Name>,
    by_name: IndexMap<Name, SourceRoot>,
    collisions: Vec<SpellingCollision>,
}

/// Why a [`Spelling`] is not injective.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpellingCollision {
    /// Two or more roots spell the same; the table holds the first.
    SharedName { name: Name, roots: Vec<SourceRoot> },
    /// A nameless root is reached under several edge names; the table holds
    /// the first.
    ManyNames { root: SourceRoot, names: Vec<Name> },
}

impl std::fmt::Display for SpellingCollision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SharedName { name, roots } => write!(
                f,
                "{} packages are spelled `{name}`; name them in their manifests",
                roots.len()
            ),
            Self::ManyNames { names, .. } => write!(
                f,
                "an unnamed package is depended on under {} different names ({}); name it in \
                 its manifest",
                names.len(),
                names
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

impl Spelling {
    /// A table from explicit `(root, spelling)` pairs, collisions included —
    /// for callers that hold the roots and their names outside a database
    /// (tests, tooling over an explicit graph).
    pub fn from_pairs(pairs: impl IntoIterator<Item = (SourceRoot, Name)>) -> Self {
        let mut by_root: IndexMap<SourceRoot, Name> = IndexMap::new();
        let mut by_name: IndexMap<Name, SourceRoot> = IndexMap::new();
        let mut collisions = Vec::new();
        for (root, name) in pairs {
            by_root.insert(root, name.clone());
            match by_name.entry(name) {
                indexmap::map::Entry::Vacant(slot) => {
                    slot.insert(root);
                }
                indexmap::map::Entry::Occupied(slot) => {
                    collisions.push(SpellingCollision::SharedName {
                        name: slot.key().clone(),
                        roots: vec![*slot.get(), root],
                    });
                }
            }
        }
        Self {
            by_root,
            by_name,
            collisions,
        }
    }

    /// The spelling of a live root.
    pub fn of(&self, root: SourceRoot) -> &Name {
        self.by_root
            .get(&root)
            .unwrap_or_else(|| unreachable!("every live root is in the spelling table: {root:?}"))
    }

    /// The root spelled `name`, if the table holds one.
    pub fn root(&self, name: &Name) -> Option<SourceRoot> {
        self.by_name.get(name).copied()
    }

    /// Every root with its spelling, in table order.
    pub fn roots(&self) -> impl Iterator<Item = (SourceRoot, &Name)> + '_ {
        self.by_root.iter().map(|(root, name)| (*root, name))
    }

    /// The wire name of a compile-time head.
    pub fn wire(&self, decl: &DeclName) -> TypeName {
        TypeName::new(
            self.of(decl.root()).clone(),
            decl.namespace().clone(),
            decl.name().clone(),
        )
    }

    /// The compile-time head a wire name denotes as seen from `viewpoint`:
    /// the artifact's own root is `viewpoint`; a dependency is the root
    /// `viewpoint` spells that way ([`accessible_package`]), else the root
    /// the program spells that way if it lies in `viewpoint`'s dependency
    /// closure — an interface names its transitive dependencies' types too
    /// (a field's type, a throw set), under the spelling the program gives
    /// them. `None` is the explicit unreachable-from-here outcome.
    pub fn resolve(
        &self,
        db: &dyn crate::Db,
        viewpoint: SourceRoot,
        name: &TypeName,
    ) -> Option<DeclName> {
        let root = if name.is_local() {
            viewpoint
        } else if let Some(root) = accessible_package(db, viewpoint, name.package()) {
            root
        } else {
            let root = self.root(name.package())?;
            package_dependency_closure(db, viewpoint)
                .contains(&root)
                .then_some(root)?
        };
        Some(DeclName::in_root(
            root,
            name.namespace().clone(),
            name.name().clone(),
        ))
    }

    /// Why the table is not injective, if it is not.
    pub fn collisions(&self) -> &[SpellingCollision] {
        &self.collisions
    }
}

/// The [`Spelling`] of every live root.
#[salsa::tracked(returns(ref))]
pub fn spelling(db: &dyn crate::Db) -> Spelling {
    let roots = db.source_roots().roots(db);
    // Every name a root is reached by, in table order of the depending root.
    let mut edge_names: IndexMap<SourceRoot, Vec<Name>> = IndexMap::new();
    for &from in roots {
        for dependency in from.dependencies(db) {
            let names = edge_names.entry(dependency.root).or_default();
            if !names.contains(&dependency.name) {
                names.push(dependency.name.clone());
            }
        }
    }
    let mut by_root: IndexMap<SourceRoot, Name> = IndexMap::new();
    let mut by_name: IndexMap<Name, SourceRoot> = IndexMap::new();
    let mut collisions = Vec::new();
    for &root in roots {
        let name = match root.self_name(db) {
            Some(own) => own,
            None => match edge_names.get(&root).map(Vec::as_slice) {
                Some([only]) => only.clone(),
                Some(names @ [first, ..]) => {
                    collisions.push(SpellingCollision::ManyNames {
                        root,
                        names: names.to_vec(),
                    });
                    first.clone()
                }
                Some([]) | None => Name::new(RESERVED_USER_PACKAGE),
            },
        };
        by_root.insert(root, name.clone());
        match by_name.entry(name) {
            indexmap::map::Entry::Vacant(slot) => {
                slot.insert(root);
            }
            indexmap::map::Entry::Occupied(slot) => {
                let name = slot.key().clone();
                match collisions.iter_mut().find(|collision| {
                    matches!(collision, SpellingCollision::SharedName { name: n, .. } if *n == name)
                }) {
                    Some(SpellingCollision::SharedName { roots, .. }) => roots.push(root),
                    _ => collisions.push(SpellingCollision::SharedName {
                        name,
                        roots: vec![*slot.get(), root],
                    }),
                }
            }
        }
    }
    Spelling {
        by_root,
        by_name,
        collisions,
    }
}

// ── Package items ────────────────────────────────────────────────────────────

/// Rare/optional data for `PackageItems`. Heap-allocated only when
/// at least one conflict or shadow exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageItemsExtra<'db> {
    pub conflicts: Vec<NameConflict<'db>>,
    pub shadows: Vec<NamespaceShadow<'db>>,
}

/// All items across all namespaces within a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageItems<'db> {
    /// The package these items belong to — the scope in which they, and any
    /// impls resolving their members, are visible.
    pub root: SourceRoot,
    /// Namespace path -> items within that namespace.
    pub namespaces: IndexMap<Vec<Name>, NamespaceItems<'db>>,
    /// Conflicts and other rare data. `None` when no conflicts exist.
    pub extra: Option<Box<PackageItemsExtra<'db>>>,
}

impl<'db> PackageItems<'db> {
    pub fn conflicts(&self) -> &[NameConflict<'db>] {
        self.extra
            .as_ref()
            .map(|e| e.conflicts.as_slice())
            .unwrap_or(&[])
    }

    pub fn shadows(&self) -> &[NamespaceShadow<'db>] {
        self.extra
            .as_ref()
            .map(|e| e.shadows.as_slice())
            .unwrap_or(&[])
    }
}

// ── salsa::Update impl ────────────────────────────────────────────────────────

/// # Safety
///
/// `PackageItems<'db>` contains `NamespaceItems<'db>` which transitively
/// contains `Definition<'db>` (Salsa interned types). This impl allows
/// `PackageItems<'db>` to be stored and returned by
/// `#[salsa::tracked(returns(ref))]` queries.
///
/// `maybe_update` uses `PartialEq` for proper Salsa early-cutoff.
#[allow(unsafe_code)]
unsafe impl salsa::Update for PackageItems<'_> {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: `old_pointer` is valid, aligned, and Salsa-owned.
        #[allow(unsafe_code)]
        let old = unsafe { &*old_pointer };
        if old == &new_value {
            false
        } else {
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            true
        }
    }
}

impl<'db> PackageItems<'db> {
    /// Look up a type by explicit namespace and item name.
    ///
    /// Single hash lookup — no split-loop ambiguity.
    /// `namespace` is the namespace path (e.g. `["llm"]` or `[]` for root).
    /// `item` is the unqualified item name (e.g. `"Response"`).
    pub fn lookup_type(&self, namespace: &[Name], item: &Name) -> Option<Definition<'db>> {
        self.namespaces.get(namespace)?.types.get(item).copied()
    }

    /// Look up a value by explicit namespace and item name.
    ///
    /// Single hash lookup — no split-loop ambiguity.
    pub fn lookup_value(&self, namespace: &[Name], item: &Name) -> Option<Definition<'db>> {
        self.namespaces.get(namespace)?.values.get(item).copied()
    }
}

/// Merges all `namespace_items` within a package.
///
/// Discovers all unique namespace paths for the package from its root's
/// files, then calls `namespace_items` for each — allowing Salsa to cache
/// each namespace's contribution independently.
#[salsa::tracked(returns(ref))]
pub fn package_items<'db>(db: &'db dyn crate::Db, root: SourceRoot) -> PackageItems<'db> {
    // Discover all unique namespace paths for this package from the
    // package's own files, so edits to another root's file set never
    // invalidate this fold.
    //
    // `IndexSet` (not `HashSet`) so the downstream `namespaces` map is built
    // in a deterministic insertion order. Without this, when two namespaces
    // declare items with the same short name (e.g. two `Status` enums in
    // different `ns_*/` directories), downstream consumers that key by short
    // name (`baml_compiler2_mir::lower::enum_variants`) see whichever
    // namespace was inserted last — flipping the choice of bytecode lowering
    // path across runs.
    let mut ns_paths: indexmap::IndexSet<Vec<Name>> = indexmap::IndexSet::new();
    for file in root.files(db) {
        let pkg_info = crate::file_package::file_package(db, *file);
        debug_assert_eq!(pkg_info.root, root);
        ns_paths.insert(pkg_info.namespace_path.clone());
    }

    let mut namespaces: IndexMap<Vec<Name>, NamespaceItems<'db>> = IndexMap::new();
    let mut all_conflicts: Vec<NameConflict<'db>> = Vec::new();
    for ns_path in ns_paths {
        let ns_id = NamespaceId::new(db, root, ns_path.clone());
        let items = namespace_items(db, ns_id);
        all_conflicts.extend(items.conflicts().iter().cloned());
        namespaces.insert(ns_path, items.clone());
    }

    // Detect namespace names that shadow root-level declarations.
    let mut shadows: Vec<NamespaceShadow<'db>> = Vec::new();
    if let Some(root_ns) = namespaces.get(&vec![] as &Vec<Name>) {
        for ns_path in namespaces.keys() {
            if ns_path.is_empty() {
                continue;
            }
            let first_segment = &ns_path[0];
            if let Some(def) = root_ns
                .types
                .get(first_segment)
                .or_else(|| root_ns.values.get(first_segment))
            {
                if is_allowed_builtin_namespace_shadow(db, root, ns_path, *def) {
                    continue;
                }
                shadows.push(NamespaceShadow {
                    ns_name: first_segment.clone(),
                    ns_path: ns_path.clone(),
                    shadowed_def: *def,
                });
            }
        }
    }
    shadows.sort_by(|a, b| a.ns_name.cmp(&b.ns_name));

    all_conflicts.sort_by(|a, b| a.name.cmp(&b.name));

    let extra = if all_conflicts.is_empty() && shadows.is_empty() {
        None
    } else {
        Some(Box::new(PackageItemsExtra {
            conflicts: all_conflicts,
            shadows,
        }))
    };

    PackageItems {
        root,
        namespaces,
        extra,
    }
}

/// The one allowlisted builtin collision: the stdlib `boundary` package's
/// root-level `id` function beside its `ns_id/` namespace.
fn is_allowed_builtin_namespace_shadow(
    db: &dyn crate::Db,
    root: SourceRoot,
    ns_path: &[Name],
    def: Definition<'_>,
) -> bool {
    root.kind(db) == SourceRootKind::Stdlib
        && lang_roots(db).is(LangPackage::Boundary, root)
        && ns_path.len() == 1
        && ns_path[0].as_str() == "id"
        && def.kind() == DefinitionKind::Function
        && def.file(db).path(db).to_string_lossy() == "<builtin>/boundary/core.baml"
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicU32, Ordering},
    };

    use baml_base::{
        Dependency, FileId, Name, SourceFile, SourceRoot, SourceRootKind, SourceRootTable,
    };
    use salsa::Setter;

    use super::{
        dependency_named, is_allowed_builtin_namespace_shadow, package_dependency_closure,
        package_items,
    };
    use crate::Db;

    #[salsa::db]
    struct TestDb {
        storage: salsa::Storage<TestDb>,
        next_file_id: AtomicU32,
        roots: Option<SourceRootTable>,
        lang_roots: Option<crate::inputs::LangRootsInput>,
    }

    impl Default for TestDb {
        fn default() -> Self {
            let mut db = Self {
                storage: salsa::Storage::default(),
                next_file_id: AtomicU32::new(0),
                roots: None,
                lang_roots: None,
            };
            db.roots = Some(SourceRootTable::new(&db, Vec::new()));
            db
        }
    }

    impl TestDb {
        fn add_root(
            &mut self,
            path: impl Into<PathBuf>,
            self_name: Option<&str>,
            kind: SourceRootKind,
            dependencies: Vec<Dependency>,
        ) -> SourceRoot {
            let root = SourceRoot::new(
                self,
                path.into(),
                kind,
                self_name.map(Name::new),
                Vec::new(),
                None,
                dependencies,
            );
            let table = self.roots.expect("table present from construction");
            let mut roots = table.roots(self).clone();
            roots.push(root);
            table.set_roots(self).to(roots);
            root
        }

        fn add_file_in(
            &mut self,
            root: SourceRoot,
            path: impl Into<PathBuf>,
            content: &str,
        ) -> SourceFile {
            let file_id = FileId::new(self.next_file_id.fetch_add(1, Ordering::SeqCst));
            let file =
                SourceFile::new(self, content.to_string(), path.into(), file_id, false, root);
            let mut files = root.files(self).clone();
            files.push(file);
            root.set_files(self).to(files);
            file
        }

        fn with_builtins() -> (Self, std::collections::BTreeMap<&'static str, SourceRoot>) {
            let mut db = Self::default();
            let mut roots: std::collections::BTreeMap<&str, SourceRoot> =
                std::collections::BTreeMap::new();
            for builtin in baml_builtins2::ALL {
                let root = *roots.entry(builtin.package).or_insert_with(|| {
                    db.add_root(
                        PathBuf::from(format!("<builtin>/{}", builtin.package)),
                        Some(builtin.package),
                        SourceRootKind::Stdlib,
                        Vec::new(),
                    )
                });
                db.add_file_in(
                    root,
                    PathBuf::from(builtin.virtual_path()),
                    builtin.contents,
                );
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
            db.lang_roots = Some(crate::inputs::LangRootsInput::new(&db, lang));
            (db, roots)
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}

    #[salsa::db]
    impl Db for TestDb {
        fn lang_roots_input(&self) -> Option<crate::inputs::LangRootsInput> {
            self.lang_roots
        }

        fn source_roots(&self) -> SourceRootTable {
            self.roots.expect("table present from construction")
        }
    }

    #[test]
    fn boundary_id_builtin_namespace_shadow_is_allowlisted() {
        let (db, roots) = TestDb::with_builtins();
        let boundary = roots["boundary"];
        let id = baml_base::Name::new("id");
        let package = package_items(&db, boundary);
        let root = package.namespaces.get(&Vec::new()).expect("root namespace");
        let id_namespace = vec![id.clone()];

        let id_def = root.values.get(&id).copied().expect("boundary.id function");
        assert!(
            package.namespaces.contains_key(&id_namespace),
            "boundary.id namespace should exist"
        );
        assert!(
            is_allowed_builtin_namespace_shadow(&db, boundary, &id_namespace, id_def),
            "boundary.id root function shadowed by boundary.id namespace is the only allowed builtin collision"
        );
        assert!(
            package.shadows().is_empty(),
            "the allowlisted boundary.id collision should not emit namespace-shadow diagnostics"
        );
    }

    #[test]
    fn builtin_namespace_shadow_allowlist_rejects_other_builtin_collisions() {
        let (db, roots) = TestDb::with_builtins();
        let boundary = roots["boundary"];
        let id = baml_base::Name::new("id");
        let package = package_items(&db, boundary);
        let root = package.namespaces.get(&Vec::new()).expect("root namespace");
        let id_def = root.values.get(&id).copied().expect("boundary.id function");

        assert!(!is_allowed_builtin_namespace_shadow(
            &db,
            roots["baml"],
            std::slice::from_ref(&id),
            id_def
        ));
        assert!(!is_allowed_builtin_namespace_shadow(
            &db,
            boundary,
            &[baml_base::Name::new("other")],
            id_def
        ));
    }

    #[test]
    fn edges_resolve_by_name_and_close_transitively() {
        let mut db = TestDb::default();
        let leaf = db.add_root(
            "/leaf",
            Some("leaf"),
            SourceRootKind::Dependency,
            Vec::new(),
        );
        let mid = db.add_root(
            "/mid",
            Some("mid"),
            SourceRootKind::Dependency,
            vec![Dependency {
                name: Name::new("leaf"),
                root: leaf,
            }],
        );
        // The same root reached under a different spelling: names are on
        // edges, so the consumer's alias resolves to the one root.
        let app = db.add_root(
            "/app",
            None,
            SourceRootKind::Workspace,
            vec![
                Dependency {
                    name: Name::new("middle"),
                    root: mid,
                },
                Dependency {
                    name: Name::new("l"),
                    root: leaf,
                },
            ],
        );

        assert_eq!(dependency_named(&db, app, &Name::new("middle")), Some(mid));
        assert_eq!(dependency_named(&db, app, &Name::new("l")), Some(leaf));
        assert_eq!(dependency_named(&db, app, &Name::new("mid")), None);
        assert_eq!(dependency_named(&db, app, &Name::new("leaf")), None);
        assert_eq!(package_dependency_closure(&db, app), &[mid, leaf]);
        assert_eq!(package_dependency_closure(&db, leaf), &[]);
    }
}
