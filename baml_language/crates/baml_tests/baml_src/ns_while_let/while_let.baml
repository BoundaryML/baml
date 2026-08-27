// End-to-end semantics of `while let PATTERN = SCRUTINEE { BODY }`.
//
// The loop re-evaluates the scrutinee and re-tests the refutable pattern each
// iteration: it runs the body while the pattern matches and exits on the first
// non-match. Bindings are scoped to the body and re-bound each iteration.
//
// Converted from crates/bex_engine/tests/while_let.rs. Each Rust test's helper
// functions are kept verbatim; the trivial `function main()` wrapper is folded
// into the `test` block's assertion. Helpers that shared a name across separate
// Rust sources (two `loop_throws`) are given distinct names here since one
// namespace can't declare the same function twice. Loop state lives in helper
// functions rather than test-block locals because test-block locals are boxed
// by the VM and compare unequal to literals.

// ─── draining until the pattern fails ────────────────────────────────────────

// Re-evaluates the scrutinee each iteration; runs while the refutable pattern
// matches and exits when the scrutinee becomes `null`. countdown(3) sums
// 3 + 2 + 1 = 6.
function countdown(n: int) -> int {
    let total = 0;
    let cur: int | null = n;
    while let v: int = cur {
        total = total + v;
        if v <= 1 {
            cur = null;
        } else {
            cur = v - 1;
        }
    }
    total
}

test "while_let_drains_until_pattern_fails" {
    assert.equal(countdown(3), 6)
}

// ─── break / continue ────────────────────────────────────────────────────────

// `break` exits immediately even though the pattern would keep matching.
function find_five(start: int) -> int {
    let cur: int | null = start;
    let found = 0;
    while let v: int = cur {
        if v >= 5 {
            found = v;
            break;
        }
        cur = v + 1;
    }
    found
}

test "while_let_break_exits_loop" {
    assert.equal(find_five(1), 5)
}

// `continue` jumps back to the header, which re-evaluates the scrutinee and
// re-tests the pattern (so `cur = null; continue;` exits). Sums odds below 6:
// 1 + 3 + 5 = 9.
function sum_odds_below(n: int) -> int {
    let cur: int | null = 1;
    let total = 0;
    while let v: int = cur {
        if v >= n {
            cur = null;
            continue;
        }
        cur = v + 1;
        if v % 2 == 0 {
            continue;
        }
        total = total + v;
    }
    total
}

test "while_let_continue_re_tests_pattern" {
    assert.equal(sum_odds_below(6), 9)
}

// ─── destructuring patterns in the head ──────────────────────────────────────

class Item { value int }

// A class destructure pattern binds fields fresh each iteration. 3 + 2 + 1 = 6.
function drain(start: int) -> int {
    let cur: Item | null = Item { value: start };
    let total = 0;
    while let Item { value } = cur {
        total = total + value;
        if value <= 1 {
            cur = null;
        } else {
            cur = Item { value: value - 1 };
        }
    }
    total
}

test "while_let_destructure_binds_fields_each_iteration" {
    assert.equal(drain(3), 6)
}

class WlInner { value int }
class WlOuter { inner WlInner }

// A two-level destructure binds a deeply-nested field each iteration.
// Drains WlOuter{WlInner{value}} from 3 down: 3 + 2 + 1 = 6.
function drain_nested() -> int {
    let cur: WlOuter | null = WlOuter { inner: WlInner { value: 3 } };
    let total = 0;
    while let WlOuter { inner: WlInner { value } } = cur {
        total = total + value;
        if value <= 1 {
            cur = null;
        } else {
            cur = WlOuter { inner: WlInner { value: value - 1 } };
        }
    }
    total
}

test "while_let_nested_class_destructure_binds_each_iteration" {
    assert.equal(drain_nested(), 6)
}

// An array pattern `[let first, ..]` binds the head each iteration; the body
// shifts the array so the pattern eventually fails on the empty array.
// Sums 3 + 1 + 4 + 1 + 5 = 14.
function drain_array() -> int {
    let xs = [3, 1, 4, 1, 5];
    let total = 0;
    while let [let first, ..] = xs {
        total = total + first;
        let _ = xs.shift();
    }
    total
}

test "while_let_array_rest_pattern_drains" {
    assert.equal(drain_array(), 14)
}

// ─── binding cells and scoping ───────────────────────────────────────────────

// The arm binds with fresh cells, so a closure created each iteration captures
// a DISTINCT cell holding that iteration's value. After the loop, invoking all
// three closures sums 3 + 2 + 1 = 6. A regression to one shared cell yields 3.
function collect() -> int {
    let sum = 0;
    let fns = [];
    let cur: int | null = 3;
    while let v: int = cur {
        fns.push(() -> {
            sum += v;
        });
        if v <= 1 { cur = null; } else { cur = v - 1; }
    }
    for (let f in fns) {
        f();
    }
    sum
}

test "while_let_closure_captures_fresh_cell_per_iteration" {
    assert.equal(collect(), 6)
}

// A non-shadowed outer `acc` mutated in the body persists past the loop, while
// an outer `v` shadowed by the pattern binding is RESTORED after the loop.
// acc(6) + outer v(999) = 1005.
function scope() -> int {
    let acc = 0;
    let v = 999;
    let cur: int | null = 3;
    while let v: int = cur {
        acc += v;
        if v <= 1 { cur = null; } else { cur = v - 1; }
    }
    acc + v
}

test "while_let_outer_mutation_persists_inner_shadow_restored" {
    assert.equal(scope(), 1005)
}

