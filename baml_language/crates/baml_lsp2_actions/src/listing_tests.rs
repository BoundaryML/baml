//! Tests for `listing` module.

use crate::testing::ProjectTest;

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
fn list_package_items_sorted_by_file_then_line() {
    let project = make_multi_ns_project();
    let entries = project.list_package_items_user();

    // Verify sorted by (file_path, line).
    for window in entries.windows(2) {
        let a = &window[0];
        let b = &window[1];
        assert!(
            (a.file_path.as_str(), a.line) <= (b.file_path.as_str(), b.line),
            "Entries not sorted: {:?} vs {:?}",
            (a.file_path.as_str(), a.line),
            (b.file_path.as_str(), b.line),
        );
    }
}

#[test]
fn list_package_items_fqns_include_namespace() {
    let project = make_multi_ns_project();
    let entries = project.list_package_items_user();

    // Root namespace items have bare names.
    assert!(entries.iter().any(|e| e.fqn == "Point"));

    // Namespaced items have qualified names.
    assert!(entries.iter().any(|e| e.fqn == "llm.Config"));
    assert!(entries.iter().any(|e| e.fqn == "llm.LlmIdentity"));
    assert!(entries.iter().any(|e| e.fqn == "lorem.Resume"));
}

#[test]
fn list_namespace_items_llm() {
    let project = make_multi_ns_project();
    let entries = project.list_namespace_items_user(&["llm"]);
    assert!(entries.is_some());
    let entries = entries.unwrap();

    // Should only contain llm namespace items.
    assert!(entries.iter().all(|e| e.fqn.starts_with("llm.")));
    assert!(entries.iter().any(|e| e.fqn == "llm.Config"));
    assert!(entries.iter().any(|e| e.fqn == "llm.LlmIdentity"));

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
    assert!(entries.iter().any(|e| e.fqn == "lorem.Resume"));
}
