//! End-to-end engine tests for `while let PATTERN = SCRUTINEE { BODY }`.
//!
//! The loop re-evaluates the scrutinee and re-tests the refutable pattern each
//! iteration: it runs the body while the pattern matches and exits on the first
//! non-match. Bindings are scoped to the body and re-bound each iteration.

mod common;

use bex_engine::BexExternalValue;
use common::{EngineProgram, assert_engine_executes};

#[tokio::test]
async fn while_let_drains_until_pattern_fails() -> anyhow::Result<()> {
    // Re-evaluates the scrutinee each iteration; runs while the refutable
    // pattern matches and exits when the scrutinee becomes `null`.
    // countdown(3) sums 3 + 2 + 1 = 6.
    assert_engine_executes(EngineProgram {
        source: r#"
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

            function main() -> int {
                countdown(3)
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(6)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn while_let_break_exits_loop() -> anyhow::Result<()> {
    // `break` exits immediately even though the pattern would keep matching.
    assert_engine_executes(EngineProgram {
        source: r#"
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

            function main() -> int {
                find_five(1)
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(5)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn while_let_continue_re_tests_pattern() -> anyhow::Result<()> {
    // `continue` jumps back to the header, which re-evaluates the scrutinee and
    // re-tests the pattern (so `cur = null; continue;` exits). Sums odds below
    // 6: 1 + 3 + 5 = 9.
    assert_engine_executes(EngineProgram {
        source: r#"
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

            function main() -> int {
                sum_odds_below(6)
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(9)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn while_let_destructure_binds_fields_each_iteration() -> anyhow::Result<()> {
    // A class destructure pattern binds fields fresh each iteration. 3+2+1 = 6.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Item { value int }

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

            function main() -> int {
                drain(3)
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(6)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn while_let_closure_captures_fresh_cell_per_iteration() -> anyhow::Result<()> {
    // The arm binds with `bind_pattern_with_fresh_cells`, so a closure created
    // each iteration captures a DISTINCT cell holding that iteration's value.
    // After the loop, invoking all three closures sums 3 + 2 + 1 = 6. A
    // regression to plain `bind_pattern` (one shared cell) would yield 3.
    assert_engine_executes(EngineProgram {
        source: r#"
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

            function main() -> int {
                collect()
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(6)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn while_let_zero_iterations_falls_through() -> anyhow::Result<()> {
    // Pattern fails on the FIRST eval → body never runs and control falls
    // through (the loop never diverges). total stays 100, then + 7 = 107.
    assert_engine_executes(EngineProgram {
        source: r#"
            function run() -> int {
                let cur: int | null = null;
                let total = 100;
                while let v: int = cur {
                    total = total + v;
                    cur = null;
                }
                total + 7
            }

            function main() -> int {
                run()
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(107)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn while_let_zero_iterations_post_loop_throw_is_reachable() -> anyhow::Result<()> {
    // Strengthens the non-divergence guard: after a zero-iteration loop, a
    // following `throw` must be reachable.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Empty {}

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

            function main() -> int throws Empty {
                check(null)
            }
        "#,
        entry: "main",
        expected: Err("Empty"),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn while_let_outer_mutation_persists_inner_shadow_restored() -> anyhow::Result<()> {
    // A non-shadowed outer `acc` mutated in the body persists past the loop,
    // while an outer `v` shadowed by the pattern binding is RESTORED after the
    // loop. acc(6) + outer v(999) = 1005.
    assert_engine_executes(EngineProgram {
        source: r#"
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

            function main() -> int {
                scope()
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(1005)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn while_let_body_local_let_does_not_leak() -> anyhow::Result<()> {
    // A body-introduced `let tmp` shadowing an outer `tmp` is rolled back each
    // iteration and must not leak past the loop. After the loop `tmp` reads the
    // outer 50, not the last body value.
    assert_engine_executes(EngineProgram {
        source: r#"
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

            function main() -> int {
                body_local()
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(50)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn while_let_scrutinee_evaluated_once_per_iteration() -> anyhow::Result<()> {
    // The scrutinee is materialized into a single local per header pass, shared
    // by the pattern test and the bind — so a side-effecting scrutinee is
    // evaluated exactly once per iteration. `next()` returns 3,2,1 then null:
    // 3 successful passes + 1 final failing pass = 4 calls.
    assert_engine_executes(EngineProgram {
        source: r#"
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

            function count_evals() -> int {
                let c = Counter { n: 3, calls: 0 };
                let total = 0;
                while let v: int = c.next() {
                    total = total + v;
                }
                c.calls
            }

            function main() -> int {
                count_evals()
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(4)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn while_let_nested_inner_break_does_not_exit_outer() -> anyhow::Result<()> {
    // Nested while-let loops exercise the LoopContext take/restore. An inner
    // `break` exits only the inner loop. 9 inner passes (+1 each) + 3 outer
    // marks (+100 each) = 309.
    assert_engine_executes(EngineProgram {
        source: r#"
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

            function main() -> int {
                nested()
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(309)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn while_let_union_typed_binding_dispatches_across_iterations() -> anyhow::Result<()> {
    // A union-typed refutable binding `x: WlA | WlB` in the head matches either
    // member and binds the shared field; the scrutinee drains WlA(3) -> WlB(2)
    // -> WlB(1) then exits via the non-matching WlStop. Sums x.v = 3 + 2 + 1 = 6.
    // (Regression: before the union-binding fix the first member never matched.)
    assert_engine_executes(EngineProgram {
        source: r#"
            interface WlHasV { v int }
            class WlA { v int  implements WlHasV {} }
            class WlB { v int  implements WlHasV {} }
            class WlStop {}

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

            function main() -> int {
                alternate()
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(6)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn while_let_array_rest_pattern_drains() -> anyhow::Result<()> {
    // An array pattern `[let first, ..]` binds the head each iteration; the body
    // shifts the array so the pattern eventually fails on the empty array.
    // Sums 3 + 1 + 4 + 1 + 5 = 14.
    assert_engine_executes(EngineProgram {
        source: r#"
            function drain_array() -> int {
                let xs = [3, 1, 4, 1, 5];
                let total = 0;
                while let [let first, ..] = xs {
                    total = total + first;
                    let _ = xs.shift();
                }
                total
            }

            function main() -> int {
                drain_array()
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(14)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn while_let_body_throw_propagates_uncaught() -> anyhow::Result<()> {
    // A `throw` in the body terminates the body (no back-edge) and, uncaught,
    // escapes the function.
    assert_engine_executes(EngineProgram {
        source: r#"
            class WlBoom {}

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

            function main() -> int throws WlBoom {
                loop_throws(4)
            }
        "#,
        entry: "main",
        expected: Err("WlBoom"),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn while_let_body_throw_caught_by_outer_catch() -> anyhow::Result<()> {
    // A `throw` from inside the loop body is caught by an outer `catch`.
    assert_engine_executes(EngineProgram {
        source: r#"
            class WlBoom2 {}

            function loop_throws(start: int) -> int throws WlBoom2 {
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
                loop_throws(3) catch (e) {
                    _ => 42
                }
            }

            function main() -> int {
                caught()
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(42)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn while_let_return_from_body_exits_function() -> anyhow::Result<()> {
    // A `return` inside the body exits the whole function (back-edge suppressed
    // by the divergence guard). Returns 99 at v == 2.
    assert_engine_executes(EngineProgram {
        source: r#"
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

            function main() -> int {
                early(4)
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(99)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn while_let_nested_class_destructure_binds_each_iteration() -> anyhow::Result<()> {
    // A two-level destructure binds a deeply-nested field each iteration.
    // Drains WlOuter{WlInner{value}} from 3 down: 3 + 2 + 1 = 6.
    assert_engine_executes(EngineProgram {
        source: r#"
            class WlInner { value int }
            class WlOuter { inner WlInner }

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

            function main() -> int {
                drain_nested()
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(6)),
        ..Default::default()
    })
    .await
}
