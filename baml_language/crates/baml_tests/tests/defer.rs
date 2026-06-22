//! BEP-042 `defer` runtime tests.
//!
//! Stage 1 covers the static exit edges: normal block fall-through, `return`,
//! `break`/`continue`, and explicit `throw`. (Defers on an exception
//! propagating out of a *call* — the unwind landing pads — are Stage 2.)

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[track_caller]
fn expect_string(v: BexExternalValue) -> String {
    match v {
        BexExternalValue::String(s) => s.to_string(),
        other => panic!("expected String, got {other:?}"),
    }
}

#[tokio::test]
async fn defer_runs_lifo_on_normal_block_exit() {
    // Two defers run last-declared-first at the inner block's exit. Observed via
    // an outer local the defers mutate, read after the inner block.
    let output = baml_test!(
        r#"
function main() -> string {
  let log = "start;"
  {
    defer { log = log + "A;" }
    defer { log = log + "B;" }
    log = log + "body;"
  }
  log
}
"#
    );
    assert_eq!(expect_string(output.result.unwrap()), "start;body;B;A;");
}

#[tokio::test]
async fn defer_is_block_scoped_not_function_scoped() {
    // The defer runs when the inner `{ }` exits, BEFORE code after the inner
    // block — not deferred to function exit.
    let output = baml_test!(
        r#"
function main() -> string {
  let log = ""
  {
    defer { log = log + "inner_defer;" }
    log = log + "inner_body;"
  }
  log = log + "after_inner;"
  log
}
"#
    );
    assert_eq!(
        expect_string(output.result.unwrap()),
        "inner_body;inner_defer;after_inner;"
    );
}

#[tokio::test]
async fn defer_runs_per_loop_iteration() {
    // A defer in a loop body runs at the end of EACH iteration.
    let output = baml_test!(
        r#"
function main() -> string {
  let log = ""
  for (let i in [1, 2, 3]) {
    defer { log = log + "d;" }
    log = log + "i;"
  }
  log
}
"#
    );
    assert_eq!(expect_string(output.result.unwrap()), "i;d;i;d;i;d;");
}

#[tokio::test]
async fn defer_sees_final_value_of_mutated_local() {
    // The deferred block reads the FINAL value of a mutated local at scope exit,
    // not a snapshot from the `defer` site.
    let output = baml_test!(
        r#"
function main() -> string {
  let log = ""
  {
    let status = "pending"
    defer { log = "final:" + status }
    status = "complete"
  }
  log
}
"#
    );
    assert_eq!(expect_string(output.result.unwrap()), "final:complete");
}

#[track_caller]
fn expect_strings(v: BexExternalValue) -> Vec<String> {
    match v {
        BexExternalValue::Array { items, .. } => items
            .into_iter()
            .map(|it| match it {
                BexExternalValue::String(s) => s.to_string(),
                other => panic!("expected String element, got {other:?}"),
            })
            .collect(),
        other => panic!("expected Array, got {other:?}"),
    }
}

// ── Stage 2: defers on an exception propagating out of a call ──────────────

#[tokio::test]
async fn defer_runs_when_called_function_throws() {
    // The BEP's motivating case: a defer runs when an exception propagates out
    // of a *call*, not just an explicit `throw` in the same scope. Observed via
    // a shared array the defer pushes to (read after the catch).
    let output = baml_test!(
        r#"
function boom() -> void { throw "boom" }

function risky(log: string[]) -> void {
  defer { log.push("cleanup") }
  boom()
}

function main() -> string[] {
  let log: string[] = []
  risky(log) catch (e) {
    _ => { log.push("caught") }
  }
  log
}
"#
    );
    assert_eq!(
        expect_strings(output.result.unwrap()),
        vec!["cleanup", "caught"]
    );
}

#[tokio::test]
async fn defer_unwind_is_lifo_with_multiple_defers() {
    // Two armed defers, then a call throws after both: they run last-first.
    let output = baml_test!(
        r#"
function boom() -> void { throw "boom" }

function risky(log: string[]) -> void {
  defer { log.push("a") }
  defer { log.push("b") }
  boom()
}

function main() -> string[] {
  let log: string[] = []
  risky(log) catch (e) {
    _ => {}
  }
  log
}
"#
    );
    assert_eq!(expect_strings(output.result.unwrap()), vec!["b", "a"]);
}

