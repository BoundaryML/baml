//! Snapshot tests for `describe_command` rendering output.
//!
//! These tests exercise the real `write_description` and `write_listing`
//! functions to verify the exact CLI output format agents will see.

use std::path::Path;

use baml_db::baml_compiler2_hir;
use baml_lsp2_actions::ResolvedTarget;
use baml_project::ProjectDatabase;

use crate::describe_command::{
    definition_line_range, dispatch, write_description, write_keyword, write_listing,
};

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

fn truncation_budgets(output: &str) -> Vec<usize> {
    output
        .split("`--budget ")
        .skip(1)
        .map(|suffix| {
            suffix
                .split('`')
                .next()
                .expect("budget hint must have a closing backtick")
                .parse()
                .expect("budget hint must contain an integer")
        })
        .collect()
}

fn assert_reported_budget_is_minimum(
    db: &ProjectDatabase,
    desc: &baml_lsp2_actions::SymbolDescription,
    probe_budget: usize,
) -> usize {
    let truncated = capture_description(db, desc, probe_budget);
    let hinted_budgets = truncation_budgets(&truncated);
    assert!(
        !hinted_budgets.is_empty(),
        "expected a budget hint:\n{truncated}"
    );

    let required = hinted_budgets[0];
    assert!(
        hinted_budgets.iter().all(|budget| *budget == required),
        "all truncation markers must report the same full-output budget: {hinted_budgets:?}\n{truncated}"
    );

    let previous = required
        .checked_sub(1)
        .expect("a truncated description must require a positive budget");
    let almost_full = capture_description(db, desc, previous);
    assert!(
        !truncation_budgets(&almost_full).is_empty(),
        "the reported budget must be minimal; budget {previous} unexpectedly rendered everything:\n{almost_full}"
    );

    let exactly_full = capture_description(db, desc, required);
    assert!(
        truncation_budgets(&exactly_full).is_empty(),
        "the reported budget must render the complete description:\n{exactly_full}"
    );

    required
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

// ── Default state (no user files) ─────────────────────────────────────────
//
// `set_project_root` loads the `baml.*` stdlib regardless of user files, so
// `baml describe` resolves builtins even with an empty project — the
// stdlib-only "default state" the CLI falls back to when there's no
// `baml.toml`. See `project_load::load_project_or_default`.

/// `baml describe baml.String` resolves against the stdlib with zero user
/// files loaded.
#[test]
fn dispatch_resolves_stdlib_class_with_no_user_files() {
    let db = make_db(&[]);
    let output = describe_via_dispatch(&db, "baml.String");
    assert!(
        output.contains("String"),
        "expected stdlib `String` description with no user files, got:\n{output}",
    );
    assert!(
        !output.starts_with("NOT FOUND"),
        "stdlib `String` should resolve in the default state, got:\n{output}",
    );
}

/// The lowercase primitive alias `string` also resolves against the stdlib
/// in the default state (no user files).
#[test]
fn dispatch_resolves_primitive_alias_with_no_user_files() {
    let db = make_db(&[]);
    let output = describe_via_dispatch(&db, "string");
    assert!(
        !output.starts_with("NOT FOUND") && !output.starts_with("NO DESCRIPTION"),
        "primitive alias `string` should resolve in the default state, got:\n{output}",
    );
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
fn render_describe_interface() {
    let db = make_db(&[(
        "interfaces.baml",
        r#"
interface Named {
    name: string
    function label(self) -> string
}

class Person {
    name: string
    implements Named {
        function label(self) -> string {
            return self.name
        }
    }
}
"#,
    )]);
    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let descs = baml_lsp2_actions::describe(&db, &files, "Named");
    assert_eq!(descs.len(), 1);
    let output = capture_description(&db, &descs[0], 30);
    insta::assert_snapshot!(output);
}

#[test]
fn render_describe_class_shows_associated_type_bindings() {
    let db = make_db(&[(
        "interfaces.baml",
        r#"
interface Decoder<Input> {
    type Output
    function decode(self, raw: Input) -> Self.Output
}

class IntDecoder {
    implements Decoder<string> {
        type Output = int
        function decode(self, raw: string) -> Self.Output {
            return 1
        }
    }
}
"#,
    )]);
    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let descs = baml_lsp2_actions::describe(&db, &files, "IntDecoder");
    assert_eq!(descs.len(), 1);
    let output = capture_description(&db, &descs[0], 30);
    assert!(
        output.contains("type Output = int"),
        "expected class describe to include associated type bindings, got:\n{output}"
    );
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
    let listed_names: Vec<&str> = output
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect();
    assert!(
        listed_names.contains(&"baml.iter.Range"),
        "expected builtin listing to include package-qualified names, got:\n{output}"
    );
    assert!(
        !listed_names.contains(&"iter.Range"),
        "builtin listing should not emit unqualified names that cannot be described directly, got:\n{output}"
    );
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

/// `baml describe reflect` routes through the ordinary root-package path.
#[test]
fn render_reflect_package_listing() {
    let db = simple_project();
    let output = describe_via_dispatch(&db, "reflect");
    assert!(output.contains("reflect.Type"), "{output}");
    assert!(output.contains("reflect.Package"), "{output}");
    assert!(!output.contains("baml.reflect"), "{output}");
    insta::assert_snapshot!(output);
}

#[test]
fn describe_reflect_type_and_intrinsic() {
    let db = simple_project();
    let type_output = describe_via_dispatch(&db, "reflect.Type");
    assert!(type_output.contains("class Type"), "{type_output}");
    let intrinsic_output = describe_via_dispatch(&db, "reflect.Type.of");
    assert!(
        intrinsic_output.contains("function of<T>() -> reflect.Type"),
        "{intrinsic_output}"
    );
}

/// `baml describe ai.internal` — list items in the `ai.internal` namespace,
/// the home of the package-private helpers and the prompt-rendering plumbing
/// (there is deliberately no `baml.prompt` namespace).
#[test]
fn render_builtin_namespace_ai_internal() {
    let db = simple_project();
    let pkg_id = baml_compiler2_hir::package::PackageId::new(&db, baml_db::Name::new("ai"));
    let ns_path = vec![baml_db::Name::new("internal")];
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

#[test]
fn render_describe_log_info_builtin() {
    let db = simple_project();
    let output = describe_via_dispatch(&db, "log.info");
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

// ── Class method tests ───────────────────────────────────────────────────────

/// A project of user classes with methods: instance-only (`User`), mixed
/// instance + static (`Counter`), and a generic class with a cross-type
/// dependency (`Wrapper<T>` referencing `WrapperMarker`).
fn methods_project() -> ProjectDatabase {
    make_db(&[(
        "methods.baml",
        r#"
class User {
    name string
    age int

    function Greet(self) -> string {
        "Hello"
    }

    function IsAdult(self) -> bool {
        self.age >= 18
    }
}

class Counter {
    count int

    function increment(self) -> Counter {
        Counter { count: self.count + 1 }
    }

    function make() -> Counter {
        Counter { count: 0 }
    }
}

class WrapperMarker {
    reason string
}

class Wrapper<T> {
    value T

    function get_value(self) -> T {
        self.value
    }

    function get_or_marker(self) -> T | WrapperMarker {
        self.value
    }
}
"#,
    )])
}

/// A user class with only instance methods: each shows its canonical signature
/// in a `methods:` section, and the body block is fields-only.
#[test]
fn render_describe_class_with_methods() {
    let db = methods_project();
    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let descs = baml_lsp2_actions::describe(&db, &files, "User");
    assert_eq!(descs.len(), 1);
    insta::assert_snapshot!(capture_description(&db, &descs[0], 30));
}

/// A class with both an instance method (`increment`) and a static method
/// (`make`) renders both `methods:` and `static_methods:` sections.
#[test]
fn render_describe_class_with_static_methods() {
    let db = methods_project();
    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let descs = baml_lsp2_actions::describe(&db, &files, "Counter");
    assert_eq!(descs.len(), 1);
    let output = capture_description(&db, &descs[0], 30);
    assert!(output.contains("methods:"));
    assert!(output.contains("static_methods:"));
    insta::assert_snapshot!(output);
}

/// A generic class renders `class Wrapper<T>` in the body, type-variable return
/// types (`-> T`), and a cross-type dependency (`WrapperMarker`).
#[test]
fn render_describe_generic_class_with_methods() {
    let db = methods_project();
    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let descs = baml_lsp2_actions::describe(&db, &files, "Wrapper");
    assert_eq!(descs.len(), 1);
    insta::assert_snapshot!(capture_description(&db, &descs[0], 30));
}

/// `baml describe string` resolves the builtin `baml.String` class via its
/// lowercase alias and renders it (header shows `(string)`).
#[test]
fn render_describe_alias_string() {
    let db = simple_project();
    insta::assert_snapshot!(describe_via_dispatch(&db, "string"));
}

/// Lowercase primitive/keyword aliases route to their builtin `baml` class.
#[test]
fn dispatch_lowercase_aliases_resolve_to_items() {
    let db = simple_project();
    for alias in ["string", "int", "bigint", "float", "bool", "image", "json"] {
        assert!(
            matches!(dispatch(&db, alias), Some(ResolvedTarget::Item(_))),
            "alias `{alias}` should resolve to a builtin class item"
        );
    }
}

/// Comment stripping is CST-token based, so a line that *looks* like a comment
/// but lives inside a block string (e.g. an LLM prompt) is preserved, while a
/// real `//` comment is removed. A line-based stripper would corrupt the string.
#[test]
fn describe_preserves_comment_like_lines_inside_strings() {
    let db = make_db(&[(
        "prompt.baml",
        r##"
function PromptFn() -> string {
    // a real comment that must be stripped
    `
// not a comment — this is prompt content
keep this line
`
}
"##,
    )]);
    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let descs = baml_lsp2_actions::describe(&db, &files, "PromptFn");
    assert_eq!(descs.len(), 1);
    let output = capture_description(&db, &descs[0], 30);

    assert!(
        !output.contains("a real comment that must be stripped"),
        "real `//` comment should be stripped:\n{output}"
    );
    assert!(
        output.contains("not a comment — this is prompt content")
            && output.contains("keep this line"),
        "comment-like lines inside a block string must be preserved:\n{output}"
    );
}

/// Drilling into a user method (`User.Greet`) renders its signature and body.
#[test]
fn describe_user_method_drill_in_shows_body() {
    let db = methods_project();
    let output = describe_via_dispatch(&db, "User.Greet");
    assert!(
        output.contains("function Greet(self) -> string"),
        "expected method signature:\n{output}"
    );
    assert!(
        output.contains("\"Hello\""),
        "drill-in should show the method body:\n{output}"
    );
    assert!(
        output.contains("container:") && output.contains("User"),
        "owning class should be the container:\n{output}"
    );
    insta::assert_snapshot!(output);
}

/// Drilling into a builtin method (`string.length`, via the alias) resolves and
/// shows the signature, with the native body elided.
#[test]
fn describe_builtin_method_drill_in_via_alias() {
    let db = simple_project();
    let output = describe_via_dispatch(&db, "string.length");
    assert!(
        output.contains("function length(self) -> int"),
        "expected resolved builtin method signature:\n{output}"
    );
    assert!(
        !output.contains("$rust_function"),
        "native body marker must not appear:\n{output}"
    );
    assert!(
        !output.starts_with("NOT FOUND") && !output.starts_with("NO DESCRIPTION"),
        "`string.length` should resolve via the alias:\n{output}"
    );
}

/// Drilling into builtin methods by their class name (`Array.reduce`,
/// `String.split`, `Map.get`) resolves the unqualified class against the stdlib.
#[test]
fn describe_builtin_method_drill_in_via_class_name() {
    let db = simple_project();

    let cases = [
        (
            "Array.reduce",
            "function reduce<A, E>(self, reducer: (A, T) -> A throws E, initial: A) -> A throws E",
        ),
        (
            "String.split",
            "function split(self, delimiter: string) -> string[]",
        ),
        ("Map.get", "function get(self, key: K) -> V | null"),
    ];

    for (name, expected_signature) in cases {
        let output = describe_via_dispatch(&db, name);
        assert!(
            output.contains(expected_signature),
            "expected resolved builtin method signature for `{name}`:\n{output}",
        );
        assert!(
            !output.starts_with("NOT FOUND") && !output.starts_with("NO DESCRIPTION"),
            "`{name}` should resolve via the unqualified builtin class name:\n{output}",
        );
    }
}

/// User-defined class methods still resolve before the stdlib fallback, even
/// when the class name matches a builtin class such as `Array`.
#[test]
fn describe_user_defined_class_method_takes_precedence_over_builtin_fallback() {
    let db = make_db(&[(
        "shadow_builtin.baml",
        r#"
/// A user-defined class that intentionally shares a builtin class name.
class Array {
    value string

    /// Return a user-defined reduction marker.
    function reduce(self) -> string {
        "user reduce"
    }
}
"#,
    )]);

    let output = describe_via_dispatch(&db, "Array.reduce");
    assert!(
        output.contains("function reduce(self) -> string"),
        "expected user-defined method signature:\n{output}",
    );
    assert!(
        output.contains("\"user reduce\""),
        "user-defined method body should be rendered:\n{output}",
    );
    assert!(
        !output.contains("reducer: (A, T) -> A throws E"),
        "builtin `Array.reduce` must not shadow the user-defined class method:\n{output}",
    );
}

// ── definition_line_range tests ──────────────────────────────────────────────

#[test]
fn definition_line_range_trims_leading_trivia_and_trailing_ws() {
    // line 1 blank, 2 doc, 3 comment, 4 decl, 5 body, 6 close brace, 7 trailing
    let text = "\n/// doc\n// note\nclass Foo {\n  x int\n}\n";
    // Range covers the whole thing (trivia-inclusive node range).
    let (start, end) = definition_line_range(text, 0, text.len());
    assert_eq!((start, end), (4, 6), "start at `class`, end at `}}`");
}

#[test]
fn definition_line_range_comment_only_span_does_not_reverse() {
    // A span with no declaration content must never yield start > end.
    let text = "// just a comment\n// another\n";
    let (start, end) = definition_line_range(text, 0, text.len());
    assert!(start <= end, "range must not reverse: {start}-{end}");
}

// ── Truncation / budget tests ────────────────────────────────────────────────

/// The soft budget bounds the whole rendering, not just the body: method
/// sections are truncated to fit, with an explicit elision marker, while
/// their headers stay visible so the symbol's surface remains discoverable.
/// A generous budget still renders every method in full.
#[test]
fn render_describe_methods_respect_budget() {
    let db = simple_project();
    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let descs = baml_lsp2_actions::describe(&db, &files, "String");
    assert_eq!(descs.len(), 1);

    let tight = capture_description(&db, &descs[0], 5);
    let full = capture_description(&db, &descs[0], 1000);

    for needle in [
        "methods:",
        "static_methods:",
        "more lines (re-run with a higher `--budget` to see more; use `--budget ",
    ] {
        assert!(
            tight.contains(needle),
            "`{needle}` missing from describe output under budget 5:\n{tight}"
        );
    }

    // Tight output is meaningfully bounded: the budget is soft, but elided
    // method lists must not blow past it by more than the fixed per-section
    // overhead (headers + markers).
    let tight_lines = tight.lines().count();
    let full_lines = full.lines().count();
    assert!(
        tight_lines < full_lines / 2 && tight_lines <= 5 + 20,
        "budget 5 should bound output well below the full {full_lines} lines, got {tight_lines}:\n{tight}"
    );

    // A generous budget renders every method, with no elision marker.
    for needle in [
        "function to_upper_case(self) -> string",
        "static_methods:",
        "function from_code_points(unicode: int[]) -> string",
    ] {
        assert!(
            full.contains(needle),
            "`{needle}` missing from describe output under budget 1000:\n{full}"
        );
    }
    assert!(
        !full.contains("re-run with a higher `--budget`"),
        "no elision marker expected at budget 1000:\n{full}"
    );
    assert!(
        !tight.contains("function to_upper_case(self) -> string"),
        "late methods should be elided under budget 5:\n{tight}"
    );
    assert!(
        full.contains("function to_upper_case(self) -> string")
            && full.contains("function from_code_points(unicode: int[]) -> string"),
        "generous budgets should still show full method details:\n{full}"
    );

    // A characterization value: it tracks `baml.String`'s rendered size, so a
    // stdlib surface change moves it. What the test pins is the *property* the
    // helper checks — the hinted budget is minimal and renders everything.
    assert_eq!(assert_reported_budget_is_minimum(&db, &descs[0], 5), 101);
}

#[test]
fn render_describe_budget_hint_covers_dependencies_and_references() {
    let db = simple_project();
    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let point = baml_lsp2_actions::describe(&db, &files, "Point");
    assert_eq!(point.len(), 1);
    assert_reported_budget_is_minimum(&db, &point[0], 0);

    let db = methods_project();
    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let wrapper = baml_lsp2_actions::describe(&db, &files, "Wrapper");
    assert_eq!(wrapper.len(), 1);
    assert_reported_budget_is_minimum(&db, &wrapper[0], 0);
}

/// A class with a fields-only body (no docstring) still fits that body under a
/// tight budget, while later method sections use elision markers as needed.
#[test]
fn render_describe_fields_only_body_fits_tight_budget() {
    let db = methods_project();
    let files = baml_compiler2_hir::compiler2_all_files(&db);
    let descs = baml_lsp2_actions::describe(&db, &files, "User");
    assert_eq!(descs.len(), 1);

    let tight = capture_description(&db, &descs[0], 5);
    let full = capture_description(&db, &descs[0], 1000);
    assert!(
        !tight.contains("[INFO] showing"),
        "fields-only body must not truncate at budget 5:\n{tight}"
    );
    // The header + body block is identical at both budgets; only the trailing
    // list sections (methods here) give way under the tight budget.
    let body_part = |s: &str| s[..s.find("\nmethods:").expect("methods section")].to_string();
    assert_eq!(body_part(&tight), body_part(&full));
    assert!(
        tight.contains("more lines (re-run with a higher `--budget` to see more; use `--budget "),
        "methods exceeding the tight budget must be elided with a marker:\n{tight}"
    );
    for needle in ["class User {", "    name: string,", "    age: int,", "}"] {
        assert!(
            tight.contains(needle),
            "`{needle}` missing from fields-only body under budget 5:\n{tight}"
        );
    }
    assert!(
        tight.contains("methods:\n  … 2 more lines"),
        "methods should be summarized after the tight body budget is spent:\n{tight}"
    );
    assert!(
        full.contains("function Greet(self) -> string")
            && full.contains("function IsAdult(self) -> bool"),
        "generous budgets should still show full methods:\n{full}"
    );
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
fn render_keyword_generator() {
    let output = capture_keyword("generator");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_if() {
    let output = capture_keyword("if");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_test() {
    let output = capture_keyword("test");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_testset() {
    let output = capture_keyword("testset");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_spawn() {
    let output = capture_keyword("spawn");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_defer() {
    let output = capture_keyword("defer");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_cleanup() {
    let output = capture_keyword("cleanup");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_playground() {
    let output = capture_keyword("playground");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_baml_sdk() {
    let output = capture_keyword("baml_sdk");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_python() {
    let output = capture_keyword("python");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_typescript() {
    let output = capture_keyword("typescript");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_patterns() {
    let output = capture_keyword("patterns");
    insta::assert_snapshot!(output);
}

/// `defer` and `cleanup` (BEP-042) resolve to keyword topics rather than
/// falling through to "No symbol found" (regression test for B-632).
#[test]
fn dispatch_defer_and_cleanup_resolve_to_keyword() {
    let db = simple_project();
    for name in ["defer", "cleanup"] {
        assert!(
            matches!(dispatch(&db, name), Some(ResolvedTarget::Keyword(_))),
            "`{name}` should resolve to a keyword topic, not fall through to 'No symbol found'"
        );
    }
}

#[test]
fn dispatch_language_topic_resolves_to_keyword() {
    // Language/SDK + pattern topics and CLI-command topics route to keyword
    // docs, not package resolution.
    let db = simple_project();
    for name in [
        "python",
        "typescript",
        "baml_sdk",
        "patterns",
        "pattern",
        "playground",
    ] {
        assert!(
            matches!(dispatch(&db, name), Some(ResolvedTarget::Keyword(_))),
            "`{name}` should resolve to a keyword topic"
        );
    }
}

#[test]
fn dispatch_schema_attributes_and_intrinsic_types_resolve_to_topics() {
    let db = simple_project();
    for name in ["alias", "description", "skip", "void", "never", "unknown"] {
        assert!(
            matches!(dispatch(&db, name), Some(ResolvedTarget::Keyword(_))),
            "`baml describe {name}` should resolve to a shared language topic"
        );
    }
}

#[test]
fn render_schema_attribute_topic() {
    let output = capture_keyword("alias");
    assert!(output.contains("Overrides the serialized and parsed name"));
    assert!(output.contains(r#"@alias("name")"#));
    assert!(output.contains(r#"@@alias("name")"#));
}

#[test]
fn render_keyword_interface() {
    let output = capture_keyword("interface");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_interfaces() {
    let output = capture_keyword("interfaces");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_implements() {
    let output = capture_keyword("implements");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_method() {
    let output = capture_keyword("method");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_field() {
    let output = capture_keyword("field");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_blanket() {
    let output = capture_keyword("blanket");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_check() {
    let output = capture_keyword("check");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_impl() {
    let output = capture_keyword("impl");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_requires() {
    let output = capture_keyword("requires");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_associated() {
    let output = capture_keyword("associated");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_type() {
    let output = capture_keyword("type");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_types() {
    let output = capture_keyword("types");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_generic() {
    let output = capture_keyword("generic");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_generics() {
    let output = capture_keyword("generics");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_bounds() {
    let output = capture_keyword("bounds");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_extends() {
    let output = capture_keyword("extends");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_as() {
    let output = capture_keyword("as");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_projection() {
    let output = capture_keyword("projection");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_self() {
    let output = capture_keyword("self");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_ts_instanceof() {
    let output = capture_keyword("instanceof");
    insta::assert_snapshot!(output);
}

#[test]
fn render_keyword_ts_new() {
    let output = capture_keyword("new");
    insta::assert_snapshot!(output);
}

/// Keywords via dispatch should resolve to `ResolvedTarget::Keyword`.
#[test]
fn dispatch_keyword_class() {
    let db = simple_project();
    let output = describe_via_dispatch(&db, "class");
    insta::assert_snapshot!(output);
}

#[test]
fn dispatch_keyword_method_prefers_topic_over_builtin_member() {
    let db = simple_project();
    let output = describe_via_dispatch(&db, "method");
    assert!(
        output.contains("Declares a function member"),
        "`method` should resolve to the keyword topic, got:\n{output}"
    );
}

#[test]
fn dispatch_keyword_interfaces() {
    let db = simple_project();
    let output = describe_via_dispatch(&db, "interfaces");
    insta::assert_snapshot!(output);
}

#[test]
fn dispatch_keyword_google_ai_documents_enterprise_vertex_delegation() {
    let db = simple_project();
    let output = describe_via_dispatch(&db, "google-ai");
    assert!(
        output.contains("Google AI Studio API")
            && output.contains("gemini-3.5-flash")
            && output.contains("GOOGLE_GENAI_USE_ENTERPRISE=true")
            && output.contains("passthrough to the `vertex-ai` backend"),
        "google-ai documentation must explain enterprise delegation to Vertex AI:\n{output}"
    );
}

#[test]
fn dispatch_keyword_vertex_ai_documents_google_cloud_setup() {
    let db = simple_project();
    let output = describe_via_dispatch(&db, "vertex-ai");
    assert!(
        output.contains("Google Cloud Vertex AI")
            && output.contains("gemini-3.1-pro-preview")
            && output.contains("location \"global\"")
            && output.contains("GOOGLE_CLOUD_PROJECT")
            && output.contains("Application Default Credentials"),
        "vertex-ai documentation must explain Google Cloud setup:\n{output}"
    );
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
