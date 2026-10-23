//! Thread-locality classification (`baml_compiler2_mir::escape`).
//!
//! Each test lowers one function and checks, per named local, whether every
//! object it can hold is thread-local. A wrong `true` is a data race once a
//! backend drops the lock, so most cases pin down a way an object leaves.

use baml_compiler2_hir::item_data::{file_functions, function_data};
use baml_compiler2_mir::{MirFunctionKind, OptLevel, escape, lower_function};
use baml_db::testing::assert_no_diagnostic_errors;

use super::make_db;
use crate::engine::TestDbExt;

/// `(local name, thread-local)` for each named local of `function`, in
/// declaration order. A shadowed name reports its first declaration.
fn classify(source: &str, function: &str) -> Vec<(String, bool)> {
    let mut db = make_db();
    let file = db.file("test.baml", source);
    assert_no_diagnostic_errors(&db);
    let loc = *file_functions(&db, file)
        .iter()
        .find(|&&loc| function_data(&db, loc).name.as_str() == function)
        .unwrap_or_else(|| panic!("no function `{function}`"));
    let mir = lower_function(&db, loc, OptLevel::Two)
        .as_ref()
        .unwrap_or_else(|error| panic!("MIR lowering failed: {error}"));
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        panic!("`{function}` must lower to bytecode")
    };
    let facts = escape::analyze(&db, body, mir.arity);
    let mut seen = std::collections::HashSet::new();
    body.locals
        .iter()
        .enumerate()
        .filter_map(|(index, decl)| {
            let name = decl.name.as_ref()?.to_string();
            seen.insert(name.clone()).then(|| {
                (
                    name,
                    facts.local_is_thread_local(baml_compiler2_mir::Local(index)),
                )
            })
        })
        .collect()
}

#[track_caller]
fn assert_locality(source: &str, function: &str, expected: &[(&str, bool)]) {
    let actual = classify(source, function);
    for (name, local) in expected {
        let found = actual
            .iter()
            .find(|(actual, _)| actual == name)
            .unwrap_or_else(|| panic!("no local `{name}` in {actual:?}"));
        assert_eq!(
            found.1, *local,
            "`{name}` thread-local should be {local}; all: {actual:?}"
        );
    }
}

#[test]
fn fresh_array_built_and_read_is_thread_local() {
    assert_locality(
        r#"
function main() -> int {
  let xs = [1, 2];
  xs.push(3);
  let total = 0;
  for (let x in xs) { total += x; }
  total + xs[0] + xs.length()
}
"#,
        "main",
        &[("xs", true)],
    );
}

#[test]
fn returning_an_object_keeps_it_thread_local() {
    assert_locality(
        r#"
function main() -> int[] {
  let xs = [1];
  xs[0] = 2;
  xs
}
"#,
        "main",
        &[("xs", true)],
    );
}

#[test]
fn parameters_are_shared() {
    assert_locality(
        r#"
function main(ys: int[]) -> int {
  let zs = ys;
  zs[0]
}
"#,
        "main",
        &[("ys", false), ("zs", false)],
    );
}

#[test]
fn passing_to_an_unknown_call_shares_every_alias() {
    assert_locality(
        r#"
function keep(xs: int[]) -> int { xs[0] }
function main() -> int {
  let xs = [1];
  let alias = xs;
  keep(alias)
}
"#,
        "main",
        &[("xs", false), ("alias", false)],
    );
}

#[test]
fn storing_into_a_shared_object_shares_the_value() {
    assert_locality(
        r#"
class Box {
  items: int[]
}
function main(b: Box) -> int {
  let xs = [1];
  b.items = xs;
  xs[0]
}
"#,
        "main",
        &[("xs", false)],
    );
}

#[test]
fn storing_into_a_fresh_object_keeps_both_thread_local() {
    assert_locality(
        r#"
class Box {
  items: int[]
}
function main() -> int {
  let xs = [1];
  let b = Box { items: xs };
  b.items[0] + xs[0]
}
"#,
        "main",
        &[("xs", true), ("b", true)],
    );
}

#[test]
fn leaking_a_loaded_element_shares_what_was_stored() {
    assert_locality(
        r#"
function keep(xs: int[]) -> int { xs[0] }
function main() -> int {
  let inner = [1];
  let outer = [inner];
  let again = outer[0];
  keep(again)
}
"#,
        "main",
        &[("inner", false), ("again", false), ("outer", true)],
    );
}

#[test]
fn pushing_onto_a_shared_array_shares_the_item() {
    assert_locality(
        r#"
function main(out: int[][]) -> int {
  let xs = [1];
  out.push(xs);
  xs[0]
}
"#,
        "main",
        &[("xs", false)],
    );
}

#[test]
fn method_receivers_are_shared_without_summaries() {
    assert_locality(
        r#"
class Point {
  x: int
  function get(self) -> int { self.x }
}
function main() -> int {
  let p = Point { x: 1 };
  let q = Point { x: 2 };
  q.x = 3;
  p.get() + q.x
}
"#,
        "main",
        &[("p", false), ("q", true)],
    );
}

#[test]
fn closures_share_what_they_capture() {
    assert_locality(
        r#"
function main() -> int {
  let xs = [1];
  let f = () -> int { xs[0] };
  f()
}
"#,
        "main",
        &[("xs", false)],
    );
}

#[test]
fn spawned_work_shares_its_plan() {
    assert_locality(
        r#"
function first(xs: int[]) -> int { xs[0] }
function main() -> int {
  let xs = [1];
  let ys = [2];
  let f = spawn { first(xs) };
  ys[0] + await f
}
"#,
        "main",
        &[("xs", false), ("ys", true)],
    );
}

#[test]
fn thrown_values_are_shared() {
    assert_locality(
        r#"
function main() -> int {
  let xs = [1];
  if (xs[0] > 0) { throw xs; }
  0
}
"#,
        "main",
        &[("xs", false)],
    );
}

#[test]
fn string_keyed_maps_stay_thread_local() {
    assert_locality(
        r#"
function main() -> int {
  let m = {"a": 1};
  m["b"] = 2;
  if (m.has("a")) { m["a"] } else { 0 }
}
"#,
        "main",
        &[("m", true)],
    );
}

#[test]
fn map_values_that_leak_share_the_map_contents() {
    assert_locality(
        r#"
function keep(xs: int[]) -> int { xs[0] }
function main() -> int {
  let xs = [1];
  let m = {"a": xs};
  keep(m["a"])
}
"#,
        "main",
        &[("xs", false), ("m", true)],
    );
}

#[test]
fn an_iterator_that_leaks_shares_its_array() {
    assert_locality(
        r#"
function drain<T>(it: T) -> int { 0 }
function main() -> int {
  let xs = [1, 2];
  drain(xs.iter())
}
"#,
        "main",
        &[("xs", false)],
    );
}
