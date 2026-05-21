//! Snapshot tests for `describe_command` rendering output.
//!
//! These tests exercise the real `write_description` and `write_listing`
//! functions to verify the exact CLI output format agents will see.

use std::path::Path;

use baml_db::baml_compiler2_hir;
use baml_lsp2_actions::ResolvedTarget;
use baml_project::ProjectDatabase;

use crate::describe_command::{dispatch, write_description, write_keyword, write_listing};

// ── Test helpers ────────────────────────────────────────────────────────────

/// Build a `ProjectDatabase` with the given BAML source files.
///
/// `files` is a list of `(relative_path, content)` pairs. The project root
/// is set to `/test` so paths should be like `"types.baml"` or
/// `"ns_llm/models.baml"`.
fn make_db(files: &[(&str, &str)]) -> ProjectDatabase {
    let root = Path::new("/test");
    let mut db = ProjectDatabase::new();
    db.set_project_root(root);
    for (path, content) in files {
        let full_path = root.join(path);
        db.add_or_update_file(&full_path, content);
    }
    db
}

/// Capture `write_description` output as a String.
fn capture_description(
    db: &ProjectDatabase,
    desc: &baml_lsp2_actions::SymbolDescription,
    budget: usize,
) -> String {
    let mut buf = Vec::new();
    write_description(&mut buf, db, desc, budget, Path::new("/test")).unwrap();
    String::from_utf8(buf).unwrap()
}

/// Capture `write_keyword` output as a String.
fn capture_keyword(name: &str) -> String {
    let mut buf = Vec::new();
    write_keyword(&mut buf, name).unwrap();
    String::from_utf8(buf).unwrap()
}

/// Capture `write_listing` output as a String.
fn capture_listing(entries: &[baml_lsp2_actions::ListingEntry]) -> String {
    let mut buf = Vec::new();
    write_listing(&mut buf, entries, Path::new("/test")).unwrap();
    String::from_utf8(buf).unwrap()
}

// ── Test fixtures ───────────────────────────────────────────────────────────

fn simple_project() -> ProjectDatabase {
    make_db(&[
        (
            "types.baml",
            r#"
class Point {
    x int
    y int
}

enum Color {
    Red,
    Green,
    Blue,
}
"#,
        ),
        (
            "funcs.baml",
            r#"
/// Extract a point from text.
function ExtractPoint(text: string) -> Point {
    let result = Point { x: 0, y: 0 };
    return result;
}
"#,
        ),
    ])
}

/// A project with a 2-deep user namespace (`foo.bar`).
fn deep_ns_project() -> ProjectDatabase {
    make_db(&[(
        "ns_foo/ns_bar/types.baml",
        r#"
class Baz {
    field int
}
"#,
    )])
}

/// Run the real CLI dispatch for a name and render the output as a String.
///
/// Uses the new `dispatch()` function (based on `ResolvedTarget`) so tests
/// exercise the same code path as `baml describe <name>`.
fn describe_via_dispatch(db: &ProjectDatabase, name: &str) -> String {
    let files = baml_compiler2_hir::compiler2_all_files(db);
    match dispatch(db, name) {
        Some(ResolvedTarget::Keyword(ref kw)) => capture_keyword(kw),
        Some(ResolvedTarget::Package(pkg)) => {
            let entries = baml_lsp2_actions::list_package_items(db, pkg);
            capture_listing(&entries)
        }
        Some(ResolvedTarget::Namespace { package, ns_path }) => {
            let entries =
                baml_lsp2_actions::list_namespace_items(db, package, &ns_path).unwrap_or_default();
            capture_listing(&entries)
        }
        Some(ResolvedTarget::Item(def)) => {
            if let Some(desc) = baml_lsp2_actions::describe_by_definition(db, &files, def) {
                capture_description(db, &desc, 30)
            } else {
                format!("NO DESCRIPTION: {name}\n")
            }
        }
        Some(ResolvedTarget::Member {
            parent,
            member_name,
        }) => {
            if let Some(desc) =
                baml_lsp2_actions::describe_item_member(db, &files, parent, member_name.as_str())
            {
                capture_description(db, &desc, 30)
            } else {
                format!("NO DESCRIPTION: {name}\n")
            }
        }
        None => {
            // Fallback: substring describe (CLI behavior).
            let descs = baml_lsp2_actions::describe(db, &files, name);
            if descs.is_empty() {
                format!("NOT FOUND: {name}\n")
            } else {
                capture_description(db, &descs[0], 30)
            }
        }
    }
}

