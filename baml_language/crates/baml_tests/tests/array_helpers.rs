//! What the array helpers that take a closure do, observed from a program:
//! the order of the calls, the snapshot they iterate when the closure
//! changes the array, what a throwing or suspending closure leaves behind,
//! and the run-time types of what they return.
//!
//! Each program runs at `OptLevel::One`, where a helper is called as the
//! standard library implements it, and at `OptLevel::Two`, where the
//! optimizer may put a loop in the caller in its place. Both must give the
//! pinned answer.

use baml_tests::engine::{OptLevel, run_test};
use bex_external_types::BexExternalValue;
use indexmap::IndexMap;

const SOURCE: &str = r#"
class Item {
  id: int
  key: int
}

function show(xs: int[]) -> string {
  let out = "";
  for (let x in xs) { out = `${out}${x},`; }
  out
}

function ids(items: Item[]) -> string {
  let out = "";
  for (let item in items) { out = `${out}${item.id},`; }
  out
}

// ── order of calls ──────────────────────────────────────────────────────────

function order_map() -> string {
  let log: int[] = [];
  let xs = [3, 1, 2];
  let out = xs.map((x: int) -> int { log.push(x); x * 10 });
  `${show(out)}|${show(log)}`
}
function order_filter() -> string {
  let log: int[] = [];
  let xs = [3, 1, 2, 4];
  let out = xs.filter((x: int) -> bool { log.push(x); x % 2 == 0 });
  `${show(out)}|${show(log)}`
}
function order_find() -> string {
  let log: int[] = [];
  let xs = [3, 1, 2, 4];
  let hit = xs.find((x: int) -> bool { log.push(x); x < 3 });
  let miss = xs.find((x: int) -> bool { x > 9 });
  `${hit ?? -1}|${miss ?? -1}|${show(log)}`
}
function order_find_index() -> string {
  let log: int[] = [];
  let xs = [3, 1, 2, 4];
  let hit = xs.find_index((x: int) -> bool { log.push(x); x == 2 });
  let miss = xs.find_index((x: int) -> bool { x > 9 });
  `${hit ?? -1}|${miss ?? -1}|${show(log)}`
}
function order_some_every() -> string {
  let log: int[] = [];
  let xs = [3, 1, 2, 4];
  let any = xs.some((x: int) -> bool { log.push(x); x == 1 });
  let all = xs.every((x: int) -> bool { log.push(x * 10); x > 1 });
  `${any}|${all}|${show(log)}`
}
function order_reduce() -> string {
  let log: int[] = [];
  let xs = [3, 1, 2];
  let total = xs.reduce((a: int, x: int) -> int { log.push(a); a * 10 + x }, 7);
  `${total}|${show(log)}`
}
function order_flat_map() -> string {
  let log: int[] = [];
  let xs = [3, 1, 2];
  let out = xs.flat_map((x: int) -> int[] { log.push(x); [x, x + 100] });
  `${show(out)}|${show(log)}`
}
function order_for_each() -> string {
  let log: int[] = [];
  let xs = [3, 1, 2];
  xs.for_each((x: int) -> void { log.push(x); });
  show(log)
}
function order_sort_by() -> string {
  // The comparisons made, in order, as `left*10+right` of the ids compared,
  // and a stable result: equal keys keep their order.
  let log: int[] = [];
  let items = [
    Item { id: 1, key: 5 }, Item { id: 2, key: 3 }, Item { id: 3, key: 5 },
    Item { id: 4, key: 1 }, Item { id: 5, key: 3 },
  ];
  let sorted = items.sort_by((a: Item, b: Item) -> baml.ops.Ordering {
    log.push(a.id * 10 + b.id);
    if (a.key < b.key) {
      baml.ops.Ordering.Less
    } else if (a.key > b.key) {
      baml.ops.Ordering.Greater
    } else {
      baml.ops.Ordering.Equal
    }
  });
  `${ids(sorted)}|${ids(items)}|${show(log)}`
}

// ── the closure changes the array ───────────────────────────────────────────

