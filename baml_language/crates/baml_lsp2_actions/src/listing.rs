//! `listing` — flat symbol listings for packages and namespaces.

use baml_base::{Name, SourceFile};
use baml_compiler2_hir::{
    contributions::{Definition, DefinitionKind},
    package::{PackageId, PackageItems, package_items},
};
use baml_type::{BuiltinTypeName, Package};

use crate::Db;

// ── ResolvedTarget ────────────────────────────────────────────────────────────

/// A structurally-resolved target for `baml describe <name>`.
///
/// Constructed via `resolve_target()` (within-package resolution) or by the
/// CLI dispatcher (cross-package routing). Unrepresentable invalid states by
/// construction.
#[derive(Clone)]
pub enum ResolvedTarget<'db> {
    /// A whole package (e.g. `user`, `baml`, `testing`).
    /// Resolved when the input is a bare package name or empty (= user package).
    Package(PackageId<'db>),
    /// A namespace within a package. `ns_path` is non-empty by construction.
    Namespace {
        package: PackageId<'db>,
        ns_path: Vec<Name>,
    },
    /// A specific item (class, enum, function, etc.).
    Item(Definition<'db>),
    /// A named member (field, variant) of an item.
    Member {
        parent: Definition<'db>,
        member_name: Name,
    },
    /// A BAML or crosswalk keyword (e.g. `"class"`, `"interface"`).
    Keyword(String),
}

impl std::fmt::Debug for ResolvedTarget<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolvedTarget::Package(_) => write!(f, "Package(..)"),
            ResolvedTarget::Namespace { ns_path, .. } => {
                write!(f, "Namespace({ns_path:?})")
            }
            ResolvedTarget::Item(_) => write!(f, "Item(..)"),
            ResolvedTarget::Member { member_name, .. } => {
                write!(f, "Member({member_name:?})")
            }
            ResolvedTarget::Keyword(kw) => write!(f, "Keyword({kw:?})"),
        }
    }
}

/// Resolve a name within a single package to a `ResolvedTarget`.
///
/// This function only handles within-package paths: namespaces, items, members.
/// The package-name case (`baml describe baml`) is handled by the CLI dispatcher.
///
/// # Resolution preference
///
/// For input `s1.s2.…sN`, tries interpretations in order (longest prefix first):
/// 1. `[s1..sN]` is a namespace → `Namespace`
/// 2. `[s1..sN-1]` is a namespace, `sN` is an item → `Item`
/// 3. `[s1..sN-2]` is a namespace, `sN-1` is an item, `sN` is a member → `Member`
/// 4. Returns `None` if no structural interpretation matches.
///
/// For empty input (`n == 0`), returns `None` — the dispatcher handles the
/// project-level case.
pub fn resolve_target<'db>(
    db: &'db dyn Db,
    package: PackageId<'db>,
    name: &str,
) -> Option<ResolvedTarget<'db>> {
    if name.is_empty() {
        return None;
    }

    let segments: Vec<&str> = name.split('.').collect();

    // Reject any segment that is empty (e.g., trailing dots).
    if segments.iter().any(|s| s.is_empty()) {
        return None;
    }

    let n = segments.len();
    let pkg = package_items(db, package);

    // Try k = n: entire path is a namespace.
    if n >= 1 {
        let ns_path: Vec<Name> = segments.iter().map(Name::new).collect();
        if is_valid_namespace(pkg, &ns_path) {
            return Some(ResolvedTarget::Namespace { package, ns_path });
        }
    }

    // Try k = n-1: first n-1 segments are namespace, last is item name.
    if n >= 1 {
        let ns_path: Vec<Name> = segments[..n - 1].iter().map(Name::new).collect();
        let item_name = Name::new(segments[n - 1]);
        let def = pkg
            .lookup_type(&ns_path, &item_name)
            .or_else(|| pkg.lookup_value(&ns_path, &item_name));
        if let Some(def) = def.filter(|def| !def.is_language_internal(db)) {
            return Some(ResolvedTarget::Item(def));
        }
    }

    // Try k = n-2: first n-2 segments are namespace, n-1 is item, last is member.
    if n >= 2 {
        let ns_path: Vec<Name> = segments[..n - 2].iter().map(Name::new).collect();
        let item_name = Name::new(segments[n - 2]);
        let member_name = Name::new(segments[n - 1]);
        let def = pkg
            .lookup_type(&ns_path, &item_name)
            .or_else(|| pkg.lookup_value(&ns_path, &item_name));
        if let Some(def) = def.filter(|def| !def.is_language_internal(db)) {
            return Some(ResolvedTarget::Member {
                parent: def,
                member_name,
            });
        }
    }

    None
}