fn multi_ns_project() -> ProjectDatabase {
    make_db(&[
        (
            "types.baml",
            r#"
class Point {
    x int
    y int
}
"#,
        ),
        (
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
        ),
    ])
}

// ── Project listing tests ───────────────────────────────────────────────────

#[test]
fn render_project_listing() {
    let db = multi_ns_project();
    let pkg_id = baml_compiler2_hir::package::PackageId::new(&db, baml_db::Name::new("user"));
    let entries = baml_lsp2_actions::list_package_items(&db, pkg_id);
    let output = capture_listing(&entries);
    insta::assert_snapshot!(output);
}

// ── Namespace listing tests ─────────────────────────────────────────────────

#[test]
fn render_namespace_listing_llm() {
    let db = multi_ns_project();
    let pkg_id = baml_compiler2_hir::package::PackageId::new(&db, baml_db::Name::new("user"));
    let ns_path = vec![baml_db::Name::new("llm")];
    let entries = baml_lsp2_actions::list_namespace_items(&db, pkg_id, &ns_path).unwrap();
    let output = capture_listing(&entries);
    insta::assert_snapshot!(output);
}

// ── Item detail tests ───────────────────────────────────────────────────────

#[test]
fn render_describe_class() {
    let db = simple_project();
    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let descs = baml_lsp2_actions::describe(&db, &files, "Point");
    assert_eq!(descs.len(), 1);
    let output = capture_description(&db, &descs[0], 30);
    insta::assert_snapshot!(output);
}

#[test]
fn render_describe_enum() {
    let db = simple_project();
    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let descs = baml_lsp2_actions::describe(&db, &files, "Color");
    assert_eq!(descs.len(), 1);
    let output = capture_description(&db, &descs[0], 30);
    insta::assert_snapshot!(output);
}

#[test]
fn render_describe_function_with_docstring() {
    let db = simple_project();
    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let descs = baml_lsp2_actions::describe(&db, &files, "ExtractPoint");
    assert_eq!(descs.len(), 1);
    let output = capture_description(&db, &descs[0], 30);
    insta::assert_snapshot!(output);
}

// ── Dot-notation describe_by_definition tests ───────────────────────────────

#[test]
fn render_describe_ns_item() {
    let db = multi_ns_project();
    let pkg_id = baml_compiler2_hir::package::PackageId::new(&db, baml_db::Name::new("user"));
    let pkg = baml_compiler2_hir::package::package_items(&db, pkg_id);

    let ns_path = vec![baml_db::Name::new("llm")];
    let item_name = baml_db::Name::new("Config");
    let def = pkg.lookup_type(&ns_path, &item_name).unwrap();

    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let desc = baml_lsp2_actions::describe_by_definition(&db, &files, def).unwrap();
    let output = capture_description(&db, &desc, 30);
    insta::assert_snapshot!(output);
}

// ── Builtin / standard library listing tests ────────────────────────────────

/// `baml describe baml` — list all items in the builtin `baml` package.
#[test]
fn render_builtin_package_listing() {
    let db = simple_project();
    let pkg_id = baml_compiler2_hir::package::PackageId::new(&db, baml_db::Name::new("baml"));
    let entries = baml_lsp2_actions::list_package_items(&db, pkg_id);
    assert!(!entries.is_empty());
    let output = capture_listing(&entries);
    insta::assert_snapshot!(output);
}