#[tokio::test]
async fn defer_unwind_runs_only_armed_defers() {
    // A throw between two defers runs ONLY the defer armed before it (stair-step
    // regions). `maybe_throw(1)` throws at runtime, but the compiler can't
    // prove it, so the second defer is reachable code.
    let output = baml_test!(
        r#"
function maybe_throw(x: int) -> void {
  if (x > 0) { throw "boom" }
}

function risky(log: string[]) -> void {
  defer { log.push("a") }
  maybe_throw(1)
  defer { log.push("b") }
  log.push("body")
}

function main() -> string[] {
  let log: string[] = []
  risky(log) catch (e) {
    _ => {}
  }
  log
}
"#
    );
    assert_eq!(expect_strings(output.result.unwrap()), vec!["a"]);
}

// ── Stage 1: the remaining static exit paths ───────────────────────────────

#[tokio::test]
async fn defer_runs_on_early_return() {
    // A defer runs when the function `return`s early (observed via a shared
    // array, since the early return skips the rest of the body).
    let output = baml_test!(
        r#"
function helper(log: string[], early: bool) -> int {
  defer { log.push("cleanup") }
  if (early) { return 1 }
  log.push("after")
  2
}

function main() -> string[] {
  let log: string[] = []
  helper(log, true)
  log
}
"#
    );
    assert_eq!(expect_strings(output.result.unwrap()), vec!["cleanup"]);
}

#[tokio::test]
async fn defer_runs_on_break() {
    // The defer in the loop body runs on the iteration that `break`s.
    let output = baml_test!(
        r#"
function main() -> string[] {
  let log: string[] = []
  for (let i in [1, 2, 3]) {
    defer { log.push("d") }
    if (i == 2) { break }
    log.push("i")
  }
  log
}
"#
    );
    // i=1: "i" then "d"; i=2: break → "d".
    assert_eq!(expect_strings(output.result.unwrap()), vec!["i", "d", "d"]);
}

#[tokio::test]
async fn defer_runs_on_continue() {
    // The defer in the loop body runs on the iteration that `continue`s.
    let output = baml_test!(
        r#"
function main() -> string[] {
  let log: string[] = []
  for (let i in [1, 2]) {
    defer { log.push("d") }
    if (i == 1) { continue }
    log.push("body")
  }
  log
}
"#
    );
    // i=1: continue → "d"; i=2: "body" then "d".
    assert_eq!(
        expect_strings(output.result.unwrap()),
        vec!["d", "body", "d"]
    );
}

#[tokio::test]
async fn defer_runs_on_explicit_throw_in_same_function() {
    // An explicit `throw` in the deferring function runs the defer, then the
    // throw propagates to the caller's catch.
    let output = baml_test!(
        r#"
function risky(log: string[]) -> void {
  defer { log.push("cleanup") }
  throw "boom"
}

function main() -> string[] {
  let log: string[] = []
  risky(log) catch (e) {
    _ => { log.push("caught") }
  }
  log
}
"#
    );
    assert_eq!(
        expect_strings(output.result.unwrap()),
        vec!["cleanup", "caught"]
    );
}

#[tokio::test]
async fn defer_that_throws_still_runs_remaining_defers() {
    // A defer that throws mid-unwind: the remaining (earlier-declared) defers
    // still run, and the most-recent throw propagates (replace-semantics).
    let output = baml_test!(
        r#"
function trigger() -> void { throw "X" }

function risky(log: string[]) -> void {
  defer { log.push("a") }                    // runs LAST
  defer { log.push("b"); throw "from_b" }    // runs FIRST, throws
  trigger()                                   // throws "X" -> unwind
}

function main() -> string[] {
  let log: string[] = []
  risky(log) catch (e) {
    _ => { log.push(e) }
  }
  log
}
"#
    );
    // b runs (push "b", throw from_b), a still runs (push "a"), from_b replaces
    // X and is what the caller catches.
    assert_eq!(
        expect_strings(output.result.unwrap()),
        vec!["b", "a", "from_b"]
    );
}

#[tokio::test]
async fn await_inside_defer() {
    // `await` is allowed inside a defer body; it suspends/resumes correctly.
    let output = baml_test!(
        r#"
function work() -> int { 42 }

function main() -> string[] {
  let log: string[] = []
  {
    defer {
      let f = spawn { work() }
      await f
      log.push("awaited")
    }
    log.push("body")
  }
  log
}
"#
    );
    assert_eq!(
        expect_strings(output.result.unwrap()),
        vec!["body", "awaited"]
    );
}