function mutate_map() -> string {
  let xs = [1, 2, 3];
  let out = xs.map((x: int) -> int { xs.push(x + 100); x * 2 });
  `${show(out)}|${show(xs)}`
}
function mutate_filter() -> string {
  let xs = [1, 2, 3, 4];
  let out = xs.filter((x: int) -> bool { xs.clear(); x > 1 });
  `${show(out)}|${show(xs)}`
}
function mutate_find() -> string {
  let xs = [1, 2, 3];
  let hit = xs.find((x: int) -> bool { xs.clear(); x == 3 });
  `${hit ?? -1}|${show(xs)}`
}
function mutate_reduce() -> string {
  let xs = [1, 2, 3];
  let total = xs.reduce((a: int, x: int) -> int { xs.push(9); a + x }, 0);
  `${total}|${show(xs)}`
}
function mutate_some_every() -> string {
  let xs = [1, 2, 3];
  let any = xs.some((x: int) -> bool { xs.clear(); x == 3 });
  let ys = [1, 2, 3];
  let all = ys.every((x: int) -> bool { ys.clear(); x < 9 });
  `${any}|${all}|${show(xs)}|${show(ys)}`
}
function mutate_flat_map() -> string {
  let xs = [1, 2];
  let out = xs.flat_map((x: int) -> int[] { xs.push(7); [x, x] });
  `${show(out)}|${show(xs)}`
}
function mutate_for_each() -> string {
  let seen: int[] = [];
  let xs = [1, 2, 3];
  xs.for_each((x: int) -> void { xs.clear(); seen.push(x); });
  `${show(seen)}|${show(xs)}`
}
function mutate_sort_by() -> string {
  // The comparator empties the array; the sort still writes back what it
  // read at the start, sorted.
  let xs = [3, 1, 2];
  let sorted = xs.sort_by((a: int, b: int) -> baml.ops.Ordering {
    xs.clear();
    if (a < b) { baml.ops.Ordering.Less } else if (a > b) { baml.ops.Ordering.Greater } else { baml.ops.Ordering.Equal }
  });
  `${show(sorted)}|${show(xs)}`
}

// ── the closure throws ──────────────────────────────────────────────────────

function throw_map() -> string {
  let log: int[] = [];
  let xs = [1, 2, 3, 4];
  let out = xs.map((x: int) -> int throws string {
    log.push(x);
    if (x == 3) { throw "three" }
    x
  }) catch (e) { _ => [-1] };
  `${show(out)}|${show(log)}|${show(xs)}`
}
function throw_filter() -> string {
  let log: int[] = [];
  let xs = [1, 2, 3, 4];
  let out = xs.filter((x: int) -> bool throws string {
    log.push(x);
    if (x == 3) { throw "three" }
    true
  }) catch (e) { _ => [-1] };
  `${show(out)}|${show(log)}`
}
function throw_reduce() -> string {
  let log: int[] = [];
  let xs = [1, 2, 3, 4];
  let total = xs.reduce((a: int, x: int) -> int throws string {
    log.push(x);
    if (x == 3) { throw "three" }
    a + x
  }, 0) catch (e) { _ => -1 };
  `${total}|${show(log)}`
}
function throw_sort_by() -> string {
  // A comparator that throws leaves the array as it was.
  let calls = [0];
  let xs = [3, 1, 2, 0];
  let sorted = xs.sort_by((a: int, b: int) -> baml.ops.Ordering throws string {
    calls[0] = calls[0] + 1;
    if (calls[0] == 3) { throw "third" }
    if (a < b) { baml.ops.Ordering.Less } else if (a > b) { baml.ops.Ordering.Greater } else { baml.ops.Ordering.Equal }
  }) catch (e) { _ => [-1] };
  `${show(sorted)}|${show(xs)}|${calls[0]}`
}

// ── run-time types of the results ───────────────────────────────────────────

function kind(value: unknown) -> string {
  if (value is int[]) { "int[]" }
  else if (value is string[]) { "string[]" }
  else if (value is (int | string)[]) { "(int|string)[]" }
  else if (value is Item[]) { "Item[]" }
  else { "other" }
}
function types() -> string {
  let xs = [1, 2, 3];
  let none: int[] = [];
  let mixed: (int | string)[] = [1, "a"];
  let a = kind(xs.map((x: int) -> string { `${x}` }));
  let b = kind(none.map((x: int) -> string { `${x}` }));
  let c = kind(xs.filter((x: int) -> bool { x > 1 }));
  let d = kind(none.filter((x: int) -> bool { true }));
  let e = kind(mixed.filter((x: int | string) -> bool { true }));
  let f = kind(xs.flat_map((x: int) -> string[] { [`${x}`] }));
  let g = kind(xs.map((x: int) -> Item { Item { id: x, key: x } }));
  `${a}|${b}|${c}|${d}|${e}|${f}|${g}`
}

// ── empty arrays ────────────────────────────────────────────────────────────

