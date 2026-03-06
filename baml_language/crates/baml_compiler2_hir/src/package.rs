//! Package-level cross-file symbol aggregation.
//!
//! `package_items` merges all `namespace_items` within a package into a single
//! lookup structure. This is the top-level cross-file query used by the TIR
//! layer for name resolution.

use baml_base::Name;
use rustc_hash::FxHashMap;

use crate::{
    contributions::Definition,
    namespace::{NameConflict, NamespaceId, NamespaceItems, namespace_items},
};

/// Interned package identity.
#[salsa::interned]
pub struct PackageId<'db> {
    pub name: Name,
}

/// Rare/optional data for `PackageItems`. Heap-allocated only when
/// at least one conflict exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageItemsExtra<'db> {
    pub conflicts: Vec<NameConflict<'db>>,
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
unsafe impl<'db> salsa::Update for PackageItems<'db> {
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
    /// Look up a type by path segments (e.g., `["llm", "render_prompt"]`).
    ///
    /// Tries progressively longer namespace prefixes: for path `["a", "B"]`,
    /// first tries namespace `["a"]` + name `"B"`, then namespace `[]` + name `"a"`.
    pub fn lookup_type(&self, path: &[Name]) -> Option<Definition<'db>> {
        if path.is_empty() {
            return None;
        }
        for split in (0..path.len()).rev() {
            let ns_path = &path[..split];
            let item_name = &path[split];
            if let Some(ns) = self.namespaces.get(ns_path) {
                if let Some(def) = ns.types.get(item_name) {
                    return Some(*def);
                }
            }
        }
        None
    }

    /// Look up a value by path segments.
    pub fn lookup_value(&self, path: &[Name]) -> Option<Definition<'db>> {
        if path.is_empty() {
            return None;
        }
        for split in (0..path.len()).rev() {
            let ns_path = &path[..split];
            let item_name = &path[split];
            if let Some(ns) = self.namespaces.get(ns_path) {
                if let Some(def) = ns.values.get(item_name) {
                    return Some(*def);
                }
            }
        }
        None
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

    all_conflicts.sort_by(|a, b| a.name.cmp(&b.name));

    let extra = if all_conflicts.is_empty() {
        None
    } else {
        Some(Box::new(PackageItemsExtra {
            conflicts: all_conflicts,
        }))
    };

    PackageItems { namespaces, extra }
}
