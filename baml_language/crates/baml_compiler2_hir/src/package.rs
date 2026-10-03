//! Package-level cross-file symbol aggregation.
//!
//! A package IS a [`SourceRoot`]: package identity is the root's input id,
//! every package-level query is keyed on it, and a package reaches another
//! only through an edge in its own [`SourceRoot::dependencies`] list, under
//! the name that edge carries. `package_items` merges all `namespace_items`
//! within a package into a single lookup structure — the top-level
//! cross-file query used by the TIR layer for name resolution.

use std::collections::{HashMap, VecDeque};

use baml_base::{LangRoots, Name, SourceFile, SourceRoot, SourceRootKind, Span};
use baml_compiler_diagnostics::diagnostic::{Diagnostic, DiagnosticId, DiagnosticPhase};
use baml_type::{
    DeclName, PathName, RESERVED_USER_PACKAGE, TypeName,
    wire::{DepSlot, Digest, EdgePath, Locator},
};
use indexmap::IndexMap;

use crate::{
    contributions::Definition,
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

/// The roots emitted into `root`'s program — `root` and its dependency
/// closure — in source-root table order (`Stdlib < Dependency < Workspace <
/// Dynamic`), so the stdlib keeps its user-independent prefix of every index
/// space. This is the world an emit sees: never the whole database, which
/// may hold other workspace roots that cannot share a program with this one.
#[salsa::tracked(returns(ref))]
pub fn world_roots(db: &dyn crate::Db, root: SourceRoot) -> Vec<SourceRoot> {
    let visible = visible_packages(db, root);
    db.source_roots()
        .roots(db)
        .iter()
        .copied()
        .filter(|candidate| visible.contains(candidate))
        .collect()
}

/// Every file of [`world_roots`], root by root in table order, each root's
/// files in its own order — the file set a program built for `root` is
/// emitted from.
#[salsa::tracked(returns(ref))]
pub fn world_files(db: &dyn crate::Db, root: SourceRoot) -> Vec<SourceFile> {
    world_roots(db, root)
        .iter()
        .flat_map(|root| root.files(db).iter().copied())
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

/// A package's edge table: every package it reaches, by the name it reaches
/// it under — its manifest edges, then the language packages under their
/// fixed names. The language packages are reachable from every package,
/// their own siblings included: `baml` calls the metatype's companion
/// methods in `reflect` while `reflect` depends on `baml`, which no manifest
/// edge could express without a cycle. A non-stdlib root already carries the
/// prelude as explicit edges, so for it the fixed names add nothing.
pub fn edge_table(db: &dyn crate::Db, root: SourceRoot) -> Vec<(Name, SourceRoot)> {
    let mut edges: Vec<(Name, SourceRoot)> = root
        .dependencies(db)
        .iter()
        .map(|dependency| (dependency.name.clone(), dependency.root))
        .collect();
    let lang = lang_roots(db);
    for package in baml_base::LangPackage::ALL {
        let Some(lang_root) = lang.get(package) else {
            continue;
        };
        let name = Name::new(package.manifest_name());
        if lang_root != root && !edges.iter().any(|(edge, _)| *edge == name) {
            edges.push((name, lang_root));
        }
    }
    edges
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
/// runtime compile). Such a package has no files: its interface is the
/// semantic authority, and its rows are its declarations.
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
/// This is the rendering boundary's table. Compile-time heads carry the root
/// itself ([`DeclName`]) and never a spelling, through MIR and into emit; the
/// spelling is consulted exactly where a head is SHOWN as a program-wide name
/// — a runtime declaration's display name, operand metadata and MIR dumps,
/// describe/export output, canonical dumps. It is write-only: nothing reads a
/// spelling back into a declaration. No artifact uses it for identity: a
/// package interface blob's heads are slots of its own dependency table
/// ([`DependencyTable`]), a unit's are declaration operands, and a cached
/// seed's are located by edge path ([`located_head`]), so each means the same
/// declarations whatever the program calls its packages.
///
/// A spelling two roots share, or a root reached under two names, is a
/// [`collision`](Self::collisions): what is rendered is then ambiguous to a
/// reader, and nothing more. The compiler needs no injective table — such a
/// program compiles and links — so the report is for a surface that wants to
/// warn about it (the package provider, the CLI).
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
    /// A table from explicit `(root, spelling)` pairs, shared-name collisions
    /// included — for callers that hold the roots and their names outside a
    /// database (tests, tooling over an explicit graph).
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
                    // One entry per name, holding every root that claims it,
                    // the way `spelling` builds them. Pushing a fresh pair per
                    // clash would report three colliding roots as two separate
                    // pairs, and the same table would describe itself
                    // differently depending on which constructor built it.
                    let name = slot.key().clone();
                    let first = *slot.get();
                    match collisions.iter_mut().find_map(|collision| match collision {
                        SpellingCollision::SharedName { name: seen, roots } if *seen == name => {
                            Some(roots)
                        }
                        SpellingCollision::SharedName { .. }
                        | SpellingCollision::ManyNames { .. } => None,
                    }) {
                        Some(roots) => roots.push(root),
                        None => collisions.push(SpellingCollision::SharedName {
                            name,
                            roots: vec![first, root],
                        }),
                    }
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

    /// Why the table is not injective, if it is not.
    pub fn collisions(&self) -> &[SpellingCollision] {
        &self.collisions
    }

    /// This table restricted to `roots`: every root keeps the spelling it has
    /// here (a root's spelling depends on its own name and the edges reaching
    /// it, never on which other roots share a program with it), and the
    /// collisions are judged among `roots` alone — two workspace roots that
    /// both take the default spelling collide only if one program holds both.
    /// A root reached under several edge names stays a collision wherever it
    /// appears.
    #[must_use]
    pub fn within(&self, roots: &[SourceRoot]) -> Spelling {
        let mut restricted =
            Self::from_pairs(roots.iter().map(|&root| (root, self.of(root).clone())));
        restricted.collisions.extend(
            self.collisions
                .iter()
                .filter(|collision| match collision {
                    SpellingCollision::ManyNames { root, .. } => roots.contains(root),
                    SpellingCollision::SharedName { .. } => false,
                })
                .cloned(),
        );
        restricted
    }
}

/// The [`Spelling`] of the program built for `root`: [`spelling`] restricted
/// to [`world_roots`], so a shared default spelling between two workspace
/// roots that never meet in one program is no collision for either program.
/// The table a surface reads to report that ONE program renders ambiguously;
/// rendering itself keeps the database-wide one.
#[salsa::tracked(returns(ref))]
pub fn spelling_within(db: &dyn crate::Db, root: SourceRoot) -> Spelling {
    spelling(db).within(world_roots(db, root))
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

// ── Salsa retention safety ────────────────────────────────────────────────

// SAFETY: This type owns its data. Its database lifetime only appears
// in Salsa identities; it contains no references into query storage.
#[allow(unsafe_code)]
unsafe impl salsa::SalsaValue for PackageItems<'_> {}

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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use baml_base::{Dependency, Name, SourceRoot, SourceRootKind, SourceRootTable};
    use salsa::Setter;

    use super::{dependency_named, package_dependency_closure};
    use crate::Db;

    #[salsa::db]
    struct TestDb {
        storage: salsa::Storage<TestDb>,
        roots: Option<SourceRootTable>,
    }

    impl Default for TestDb {
        fn default() -> Self {
            let mut db = Self {
                storage: salsa::Storage::default(),
                roots: None,
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
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}

    #[salsa::db]
    impl Db for TestDb {
        fn source_roots(&self) -> SourceRootTable {
            self.roots.expect("table present from construction")
        }
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

// ── Dependency tables ────────────────────────────────────────────────────────

/// The shortest edge path from `owner` to `target`, as `(parent, edge name,
/// child)` hops in order — breadth-first over the edge tables, so the located
/// root is always the nearest one. `None` when no edge path from `owner`
/// reaches `target`.
pub fn edge_path(
    db: &dyn crate::Db,
    owner: SourceRoot,
    target: SourceRoot,
) -> Option<Vec<(SourceRoot, Name, SourceRoot)>> {
    let mut parents: HashMap<SourceRoot, (SourceRoot, Name)> = HashMap::new();
    let mut queue = VecDeque::from([owner]);
    while let Some(current) = queue.pop_front() {
        if current == target {
            break;
        }
        for (edge, root) in edge_table(db, current) {
            if root == owner || parents.contains_key(&root) {
                continue;
            }
            parents.insert(root, (current, edge));
            queue.push_back(root);
        }
    }
    let mut hops = Vec::new();
    let mut child = target;
    while let Some((parent, edge)) = parents.get(&child) {
        hops.push((*parent, edge.clone(), child));
        child = *parent;
    }
    if child != owner {
        return None;
    }
    hops.reverse();
    Some(hops)
}

/// `target` as `owner` locates it: the edge names of the shortest edge path
/// ([`edge_path`]), empty for `owner` itself. A function of the two packages'
/// edge tables alone, so every artifact of `owner` spells `target` the same
/// way.
///
/// # Panics
///
/// When no edge path from `owner` reaches `target` — the same internal
/// invariant as [`DependencyTable::slot`].
pub fn edge_spelling(db: &dyn crate::Db, owner: SourceRoot, target: SourceRoot) -> EdgePath {
    EdgePath(
        edge_path(db, owner, target)
            .unwrap_or_else(|| {
                unreachable!(
                    "package `{}` names package `{}`, which no edge path from it reaches",
                    owner.path(db).display(),
                    target.path(db).display()
                )
            })
            .into_iter()
            .map(|(_, edge, _)| edge)
            .collect(),
    )
}

/// The package `path` locates from `owner`: each edge followed through the
/// edge table of the package it leaves — the inverse of [`edge_spelling`].
/// `None` when a hop names an edge the package it leaves does not have.
pub fn edge_target(db: &dyn crate::Db, owner: SourceRoot, path: &EdgePath) -> Option<SourceRoot> {
    path.0.iter().try_fold(owner, |current, edge| {
        edge_table(db, current)
            .into_iter()
            .find_map(|(name, root)| (name == *edge).then_some(root))
    })
}

/// A compile-time head as `owner` locates it: its declaring package by edge
/// path ([`edge_spelling`]). The spelling of a head in anything several
/// artifacts of `owner` must agree on, or that outlives the compile — an
/// identity component, a cached seed.
///
/// # Panics
///
/// When no edge path from `owner` reaches the head's package (see
/// [`edge_spelling`]).
pub fn located_head(db: &dyn crate::Db, owner: SourceRoot, decl: &DeclName) -> PathName {
    decl.map_key(|declaring| edge_spelling(db, owner, *declaring))
}

/// The compile-time head a located name denotes from `owner` — the inverse of
/// [`located_head`]. `None` when its path leaves `owner`'s edges
/// ([`edge_target`]).
pub fn resolve_located(db: &dyn crate::Db, owner: SourceRoot, name: &PathName) -> Option<DeclName> {
    name.try_map_key(|path| edge_target(db, owner, path).ok_or(()))
        .ok()
}

/// An artifact's dependency table under construction: one slot per package
/// root the artifact's owner names, located through the owner's edge table
/// directly or through an earlier slot's ([`Locator`]). The one interner
/// behind every wire artifact — a package interface's rows and a compilation
/// unit's imports spell foreign declarations by these slots — so the two
/// agree on what a slot means by construction.
#[derive(Debug, Clone)]
pub struct DependencyTable {
    owner: SourceRoot,
    /// Slot `k + 1`: the root it binds, and the hop that located it.
    entries: Vec<(SourceRoot, DepSlot, Name)>,
}

impl DependencyTable {
    pub fn new(owner: SourceRoot) -> Self {
        Self {
            owner,
            entries: Vec::new(),
        }
    }

    /// The package the table belongs to: slot [`DepSlot::SELF`].
    pub fn owner(&self) -> SourceRoot {
        self.owner
    }

    /// The slot `root` is (or becomes) bound to. A root the owner has no
    /// direct edge to is located through the nearest package that does: the
    /// path from the owner is interned hop by hop, each hop `via` its
    /// parent's slot.
    ///
    /// # Panics
    ///
    /// When no edge path from the owner reaches `root`. Every head an
    /// artifact names lies in its owner's dependency closure — the checker
    /// resolved it through those edges — so this is an internal invariant,
    /// not an input error.
    pub fn slot(&mut self, db: &dyn crate::Db, root: SourceRoot) -> DepSlot {
        if root == self.owner {
            return DepSlot::SELF;
        }
        if let Some(slot) = self.existing(root) {
            return slot;
        }
        let path = edge_path(db, self.owner, root).unwrap_or_else(|| {
            unreachable!(
                "package `{}` names package `{}`, which no edge path from it reaches",
                self.owner.path(db).display(),
                root.path(db).display()
            )
        });
        for (parent, edge, hop) in path {
            if self.existing(hop).is_some() {
                continue;
            }
            let via = if parent == self.owner {
                DepSlot::SELF
            } else {
                self.existing(parent)
                    .unwrap_or_else(|| unreachable!("a path's parent is interned before its child"))
            };
            self.entries.push((hop, via, edge));
        }
        self.existing(root)
            .unwrap_or_else(|| unreachable!("the path ends at the root"))
    }

    fn existing(&self, root: SourceRoot) -> Option<DepSlot> {
        self.entries
            .iter()
            .position(|(bound, _, _)| *bound == root)
            .map(DepSlot::of_dependency_index)
    }

    /// The root bound to `slot`.
    pub fn root_of(&self, slot: DepSlot) -> SourceRoot {
        match slot.dependency_index() {
            None => self.owner,
            Some(index) => self.entries[index].0,
        }
    }

    /// The roots bound to slots `1..`, in slot order.
    pub fn roots(&self) -> impl Iterator<Item = SourceRoot> + '_ {
        self.entries.iter().map(|(root, _, _)| *root)
    }

    /// The table as an artifact carries it: a direct entry is
    /// [`Locator::Direct`] with `digest(root)` — the interface payload the
    /// owner compiled against — or [`Locator::Prelude`] for a language
    /// package (a `Stdlib` root; no payload by design); a transitive entry is
    /// [`Locator::Transitive`], covered by its parent's digest.
    pub fn locators(
        &self,
        db: &dyn crate::Db,
        mut digest: impl FnMut(SourceRoot) -> Digest,
    ) -> Vec<Locator> {
        self.entries
            .iter()
            .map(|(root, via, edge)| {
                if !via.is_self() {
                    Locator::Transitive {
                        via: *via,
                        edge: edge.clone(),
                    }
                } else if root.kind(db) == SourceRootKind::Stdlib {
                    Locator::Prelude { edge: edge.clone() }
                } else {
                    Locator::Direct {
                        edge: edge.clone(),
                        digest: digest(*root),
                    }
                }
            })
            .collect()
    }
}