function empty() -> string {
  let calls = [0];
  let none: int[] = [];
  let a = none.map((x: int) -> int { calls[0] = calls[0] + 1; x });
  let b = none.filter((x: int) -> bool { calls[0] = calls[0] + 1; true });
  let c = none.find((x: int) -> bool { calls[0] = calls[0] + 1; true });
  let d = none.reduce((acc: int, x: int) -> int { calls[0] = calls[0] + 1; acc + x }, 42);
  let e = none.some((x: int) -> bool { calls[0] = calls[0] + 1; true });
  let f = none.every((x: int) -> bool { calls[0] = calls[0] + 1; false });
  let g = none.sort_by((x: int, y: int) -> baml.ops.Ordering { calls[0] = calls[0] + 1; baml.ops.Ordering.Equal });
  `${a.length()}|${b.length()}|${c ?? -1}|${d}|${e}|${f}|${g.length()}|${calls[0]}`
}

// ── the closure suspends ────────────────────────────────────────────────────

function suspend() -> string {
  let xs = [1, 2, 3];
  let out = xs.map((x: int) -> int {
    let doubled = spawn { x * 2 };
    await doubled
  });
  let kept = xs.filter((x: int) -> bool {
    let odd = spawn { x % 2 == 1 };
    await odd
  });
  `${show(out)}|${show(kept)}`
}

// ── an error that escapes ───────────────────────────────────────────────────

function escape_map() -> int[] {
  let xs = [1, 2, 3];
  xs.map((x: int) -> int throws string {
    if (x == 2) {
      throw "two"
    }
    x
  })
}
function escape_sort_by() -> int[] {
  let xs = [2, 1];
  xs.sort_by((a: int, b: int) -> baml.ops.Ordering throws string {
    throw "compare"
  })
}
function escape_reduce() -> int {
  let xs = [1, 2, 3];
  xs.reduce((a: int, x: int) -> int throws string {
    if (x == 2) {
      throw "two"
    }
    a + x
  }, 0)
}
function escape_find() -> int? {
  let xs = [1, 2, 3];
  xs.filter((x: int) -> bool { x > 1 }).find((x: int) -> bool throws string {
    throw "found"
  })
}

