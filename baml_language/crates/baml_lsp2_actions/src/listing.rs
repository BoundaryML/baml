//! `listing` — flat symbol listings for packages and namespaces.

use baml_base::SourceFile;
use baml_compiler2_hir::{
    contributions::{Definition, DefinitionKind},
    package::{PackageId, PackageItems, package_items},
};

use crate::Db;

/// A single entry in a flat symbol listing.
#[derive(Clone)]
pub struct ListingEntry {
    /// Symbol kind (class, function, enum, etc.).
    pub kind: DefinitionKind,
    /// Fully-qualified name with namespace prefix (e.g. "llm.Config" or "`ExtractResume`").
    pub fqn: String,
    /// The source file where this symbol is defined.
    pub file: SourceFile,
    /// Relative file path string for display.
    pub file_path: String,
    /// 1-based line number.
    pub line: usize,
}

/// List all items in a package as a flat listing sorted by (`file_path`, line).
///
/// Iterates `package_items(db, package_id).namespaces` and chains
/// `types` and `values` from each `NamespaceItems`. The FQN is constructed
/// as `ns_path.join(".") + "." + item_name` (or bare `item_name` for root namespace).
pub fn list_package_items(db: &dyn Db, package_id: PackageId<'_>) -> Vec<ListingEntry> {
    let pkg = package_items(db, package_id);
    collect_entries_from_package(db, pkg)
}

/// Collect listing entries from a `PackageItems`, including all namespaces.
fn collect_entries_from_package(db: &dyn Db, pkg: &PackageItems<'_>) -> Vec<ListingEntry> {
    let mut entries = Vec::new();

    for (ns_path, ns_items) in &pkg.namespaces {
        let ns_prefix = if ns_path.is_empty() {
            String::new()
        } else {
            let parts: Vec<&str> = ns_path.iter().map(baml_base::Name::as_str).collect();
            format!("{}.", parts.join("."))
        };

        // Collect from types (classes, enums, type aliases).
        for (name, def) in &ns_items.types {
            if let Some(entry) = make_entry(db, &ns_prefix, name.as_str(), *def) {
                entries.push(entry);
            }
        }

        // Collect from values (functions, clients, generators, etc.).
        for (name, def) in &ns_items.values {
            if let Some(entry) = make_entry(db, &ns_prefix, name.as_str(), *def) {
                entries.push(entry);
            }
        }
    }

    // Sort by (file_path, line) for deterministic, natural reading order.
    entries.sort_by(|a, b| a.file_path.cmp(&b.file_path).then(a.line.cmp(&b.line)));
    entries
}

/// List all items in a specific namespace within a package.
///
/// Returns `None` if the namespace path doesn't exist in `PackageItems::namespaces`.
/// Returns `Some(entries)` sorted by `(file_path, line)` if found.
/// Also includes items from child namespaces (e.g., `baml describe baml`
/// includes `baml.env.GetEnv`).
pub fn list_namespace_items(
    db: &dyn Db,
    package_id: PackageId<'_>,
    namespace_path: &[baml_base::Name],
) -> Option<Vec<ListingEntry>> {
    let pkg = package_items(db, package_id);

    // Check that the requested namespace path exists or has children.
    let has_exact = pkg.namespaces.contains_key(namespace_path);
    let has_children = pkg
        .namespaces
        .keys()
        .any(|k| k.len() > namespace_path.len() && k.starts_with(namespace_path));

    if !has_exact && !has_children {
        return None;
    }

    let mut entries = Vec::new();

    for (ns_path, ns_items) in &pkg.namespaces {
        // Include exact match and child namespaces.
        if ns_path.len() < namespace_path.len() {
            continue;
        }
        if !ns_path.starts_with(namespace_path) {
            continue;
        }

        let ns_prefix = if ns_path.is_empty() {
            String::new()
        } else {
            let parts: Vec<&str> = ns_path.iter().map(baml_base::Name::as_str).collect();
            format!("{}.", parts.join("."))
        };

        for (name, def) in &ns_items.types {
            if let Some(entry) = make_entry(db, &ns_prefix, name.as_str(), *def) {
                entries.push(entry);
            }
        }
        for (name, def) in &ns_items.values {
            if let Some(entry) = make_entry(db, &ns_prefix, name.as_str(), *def) {
                entries.push(entry);
            }
        }
    }

    entries.sort_by(|a, b| a.file_path.cmp(&b.file_path).then(a.line.cmp(&b.line)));
    Some(entries)
}

/// Collect all known non-user package names from the database.
///
/// Scans `compiler2_all_files(db)` via `file_package(db, file)` and collects
/// unique package names, filtering out `"user"`. Pattern established in
/// `check.rs` and `ppir/lib.rs`.
pub fn non_user_package_names(db: &dyn Db) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    for file in baml_compiler2_hir::compiler2_all_files(db) {
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
        let pkg_name = pkg_info.package.as_str().to_string();
        if pkg_name != "user" {
            names.insert(pkg_name);
        }
    }
    names
}

/// Build a single `ListingEntry` from a definition.
fn make_entry(
    db: &dyn Db,
    ns_prefix: &str,
    name: &str,
    def: Definition<'_>,
) -> Option<ListingEntry> {
    let (file, name_span) = crate::utils::definition_span(db, def)?;
    let file_path = file.path(db).display().to_string();
    let text = file.text(db);
    let offset: usize = name_span.start().into();
    let line = text[..offset.min(text.len())]
        .chars()
        .filter(|&c| c == '\n')
        .count()
        + 1;

    Some(ListingEntry {
        kind: def.kind(),
        fqn: format!("{ns_prefix}{name}"),
        file,
        file_path,
        line,
    })
}