/// Resolve a lowercase builtin type spelling, optionally followed by a member,
/// to its definition in the `baml` package.
///
/// Intrinsic types (`void`, `never`, `unknown`) intentionally return `None`
/// because they have language-reference topics rather than addressable stdlib
/// definitions.
pub fn resolve_builtin_type_target<'db>(
    db: &'db dyn Db,
    name: &str,
) -> Option<ResolvedTarget<'db>> {
    let (alias, member_path) = name.split_once('.').unwrap_or((name, ""));
    let builtin = BuiltinTypeName::from_alias(alias)?;
    let definition_path = builtin.builtin_definition_path()?;
    let mut target = definition_path.join(".");
    if !member_path.is_empty() {
        target.push('.');
        target.push_str(member_path);
    }

    let package = PackageId::new(db, Name::new(baml_base::BAML_PACKAGE));
    resolve_target(db, package, &target)
}

/// Check if the given namespace path exists in the package (either as an exact
/// entry or as a parent of some child namespace).
fn is_valid_namespace(pkg: &PackageItems<'_>, ns_path: &[Name]) -> bool {
    if ns_path.is_empty() {
        return false; // root namespace is not returned as a Namespace target
    }
    let has_exact = pkg.namespaces.contains_key(ns_path);
    let has_children = pkg
        .namespaces
        .keys()
        .any(|k| k.len() > ns_path.len() && k.starts_with(ns_path));
    has_exact || has_children
}

/// A single entry in a flat symbol listing.
#[derive(Clone)]
pub struct ListingEntry {
    /// Package name that owns this symbol (for example `"user"` or `"baml"`).
    pub package_name: Name,
    /// Symbol kind (class, function, enum, etc.).
    pub kind: DefinitionKind,
    /// Namespace path components (e.g. `["llm"]` or `["foo", "bar"]`; empty = root namespace).
    pub ns_path: Vec<Name>,
    /// Unqualified item name (e.g. `"Config"` or `"ExtractResume"`).
    pub item_name: Name,
    /// The source file where this symbol is defined.
    pub file: SourceFile,
    /// Relative file path string for display.
    pub file_path: String,
    /// 1-based line number.
    pub line: usize,
}

impl ListingEntry {
    /// Build the copy/paste-safe symbol path shown by `baml describe`.
    ///
    /// # Returns
    ///
    /// - User-package entries as user-local paths (for example `"llm.Config"`).
    /// - Non-user package entries as package-qualified paths (for example
    ///   `"baml.iter.Range"`), so the emitted text can be passed back into
    ///   `baml describe` verbatim.
    pub fn fqn(&self) -> String {
        let local_path = if self.ns_path.is_empty() {
            self.item_name.as_str().to_string()
        } else {
            let parts: Vec<&str> = self.ns_path.iter().map(Name::as_str).collect();
            format!("{}.{}", parts.join("."), self.item_name.as_str())
        };

        if is_user_package_name(&self.package_name) {
            local_path
        } else {
            format!("{}.{}", self.package_name.as_str(), local_path)
        }
    }
}

