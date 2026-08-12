// BEP-042 `defer` runtime tests.
//
// Functions use per-test prefixes so they coexist in one corpus namespace.
// Test bodies with locals live in top-level helper functions (test-block
// local-boxing VM bug).
//
// Stage 1 covers the static exit edges: normal block fall-through, `return`,
// `break`/`continue`, and explicit `throw`. Stage 2 covers defers on an
// exception propagating out of a *call* (the unwind landing pads).

// ─── Stage 1: normal block exit, scoping, loops, mutation visibility ─────────

// Two defers run last-declared-first at the inner block's exit. Observed via
// an outer local the defers mutate, read after the inner block.
function defer_lifo_normal_exit_main() -> string {
  let log = "start;"
  {
    defer { log = log + "A;" }
    defer { log = log + "B;" }
    log = log + "body;"
  }
  log
}

test "defer_runs_lifo_on_normal_block_exit" {
    assert.equal(defer_lifo_normal_exit_main(), "start;body;B;A;")
}

// The defer runs when the inner `{ }` exits, BEFORE code after the inner
// block — not deferred to function exit.
function defer_block_scoped_main() -> string {
  let log = ""
  {
    defer { log = log + "inner_defer;" }
    log = log + "inner_body;"
  }
  log = log + "after_inner;"
  log
}

test "defer_is_block_scoped_not_function_scoped" {
    assert.equal(defer_block_scoped_main(), "inner_body;inner_defer;after_inner;")
}

// A defer in a loop body runs at the end of EACH iteration.
function defer_per_loop_iteration_main() -> string {
  let log = ""
  for (let i in [1, 2, 3]) {
    defer { log = log + "d;" }
    log = log + "i;"
  }
  log
}

test "defer_runs_per_loop_iteration" {
    assert.equal(defer_per_loop_iteration_main(), "i;d;i;d;i;d;")
}

// The deferred block reads the FINAL value of a mutated local at scope exit,
// not a snapshot from the `defer` site.
function defer_sees_final_value_main() -> string {
  let log = ""
  {
    let status = "pending"
    defer { log = "final:" + status }
    status = "complete"
  }
  log
}

test "defer_sees_final_value_of_mutated_local" {
    assert.equal(defer_sees_final_value_main(), "final:complete")
}

// ─── Stage 2: defers on an exception propagating out of a call ───────────────

// The BEP's motivating case: a defer runs when an exception propagates out
// of a *call*, not just an explicit `throw` in the same scope. Observed via
// a shared array the defer pushes to (read after the catch).
function defer_call_throws_boom() -> void { throw "boom" }

function defer_call_throws_risky(log: string[]) -> void {
  defer { log.push("cleanup") }
  defer_call_throws_boom()
}

function defer_call_throws_main() -> string[] {
  let log: string[] = []
  defer_call_throws_risky(log) catch (e) {
    _ => { log.push("caught") }
  }
  log
}

test "defer_runs_when_called_function_throws" {
    assert.equal(defer_call_throws_main(), ["cleanup", "caught"])
}

// Two armed defers, then a call throws after both: they run last-first.
function defer_unwind_lifo_boom() -> void { throw "boom" }

function defer_unwind_lifo_risky(log: string[]) -> void {
  defer { log.push("a") }
  defer { log.push("b") }
  defer_unwind_lifo_boom()
}

function defer_unwind_lifo_main() -> string[] {
  let log: string[] = []
  defer_unwind_lifo_risky(log) catch (e) {
    _ => {}
  }
  log
}

test "defer_unwind_is_lifo_with_multiple_defers" {
    assert.equal(defer_unwind_lifo_main(), ["b", "a"])
}

// A throw between two defers runs ONLY the defer armed before it (stair-step
// regions). `maybe_throw(1)` throws at runtime, but the compiler can't
// prove it, so the second defer is reachable code.
function defer_armed_only_maybe_throw(x: int) -> void {
  if (x > 0) { throw "boom" }
}

