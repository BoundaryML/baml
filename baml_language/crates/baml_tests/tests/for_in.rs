//! What `for (let x in xs)` does, observed from a program: which elements a
//! loop sees when its body changes the array, how often the iterated
//! expression and an iterator's methods run, what a throwing or suspending
//! body leaves behind, and the stack trace of an error that escapes.
//!
//! Each program runs at `OptLevel::One` and at `OptLevel::Two`, where the
//! optimizer may put the iterator's code in the loop. Both must give the
//! pinned answer.

use baml_tests::engine::{OptLevel, run_test};
use bex_external_types::BexExternalValue;
use indexmap::IndexMap;

const SOURCE: &str = r#"
class Point {
  x: int
  y: int
}

function show(xs: int[]) -> string {
  let out = "";
  for (let x in xs) { out = `${out}${x},`; }
  out
}

// A loop over an array whose element type is a type parameter.
function each<T>(xs: T[], visit: (T) -> void throws never) -> void {
  for (let x in xs) { visit(x); }
}

// ── which elements a loop sees ──────────────────────────────────────────────

function plain() -> string {
  let ints = [3, 1, 2];
  let points = [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }];
  let maybe: int?[] = [1, null, 3];
  let none: int[] = [];
  let out = "";
  for (let x in ints) { out = `${out}${x},`; }
  out = `${out}|`;
  for (let p in points) { out = `${out}${p.x + p.y},`; }
  out = `${out}|`;
  for (let m in maybe) { out = `${out}${m ?? -1},`; }
  out = `${out}|`;
  for (let x in none) { out = `${out}never`; }
  out
}
function generic() -> string {
  let log: int[] = [];
  each([3, 1, 2], (x: int) -> void { log.push(x); });
  let names = "";
  each(["a", "b"], (s: string) -> void { names = `${names}${s}`; });
  let maybe: int?[] = [null, 7];
  each(maybe, (m: int?) -> void { log.push(m ?? -1); });
  let none: int[] = [];
  each(none, (x: int) -> void { log.push(99); });
  `${show(log)}|${names}`
}
function break_and_continue() -> string {
  let out = "";
  for (let x in [1, 2, 3, 4, 5, 6]) {
    if (x % 2 == 0) { continue; }
    if (x == 5) { break; }
    out = `${out}${x},`;
  }
  out
}
function nested_same_array() -> string {
  let xs = [1, 2, 3];
  let out = "";
  for (let a in xs) {
    for (let b in xs) { out = `${out}${a * 10 + b},`; }
  }
  out
}

// ── the body changes the array ──────────────────────────────────────────────

function body_appends() -> string {
  let xs = [1, 2, 3];
  let seen: int[] = [];
  for (let x in xs) {
    seen.push(x);
    if (x < 3) { xs.push(x + 10); }
  }
  `${show(seen)}|${show(xs)}`
}
function body_clears() -> string {
  let xs = [1, 2, 3];
  let seen: int[] = [];
  for (let x in xs) {
    seen.push(x);
    if (x == 2) { xs.clear(); }
  }
  `${show(seen)}|${show(xs)}`
}
function body_writes_ahead() -> string {
  let xs = [1, 2, 3];
  let seen: int[] = [];
  for (let x in xs) {
    seen.push(x);
    xs[2] = 30;
  }
  show(seen)
}
function body_rebinds() -> string {
  let xs = [1, 2, 3];
  let seen: int[] = [];
  for (let x in xs) {
    seen.push(x);
    xs = [7, 8, 9, 10];
  }
  `${show(seen)}|${show(xs)}`
}
function generic_body_appends() -> string {
  let xs = [1, 2, 3];
  let seen: int[] = [];
  each(xs, (x: int) -> void {
    seen.push(x);
    if (x < 3) { xs.push(x + 10); }
  });
  `${show(seen)}|${show(xs)}`
}
function generic_body_clears() -> string {
  let xs = [1, 2, 3];
  let seen: int[] = [];
  each(xs, (x: int) -> void {
    seen.push(x);
    if (x == 2) { xs.clear(); }
  });
  `${show(seen)}|${show(xs)}`
}
function for_each_body_changes() -> string {
  let xs = [1, 2, 3];
  let seen: int[] = [];
  xs.for_each((x: int) -> void {
    seen.push(x);
    if (x == 1) { xs.push(4); }
    if (x == 2) { xs[2] = 30; }
  });
  let ys = [1, 2, 3];
  let kept = ys.filter_map((y: int) -> int? {
    ys.clear();
    if (y == 2) { null } else { y * 10 }
  });
  `${show(seen)}|${show(xs)}|${show(kept)}|${show(ys)}`
}

// ── how often things run ────────────────────────────────────────────────────

