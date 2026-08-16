//! Package-level cross-file symbol aggregation.
//!
//! `package_items` merges all `namespace_items` within a package into a single
//! lookup structure. This is the top-level cross-file query used by the TIR
//! layer for name resolution.

use baml_base::{Name, Span};
use baml_compiler_diagnostics::diagnostic::{Diagnostic, DiagnosticId, DiagnosticPhase};
use rustc_hash::FxHashMap;

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

/// Interned package identity.
#[salsa::interned]
pub struct PackageId<'db> {
    pub name: Name,
}

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
    /// The name of the package these items belong to — the scope in which they,
    /// and any impls resolving their members, are visible. Reconstruct the
    /// interned id with `PackageId::new(db, package.clone())` when one is needed.
    pub package: Name,
    /// Namespace path -> items within that namespace.
    pub namespaces: FxHashMap<Vec<Name>, NamespaceItems<'db>>,
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
/// Discovers all unique namespace paths for the package by scanning project
/// files, then calls `namespace_items` for each — allowing Salsa to cache
/// each namespace's contribution independently.
#[salsa::tracked(returns(ref))]
pub fn package_items<'db>(db: &'db dyn crate::Db, package_id: PackageId<'db>) -> PackageItems<'db> {
    let package_name = package_id.name(db);

    // Discover all unique namespace paths for this package.
    // Use compiler2_all_files() so that compiler2-only builtin stubs (e.g.
    // Array<T>, Map<K,V>) are visible here without being added to the v1
    // compiler's project.files() list.
    //
    // `IndexSet` (not `HashSet`) so the downstream `namespaces` map is built
    // in a deterministic insertion order. Without this, when two namespaces
    // declare items with the same short name (e.g. two `Status` enums in
    // different `ns_*/` directories), downstream consumers that key by short
    // name (`baml_compiler2_mir::lower::enum_variants`) see whichever
    // namespace was inserted last — flipping the choice of bytecode lowering
    // path across runs.
    let mut ns_paths: indexmap::IndexSet<Vec<Name>> = indexmap::IndexSet::new();
    for file in crate::compiler2_all_files(db) {
        let pkg_info = crate::file_package::file_package(db, file);
        if pkg_info.package == *package_name {
            ns_paths.insert(pkg_info.namespace_path.clone());
        }
    }

    let mut namespaces: FxHashMap<Vec<Name>, NamespaceItems<'db>> = FxHashMap::default();
    let mut all_conflicts: Vec<NameConflict<'db>> = Vec::new();
    for ns_path in ns_paths {
        let ns_id = NamespaceId::new(db, package_name.clone(), ns_path.clone());
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
                if is_allowed_builtin_namespace_shadow(db, &package_name, ns_path, *def) {
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
        package: package_name,
        namespaces,
        extra,
    }
}

fn is_allowed_builtin_namespace_shadow(
    db: &dyn crate::Db,
    package_name: &Name,
    ns_path: &[Name],
    def: Definition<'_>,
) -> bool {
    package_name.as_str() == "boundary"
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

    use baml_base::{FileId, SourceFile};
    use baml_workspace::{Compiler2ExtraFiles, Project};
    use salsa::Setter;

    use super::{PackageId, is_allowed_builtin_namespace_shadow, package_items};
    use crate::Db;

    #[salsa::db]
    struct TestDb {
        storage: salsa::Storage<TestDb>,
        next_file_id: AtomicU32,
        project: Option<Project>,
        extra: Option<Compiler2ExtraFiles>,
    }

    impl Default for TestDb {
        fn default() -> Self {
            Self {
                storage: salsa::Storage::default(),
                next_file_id: AtomicU32::new(0),
                project: None,
                extra: None,
            }
        }
    }

    impl TestDb {
        fn add_file(&mut self, path: impl Into<PathBuf>, content: &str) -> SourceFile {
            let file_id = FileId::new(self.next_file_id.fetch_add(1, Ordering::SeqCst));
            SourceFile::new(self, content.to_string(), path.into(), file_id, false)
        }

        fn with_builtins() -> Self {
            let mut db = Self::default();
            let builtin_files = baml_builtins2::ALL
                .iter()
                .map(|builtin| db.add_file(PathBuf::from(builtin.virtual_path()), builtin.contents))
                .collect::<Vec<_>>();
            let project = Project::new(&db, PathBuf::from("/test"), Vec::new());
            project.set_files(&mut db).to(Vec::new());
            db.project = Some(project);
            db.extra = Some(Compiler2ExtraFiles::new(&db, builtin_files));
            db
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}

    #[salsa::db]
    impl baml_workspace::Db for TestDb {
        fn project(&self) -> Project {
            self.project.expect("test db initialized")
        }
    }

    #[salsa::db]
    impl Db for TestDb {
        fn compiler2_extra_files(&self) -> Option<Compiler2ExtraFiles> {
            self.extra
        }
    }

    #[test]
    fn boundary_id_builtin_namespace_shadow_is_allowlisted() {
        let db = TestDb::with_builtins();
        let boundary = baml_base::Name::new("boundary");
        let id = baml_base::Name::new("id");
        let package = package_items(&db, PackageId::new(&db, boundary.clone()));
        let root = package.namespaces.get(&Vec::new()).expect("root namespace");
        let id_namespace = vec![id.clone()];

        let id_def = root.values.get(&id).copied().expect("boundary.id function");
        assert!(
            package.namespaces.contains_key(&id_namespace),
            "boundary.id namespace should exist"
        );
        assert!(
            is_allowed_builtin_namespace_shadow(&db, &boundary, &id_namespace, id_def),
            "boundary.id root function shadowed by boundary.id namespace is the only allowed builtin collision"
        );
        assert!(
            package.shadows().is_empty(),
            "the allowlisted boundary.id collision should not emit namespace-shadow diagnostics"
        );
    }

    #[test]
    fn builtin_namespace_shadow_allowlist_rejects_other_builtin_collisions() {
        let db = TestDb::with_builtins();
        let boundary = baml_base::Name::new("boundary");
        let id = baml_base::Name::new("id");
        let package = package_items(&db, PackageId::new(&db, boundary.clone()));
        let root = package.namespaces.get(&Vec::new()).expect("root namespace");
        let id_def = root.values.get(&id).copied().expect("boundary.id function");

        assert!(!is_allowed_builtin_namespace_shadow(
            &db,
            &baml_base::Name::new("baml"),
            std::slice::from_ref(&id),
            id_def
        ));
        assert!(!is_allowed_builtin_namespace_shadow(
            &db,
            &boundary,
            &[baml_base::Name::new("other")],
            id_def
        ));
    }
}

/// Whether `name` is reserved against mounting (BEP-066 mounted-package linking): builtin
/// packages, `user`, `root`, and `env`. The complete list is single-sourced in
/// [`baml_builtins2::reserved_package_names`], shared with runtime reflection.
pub fn is_reserved_package_name(name: &str) -> bool {
    baml_builtins2::reserved_package_names().contains(&name)
}

/// The names of every mounted source-less dependency package (BEP-066
/// mounted-package linking): the keys of the [`baml_workspace::MountedPackages`]
/// input, minus any
/// reserved name ([`is_reserved_package_name`] — a blob may not shadow the
/// stdlib, `user`, `root`, or `env`). Deterministically ordered (`BTreeMap`
/// keys). Empty for databases that mount nothing.
///
/// Reading the input inside a tracked query records a dependency on the mount
/// map, so mounting/unmounting invalidates dependents for free.
pub fn mounted_package_names(db: &dyn crate::Db) -> Vec<Name> {
    let Some(mounted) = db.mounted_packages() else {
        return Vec::new();
    };
    mounted
        .by_package(db)
        .keys()
        .filter(|name| !is_reserved_package_name(name))
        .map(|name| Name::new(name.as_str()))
        .collect()
}

/// Compiler-built, source-less stdlib packages carried by the mounted-package
/// interface transport.
///
/// Reserved names are accepted only when both the immutable marker and the
/// blob are present, and only for names in the embedded stdlib manifest. A
/// caller therefore cannot use ordinary mounting to shadow a builtin package.
pub fn precompiled_package_names(db: &dyn crate::Db) -> Vec<Name> {
    let Some(mounted) = db.mounted_packages() else {
        return Vec::new();
    };
    mounted
        .immutable_precompiled(db)
        .iter()
        .filter(|name| {
            baml_builtins2::stdlib_package_names().contains(&name.as_str())
                && mounted.by_package(db).contains_key(name.as_str())
        })
        .map(|name| Name::new(name.as_str()))
        .collect()
}

/// Every source-less dependency visible to compiler2, regardless of whether
/// it is an ordinary mutable mount or a compiler-built immutable stdlib row.
pub fn external_package_names(db: &dyn crate::Db) -> Vec<Name> {
    let mut names = mounted_package_names(db);
    names.extend(precompiled_package_names(db));
    names.sort();
    names.dedup();
    names
}

/// Whether `name` is a mounted source-less dependency package (a
/// non-reserved key of the `MountedPackages` input).
pub fn is_mounted_package(db: &dyn crate::Db, name: &Name) -> bool {
    if is_reserved_package_name(name.as_str()) {
        return false;
    }
    db.mounted_packages()
        .is_some_and(|mounted| mounted.by_package(db).contains_key(name.as_str()))
}

/// Whether `name` is a compiler-built immutable stdlib dependency.
pub fn is_precompiled_package(db: &dyn crate::Db, name: &Name) -> bool {
    baml_builtins2::stdlib_package_names().contains(&name.as_str())
        && db.mounted_packages().is_some_and(|mounted| {
            mounted.immutable_precompiled(db).contains(name.as_str())
                && mounted.by_package(db).contains_key(name.as_str())
        })
}

/// Whether `name` is any source-less dependency served from a serialized
/// `PackageInterface`.
pub fn is_external_package(db: &dyn crate::Db, name: &Name) -> bool {
    is_mounted_package(db, name) || is_precompiled_package(db, name)
}

/// The *direct* dependencies of `package_id` (hardcoded for now).
///
/// Note these lists are not uniformly flattened: `testing`/`assert` list `baml`
/// but not `baml`'s own `log`. Callers that need every package whose
/// items could be visible from `package_id` (interface coherence,
/// `type_implements_with_deps`) must use [`package_dependency_closure`], not this
/// direct list.
#[salsa::tracked(returns(ref))]
pub fn package_dependencies<'db>(
    db: &'db dyn crate::Db,
    package_id: PackageId<'db>,
) -> Vec<PackageId<'db>> {
    match package_id.name(db).as_str() {
        // "log" has no deps — it only uses primitives, and "baml" depends on
        // it so the stdlib can emit log events.
        "log" => vec![],
        // "boundary" has no deps — it only returns the current boundary id as
        // a primitive string.
        "boundary" => vec![],
        // "baml" depends on "log" so stdlib code can call log.info/debug/etc.
        // Reflection lives inside "baml" itself (`baml.reflect` / `baml.type`,
        // BEP-066), so it is no dependency.
        "baml" => vec![PackageId::new(db, Name::new("log"))],
        // The "testing" and "assert" packages depend on "baml" only.
        "testing" | "assert" => vec![PackageId::new(db, Name::new("baml"))],
        // The "ai" package uses BAML primitives, runtime type reflection, and
        // prompt schema rendering, all of which now live in the "baml" package.
        "ai" => vec![PackageId::new(db, Name::new("baml"))],
        // Provider packages implement `ai.Client`; claude_code also logs its
        // own event stream.
        "openai" | "anthropic" | "google" | "claude_code" => vec![
            PackageId::new(db, Name::new("baml")),
            PackageId::new(db, Name::new("log")),
            PackageId::new(db, Name::new("ai")),
        ],
        // User packages depend on public builtin packages — plus every mounted
        // source-less package (BEP-066 mounted-package linking) and every auxiliary source
        // package installed through `compiler2_extra_files`. The latter makes
        // the source side of the source-vs-blob contract real: a package such
        // as `<builtin>/app/…` is the same direct dependency whether its source
        // or its mounted interface is present. A mounted/auxiliary package
        // itself keeps the stdlib list only, avoiding dependency cycles.
        name => {
            let mut deps = vec![
                PackageId::new(db, Name::new("baml")),
                PackageId::new(db, Name::new("boundary")),
                PackageId::new(db, Name::new("testing")),
                PackageId::new(db, Name::new("assert")),
                PackageId::new(db, Name::new("log")),
                PackageId::new(db, Name::new("ai")),
                PackageId::new(db, Name::new("openai")),
                PackageId::new(db, Name::new("anthropic")),
                PackageId::new(db, Name::new("google")),
                PackageId::new(db, Name::new("claude_code")),
            ];
            let mounted = mounted_package_names(db);
            if !mounted.iter().any(|m| m.as_str() == name) {
                deps.extend(
                    mounted
                        .into_iter()
                        .map(|mounted_name| PackageId::new(db, mounted_name)),
                );
            }
            if name == "user" {
                let source_packages: std::collections::BTreeSet<Name> =
                    crate::compiler2_all_files(db)
                        .into_iter()
                        .map(|file| crate::file_package::file_package(db, file).package)
                        .filter(|package| {
                            package.as_str() != name
                                && !is_reserved_package_name(package.as_str())
                                && !is_external_package(db, package)
                        })
                        .collect();
                deps.extend(
                    source_packages
                        .into_iter()
                        .map(|package| PackageId::new(db, package)),
                );
            }
            deps
        }
    }
}

/// The full transitive dependency closure of `package_id` (excluding itself), in
/// deterministic breadth-first order with duplicates removed.
///
/// Unlike [`package_dependencies`] (direct-only, not uniformly flattened), this
/// is what coherence and membership checks need: every package whose impls could
/// be visible from `package_id`, regardless of how flat the direct lists happen
/// to be. The walk is cycle-safe (a `seen` set), though the dependency graph is
/// currently a DAG.
#[salsa::tracked(returns(ref))]
pub fn package_dependency_closure<'db>(
    db: &'db dyn crate::Db,
    package_id: PackageId<'db>,
) -> Vec<PackageId<'db>> {
    let mut seen: std::collections::HashSet<PackageId<'db>> = std::collections::HashSet::new();
    let mut order: Vec<PackageId<'db>> = Vec::new();
    let mut queue: std::collections::VecDeque<PackageId<'db>> =
        package_dependencies(db, package_id)
            .iter()
            .copied()
            .collect();
    while let Some(dep) = queue.pop_front() {
        if dep == package_id || !seen.insert(dep) {
            continue;
        }
        order.push(dep);
        queue.extend(package_dependencies(db, dep).iter().copied());
    }
    order
}