/// `baml describe baml.env` — list items in the `env` sub-namespace.
#[test]
fn render_builtin_namespace_env() {
    let db = simple_project();
    let pkg_id = baml_compiler2_hir::package::PackageId::new(&db, baml_db::Name::new("baml"));
    let ns_path = vec![baml_db::Name::new("env")];
    let entries = baml_lsp2_actions::list_namespace_items(&db, pkg_id, &ns_path).unwrap();
    assert!(!entries.is_empty());
    let output = capture_listing(&entries);
    insta::assert_snapshot!(output);
}

/// `baml describe baml.llm` — list items in the `llm` sub-namespace.
#[test]
fn render_builtin_namespace_llm() {
    let db = simple_project();
    let pkg_id = baml_compiler2_hir::package::PackageId::new(&db, baml_db::Name::new("baml"));
    let ns_path = vec![baml_db::Name::new("llm")];
    let entries = baml_lsp2_actions::list_namespace_items(&db, pkg_id, &ns_path).unwrap();
    assert!(!entries.is_empty());
    let output = capture_listing(&entries);
    insta::assert_snapshot!(output);
}

/// `baml describe baml.math` — list items in the `math` sub-namespace.
#[test]
fn render_builtin_namespace_math() {
    let db = simple_project();
    let pkg_id = baml_compiler2_hir::package::PackageId::new(&db, baml_db::Name::new("baml"));
    let ns_path = vec![baml_db::Name::new("math")];
    let entries = baml_lsp2_actions::list_namespace_items(&db, pkg_id, &ns_path).unwrap();
    assert!(!entries.is_empty());
    let output = capture_listing(&entries);
    insta::assert_snapshot!(output);
}

/// `baml describe testing` — list items in the `testing` package.
#[test]
fn render_testing_package_listing() {
    let db = simple_project();
    let pkg_id = baml_compiler2_hir::package::PackageId::new(&db, baml_db::Name::new("testing"));
    let entries = baml_lsp2_actions::list_package_items(&db, pkg_id);
    assert!(!entries.is_empty());
    let output = capture_listing(&entries);
    insta::assert_snapshot!(output);
}

/// `baml describe assert` — list items in the `assert` package.
#[test]
fn render_assert_package_listing() {
    let db = simple_project();
    let pkg_id = baml_compiler2_hir::package::PackageId::new(&db, baml_db::Name::new("assert"));
    let entries = baml_lsp2_actions::list_package_items(&db, pkg_id);
    assert!(!entries.is_empty());
    let output = capture_listing(&entries);
    insta::assert_snapshot!(output);
}

/// Describe a builtin type (String) via `describe()` with compiler2 visible files.
#[test]
fn render_describe_builtin_string() {
    let db = simple_project();
    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let descs = baml_lsp2_actions::describe(&db, &files, "String");
    assert_eq!(descs.len(), 1);
    let output = capture_description(&db, &descs[0], 30);
    insta::assert_snapshot!(output);
}

/// Describe a builtin function (deep_copy) via `describe()` with compiler2 visible files.
#[test]
fn render_describe_builtin_deep_copy() {
    let db = simple_project();
    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let descs = baml_lsp2_actions::describe(&db, &files, "deep_copy");
    assert_eq!(descs.len(), 1);
    let output = capture_description(&db, &descs[0], 30);
    insta::assert_snapshot!(output);
}

/// Describe a builtin item via describe_by_definition (baml.String).
#[test]
fn render_describe_builtin_item_by_definition() {
    let db = simple_project();
    let pkg_id = baml_compiler2_hir::package::PackageId::new(&db, baml_db::Name::new("baml"));
    let pkg = baml_compiler2_hir::package::package_items(&db, pkg_id);

    let root_ns: Vec<baml_db::Name> = vec![];
    let item_name = baml_db::Name::new("String");
    let def = pkg.lookup_type(&root_ns, &item_name).unwrap();

    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let desc = baml_lsp2_actions::describe_by_definition(&db, &files, def).unwrap();
    let output = capture_description(&db, &desc, 30);
    insta::assert_snapshot!(output);
}

