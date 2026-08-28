//! `listing` — flat symbol listings for packages and namespaces, plus the
//! structural name resolution behind `baml describe`.

use std::collections::HashMap;

use baml_base::{Name, SourceFile};
use baml_compiler2_hir::{
    contributions::{Definition, DefinitionKind},
    package::{PackageId, PackageItems, package_items},
};
use baml_type::{BuiltinTypeName, Package};
use text_size::TextSize;

use crate::line_index::LineIndex;

// ── ResolvedTarget ────────────────────────────────────────────────────────────

/// A structurally-resolved target for `baml describe <name>`.
///
/// Constructed via `resolve_target()` (within-package resolution) or by the
/// CLI dispatcher (cross-package routing). Unrepresentable invalid states by
/// construction.
#[derive(Clone)]
pub enum ResolvedTarget<'db> {
    /// A whole package (e.g. the workspace package, `baml`, `testing`).
    /// Resolved when the input is a bare package name or empty (= the
    /// workspace package).
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
    db: &'db dyn baml_compiler2_ppir::Db,
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
    db: &'db dyn baml_compiler2_ppir::Db,
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
    /// Package name that owns this symbol — the workspace package's reserved
    /// name, or a dependency/builtin package name such as `baml`.
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
    /// - Workspace-package entries as package-local paths (for example
    ///   `"llm.Config"`).
    /// - Other packages' entries as package-qualified paths (for example
    ///   `"baml.iter.Range"`), so the emitted text can be passed back into
    ///   `baml describe` verbatim.
    pub fn fqn(&self) -> String {
        let local_path = if self.ns_path.is_empty() {
            self.item_name.as_str().to_string()
        } else {
            let parts: Vec<&str> = self.ns_path.iter().map(Name::as_str).collect();
            format!("{}.{}", parts.join("."), self.item_name.as_str())
        };

        if is_local_package_name(&self.package_name) {
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
pub fn list_package_items(
    db: &dyn baml_compiler2_ppir::Db,
    package_id: PackageId<'_>,
) -> Vec<ListingEntry> {
    let pkg = package_items(db, package_id);
    let package_name = package_id.name(db);
    collect_entries_from_package(db, pkg, &package_name)
}

/// Collect listing entries from a `PackageItems`, including all namespaces.
fn collect_entries_from_package(
    db: &dyn baml_compiler2_ppir::Db,
    pkg: &PackageItems<'_>,
    package_name: &Name,
) -> Vec<ListingEntry> {
    let mut entries = Vec::new();
    let mut line_indexes = HashMap::new();

    for (ns_path, ns_items) in &pkg.namespaces {
        // Collect from types (classes, enums, type aliases) and values
        // (functions, clients, generators, etc.).
        for (name, def) in ns_items.types.iter().chain(ns_items.values.iter()) {
            if let Some(entry) = make_entry(
                db,
                &mut line_indexes,
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
/// Returns `Some(entries)` sorted by `(file_path, line)` if found. Also
/// includes items from child namespaces (e.g., `baml describe baml`
/// includes `baml.env.GetEnv`).
pub fn list_namespace_items(
    db: &dyn baml_compiler2_ppir::Db,
    package_id: PackageId<'_>,
    namespace_path: &[Name],
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
    let mut line_indexes = HashMap::new();

    for (ns_path, ns_items) in &pkg.namespaces {
        // Include exact match and child namespaces.
        if ns_path.len() < namespace_path.len() {
            continue;
        }
        if !ns_path.starts_with(namespace_path) {
            continue;
        }

        for (name, def) in ns_items.types.iter().chain(ns_items.values.iter()) {
            if let Some(entry) = make_entry(
                db,
                &mut line_indexes,
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

/// Every package name known to the database except the workspace packages':
/// the packages of non-`Workspace` source roots (stdlib builtins plus
/// source-bearing dependency and dynamic roots) and the mounted source-less
/// packages, deduplicated and sorted.
///
/// The `baml describe` dispatcher uses this for cross-package routing: a
/// leading path segment naming one of these packages addresses that package
/// instead of a workspace item.
pub fn non_workspace_package_names(db: &dyn baml_compiler2_ppir::Db) -> Vec<Name> {
    let mut names: Vec<Name> = db
        .source_roots()
        .roots(db)
        .iter()
        .filter(|root| match root.kind(db) {
            baml_base::SourceRootKind::Stdlib
            | baml_base::SourceRootKind::Dependency
            | baml_base::SourceRootKind::Dynamic => true,
            baml_base::SourceRootKind::Workspace => false,
        })
        .map(|root| root.package(db))
        .collect();
    names.extend(baml_compiler2_hir::package::external_package_names(db));
    names.sort();
    names.dedup();
    names
}

/// Return whether `package_name` identifies the implicit local (workspace)
/// package.
///
/// This funnels package-locality checks through the typed `Package`
/// classifier instead of comparing raw package-name string literals at call
/// sites.
fn is_local_package_name(package_name: &Name) -> bool {
    matches!(Package::from_name(package_name.clone()), Package::Local)
}

/// Build a single `ListingEntry` from a definition.
fn make_entry<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    line_indexes: &mut HashMap<SourceFile, LineIndex<'db>>,
    package_name: Name,
    ns_path: Vec<Name>,
    item_name: Name,
    def: Definition<'db>,
) -> Option<ListingEntry> {
    if def.is_language_internal(db) {
        return None;
    }
    let (file, name_span) = crate::syntax::definition_span(db, def)?;
    let file_path = file.path(db).display().to_string();
    let line = entry_line(db, line_indexes, file, name_span.start());

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

/// One-based line number of `offset` within `file`.
///
/// Line indexes are built once per file in `line_indexes` and shared across
/// entries, so a listing never rescans a file's text per entry.
fn entry_line<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    line_indexes: &mut HashMap<SourceFile, LineIndex<'db>>,
    file: SourceFile,
    offset: TextSize,
) -> usize {
    let index = line_indexes
        .entry(file)
        .or_insert_with(|| LineIndex::new(file.text(db)));
    let (line, _column) = index
        .offset_to_position(offset.into())
        .unwrap_or_else(|| unreachable!("contribution name spans lie inside their file's text"));
    usize::try_from(line).unwrap_or_else(|_| unreachable!("a u32 line number fits in usize")) + 1
}

#[cfg(test)]
mod tests {
    use baml_compiler2_hir::package::sole_workspace_package;

    use super::*;
    use crate::test_support::ProjectTest;

    // ── Feature-specific helpers ─────────────────────────────────────────────

    /// Run `list_package_items()` for the fixture's workspace package.
    fn list_package_items_user(project: &ProjectTest) -> Vec<ListingEntry> {
        let package_id = sole_workspace_package(&project.db);
        list_package_items(&project.db, package_id)
    }

    /// Run `list_namespace_items()` for a workspace-package namespace.
    fn list_namespace_items_user(
        project: &ProjectTest,
        ns_segments: &[&str],
    ) -> Option<Vec<ListingEntry>> {
        let package_id = sole_workspace_package(&project.db);
        let ns_path: Vec<Name> = ns_segments.iter().map(Name::new).collect();
        list_namespace_items(&project.db, package_id, &ns_path)
    }

    /// Format a `ListingEntry` for snapshot comparison.
    fn format_listing_entry(project: &ProjectTest, entry: &ListingEntry) -> String {
        let filename = entry
            .file
            .path(&project.db)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        format!(
            "{:<16} {:<32} {filename}:{}",
            entry.kind.as_str(),
            entry.fqn(),
            entry.line,
        )
    }

    // ── Fixtures ─────────────────────────────────────────────────────────────

    fn make_multi_ns_project() -> ProjectTest {
        let mut builder = ProjectTest::builder();
        builder.source(
            "types.baml",
            r#"
class Point {
    x int
    y int
}
"#,
        );
        builder.source(
            "ns_llm/models.baml",
            r#"
class Config {
    model string
    temperature float
}

function llm_identity(input: string) -> string {
    return input;
}
"#,
        );
        builder.source(
            "ns_lorem/types.baml",
            r#"
class Resume {
    name string
}
"#,
        );
        builder.build()
    }

    fn make_deep_ns_project() -> ProjectTest {
        let mut builder = ProjectTest::builder();
        builder.source(
            "ns_foo/ns_bar/types.baml",
            r#"
class Baz {
    field int
}
"#,
        );
        builder.build()
    }

    // ── Package listing ──────────────────────────────────────────────────────

    #[test]
    fn list_package_items_multi_namespace() {
        let project = make_multi_ns_project();
        let entries = list_package_items_user(&project);

        // Should include items from all namespaces.
        assert!(!entries.is_empty());

        // Build snapshot.
        let listing: String = entries
            .iter()
            .map(|e| format_listing_entry(&project, e))
            .collect::<Vec<_>>()
            .join("\n");
        insta::assert_snapshot!(listing);
    }

    #[test]
    fn list_package_items_sorted_by_ns_then_file_then_line() {
        let project = make_multi_ns_project();
        let entries = list_package_items_user(&project);

        // Verify sorted by (ns_path, file_path, line): root namespace first,
        // then namespaces alphabetically, then by file, then by span.
        let fqns: Vec<String> = entries.iter().map(ListingEntry::fqn).collect();
        // Root items come before namespaced items.
        let root_end = fqns
            .iter()
            .position(|f| f.contains('.'))
            .unwrap_or(fqns.len());
        for fqn in &fqns[..root_end] {
            assert!(!fqn.contains('.'), "expected root item, got {fqn}");
        }
        for fqn in &fqns[root_end..] {
            assert!(fqn.contains('.'), "expected namespaced item, got {fqn}");
        }
    }

    #[test]
    fn list_package_items_fqns_include_namespace() {
        let project = make_multi_ns_project();
        let entries = list_package_items_user(&project);

        // Root namespace items have bare names.
        assert!(entries.iter().any(|e| e.fqn() == "Point"));

        // Namespaced items have qualified names.
        assert!(entries.iter().any(|e| e.fqn() == "llm.Config"));
        assert!(entries.iter().any(|e| e.fqn() == "llm.llm_identity"));
        assert!(entries.iter().any(|e| e.fqn() == "lorem.Resume"));
    }

    /// Builtin package listings include the package segment so names can round-trip
    /// through `baml describe` without callers needing to guess the package.
    #[test]
    fn list_package_items_builtin_fqns_include_package_name() {
        let project = make_multi_ns_project();
        let pkg_id = PackageId::new(&project.db, Name::new("baml"));
        let entries = list_package_items(&project.db, pkg_id);

        assert!(
            entries.iter().any(|e| e.fqn() == "baml.iter.Range"),
            "expected builtin listing to include package-qualified names"
        );
        assert!(
            !entries.iter().any(|e| e.fqn() == "iter.Range"),
            "builtin listing must not emit unqualified names"
        );
    }

    #[test]
    fn language_internal_functions_are_hidden_from_listing() {
        let mut builder = ProjectTest::builder();
        builder.source(
            "tests.baml",
            r#"
function identity(value: string) -> string {
    value
}

test "identity" {
    assert.equal(identity("ok"), "ok")
}
"#,
        );
        let project = builder.build();
        let pkg_id = sole_workspace_package(&project.db);
        let pkg = package_items(&project.db, pkg_id);
        let (internal_name, internal_def) = pkg
            .namespaces
            .values()
            .flat_map(|namespace| namespace.values.iter())
            .find(|(name, _)| name.as_str().starts_with("$init_test"))
            .expect("test lowering should synthesize an init function");

        assert!(internal_def.is_language_internal(&project.db));
        assert!(
            list_package_items(&project.db, pkg_id)
                .iter()
                .all(|entry| entry.item_name.as_str() != internal_name.as_str())
        );
        assert!(resolve_target(&project.db, pkg_id, internal_name.as_str()).is_none());
    }

    /// An LLM function is the sole listed function declaration; operation
    /// projections are metadata and PPIR partial types remain hidden.
    #[test]
    fn llm_function_listing_has_no_operation_companions() {
        let mut builder = ProjectTest::builder();
        builder.source(
            "functions.baml",
            r##"
function summarize(input: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `summarize ${input}`
}
"##,
        );
        let project = builder.build();
        let pkg_id = sole_workspace_package(&project.db);
        let entries = list_package_items(&project.db, pkg_id);

        assert!(
            entries
                .iter()
                .any(|entry| entry.item_name.as_str() == "summarize")
        );
        for name in [
            "summarize$spec",
            "summarize$render_prompt",
            "summarize$parse",
            "summarize$stream",
            "summarize@stream",
        ] {
            assert!(
                entries.iter().all(|entry| entry.item_name.as_str() != name),
                "unexpected synthetic function `{name}` in describe listing; got {:?}",
                entries
                    .iter()
                    .map(|entry| entry.item_name.as_str())
                    .collect::<Vec<_>>()
            );
            assert!(resolve_target(&project.db, pkg_id, name).is_none());
        }
    }

    // ── Namespace listing ────────────────────────────────────────────────────

    #[test]
    fn list_namespace_items_llm() {
        let project = make_multi_ns_project();
        let entries = list_namespace_items_user(&project, &["llm"]);
        assert!(entries.is_some());
        let entries = entries.unwrap();

        // Should only contain llm namespace items.
        assert!(entries.iter().all(|e| e.fqn().starts_with("llm.")));
        assert!(entries.iter().any(|e| e.fqn() == "llm.Config"));
        assert!(entries.iter().any(|e| e.fqn() == "llm.llm_identity"));

        let listing: String = entries
            .iter()
            .map(|e| format_listing_entry(&project, e))
            .collect::<Vec<_>>()
            .join("\n");
        insta::assert_snapshot!(listing);
    }

    #[test]
    fn list_namespace_items_nonexistent() {
        let project = make_multi_ns_project();
        let entries = list_namespace_items_user(&project, &["nonexistent"]);
        assert!(entries.is_none());
    }

    #[test]
    fn list_namespace_items_lorem() {
        let project = make_multi_ns_project();
        let entries = list_namespace_items_user(&project, &["lorem"]);
        assert!(entries.is_some());
        let entries = entries.unwrap();
        assert!(entries.iter().any(|e| e.fqn() == "lorem.Resume"));
    }

    // ── Round-trip property tests ────────────────────────────────────────────

    /// Critical invariant: every FQN emitted by listing must resolve back to its definition.
    /// This prevents the class of bug where listings show paths that don't navigate.
    #[test]
    fn round_trip_listing_to_resolve() {
        let project = make_multi_ns_project();
        let pkg_id = sole_workspace_package(&project.db);
        let entries = list_package_items(&project.db, pkg_id);

        for entry in &entries {
            let fqn = entry.fqn();
            let resolved = resolve_target(&project.db, pkg_id, &fqn);
            assert!(
                matches!(resolved, Some(ResolvedTarget::Item(_))),
                "FQN `{fqn}` was listed but does not resolve as Item; got {:?}",
                resolved.as_ref().map(std::mem::discriminant),
            );
        }
    }

    /// Same round-trip property on a project with a 2-deep namespace.
    #[test]
    fn round_trip_listing_to_resolve_deep_ns() {
        let project = make_deep_ns_project();
        let pkg_id = sole_workspace_package(&project.db);
        let entries = list_package_items(&project.db, pkg_id);

        assert!(
            !entries.is_empty(),
            "expected at least one entry in deep_ns project"
        );

        for entry in &entries {
            let fqn = entry.fqn();
            let resolved = resolve_target(&project.db, pkg_id, &fqn);
            assert!(
                matches!(resolved, Some(ResolvedTarget::Item(_))),
                "FQN `{fqn}` was listed but does not resolve as Item; got {:?}",
                resolved.as_ref().map(std::mem::discriminant),
            );
        }
    }

    /// For every namespace in the package, resolve its dotted form and get back Namespace.
    #[test]
    fn round_trip_namespace() {
        let project = make_multi_ns_project();
        let pkg_id = sole_workspace_package(&project.db);
        let pkg = package_items(&project.db, pkg_id);

        for ns_path in pkg.namespaces.keys() {
            if ns_path.is_empty() {
                continue; // root namespace — not a Namespace target
            }
            let dotted: String = ns_path
                .iter()
                .map(Name::as_str)
                .collect::<Vec<_>>()
                .join(".");
            let resolved = resolve_target(&project.db, pkg_id, &dotted);
            assert!(
                matches!(resolved, Some(ResolvedTarget::Namespace { .. })),
                "namespace path `{dotted}` should resolve as Namespace; got {:?}",
                resolved.as_ref().map(std::mem::discriminant),
            );
        }
    }

    /// Same namespace round-trip on a project with a 2-deep namespace.
    #[test]
    fn round_trip_namespace_deep() {
        let project = make_deep_ns_project();
        let pkg_id = sole_workspace_package(&project.db);
        let pkg = package_items(&project.db, pkg_id);

        let mut checked = 0;
        for ns_path in pkg.namespaces.keys() {
            if ns_path.is_empty() {
                continue;
            }
            let dotted: String = ns_path
                .iter()
                .map(Name::as_str)
                .collect::<Vec<_>>()
                .join(".");
            let resolved = resolve_target(&project.db, pkg_id, &dotted);
            assert!(
                matches!(resolved, Some(ResolvedTarget::Namespace { .. })),
                "namespace path `{dotted}` should resolve as Namespace; got {:?}",
                resolved.as_ref().map(std::mem::discriminant),
            );
            checked += 1;
        }
        assert!(
            checked > 0,
            "expected at least one non-root namespace in deep_ns project"
        );
    }

    /// For every item, walk its file outline children and verify member round-trip.
    #[test]
    fn round_trip_member() {
        let project = make_multi_ns_project();
        let pkg_id = sole_workspace_package(&project.db);
        let entries = list_package_items(&project.db, pkg_id);

        let mut checked = 0;

        for entry in &entries {
            let item_fqn = entry.fqn();

            // Look up the item's definition.
            let def = {
                let pkg = package_items(&project.db, pkg_id);
                pkg.lookup_type(&entry.ns_path, &entry.item_name)
                    .or_else(|| pkg.lookup_value(&entry.ns_path, &entry.item_name))
            };
            let Some(def) = def else { continue };

            // Find item in outline and walk its children.
            let Some((item_file, item_name_span)) =
                crate::syntax::definition_span(&project.db, def)
            else {
                continue;
            };
            let outline = crate::outline::file_outline(&project.db, item_file);
            let item_name_text = {
                let text = item_file.text(&project.db);
                text[item_name_span].to_string()
            };

            for outline_item in outline {
                if outline_item.name != item_name_text {
                    continue;
                }
                for child in &outline_item.children {
                    let member_path = format!("{item_fqn}.{}", child.name);
                    let resolved = resolve_target(&project.db, pkg_id, &member_path);
                    assert!(
                        matches!(resolved, Some(ResolvedTarget::Member { .. })),
                        "member `{member_path}` should resolve as Member; got {:?}",
                        resolved.as_ref().map(std::mem::discriminant),
                    );
                    checked += 1;
                }
            }
        }

        assert!(checked > 0, "expected at least one member to be checked");
    }

    // ── Package-name enumeration ─────────────────────────────────────────────

    #[test]
    fn non_workspace_package_names_excludes_workspace_and_is_sorted() {
        let project = make_multi_ns_project();
        let names = non_workspace_package_names(&project.db);
        let workspace = sole_workspace_package(&project.db).name(&project.db);

        assert!(
            names.iter().all(|name| *name != workspace),
            "workspace package must not be listed; got {names:?}"
        );
        assert!(
            names.iter().any(|name| name.as_str() == "baml"),
            "stdlib packages should be listed; got {names:?}"
        );
        assert!(names.is_sorted(), "names should be sorted; got {names:?}");
        assert!(
            names.windows(2).all(|pair| pair[0] != pair[1]),
            "names should be deduplicated; got {names:?}"
        );
    }
}
