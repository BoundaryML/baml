//! The operands a spread in a list or map literal rejects, one test each.
//!
//! `[...operand]` needs an operand that is statically a list, and
//! `{ ...operand }` one that is statically a map, after alias expansion.
//! Everything else is E0190 and the program does not compile: nothing is
//! spread dynamically, and `null` is never read as an empty container. The
//! accepted forms and their types are pinned by the caret fixtures
//! `type_spec/fixtures/{list_spread_element_types,map_spread_entry_types}.baml`;
//! runtime behavior by `baml_src/ns_list_map_spread/`.
//!
//! These are Rust tests because a program that must not compile cannot join
//! the shared corpus, and each asserts on one diagnostic's id and text.

use baml_compiler_diagnostics::DiagnosticId;

use super::support::make_db;
use crate::engine::TestDbExt;

/// Every diagnostic of `source`: its id and message.
fn diagnostics(source: &str) -> Vec<(DiagnosticId, String)> {
    let mut db = make_db();
    db.file("test.baml", source);
    baml_db::collect_compiler2_diagnostics(&db)
        .iter()
        .map(|diag| (diag.id, diag.message_with_primary_label().into_owned()))
        .collect()
}

/// `source` is rejected with exactly one diagnostic: E0190, naming the
/// operand's type `operand_ty` and the container `noun` it was spread into.
#[track_caller]
fn assert_invalid_operand(source: &str, operand_ty: &str, noun: &str) {
    let diags = diagnostics(source);
    let expected = format!(
        "cannot spread a value of type `{operand_ty}` into a {noun}: the operand of `...` must be a {noun}"
    );
    assert!(
        matches!(
            diags.as_slice(),
            [(DiagnosticId::InvalidSpreadOperand, message)] if message.starts_with(&expected)
        ),
        "expected exactly one E0190 starting with {expected:?}, got {diags:#?}"
    );
    assert_eq!(DiagnosticId::InvalidSpreadOperand.code(), "E0190");
}

/// `source` is rejected with exactly one diagnostic: a type mismatch whose
/// message contains `needle`.
#[track_caller]
fn assert_mismatch(source: &str, needle: &str) {
    let diags = diagnostics(source);
    assert!(
        matches!(
            diags.as_slice(),
            [(DiagnosticId::TypeMismatch, message)] if message.contains(needle)
        ),
        "expected exactly one type mismatch containing {needle:?}, got {diags:#?}"
    );
}

#[track_caller]
fn assert_accepted(source: &str) {
    let diags = diagnostics(source);
    assert!(diags.is_empty(), "expected no diagnostics, got {diags:#?}");
}

// ─── Lists ───────────────────────────────────────────────────────────────────

#[test]
fn list_spread_rejects_a_nullable_list() {
    assert_invalid_operand(
        "function f(xs: int[]?) -> int[] { [...xs] }",
        "int[] | null",
        "list",
    );
    // No expectation changes nothing: the operand is judged by itself.
    assert_invalid_operand(
        "function f(xs: int[]?) -> int { let ys = [1, ...xs]; 0 }",
        "int[] | null",
        "list",
    );
}

#[test]
fn list_spread_of_a_nullable_list_names_the_remedy() {
    let diags = diagnostics("function f(xs: int[]?) -> int[] { [...xs] }");
    assert!(
        diags[0].1.contains("`null` is not spread as an empty list"),
        "{diags:#?}"
    );
    // Either remedy the message names compiles.
    assert_accepted("function f(xs: int[]?) -> int[] { [...(xs ?? [])] }");
    assert_accepted("function f(xs: int[]?) -> int[] { if (xs != null) { [...xs] } else { [] } }");
}

#[test]
fn list_spread_rejects_a_union_with_a_non_list_member() {
    assert_invalid_operand(
        "function f(xs: int[] | string) -> int[] { [...xs] }",
        "string | int[]",
        "list",
    );
}

#[test]
fn list_spread_rejects_a_union_of_lists() {
    // Sound in principle, but the operand is not statically ONE list type:
    // the rule is kept to a single list, and the author narrows first.
    assert_invalid_operand(
        "function f(xs: int[] | string[]) -> (int | string)[] { [...xs] }",
        "int[] | string[]",
        "list",
    );
}