function expression_runs_once() -> string {
  let log: int[] = [];
  let make = () -> int[] throws never { log.push(0); [1, 2, 3] };
  for (let x in make()) { log.push(x); }
  show(log)
}
function closures_keep_their_own_element() -> string {
  let fns: (() -> int throws never)[] = [];
  for (let x in [1, 2, 3]) {
    fns.push(() -> int { x * 10 });
  }
  let out = "";
  for (let f in fns) { out = `${out}${f()},`; }
  out
}

class Window {
  values: int[]
  pos: int
  end: int
  log: string[]

  implements baml.iter.Iterable {
    type Item = int
    type Error = never

    function iter(self) -> baml.iter.Iterator<Item = int, Error = never> throws never {
      self.log.push("iter");
      self
    }
  }

  implements baml.iter.Iterator {
    type Item = int
    type Error = never

    function next(self) -> int | baml.iter.Done throws never {
      self.log.push("next");
      if (self.pos >= self.end) {
        return baml.iter.Done {};
      }
      let v = self.values[self.pos];
      self.pos += 1;
      v
    }
  }
}

function join(parts: string[]) -> string {
  let out = "";
  for (let part in parts) { out = `${out}${part} `; }
  out
}
function iterator_object() -> string {
  let log: string[] = [];
  let seen: int[] = [];
  for (let v in Window { values: [5, 6, 7, 8], pos: 1, end: 3, log: log }) {
    log.push(`body${v}`);
    seen.push(v);
  }
  `${show(seen)}|${join(log)}`
}
function iterator_object_break() -> string {
  let log: string[] = [];
  let w = Window { values: [5, 6, 7, 8], pos: 0, end: 4, log: log };
  for (let v in w) {
    if (v == 6) { break; }
  }
  let rest: int[] = [];
  for (let v in w) { rest.push(v); }
  `${show(rest)}|${w.pos}|${join(log)}`
}
function iterator_body_moves_it() -> string {
  let w = Window { values: [1, 2, 3, 4, 5, 6], pos: 0, end: 6, log: [] };
  let seen: int[] = [];
  for (let v in w) {
    seen.push(v);
    w.pos += 1;
    if (v == 3) { w.end = 4; }
  }
  show(seen)
}
function iterator_through_interface(it: baml.iter.Iterable<Item = int, Error = never>) -> int {
  let total = 0;
  for (let v in it) { total += v; }
  total
}
function iterator_unknown_class() -> string {
  let a = iterator_through_interface(Window { values: [1, 2, 3], pos: 0, end: 3, log: [] });
  let b = iterator_through_interface([10, 20]);
  let c = iterator_through_interface([1, 2, 3, 4].iter().filter((x: int) -> bool { x % 2 == 0 }));
  `${a}|${b}|${c}`
}
// An iterator local that holds more than one class, or a caller's value.
function either(use_window: bool) -> string {
  let it: baml.iter.Iterator<Item = int, Error = never> = [9, 8].iter();
  if (use_window) {
    it = Window { values: [1, 2, 3], pos: 0, end: 2, log: [] };
  }
  let out = "";
  for (let v in it) { out = `${out}${v},`; }
  out
}
function rebound(it: baml.iter.Iterable<Item = int, Error = never>, swap: bool) -> string {
  if (swap) {
    it = Window { values: [1, 2, 3], pos: 0, end: 2, log: [] };
  }
  let out = "";
  for (let v in it) { out = `${out}${v},`; }
  out
}
function iterator_of_several_classes() -> string {
  `${either(true)}|${either(false)}|${rebound([7, 6], true)}|${rebound([7, 6], false)}`
}
function adapter_chain() -> string {
  let log: int[] = [];
  let xs = [1, 2, 3, 4, 5, 6];
  let total = xs.iter()
    .filter((x: int) -> bool { log.push(x); x % 2 == 0 })
    .map((x: int) -> int { log.push(x * 10); x + 1 })
    .reduce((a: int, x: int) -> int { a + x }, 0);
  let out = "";
  for (let x in xs.iter().map((x: int) -> int { x * 2 }).take(2)) { out = `${out}${x},`; }
  `${total}|${show(log)}|${out}`
}

// ── the body throws ─────────────────────────────────────────────────────────

function throwing_body_stops() -> string {
  let seen: int[] = [];
  let caught = {
    for (let x in [1, 2, 3]) {
      seen.push(x);
      if (x == 2) { throw "two" }
    }
    "none"
  } catch (e) {
    let reason: string => reason,
  };
  `${show(seen)}|${caught}`
}
function sum_checked<T>(xs: T[], weigh: (T) -> int throws string) -> int throws string {
  let total = 0;
  for (let x in xs) {
    total += weigh(x);
  }
  total
}
function escape_plain() -> int {
  let total = 0;
  for (let x in [1, 2, 3]) {
    if (x == 2) {
      throw "plain"
    }
    total += x;
  }
  total
}
function escape_generic() -> int {
  sum_checked([1, 2, 3], (x: int) -> int throws string {
    if (x == 2) {
      throw "generic"
    }
    x
  })
}
function escape_for_each() -> void {
  let xs = [1, 2, 3];
  xs.for_each((x: int) -> void throws string {
    if (x == 2) {
      throw "each"
    }
  });
}
function escape_filter_map() -> int[] {
  let xs = [1, 2, 3];
  xs.filter_map((x: int) -> int? throws string {
    if (x == 2) {
      throw "kept"
    }
    x
  })
}