function defer_armed_only_risky(log: string[]) -> void {
  defer { log.push("a") }
  defer_armed_only_maybe_throw(1)
  defer { log.push("b") }
  log.push("body")
}

function defer_armed_only_main() -> string[] {
  let log: string[] = []
  defer_armed_only_risky(log) catch (e) {
    _ => {}
  }
  log
}

test "defer_unwind_runs_only_armed_defers" {
    assert.equal(defer_armed_only_main(), ["a"])
}

// ─── Stage 1: the remaining static exit paths ────────────────────────────────

// A defer runs when the function `return`s early (observed via a shared
// array, since the early return skips the rest of the body).
function defer_early_return_helper(log: string[], early: bool) -> int {
  defer { log.push("cleanup") }
  if (early) { return 1 }
  log.push("after")
  2
}

function defer_early_return_main() -> string[] {
  let log: string[] = []
  defer_early_return_helper(log, true)
  log
}

test "defer_runs_on_early_return" {
    assert.equal(defer_early_return_main(), ["cleanup"])
}

// The defer in the loop body runs on the iteration that `break`s.
// i=1: "i" then "d"; i=2: break → "d".
function defer_on_break_main() -> string[] {
  let log: string[] = []
  for (let i in [1, 2, 3]) {
    defer { log.push("d") }
    if (i == 2) { break }
    log.push("i")
  }
  log
}

test "defer_runs_on_break" {
    assert.equal(defer_on_break_main(), ["i", "d", "d"])
}

// The defer in the loop body runs on the iteration that `continue`s.
// i=1: continue → "d"; i=2: "body" then "d".
function defer_on_continue_main() -> string[] {
  let log: string[] = []
  for (let i in [1, 2]) {
    defer { log.push("d") }
    if (i == 1) { continue }
    log.push("body")
  }
  log
}

test "defer_runs_on_continue" {
    assert.equal(defer_on_continue_main(), ["d", "body", "d"])
}

// An explicit `throw` in the deferring function runs the defer, then the
// throw propagates to the caller's catch.
function defer_explicit_throw_risky(log: string[]) -> void {
  defer { log.push("cleanup") }
  throw "boom"
}

function defer_explicit_throw_main() -> string[] {
  let log: string[] = []
  defer_explicit_throw_risky(log) catch (e) {
    _ => { log.push("caught") }
  }
  log
}

test "defer_runs_on_explicit_throw_in_same_function" {
    assert.equal(defer_explicit_throw_main(), ["cleanup", "caught"])
}

// A defer that throws mid-unwind: the remaining (earlier-declared) defers
// still run, and the most-recent throw propagates (replace-semantics).
// b runs (push "b", throw from_b), a still runs (push "a"), from_b replaces
// X and is what the caller catches.
function defer_throwing_defer_trigger() -> void { throw "X" }

function defer_throwing_defer_risky(log: string[]) -> void {
  defer { log.push("a") }                    // runs LAST
  defer { log.push("b"); throw "from_b" }    // runs FIRST, throws
  defer_throwing_defer_trigger()             // throws "X" -> unwind
}

function defer_throwing_defer_main() -> string[] {
  let log: string[] = []
  defer_throwing_defer_risky(log) catch (e) {
    _ => { log.push(e) }
  }
  log
}

test "defer_that_throws_still_runs_remaining_defers" {
    assert.equal(defer_throwing_defer_main(), ["b", "a", "from_b"])
}

// `await` is allowed inside a defer body; it suspends/resumes correctly.
function defer_await_inside_work() -> int { 42 }

function defer_await_inside_main() -> string[] {
  let log: string[] = []
  {
    defer {
      let f = spawn { defer_await_inside_work() }
      await f
      log.push("awaited")
    }
    log.push("body")
  }
  log
}

test "await_inside_defer" {
    assert.equal(defer_await_inside_main(), ["body", "awaited"])
}