#[test]
fn list_spread_rejects_unknown() {
    assert_invalid_operand(
        "function f(xs: unknown) -> unknown[] { [...xs] }",
        "unknown",
        "list",
    );
}

#[test]
fn list_spread_rejects_json() {
    // `baml.json.json` is a union with non-list members (and a list one).
    assert_invalid_operand(
        "function f(xs: baml.json.json) -> baml.json.json[] { [...xs] }",
        "baml.json.json",
        "list",
    );
    // Narrowed to its list arm, it is a list.
    assert_accepted(
        r#"function f(xs: baml.json.json) -> baml.json.json[] {
    match (xs) {
        let items: baml.json.json[] => [...items],
        _ => [],
    }
}"#,
    );
}

#[test]
fn list_spread_rejects_a_recursive_union_alias() {
    assert_invalid_operand(
        "type Tree = int | Tree[];\nfunction f(xs: Tree) -> Tree[] { [...xs] }",
        "Tree",
        "list",
    );
}

#[test]
fn list_spread_rejects_a_map() {
    assert_invalid_operand(
        "function f(m: map<string, int>) -> int[] { [...m] }",
        "map<string, int>",
        "list",
    );
}

#[test]
fn list_spread_rejects_a_class_instance() {
    assert_invalid_operand(
        "class P { x: int }\nfunction f(p: P) -> int[] { [...p] }",
        "P",
        "list",
    );
}

#[test]
fn list_spread_rejects_a_scalar_and_a_string() {
    assert_invalid_operand("function f(x: int) -> int[] { [0, ...x] }", "int", "list");
    // A string is iterable, but it is no list.
    assert_invalid_operand(
        "function f(s: string) -> string[] { [...s] }",
        "string",
        "list",
    );
}

#[test]
fn list_spread_rejects_a_type_variable() {
    assert_invalid_operand("function f<T>(xs: T) -> unknown[] { [...xs] }", "T", "list");
    // A bound is an interface, and no interface proves its implementor is a
    // list: an `Iterable` is still not statically a list.
    assert_invalid_operand(
        "function f<T extends baml.iter.Iterable<Item = int>>(xs: T) -> unknown[] { [...xs] }",
        "T",
        "list",
    );
    // A list OF a type variable is a list.
    assert_accepted("function f<T>(xs: T[]) -> T[] { [...xs] }");
}

#[test]
fn list_spread_rejects_an_interface_existential() {
    assert_invalid_operand(
        "function f(xs: baml.iter.Iterable<Item = int>) -> int[] { [...xs] }",
        "baml.iter.Iterable<Error = never, Item = int>",
        "list",
    );
}

#[test]
fn list_spread_checks_the_element_type_against_the_expectation() {
    assert_mismatch(
        "function f(xs: string[]) -> int[] { [1, ...xs] }",
        "expected `int`, found `string`: every element of the spread list",
    );
    // A literal element type is not subverted: a `string[]` operand does not
    // fit a list of two string literals.
    assert_mismatch(
        r#"function f(xs: string[]) -> ("a" | "b")[] { ["a", ...xs] }"#,
        r#"expected `"a" | "b"`, found `string`"#,
    );
}

#[test]
fn list_spread_does_not_relax_invariance_of_the_operand_itself() {
    assert_mismatch(
        r#"interface Animal { function name(self) -> string throws never }
class Dog { implements Animal { function name(self) -> string throws never { "d" } } }
function f(dogs: Dog[]) -> Animal[] { dogs }"#,
        "expected `Animal[]`, found `Dog[]`",
    );
    assert_accepted(
        r#"interface Animal { function name(self) -> string throws never }
class Dog { implements Animal { function name(self) -> string throws never { "d" } } }
function f(dogs: Dog[]) -> Animal[] { [...dogs] }"#,
    );
}

#[test]
fn list_spread_operand_throws_must_be_declared() {
    let diags = diagnostics(
        r#"function fails() -> int[] throws string { throw "no" }
function f() -> int[] throws never { [0, ...fails()] }"#,
    );
    assert_eq!(diags.len(), 1, "{diags:#?}");
    assert!(diags[0].1.contains("string"), "{diags:#?}");
}

