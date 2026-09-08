//! Package-level cross-file symbol aggregation.
//!
//! A package IS a [`SourceRoot`]: package identity is the root's input id,
//! every package-level query is keyed on it, and a package reaches another
//! only through an edge in its own [`SourceRoot::dependencies`] list, under
//! the name that edge carries. `package_items` merges all `namespace_items`
//! within a package into a single lookup structure — the top-level
//! cross-file query used by the TIR layer for name resolution.

use baml_base::{Name, SourceRoot, SourceRootKind, Span};
use baml_compiler_diagnostics::diagnostic::{Diagnostic, DiagnosticId, DiagnosticPhase};
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

/// The sole workspace root, if there is one.
///
/// Stopgap for the single-workspace-root state: the compiler's type heads
/// still spell a package by name (see [`wire_name`]), so a database holds at
/// most one `Workspace` root until heads carry the root itself. Callers that
/// need "the user's package" without a request file to derive it from use
/// this, so the multi-root sweep has one seam to widen.
pub fn sole_workspace_root(db: &dyn crate::Db) -> Option<SourceRoot> {
    let roots = workspace_roots(db);
    debug_assert!(
        roots.len() <= 1,
        "multiple workspace roots in one database require root-carrying type heads"
    );
    roots.first().copied()
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
/// [`wire_name`] (a package may qualify its own items by name), else the
/// dependency reached by the edge named `name`. Anything else is invisible.
///
/// The self-by-name half reads the interim wire name; once heads carry the
/// root it reads `self_name` alone, so an unnamed package has no name to
/// spell itself by besides `root`.
pub fn accessible_package(db: &dyn crate::Db, root: SourceRoot, name: &Name) -> Option<SourceRoot> {
    if wire_name(db, root) == *name {
        return Some(root);
    }
    dependency_named(db, root, name)
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

// ── The wire-name seam ───────────────────────────────────────────────────────

/// The name `root`'s items are spelled with in every name-keyed structure of
/// this database: its own name, else the unnamed-package default.
///
/// Interim seam. Type heads still identify a package by name
/// (`QualifiedTypeName`'s package field), so every root must have exactly one
/// spelling and the spelling must be unique per database — the database
/// enforces that at root creation. When heads carry the root itself, this
/// function and [`roots_by_wire_name`] are deleted and a package is spelled
/// per viewpoint through its consumer's edge name.
pub fn wire_name(db: &dyn crate::Db, root: SourceRoot) -> Name {
    root.self_name(db)
        .unwrap_or_else(|| Name::new(baml_type::RESERVED_USER_PACKAGE))
}

/// Every live root keyed by its [`wire_name`], in table order.
#[salsa::tracked(returns(ref))]
pub fn roots_by_wire_name(db: &dyn crate::Db) -> IndexMap<Name, SourceRoot> {
    let mut index: IndexMap<Name, SourceRoot> = IndexMap::new();
    for &root in db.source_roots().roots(db) {
        let name = wire_name(db, root);
        let previous = index.insert(name.clone(), root);
        debug_assert!(
            previous.is_none(),
            "two live roots share the wire name `{name}`; the database must reject the second"
        );
    }
    index
}

/// The root spelled `name` (the inverse of [`wire_name`]).
pub fn root_by_wire_name(db: &dyn crate::Db, name: &Name) -> Option<SourceRoot> {
    roots_by_wire_name(db).get(name).copied()
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
        && root
            .self_name(db)
            .is_some_and(|name| name.as_str() == "boundary")
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
    }

    impl Default for TestDb {
        fn default() -> Self {
            let mut db = Self {
                storage: salsa::Storage::default(),
                next_file_id: AtomicU32::new(0),
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
            (db, roots)
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