// A body-introduced `let tmp` shadowing an outer `tmp` is rolled back each
// iteration and must not leak past the loop. After the loop `tmp` reads the
// outer 50, not the last body value.
function body_local() -> int {
    let tmp = 50;
    let cur: int | null = 2;
    while let v: int = cur {
        let tmp = v * 10;
        if tmp > 1000000 { cur = null; }
        if v <= 1 { cur = null; } else { cur = v - 1; }
    }
    tmp
}

test "while_let_body_local_let_does_not_leak" {
    assert.equal(body_local(), 50)
}

// ─── zero-iteration loops fall through ───────────────────────────────────────

// Pattern fails on the FIRST eval, so the body never runs and control falls
// through (the loop never diverges). total stays 100, then + 7 = 107.
function run() -> int {
    let cur: int | null = null;
    let total = 100;
    while let v: int = cur {
        total = total + v;
        cur = null;
    }
    total + 7
}

test "while_let_zero_iterations_falls_through" {
    assert.equal(run(), 107)
}

class Empty {}

// Strengthens the non-divergence guard: after a zero-iteration loop, a
// following `throw` must be reachable.
function check(x: int | null) -> int throws Empty {
    let touched = 0;
    while let v: int = x {
        touched = touched + 1;
        x = null;
    }
    if touched == 0 {
        throw Empty {};
    }
    touched
}

test "while_let_zero_iterations_post_loop_throw_is_reachable" {
    {
        check(null);
        baml.sys.panic("expected Empty throw")
    } catch (e) {
        Empty => null
    }
}

// ─── scrutinee evaluation ────────────────────────────────────────────────────

class Counter {
    n int
    calls int
    function next(self) -> int | null {
        self.calls += 1;
        if self.n <= 0 {
            null
        } else {
            let cur = self.n;
            self.n = self.n - 1;
            cur
        }
    }
}

// The scrutinee is materialized into a single local per header pass, shared by
// the pattern test and the bind — so a side-effecting scrutinee is evaluated
// exactly once per iteration. `next()` returns 3, 2, 1 then null: 3 successful
// passes + 1 final failing pass = 4 calls.
function count_evals() -> int {
    let c = Counter { n: 3, calls: 0 };
    let total = 0;
    while let v: int = c.next() {
        total = total + v;
    }
    c.calls
}

test "while_let_scrutinee_evaluated_once_per_iteration" {
    assert.equal(count_evals(), 4)
}

// ─── nesting ─────────────────────────────────────────────────────────────────

// Nested while-let loops exercise the loop-context take/restore. An inner
// `break` exits only the inner loop. 9 inner passes (+1 each) + 3 outer marks
// (+100 each) = 309.
function nested() -> int {
    let outer: int | null = 3;
    let result = 0;
    while let o: int = outer {
        let inner: int | null = 10;
        while let i: int = inner {
            result = result + 1;
            if i <= 8 { break; }
            inner = i - 1;
        }
        result = result + 100;
        if o <= 1 { outer = null; } else { outer = o - 1; }
    }
    result
}

test "while_let_nested_inner_break_does_not_exit_outer" {
    assert.equal(nested(), 309)
}

// ─── union-typed head binding ────────────────────────────────────────────────

interface WlHasV { v int }
class WlA { v int  implements WlHasV {} }
class WlB { v int  implements WlHasV {} }
class WlStop {}

// A union-typed refutable binding `x: WlA | WlB` in the head matches either
// member and binds the shared field; the scrutinee drains WlA(3) -> WlB(2) ->
// WlB(1) then exits via the non-matching WlStop. Sums x.v = 3 + 2 + 1 = 6.
// (Regression: before the union-binding fix the first member never matched.)
function alternate() -> int {
    let cur: WlA | WlB | WlStop = WlA { v: 3 };
    let total = 0;
    while let x: WlA | WlB = cur {
        total = total + x.v;
        if x.v <= 1 {
            cur = WlStop {};
        } else {
            cur = WlB { v: x.v - 1 };
        }
    }
    total
}

test "while_let_union_typed_binding_dispatches_across_iterations" {
    assert.equal(alternate(), 6)
}

// ─── non-local exits from the body ───────────────────────────────────────────

class WlBoom {}

// A `throw` in the body terminates the body (no back-edge) and, uncaught,
// escapes the function.
function loop_throws(start: int) -> int throws WlBoom {
    let cur: int | null = start;
    while let v: int = cur {
        if v == 2 {
            throw WlBoom {};
        }
        cur = v - 1;
    }
    0
}

test "while_let_body_throw_propagates_uncaught" {
    {
        loop_throws(4);
        baml.sys.panic("expected WlBoom throw")
    } catch (e) {
        WlBoom => null
    }
}

class WlBoom2 {}

// A `throw` from inside the loop body is caught by an outer `catch`.
function loop_throws2(start: int) -> int throws WlBoom2 {
    let cur: int | null = start;
    while let v: int = cur {
        if v == 1 {
            throw WlBoom2 {};
        }
        cur = v - 1;
    }
    0
}

function caught() -> int {
    loop_throws2(3) catch (e) {
        _ => 42
    }
}

test "while_let_body_throw_caught_by_outer_catch" {
    assert.equal(caught(), 42)
}

// A `return` inside the body exits the whole function (back-edge suppressed by
// the divergence guard). Returns 99 at v == 2.
function early(start: int) -> int {
    let cur: int | null = start;
    while let v: int = cur {
        if v == 2 {
            return 99;
        }
        cur = v - 1;
    }
    -1
}

test "while_let_return_from_body_exits_function" {
    assert.equal(early(4), 99)
}
