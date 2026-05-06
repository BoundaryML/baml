//! Tests for `listing` module.

use crate::{listing::ResolvedTarget, testing::ProjectTest};

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

function LlmIdentity(input: string) -> string {
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

#[test]
fn list_package_items_multi_namespace() {
    let project = make_multi_ns_project();
    let entries = project.list_package_items_user();

    // Should include items from all namespaces.
    assert!(!entries.is_empty());

    // Build snapshot.
    let listing: String = entries
        .iter()
        .map(|e| project.format_listing_entry(e))
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(listing);
}

#[test]
fn list_package_items_sorted_by_ns_then_file_then_line() {
    let project = make_multi_ns_project();
    let entries = project.list_package_items_user();

    // Verify sorted by (ns_path, file_path, line): root namespace first,
    // then namespaces alphabetically, then by file, then by span.
    let fqns: Vec<String> = entries
        .iter()
        .map(super::listing::ListingEntry::fqn)
        .collect();
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
    let entries = project.list_package_items_user();

    // Root namespace items have bare names.
    assert!(entries.iter().any(|e| e.fqn() == "Point"));

    // Namespaced items have qualified names.
    assert!(entries.iter().any(|e| e.fqn() == "llm.Config"));
    assert!(entries.iter().any(|e| e.fqn() == "llm.LlmIdentity"));
    assert!(entries.iter().any(|e| e.fqn() == "lorem.Resume"));
}

#[test]
fn list_namespace_items_llm() {
    let project = make_multi_ns_project();
    let entries = project.list_namespace_items_user(&["llm"]);
    assert!(entries.is_some());
    let entries = entries.unwrap();

    // Should only contain llm namespace items.
    assert!(entries.iter().all(|e| e.fqn().starts_with("llm.")));
    assert!(entries.iter().any(|e| e.fqn() == "llm.Config"));
    assert!(entries.iter().any(|e| e.fqn() == "llm.LlmIdentity"));

    let listing: String = entries
        .iter()
        .map(|e| project.format_listing_entry(e))
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(listing);
}

#[test]
fn list_namespace_items_nonexistent() {
    let project = make_multi_ns_project();
    let entries = project.list_namespace_items_user(&["nonexistent"]);
    assert!(entries.is_none());
}

#[test]
fn list_namespace_items_lorem() {
    let project = make_multi_ns_project();
    let entries = project.list_namespace_items_user(&["lorem"]);
    assert!(entries.is_some());
    let entries = entries.unwrap();
    assert!(entries.iter().any(|e| e.fqn() == "lorem.Resume"));
}

// ── Deep namespace fixture ───────────────────────────────────────────────────

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

// ── Round-trip property tests ────────────────────────────────────────────────

/// Critical invariant: every FQN emitted by listing must resolve back to its definition.
/// This prevents the class of bug where listings show paths that don't navigate.
#[test]
fn round_trip_listing_to_resolve() {
    let project = make_multi_ns_project();
    let pkg_id =
        baml_compiler2_hir::package::PackageId::new(&project.db, baml_base::Name::new("user"));
    let entries = crate::listing::list_package_items(&project.db, pkg_id);

    for entry in &entries {
        let fqn = entry.fqn();
        let resolved = crate::listing::resolve_target(&project.db, pkg_id, &fqn);
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
    let pkg_id =
        baml_compiler2_hir::package::PackageId::new(&project.db, baml_base::Name::new("user"));
    let entries = crate::listing::list_package_items(&project.db, pkg_id);

    assert!(
        !entries.is_empty(),
        "expected at least one entry in deep_ns project"
    );

    for entry in &entries {
        let fqn = entry.fqn();
        let resolved = crate::listing::resolve_target(&project.db, pkg_id, &fqn);
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
    let pkg_id =
        baml_compiler2_hir::package::PackageId::new(&project.db, baml_base::Name::new("user"));
    let pkg = baml_compiler2_hir::package::package_items(&project.db, pkg_id);

    for ns_path in pkg.namespaces.keys() {
        if ns_path.is_empty() {
            continue; // root namespace — not a Namespace target
        }
        let dotted: String = ns_path
            .iter()
            .map(baml_base::Name::as_str)
            .collect::<Vec<_>>()
            .join(".");
        let resolved = crate::listing::resolve_target(&project.db, pkg_id, &dotted);
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
    let pkg_id =
        baml_compiler2_hir::package::PackageId::new(&project.db, baml_base::Name::new("user"));
    let pkg = baml_compiler2_hir::package::package_items(&project.db, pkg_id);

    let mut checked = 0;
    for ns_path in pkg.namespaces.keys() {
        if ns_path.is_empty() {
            continue;
        }
        let dotted: String = ns_path
            .iter()
            .map(baml_base::Name::as_str)
            .collect::<Vec<_>>()
            .join(".");
        let resolved = crate::listing::resolve_target(&project.db, pkg_id, &dotted);
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
    let pkg_id =
        baml_compiler2_hir::package::PackageId::new(&project.db, baml_base::Name::new("user"));
    let entries = crate::listing::list_package_items(&project.db, pkg_id);

    let files = baml_compiler2_hir::compiler2_all_files(&project.db);
    let mut checked = 0;

    for entry in &entries {
        let item_fqn = entry.fqn();

        // Look up the item's definition.
        let def = {
            let pkg = baml_compiler2_hir::package::package_items(&project.db, pkg_id);
            pkg.lookup_type(&entry.ns_path, &entry.item_name)
                .or_else(|| pkg.lookup_value(&entry.ns_path, &entry.item_name))
        };
        let Some(def) = def else { continue };

        // Find item in outline and walk its children.
        let Some((item_file, item_name_span)) = crate::utils::definition_span(&project.db, def)
        else {
            continue;
        };
        let outline = crate::outline::file_outline(&project.db, item_file);
        let item_name_text = {
            let text = item_file.text(&project.db);
            let s: usize = item_name_span.start().into();
            let e: usize = item_name_span.end().into();
            text[s..e].to_string()
        };

        for outline_item in outline {
            if outline_item.name != item_name_text {
                continue;
            }
            for child in &outline_item.children {
                let member_path = format!("{item_fqn}.{}", child.name);
                let resolved = crate::listing::resolve_target(&project.db, pkg_id, &member_path);
                assert!(
                    matches!(resolved, Some(ResolvedTarget::Member { .. })),
                    "member `{member_path}` should resolve as Member; got {:?}",
                    resolved.as_ref().map(std::mem::discriminant),
                );
                // Verify member is resolvable via describe_item_member.
                let desc =
                    crate::describe::describe_item_member(&project.db, &files, def, &child.name);
                assert!(
                    desc.is_some(),
                    "describe_item_member for `{member_path}` should succeed",
                );
                checked += 1;
            }
        }
    }

    assert!(checked > 0, "expected at least one member to be checked");
}