/// List all items in a package as a flat listing sorted by (namespace, `file_path`, line).
///
/// Iterates `package_items(db, package_id).namespaces` and chains
/// `types` and `values` from each `NamespaceItems`. The FQN is constructed
/// as `ns_path.join(".") + "." + item_name` (or bare `item_name` for root namespace).
pub fn list_package_items(db: &dyn Db, package_id: PackageId<'_>) -> Vec<ListingEntry> {
    let pkg = package_items(db, package_id);
    let package_name = package_id.name(db);
    collect_entries_from_package(db, pkg, &package_name)
}

/// Collect listing entries from a `PackageItems`, including all namespaces.
fn collect_entries_from_package(
    db: &dyn Db,
    pkg: &PackageItems<'_>,
    package_name: &Name,
) -> Vec<ListingEntry> {
    let mut entries = Vec::new();

    for (ns_path, ns_items) in &pkg.namespaces {
        // Collect from types (classes, enums, type aliases).
        for (name, def) in &ns_items.types {
            if let Some(entry) = make_entry(
                db,
                package_name.clone(),
                ns_path.clone(),
                name.clone(),
                *def,
            ) {
                entries.push(entry);
            }
        }

        // Collect from values (functions, clients, generators, etc.).
        for (name, def) in &ns_items.values {
            if let Some(entry) = make_entry(
                db,
                package_name.clone(),
                ns_path.clone(),
                name.clone(),
                *def,
            ) {
                entries.push(entry);
            }
        }
    }

    // Sort by (ns_path, file_path, line): root namespace first, then
    // namespaces alphabetically, then by file path, then by source position.
    entries.sort_by(|a, b| {
        // Compare ns_path lexicographically by joining segments.
        let a_ns: Vec<&str> = a.ns_path.iter().map(Name::as_str).collect();
        let b_ns: Vec<&str> = b.ns_path.iter().map(Name::as_str).collect();
        a_ns.cmp(&b_ns)
            .then(a.file_path.cmp(&b.file_path))
            .then(a.line.cmp(&b.line))
    });
    entries
}

/// List all items in a specific namespace within a package.
///
/// Returns `None` if the namespace path doesn't exist in `PackageItems::namespaces`.
/// Returns `Some(entries)` sorted by `(namespace, file_path, line)` if found.
/// Also includes items from child namespaces (e.g., `baml describe baml`
/// includes `baml.env.GetEnv`).
pub fn list_namespace_items(
    db: &dyn Db,
    package_id: PackageId<'_>,
    namespace_path: &[baml_base::Name],
) -> Option<Vec<ListingEntry>> {
    let pkg = package_items(db, package_id);
    let package_name = package_id.name(db);

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

        for (name, def) in &ns_items.types {
            if let Some(entry) = make_entry(
                db,
                package_name.clone(),
                ns_path.clone(),
                name.clone(),
                *def,
            ) {
                entries.push(entry);
            }
        }
        for (name, def) in &ns_items.values {
            if let Some(entry) = make_entry(
                db,
                package_name.clone(),
                ns_path.clone(),
                name.clone(),
                *def,
            ) {
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
        if !is_user_package_name(&pkg_info.package) {
            names.insert(pkg_info.package.as_str().to_string());
        }
    }
    names
}

/// Return whether `package_name` identifies the implicit user package.
///
/// This funnels package-locality checks through the typed `Package` classifier
/// instead of comparing raw `"user"` string literals at call sites.
fn is_user_package_name(package_name: &Name) -> bool {
    matches!(Package::from_name(package_name.clone()), Package::Local)
}

/// Build a single `ListingEntry` from a definition.
fn make_entry(
    db: &dyn Db,
    package_name: Name,
    ns_path: Vec<Name>,
    item_name: Name,
    def: Definition<'_>,
) -> Option<ListingEntry> {
    if def.is_language_internal(db) {
        return None;
    }
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
        package_name,
        kind: def.kind(),
        ns_path,
        item_name,
        file,
        file_path,
        line,
    })
}