/// `non_user_package_names()` should return the builtin packages.
#[test]
fn non_user_package_names_includes_builtins() {
    let db = simple_project();
    let names = baml_lsp2_actions::non_user_package_names(&db);
    assert!(names.contains("baml"), "expected 'baml' in {names:?}");
    assert!(names.contains("testing"), "expected 'testing' in {names:?}");
    assert!(names.contains("assert"), "expected 'assert' in {names:?}");
    assert!(!names.contains("user"), "should not contain 'user'");
}

// ── Member detail tests ─────────────────────────────────────────────────────

#[test]
fn render_describe_member_field() {
    let db = simple_project();
    let pkg_id = baml_compiler2_hir::package::PackageId::new(&db, baml_db::Name::new("user"));
    let pkg = baml_compiler2_hir::package::package_items(&db, pkg_id);

    let root_ns: Vec<baml_db::Name> = vec![];
    let item_name = baml_db::Name::new("Point");
    let def = pkg.lookup_type(&root_ns, &item_name).unwrap();

    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let desc = baml_lsp2_actions::describe_item_member(&db, &files, def, "x").unwrap();
    let output = capture_description(&db, &desc, 30);
    insta::assert_snapshot!(output);
}

#[test]
fn render_describe_ns_member() {
    let db = multi_ns_project();
    let pkg_id = baml_compiler2_hir::package::PackageId::new(&db, baml_db::Name::new("user"));
    let pkg = baml_compiler2_hir::package::package_items(&db, pkg_id);

    let ns_path = vec![baml_db::Name::new("llm")];
    let item_name = baml_db::Name::new("Config");
    let def = pkg.lookup_type(&ns_path, &item_name).unwrap();

    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let desc = baml_lsp2_actions::describe_item_member(&db, &files, def, "model").unwrap();
    let output = capture_description(&db, &desc, 30);
    insta::assert_snapshot!(output);
}

// ── Deep namespace tests (fixed) ────────────────────────────────────────────
//
// These tests verify that the CLI dispatch correctly handles namespaces deeper
// than 1 segment. Previously `baml describe foo.bar.Baz` would return NOT FOUND
// because the dispatch only checked `segments[0]` against the namespace map.
// The new dispatch uses `resolve_target` which handles arbitrary namespace depth.

/// `list_package_items` correctly produces a deep namespace FQN.
#[test]
fn deep_namespace_listing_produces_dotted_fqn() {
    let db = deep_ns_project();
    let pkg_id = baml_compiler2_hir::package::PackageId::new(&db, baml_db::Name::new("user"));
    let entries = baml_lsp2_actions::list_package_items(&db, pkg_id);
    let output = capture_listing(&entries);
    insta::assert_snapshot!(output);
}

/// The primitives DO support deep namespaces.
/// `lookup_type` with the full ns path `["foo", "bar"]` finds `Baz`.
#[test]
fn deep_namespace_primitive_lookup_works() {
    let db = deep_ns_project();
    let pkg_id = baml_compiler2_hir::package::PackageId::new(&db, baml_db::Name::new("user"));
    let pkg = baml_compiler2_hir::package::package_items(&db, pkg_id);

    let full_ns = vec![baml_db::Name::new("foo"), baml_db::Name::new("bar")];
    let item_name = baml_db::Name::new("Baz");
    let def = pkg.lookup_type(&full_ns, &item_name);
    assert!(def.is_some(), "deep namespace lookup should work");

    // Looking up `foo`-namespaced `bar` fails (correct: `foo.bar` is not a type).
    let single_ns = vec![baml_db::Name::new("foo")];
    let def_single = pkg.lookup_type(&single_ns, &baml_db::Name::new("bar"));
    assert!(
        def_single.is_none(),
        "looking up `foo`-namespaced `bar` should fail since `foo` isn't a leaf namespace"
    );
}

