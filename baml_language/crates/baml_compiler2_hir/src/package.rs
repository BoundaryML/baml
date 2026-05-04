//! Package-level cross-file symbol aggregation.
//!
//! `package_items` merges all `namespace_items` within a package into a single
//! lookup structure. This is the top-level cross-file query used by the TIR
//! layer for name resolution.

use baml_base::{Name, Span};
use baml_compiler_diagnostics::diagnostic::{Diagnostic, DiagnosticId, DiagnosticPhase};
use rustc_hash::FxHashMap;

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
            "Namespace `{}` (from `ns_{}/`) shadows root-level {} `{}`",
            self.ns_name,
            self.ns_name,
            def.kind_name(),
            self.ns_name
        );

        let mut diag = Diagnostic::warning(DiagnosticId::NamespaceShadow, message);

        if let Some(range) = name_span {
            diag = diag.with_primary(
                Span { file_id, range },
                format!(
                    "this {} is shadowed by namespace `{}`",
                    def.kind_name(),
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
    let mut ns_paths: std::collections::HashSet<Vec<Name>> = std::collections::HashSet::new();
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

    PackageItems { namespaces, extra }
}

/// Declares the packages that `package_id` depends on.
/// Currently hardcoded: "user" depends on "baml", everything else depends on nothing.
#[salsa::tracked(returns(ref))]
pub fn package_dependencies<'db>(
    db: &'db dyn crate::Db,
    package_id: PackageId<'db>,
) -> Vec<PackageId<'db>> {
    match package_id.name(db).as_str() {
        // "log" has no deps — it only uses primitives, and "baml" depends on
        // it so the stdlib can emit log events.
        "log" => vec![],
        // "reflect" has no deps — it only uses the `type` primitive.
        "reflect" => vec![],
        // "baml" depends on "log" and "reflect" so stdlib code can call
        // log.info/debug/etc. and reflect.type_of<T>() inside ns_llm.
        "baml" => vec![
            PackageId::new(db, Name::new("log")),
            PackageId::new(db, Name::new("reflect")),
        ],
        // The "testing" and "assert" packages depend on "baml" only.
        "testing" | "assert" => vec![PackageId::new(db, Name::new("baml"))],
        // User packages depend on "baml", "testing", "assert", "log", and "reflect".
        _ => vec![
            PackageId::new(db, Name::new("baml")),
            PackageId::new(db, Name::new("testing")),
            PackageId::new(db, Name::new("assert")),
            PackageId::new(db, Name::new("log")),
            PackageId::new(db, Name::new("reflect")),
        ],
    }
}