class Failing {
  left: int

  implements baml.iter.Iterable {
    type Item = int
    type Error = string

    function iter(self) -> baml.iter.Iterator<Item = int, Error = string> throws never {
      self
    }
  }

  implements baml.iter.Iterator {
    type Item = int
    type Error = string

    function next(self) -> int | baml.iter.Done throws string {
      if (self.left == 0) {
        throw "spent"
      }
      self.left -= 1;
      self.left
    }
  }
}
function escape_next() -> int {
  let total = 0;
  for (let v in Failing { left: 2 }) {
    total += v;
  }
  total
}
function caught_next() -> string {
  let seen: int[] = [];
  let caught = {
    for (let v in Failing { left: 2 }) { seen.push(v); }
    "none"
  } catch (e) {
    let reason: string => reason,
  };
  `${show(seen)}|${caught}`
}

// ── the body suspends ───────────────────────────────────────────────────────

function suspend() -> string {
  let xs = [1, 2, 3];
  let out: int[] = [];
  for (let x in xs) {
    let doubled = spawn { x * 2 };
    out.push(await doubled);
    if (x == 1) { xs.push(4); }
  }
  let generic: int[] = [];
  each(xs, (x: int) -> void {
    let tripled = spawn { x * 3 };
    generic.push(await tripled);
  });
  let seen: int[] = [];
  for (let v in Window { values: xs, pos: 0, end: 2, log: [] }) {
    let negated = spawn { 0 - v };
    seen.push(await negated);
  }
  `${show(out)}|${show(generic)}|${show(seen)}`
}
"#;

/// Run `entry` at both levels and return what it gave, which must agree.
async fn both(entry: &str) -> Result<BexExternalValue, String> {
    let at = |opt| async move {
        run_test(SOURCE, entry, IndexMap::new(), opt)
            .await
            .result
            .map_err(|error| error.to_string())
    };
    let out_of_line = at(OptLevel::One).await;
    let optimized = at(OptLevel::Two).await;
    assert_eq!(out_of_line, optimized, "{entry}: O1 against O2");
    out_of_line
}

async fn text(entry: &str) -> String {
    match both(entry).await {
        Ok(BexExternalValue::String(text)) => text.to_string(),
        other => panic!("{entry} returns a string, got {other:?}"),
    }
}

#[tokio::test]
async fn a_loop_visits_each_element_in_order() {
    assert_eq!(text("plain").await, "3,1,2,|3,7,|1,-1,3,|");
    assert_eq!(text("generic").await, "3,1,2,-1,7,|ab");
    assert_eq!(text("break_and_continue").await, "1,3,");
    assert_eq!(
        text("nested_same_array").await,
        "11,12,13,21,22,23,31,32,33,"
    );
}

#[tokio::test]
async fn a_loop_reads_the_array_as_it_is_at_each_step() {
    assert_eq!(text("body_appends").await, "1,2,3,11,12,|1,2,3,11,12,");
    assert_eq!(text("body_clears").await, "1,2,|");
    assert_eq!(text("body_writes_ahead").await, "1,2,30,");
    assert_eq!(text("body_rebinds").await, "1,2,3,|7,8,9,10,");
    assert_eq!(
        text("generic_body_appends").await,
        "1,2,3,11,12,|1,2,3,11,12,"
    );
    assert_eq!(text("generic_body_clears").await, "1,2,|");
}

#[tokio::test]
async fn for_each_and_filter_map_iterate_the_elements_they_were_called_with() {
    assert_eq!(
        text("for_each_body_changes").await,
        "1,2,3,|1,2,30,4,|10,30,|"
    );
}

#[tokio::test]
async fn the_iterated_expression_runs_once_and_each_pass_has_its_own_binding() {
    assert_eq!(text("expression_runs_once").await, "0,1,2,3,");
    assert_eq!(text("closures_keep_their_own_element").await, "10,20,30,");
}