/// FIXED: `baml describe foo.bar` — now correctly lists the `foo.bar` namespace.
#[test]
fn fixed_describe_deep_namespace_listing() {
    let db = deep_ns_project();
    let output = describe_via_dispatch(&db, "foo.bar");
    insta::assert_snapshot!(output);
}

/// FIXED: `baml describe foo.bar.Baz` — now resolves the item correctly.
#[test]
fn fixed_describe_deep_namespace_item() {
    let db = deep_ns_project();
    let output = describe_via_dispatch(&db, "foo.bar.Baz");
    insta::assert_snapshot!(output);
}

/// FIXED: `baml describe foo.bar.Baz.field` — now resolves the member correctly.
#[test]
fn fixed_describe_deep_namespace_member() {
    let db = deep_ns_project();
    let output = describe_via_dispatch(&db, "foo.bar.Baz.field");
    insta::assert_snapshot!(output);
}

/// CONTROL: 1-deep namespaces still work.
#[test]
fn control_describe_shallow_namespace_item_works() {
    let db = multi_ns_project();
    let output = describe_via_dispatch(&db, "llm.Config");
    insta::assert_snapshot!(output);
}

// ── "Did you mean?" suggestion tests ────────────────────────────────────────

use crate::describe_command::suggest_similar;

/// Typo in an item name should suggest the correct spelling.
#[test]
fn suggest_typo_in_item_name() {
    let db = multi_ns_project();
    let suggestions = suggest_similar(&db, "Confg", 5);
    assert!(
        suggestions.iter().any(|s| s == "llm.Config"),
        "expected `llm.Config` suggestion for typo `Confg`, got {suggestions:?}",
    );
}

/// Typo in a namespace segment should suggest the correct namespace.
#[test]
fn suggest_typo_in_namespace() {
    let db = multi_ns_project();
    let suggestions = suggest_similar(&db, "llmm", 5);
    assert!(
        suggestions.iter().any(|s| s == "llm"),
        "expected `llm` suggestion for typo `llmm`, got {suggestions:?}",
    );
}

/// Substring of a real symbol should rank as a strong suggestion.
#[test]
fn suggest_substring_of_symbol() {
    let db = simple_project();
    let suggestions = suggest_similar(&db, "Extract", 5);
    assert!(
        suggestions.iter().any(|s| s == "ExtractPoint"),
        "expected `ExtractPoint` suggestion for substring `Extract`, got {suggestions:?}",
    );
}

/// Typo in a builtin namespace should suggest the right one.
#[test]
fn suggest_typo_in_builtin_namespace() {
    let db = simple_project();
    let suggestions = suggest_similar(&db, "baml.evn", 5);
    assert!(
        suggestions.iter().any(|s| s == "baml.env"),
        "expected `baml.env` suggestion for typo `baml.evn`, got {suggestions:?}",
    );
}

/// Garbage input should produce no suggestions (or very few).
#[test]
fn suggest_unrelated_input_returns_few_or_no_results() {
    let db = simple_project();
    let suggestions = suggest_similar(&db, "qzqzqzqzqz", 5);
    assert!(
        suggestions.is_empty(),
        "expected no suggestions for garbage input, got {suggestions:?}",
    );
}