#[test]
fn an_operand_that_fails_to_type_reports_once() {
    // The unresolved name is the one diagnostic: no second report says its
    // (error) type is no list.
    let diags = diagnostics("function f() -> int[] { [...missing] }");
    assert_eq!(diags.len(), 1, "{diags:#?}");
    assert_ne!(diags[0].0, DiagnosticId::InvalidSpreadOperand, "{diags:#?}");
    let diags = diagnostics("function f() -> map<string, int> { { ...missing } }");
    assert_eq!(diags.len(), 1, "{diags:#?}");
    assert_ne!(diags[0].0, DiagnosticId::InvalidSpreadOperand, "{diags:#?}");
}

// ─── Maps ────────────────────────────────────────────────────────────────────

#[test]
fn map_spread_rejects_a_nullable_map() {
    assert_invalid_operand(
        "function f(m: map<string, int>?) -> map<string, int> { { ...m } }",
        "map<string, int> | null",
        "map",
    );
    assert_accepted("function f(m: map<string, int>?) -> map<string, int> { { ...(m ?? {}) } }");
}

#[test]
fn map_spread_rejects_a_union_with_a_non_map_member() {
    assert_invalid_operand(
        "function f(m: map<string, int> | int) -> map<string, int> { { ...m } }",
        "int | map<string, int>",
        "map",
    );
}

#[test]
fn map_spread_rejects_unknown() {
    assert_invalid_operand(
        "function f(m: unknown) -> map<string, unknown> { { ...m } }",
        "unknown",
        "map",
    );
}

#[test]
fn map_spread_rejects_json() {
    assert_invalid_operand(
        "function f(m: baml.json.json) -> map<string, baml.json.json> { { ...m } }",
        "baml.json.json",
        "map",
    );
    assert_accepted(
        r#"function f(m: baml.json.json) -> map<string, baml.json.json> {
    match (m) {
        let entries: map<string, baml.json.json> => ({ ...entries }),
        _ => ({}),
    }
}"#,
    );
}

#[test]
fn map_spread_rejects_a_class_instance() {
    // A class instance spreads only into a constructor of its own class.
    assert_invalid_operand(
        "class P { x: int }\nfunction f(p: P) -> map<string, int> { { ...p } }",
        "P",
        "map",
    );
    assert_accepted("class P { x: int, y: int }\nfunction f(p: P) -> P { P { ...p, y: 5 } }");
}

#[test]
fn map_spread_rejects_a_list() {
    assert_invalid_operand(
        "function f(xs: int[]) -> map<string, int> { { ...xs } }",
        "int[]",
        "map",
    );
}

#[test]
fn map_spread_rejects_a_type_variable() {
    assert_invalid_operand(
        r#"function f<T>(m: T) -> map<string, unknown> { { "a": 1, ...m } }"#,
        "T",
        "map",
    );
    assert_accepted(
        "function f<K extends baml.Hash & baml.ops.Equals, V>(m: map<K, V>) -> map<K, V> { { ...m } }",
    );
}

#[test]
fn map_spread_checks_key_and_value_types_against_the_expectation() {
    assert_mismatch(
        r#"function f(m: map<string, string>) -> map<string, int> { { "a": 1, ...m } }"#,
        "expected `int`, found `string`: every key and value of the spread map",
    );
    assert_mismatch(
        "function f(m: map<int, int>) -> map<string, int> { { ...m } }",
        "expected `string`, found `int`",
    );
    // A literal key type is not subverted: a `string`-keyed operand does not
    // fit a map keyed by two string literals.
    assert_mismatch(
        r#"function f(m: map<string, int>) -> map<"a" | "b", int> { { "a": 1, ...m } }"#,
        r#"expected `"a" | "b"`, found `string`"#,
    );
}

#[test]
fn map_spread_operand_throws_must_be_declared() {
    let diags = diagnostics(
        r#"function fails() -> map<string, int> throws string { throw "no" }
function f() -> map<string, int> throws never { { "a": 0, ...fails() } }"#,
    );
    assert_eq!(diags.len(), 1, "{diags:#?}");
    assert!(diags[0].1.contains("string"), "{diags:#?}");
}