#[tokio::test]
async fn an_iterator_object_is_asked_once_per_pass() {
    assert_eq!(
        text("iterator_object").await,
        "6,7,|iter next body6 next body7 next "
    );
    assert_eq!(
        text("iterator_object_break").await,
        "7,8,|4|iter next next iter next next next "
    );
    assert_eq!(text("iterator_body_moves_it").await, "1,3,");
    assert_eq!(text("iterator_unknown_class").await, "6|30|6");
    assert_eq!(
        text("iterator_of_several_classes").await,
        "1,2,|9,8,|1,2,|7,6,"
    );
    assert_eq!(text("adapter_chain").await, "15|1,2,20,3,4,40,5,6,60,|2,4,");
}

#[tokio::test]
async fn a_throwing_body_stops_the_loop_where_it_threw() {
    assert_eq!(text("throwing_body_stops").await, "1,2,|two");
    assert_eq!(text("caught_next").await, "1,0,|spent");
}

#[tokio::test]
async fn a_body_may_suspend() {
    assert_eq!(text("suspend").await, "2,4,6,8,|3,6,9,12,|-1,-2,");
}

#[tokio::test]
async fn an_escaping_error_reports_the_loop_and_what_it_called() {
    insta::assert_snapshot!(both("escape_plain").await.unwrap_err(), @r#"
    Traceback (most recent call last):
      File "test.baml", line 290, in user.escape_plain
    uncaught throw: "plain"
    "#);
    insta::assert_snapshot!(both("escape_generic").await.unwrap_err(), @r#"
    Traceback (most recent call last):
      File "test.baml", line 297, in user.escape_generic
      File "test.baml", line 282, in user.sum_checked
      File "test.baml", line 299, in .<lambda(escape_generic, 0)>
    uncaught throw: "generic"
    "#);
    insta::assert_snapshot!(both("escape_for_each").await.unwrap_err(), @r#"
    Traceback (most recent call last):
      File "test.baml", line 306, in user.escape_for_each
      File "test.baml", line 308, in .<lambda(escape_for_each, 0)>
    uncaught throw: "each"
    "#);
    insta::assert_snapshot!(both("escape_filter_map").await.unwrap_err(), @r#"
    Traceback (most recent call last):
      File "test.baml", line 314, in user.escape_filter_map
      File "test.baml", line 316, in .<lambda(escape_filter_map, 0)>
    uncaught throw: "kept"
    "#);
    insta::assert_snapshot!(both("escape_next").await.unwrap_err(), @r#"
    Traceback (most recent call last):
      File "test.baml", line 349, in user.escape_next
      File "test.baml", line 340, in user.<(user.Failing as baml.iter.Iterator)>.next
    uncaught throw: "spent"
    "#);
}

/// At `OptLevel::Two` a loop over an iterator whose class the loop's
/// function builds asks that class directly: no interface call is left in it.
#[tokio::test]
async fn a_loop_over_an_iterator_it_builds_makes_no_interface_call() {
    let source = r#"
class Countdown {
  left: int

  implements baml.iter.Iterable {
    type Item = int
    type Error = never

    function iter(self) -> baml.iter.Iterator<Item = int, Error = never> throws never {
      self
    }
  }

  implements baml.iter.Iterator {
    type Item = int
    type Error = never

    function next(self) -> int | baml.iter.Done throws never {
      if (self.left == 0) {
        return baml.iter.Done {};
      }
      self.left -= 1;
      self.left
    }
  }
}

function main() -> int {
  let total = 0;
  for (let v in Countdown { left: 4 }) {
    total += v;
  }
  total
}
"#;
    let optimized = run_test(source, "main", IndexMap::new(), OptLevel::Two).await;
    assert_eq!(optimized.result.unwrap(), BexExternalValue::Int(6));
    let main = optimized
        .bytecode
        .split("function main(")
        .nth(1)
        .and_then(|rest| rest.split("\nfunction ").next())
        .unwrap_or_else(|| panic!("no `main` in:\n{}", optimized.bytecode));
    assert!(!main.contains("virtual_call"), "{main}");
}

/// An optional array narrowed by a null test is an array where the loop is:
/// the loop walks it by index.
#[tokio::test]
async fn a_loop_over_a_narrowed_optional_array_walks_it() {
    let source = r#"
function total(xs: int[]?) -> int {
  let acc = 0;
  if (xs != null) {
    for (let x in xs) {
      acc += x;
    }
  }
  acc
}

function main() -> int {
  total([1, 2, 3]) + total(null)
}
"#;
    for opt in [OptLevel::One, OptLevel::Two] {
        let output = run_test(source, "main", IndexMap::new(), opt).await;
        assert_eq!(output.result.unwrap(), BexExternalValue::Int(6));
        let total = output
            .bytecode
            .split("function total(")
            .nth(1)
            .and_then(|rest| rest.split("\nfunction ").next())
            .unwrap_or_else(|| panic!("no `total` in:\n{}", output.bytecode));
        assert!(total.contains("array_iter_next"), "{total}");
        assert!(!total.contains("virtual_call"), "{total}");
    }
}