/// Suggestions are case-insensitive — agents shouldn't have to remember casing.
#[test]
fn suggest_is_case_insensitive() {
    let db = simple_project();
    // Lowercase input should still find the correctly-cased item.
    let suggestions = suggest_similar(&db, "extractpoint", 5);
    assert!(
        suggestions.iter().any(|s| s == "ExtractPoint"),
        "expected `ExtractPoint` for lowercase `extractpoint`, got {suggestions:?}",
    );

    // Uppercase typo should also find the correctly-cased item.
    let suggestions = suggest_similar(&db, "POINT", 5);
    assert!(
        suggestions.iter().any(|s| s == "Point"),
        "expected `Point` for uppercase `POINT`, got {suggestions:?}",
    );
}

// ── Truncation hint tests ────────────────────────────────────────────────────

/// Truncated output shows `[INFO]` hint with correct line counts.
#[test]
fn render_describe_truncation_hint() {
    // Use a builtin with many methods (e.g., String, which has ~40+ methods).
    let db = simple_project();
    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let descs = baml_lsp2_actions::describe(&db, &files, "String");
    assert_eq!(descs.len(), 1);
    let output = capture_description(&db, &descs[0], 30);
    // The output should contain the [INFO] hint since String has more than 30 lines.
    assert!(
        output.contains("[INFO] Showing"),
        "expected [INFO] truncation hint in output:\n{output}"
    );
    assert!(
        output.contains("--budget"),
        "expected --budget in truncation hint:\n{output}"
    );
    insta::assert_snapshot!(output);
}

/// Full budget shows no truncation hint.
#[test]
fn render_describe_no_hint_when_full() {
    let db = simple_project();
    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let descs = baml_lsp2_actions::describe(&db, &files, "Point");
    assert_eq!(descs.len(), 1);
    // Point has only a few lines; budget of 30 is sufficient.
    let output = capture_description(&db, &descs[0], 30);
    assert!(
        !output.contains("[INFO]"),
        "should not have [INFO] hint when output is not truncated:\n{output}"
    );
}

/// Suggestions are limited to the requested count.
#[test]
fn suggest_respects_limit() {
    let db = multi_ns_project();
    // A common substring like "n" should match many things.
    let suggestions = suggest_similar(&db, "n", 3);
    assert!(
        suggestions.len() <= 3,
        "got {} suggestions, expected ≤ 3",
        suggestions.len()
    );
}

// ── Keyword tests ──────────────────────────────────────────────────────────

#[test]
fn render_keyword_class() {
    let output = capture_keyword("class");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_if() {
    let output = capture_keyword("if");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_spawn() {
    let output = capture_keyword("spawn");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_ts_interface() {
    let output = capture_keyword("interface");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_ts_instanceof() {
    let output = capture_keyword("instanceof");
    insta::assert_snapshot!(output);
}

/// Keywords via dispatch should resolve to `ResolvedTarget::Keyword`.
#[test]
fn dispatch_keyword_class() {
    let db = simple_project();
    let output = describe_via_dispatch(&db, "class");
    insta::assert_snapshot!(output);
}

/// Unknown keyword should fall through to normal resolution.
#[test]
fn dispatch_nonexistent_keyword() {
    let db = simple_project();
    let output = describe_via_dispatch(&db, "nonexistent_keyword");
    assert!(
        output.starts_with("NOT FOUND:"),
        "expected NOT FOUND for non-keyword, got: {output}"
    );
}

// ── root. disambiguation tests ─────────────────────────────────────────────

/// `root.X` resolves to user-package items.
#[test]
fn dispatch_root_prefix_resolves_user_item() {
    let db = simple_project();
    let output = describe_via_dispatch(&db, "root.Point");
    // Should resolve to the user's Point class
    assert!(
        output.contains("class Point"),
        "expected user Point class, got: {output}"
    );
    insta::assert_snapshot!(output);
}

/// `root.` with a nonexistent item returns NOT FOUND.
#[test]
fn dispatch_root_prefix_nonexistent() {
    let db = simple_project();
    let output = describe_via_dispatch(&db, "root.Nonexistent");
    assert!(
        output.starts_with("NOT FOUND:"),
        "expected NOT FOUND, got: {output}"
    );
}
