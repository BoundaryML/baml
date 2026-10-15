//! Package-level cross-file symbol aggregation.
//!
//! A package IS a [`SourceRoot`]: package identity is the root's input id,
//! every package-level query is keyed on it, and a package reaches another
//! only through an edge in its own [`SourceRoot::dependencies`] list, under
//! the name that edge carries. `package_items` merges all `namespace_items`
//! within a package into a single lookup structure — the top-level
//! cross-file query used by the TIR layer for name resolution.

use std::collections::{HashMap, VecDeque};

use baml_base::{LangRoots, Name, PathOrigin, SourceFile, SourceRoot, SourceRootKind, Span};
use baml_compiler_diagnostics::diagnostic::{Diagnostic, DiagnosticId, DiagnosticPhase};
use baml_type::{
    DeclName, PathName,
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
    /// The error, at the item's name.
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

        let kind = def.source_kind_name(db);
        let message = format!(
            "`{}` is both a root-level {kind} and the namespace `ns_{}/`",
            self.ns_name, self.ns_name
        );

        let mut diag = Diagnostic::error(DiagnosticId::NamespaceNameCollision, message);

        if let Some(range) = name_span {
            diag = diag.with_primary(
                Span { file_id, range },
                format!(
                    "rename this {kind} or the namespace: `{}.X` reads a member of the {kind}, \
                     `root.{}.X` an item of the namespace",
                    self.ns_name, self.ns_name
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
/// The ONE place an edge name becomes the package it reaches: `baml` in
/// `baml.Array` resolves through the spelling package's own edge list and
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
///
/// Tracked: a viewpoint, a binder and an edge walk each read it, and it
/// changes only with a manifest or the installed language packages.
#[salsa::tracked(returns(ref))]
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

/// The spelling of the edge from a package to itself — Rust's `crate`.
pub const ROOT_SEGMENT: &str = "root";

/// How the first segment of a path is read. Decided ONCE, here in HIR, and
/// handed to every consumer: nothing downstream maps a head name to a
/// package. The reading of the segments after the head follows from it.
///
/// Precedence for a WRITTEN path, outermost claim last: a binding in scope
/// (the caller's, before [`path_head`] is asked), an item of the writer's
/// own namespace, then `root` or an edge of the writer's package. An item
/// spelled like a package shadows the package entirely (`#4543`): with a
/// class `reflect` in the namespace, `reflect.signature` is a member of that
/// class, and the package `reflect` is unreachable by that name from there.
/// A compiler-written path is read by identity instead ([`PathHead::Lang`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathHead {
    /// `root`, or a dependency edge of the reading package: the package it
    /// reaches. The rest of the path is `<namespaces>.<item>` inside that
    /// package, then members of the item. A lexical name shadows it, as a
    /// binding does in a body.
    Package(SourceRoot),
    /// A language package the COMPILER named, by identity
    /// ([`PathOrigin::Compiler`]): read exactly like `Package`, except that
    /// no name in any scope shadows it — rustc's `$crate`. What lowered
    /// syntax reaches the standard library through.
    Lang(SourceRoot),
    /// A plain name of the surrounding scope: an item of the file's own
    /// namespace, or a name only a lexical scope declares (a generic
    /// parameter). The rest of the path are members of what it names. Also
    /// the head of an item the COMPILER named (`Owner.Method@spec`), which
    /// is the namespace's item and nothing a binding can shadow.
    Plain,
}

/// The package `first` names when it heads a qualified path written in
/// `own`: `root` is `own` itself, any other name is `own`'s edge of that
/// name ([`dependency_named`]). `None` when `first` names no package the
/// writer can see — the path is then read in the surrounding scope.
///
/// A package's own declared name is NOT among its edges: a package spells
/// itself as `root` and nothing else (Rust's `crate`), so the standard
/// library writes `root.json.json` inside `baml`, and a project named `app`
/// that writes `app.Foo` names an item `app` of its scope. The name on the
/// manifest is display metadata and the default a dependent spells the
/// package by, never a way to reach it.
///
/// The ONE place a written first segment becomes a package; whether the
/// segment is read as one at all is [`path_head`]'s call.
pub fn path_package(db: &dyn crate::Db, own: SourceRoot, first: &Name) -> Option<SourceRoot> {
    if first.as_str() == ROOT_SEGMENT {
        return Some(own);
    }
    dependency_named(db, own, first)
}

/// The [`PathHead`] of `segments`, written in the namespace `namespace` of
/// `own`, once no binding has claimed the first segment. An item of that
/// namespace claims it next ([`crate::namespace::namespace_declares`]); only
/// then is the segment read as `root` or an edge. A package segment is a
/// QUALIFIER — it has a path after it — so a lone name is never a package:
/// `log` alone is the item `log`, `log.info` starts at the package. Nothing
/// else about the path's length decides anything: `Cart.empty` is `Plain`
/// and reads as the item `Cart` followed by its member.
pub fn path_head(
    db: &dyn crate::Db,
    own: SourceRoot,
    namespace: &[Name],
    segments: &[Name],
) -> PathHead {
    match segments {
        // `root` first: nothing may be named `root` (E0176), so a refused
        // item of that name cannot take every `root.X` of its namespace
        // down with it.
        [first, _, ..] if first.as_str() == ROOT_SEGMENT => PathHead::Package(own),
        [first, _, ..] => {
            if crate::namespace::namespace_declares(db, own, namespace, first) {
                return PathHead::Plain;
            }
            path_package(db, own, first).map_or(PathHead::Plain, PathHead::Package)
        }
        [] | [_] => PathHead::Plain,
    }
}

/// The [`PathHead`] of a path the COMPILER wrote ([`PathOrigin::Compiler`]),
/// never looked up in any scope: a language package's fixed name is read by
/// identity from where the stdlib is installed (with the package not
/// installed the path names nothing, and reads as a plain name that resolves
/// to nothing); any other first segment is an item of the writer's own
/// namespace ([`baml_base::Path::compiler_item`]), read as a plain name —
/// the semantic index records no binding for it, so none shadows it.
pub fn compiler_path_head(db: &dyn crate::Db, segments: &[Name]) -> PathHead {
    let [first, _, ..] = segments else {
        unreachable!("a compiler-written path names a package or item and a member: {segments:?}")
    };
    match baml_base::LangPackage::from_manifest_name(first.as_str()) {
        Some(package) => lang_roots(db)
            .get(package)
            .map_or(PathHead::Plain, PathHead::Lang),
        None => PathHead::Plain,
    }
}

/// The [`PathHead`] of `segments` as `origin` says it was written: a written
/// path's head is a name of `own`'s scope, read from the namespace
/// `namespace` it was written in ([`path_head`]); a compiler-written path's
/// is a language package by identity or an item of the writer's namespace,
/// in no scope ([`compiler_path_head`]).
pub fn path_head_for(
    db: &dyn crate::Db,
    own: SourceRoot,
    namespace: &[Name],
    origin: PathOrigin,
    segments: &[Name],
) -> PathHead {
    match origin {
        PathOrigin::Written => path_head(db, own, namespace, segments),
        PathOrigin::Compiler => compiler_path_head(db, segments),
    }
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

    use super::{PathHead, dependency_named, package_dependency_closure, path_head};
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

    #[test]
    fn a_path_head_is_root_an_edge_or_a_plain_name() {
        let mut db = TestDb::default();
        let lib = db.add_root("/lib", Some("lib"), SourceRootKind::Dependency, Vec::new());
        let app = db.add_root(
            "/app",
            None,
            SourceRootKind::Workspace,
            vec![Dependency {
                name: Name::new("util"),
                root: lib,
            }],
        );
        let path = |segments: &[&str]| segments.iter().copied().map(Name::new).collect::<Vec<_>>();

        // `root` is the package itself; an edge name is the package it reaches.
        assert_eq!(
            path_head(&db, app, &[], &path(&["root", "Foo"])),
            PathHead::Package(app)
        );
        assert_eq!(
            path_head(&db, app, &[], &path(&["util", "Foo"])),
            PathHead::Package(lib)
        );
        // A name no edge carries - the dependency's OWN name included - is a
        // plain name of the scope: `Cart.empty` starts at the item `Cart`.
        // A package's own declared name is not an edge either: `lib` spells
        // itself as `root` only, and `lib.Foo` written inside it is the item
        // `lib` of its scope followed by a member.
        assert_eq!(
            path_head(&db, lib, &[], &path(&["lib", "Foo"])),
            PathHead::Plain
        );
        assert_eq!(
            path_head(&db, lib, &[], &path(&["root", "Foo"])),
            PathHead::Package(lib)
        );
        assert_eq!(
            path_head(&db, app, &[], &path(&["lib", "Foo"])),
            PathHead::Plain
        );
        assert_eq!(
            path_head(&db, app, &[], &path(&["Cart", "empty"])),
            PathHead::Plain
        );
        // A package segment qualifies what follows it; alone, a name is the
        // item of that name, whatever packages are called.
        assert_eq!(path_head(&db, app, &[], &path(&["util"])), PathHead::Plain);
        assert_eq!(path_head(&db, app, &[], &path(&["root"])), PathHead::Plain);
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
            let root = *root;
            if root == owner || parents.contains_key(&root) {
                continue;
            }
            parents.insert(root, (current, edge.clone()));
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
            .iter()
            .find_map(|(name, root)| (name == edge).then_some(*root))
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
    /// package (by identity; no payload by design); a transitive entry is
    /// [`Locator::Transitive`], covered by its parent's digest.
    pub fn locators(
        &self,
        db: &dyn crate::Db,
        mut digest: impl FnMut(SourceRoot) -> Digest,
    ) -> Vec<Locator> {
        let lang = lang_roots(db);
        self.entries
            .iter()
            .map(|(root, via, edge)| {
                if !via.is_self() {
                    Locator::Transitive {
                        via: *via,
                        edge: edge.clone(),
                    }
                } else if let Some(language) = lang.package_of(*root) {
                    Locator::Prelude(language)
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
