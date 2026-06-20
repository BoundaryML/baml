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