function order_find_last() -> string {
  let log: int[] = [];
  let xs = [3, 1, 2, 4];
  let hit = xs.find_last((x: int) -> bool { log.push(x); x < 3 });
  let at = xs.find_last_index((x: int) -> bool { log.push(x * 10); x == 1 });
  let miss = xs.find_last((x: int) -> bool { x > 9 });
  let none = xs.find_last_index((x: int) -> bool { x > 9 });
  `${hit ?? -1}|${at ?? -1}|${miss ?? -1}|${none ?? -1}|${show(log)}`
}
function mutate_find_last() -> string {
  let xs = [1, 2, 3];
  let hit = xs.find_last((x: int) -> bool { xs.clear(); x == 1 });
  let ys = [1, 2, 3];
  let at = ys.find_last_index((x: int) -> bool { ys.clear(); x == 1 });
  `${hit ?? -1}|${at ?? -1}|${show(xs)}|${show(ys)}`
}
function sort_by_long() -> string {
  // 37 items: an odd run at the end of every pass. The comparator counts its
  // calls and folds each pair it is given into a checksum, so a different
  // order of comparisons gives a different answer.
  let items: Item[] = [];
  let seed = 11;
  for (let i = 0; i < 37; i += 1) {
    seed = seed * 16807 % 2147483647;
    items.push(Item { id: i, key: seed % 9 });
  }
  let state = [0, 0];
  let sorted = items.sort_by((a: Item, b: Item) -> baml.ops.Ordering {
    state[0] = state[0] + 1;
    state[1] = (state[1] * 31 + a.id * 37 + b.id) % 1000003;
    if (a.key < b.key) {
      baml.ops.Ordering.Less
    } else if (a.key > b.key) {
      baml.ops.Ordering.Greater
    } else {
      baml.ops.Ordering.Equal
    }
  });
  `${ids(sorted)}|${state[0]}|${state[1]}`
}
function sort_by_small() -> string {
  let none: int[] = [];
  let one = [5];
  let two = [2, 1];
  let cmp = 0;
  let a = none.sort_by((x: int, y: int) -> baml.ops.Ordering { cmp += 1; baml.ops.Ordering.Greater });
  let b = one.sort_by((x: int, y: int) -> baml.ops.Ordering { cmp += 1; baml.ops.Ordering.Greater });
  let c = two.sort_by((x: int, y: int) -> baml.ops.Ordering { cmp += 1; baml.ops.Ordering.Greater });
  `${show(a)}|${show(b)}|${show(c)}|${show(two)}|${cmp}`
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
async fn closures_are_called_in_element_order() {
    assert_eq!(text("order_map").await, "30,10,20,|3,1,2,");
    assert_eq!(text("order_filter").await, "2,4,|3,1,2,4,");
    assert_eq!(text("order_find").await, "1|-1|3,1,");
    assert_eq!(text("order_find_index").await, "2|-1|3,1,2,");
    assert_eq!(text("order_some_every").await, "true|false|3,1,30,10,");
    assert_eq!(text("order_reduce").await, "7312|7,73,731,");
    assert_eq!(text("order_flat_map").await, "3,103,1,101,2,102,|3,1,2,");
    assert_eq!(text("order_for_each").await, "3,1,2,");
}

#[tokio::test]
async fn sort_by_compares_in_merge_order_and_is_stable() {
    assert_eq!(
        text("order_sort_by").await,
        "4,2,5,1,3,|4,2,5,1,3,|12,34,24,23,13,45,25,15,"
    );
    assert_eq!(
        text("sort_by_long").await,
        "7,8,14,9,18,2,11,19,26,27,33,4,6,12,22,23,28,1,21,30,5,10,17,31,36,35,13,16,25,29,0,3,15,20,24,32,34,|164|6185"
    );
    assert_eq!(text("sort_by_small").await, "|5,|1,2,|1,2,|1");
    assert_eq!(text("order_find_last").await, "2|1|-1|-1|4,2,40,20,10,");
    assert_eq!(text("mutate_find_last").await, "1|0||");
}

#[tokio::test]
async fn a_helper_iterates_the_elements_it_was_called_with() {
    assert_eq!(text("mutate_map").await, "2,4,6,|1,2,3,101,102,103,");
    assert_eq!(text("mutate_filter").await, "2,3,4,|");
    assert_eq!(text("mutate_find").await, "3|");
    assert_eq!(text("mutate_reduce").await, "6|1,2,3,9,9,9,");
    assert_eq!(text("mutate_some_every").await, "true|true||");
    assert_eq!(text("mutate_flat_map").await, "1,1,2,2,|1,2,7,7,");
    assert_eq!(text("mutate_for_each").await, "1,2,3,|");
    assert_eq!(text("mutate_sort_by").await, "1,2,3,|1,2,3,");
}

#[tokio::test]
async fn a_throwing_closure_stops_the_helper_where_it_threw() {
    assert_eq!(text("throw_map").await, "-1,|1,2,3,|1,2,3,4,");
    assert_eq!(text("throw_filter").await, "-1,|1,2,3,");
    assert_eq!(text("throw_reduce").await, "-1|1,2,3,");
    assert_eq!(text("throw_sort_by").await, "-1,|3,1,2,0,|3");
}

#[tokio::test]
async fn results_have_the_element_types_the_call_gave_them() {
    assert_eq!(
        text("types").await,
        "string[]|string[]|int[]|int[]|(int|string)[]|string[]|Item[]"
    );
}

#[tokio::test]
async fn an_empty_array_never_calls_the_closure() {
    assert_eq!(text("empty").await, "0|0|-1|42|false|true|0|0");
}

#[tokio::test]
async fn a_closure_may_suspend() {
    assert_eq!(text("suspend").await, "2,4,6,|1,3,");
}

#[tokio::test]
async fn an_escaping_error_reports_the_caller_and_the_closure() {
    insta::assert_snapshot!(both("escape_map").await.unwrap_err(), @r#"
    Traceback (most recent call last):
      File "test.baml", line 245, in user.escape_map
      File "", line 0, in baml.Array.map
      File "test.baml", line 247, in .<lambda(escape_map, 0)>
    uncaught throw: "two"
    "#);
    insta::assert_snapshot!(both("escape_sort_by").await.unwrap_err(), @r#"
    Traceback (most recent call last):
      File "test.baml", line 254, in user.escape_sort_by
      File "", line 0, in baml.Array.sort_by
      File "test.baml", line 255, in .<lambda(escape_sort_by, 0)>
    uncaught throw: "compare"
    "#);
    insta::assert_snapshot!(both("escape_reduce").await.unwrap_err(), @r#"
    Traceback (most recent call last):
      File "test.baml", line 260, in user.escape_reduce
      File "", line 0, in baml.Array.reduce
      File "test.baml", line 262, in .<lambda(escape_reduce, 0)>
    uncaught throw: "two"
    "#);
    insta::assert_snapshot!(both("escape_find").await.unwrap_err(), @r#"
    Traceback (most recent call last):
      File "test.baml", line 269, in user.escape_find
      File "", line 0, in baml.Array.find
      File "test.baml", line 270, in .<lambda(escape_find, 1)>
    uncaught throw: "found"
    "#);
}
