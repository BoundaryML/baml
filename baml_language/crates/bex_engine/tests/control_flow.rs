//! End-to-end control-flow tests through compiler2 + `BexEngine`.

mod common;

use bex_engine::BexExternalValue;
use common::{EngineProgram, assert_engine_executes};

#[tokio::test]
async fn match_arm_break_exits_infinite_while_loop() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function break_loop(x: int) -> int {
                let result = 0;
                while (true) {
                    match (x) {
                        1 => {
                            result = 40;
                            break;
                        },
                        _ => {
                            result = 2;
                            break;
                        },
                    };
                }

                result
            }

            function main() -> int {
                break_loop(1) + break_loop(2)
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(42)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn match_arm_continue_continues_loop_without_falling_through() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function continue_loop(x: int) -> int {
                let i = 0;
                while (i < 3) {
                    match (x) {
                        1 => {
                            i += 1;
                            continue;
                        },
                        _ => {
                            i += 10;
                            continue;
                        },
                    };

                    i = 100;
                }

                i
            }

            function main() -> int {
                continue_loop(1) + continue_loop(2)
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(13)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn match_arm_return_returns_without_falling_through() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function return_from_match(x: int) -> int {
                match (x) {
                    1 => {
                        return 40;
                    },
                    _ => {
                        return 2;
                    },
                };

                999
            }

            function main() -> int {
                return_from_match(1) + return_from_match(2)
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(42)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn if_let_takes_then_branch_when_pattern_matches() -> anyhow::Result<()> {
    // Pattern matches → then-branch runs, binding visible inside.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Ok { value string }
            class Err { message string }

            function describe(r: Ok | Err) -> string {
                if let o: Ok = r {
                    o.value
                } else {
                    "no match"
                }
            }

            function main() -> string {
                describe(Ok { value: "hit" })
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("hit".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn if_let_takes_else_branch_when_pattern_does_not_match() -> anyhow::Result<()> {
    // Pattern fails → else runs; scrutinee is narrowed to the complement, so
    // `r.message` resolves on `Err`.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Ok { value string }
            class Err { message string }

            function describe(r: Ok | Err) -> string {
                if let o: Ok = r {
                    o.value
                } else {
                    r.message
                }
            }

            function main() -> string {
                describe(Err { message: "boom" })
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("boom".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn if_let_destructure_binds_fields_at_runtime() -> anyhow::Result<()> {
    // Class destructure: shorthand field names are bound and readable in
    // the then-branch.
    assert_engine_executes(EngineProgram {
        source: r#"
            class User { name string, age int }
            class Admin { handle string }

            function greet(u: User | Admin) -> string {
                if let User { name, age } = u {
                    name
                } else {
                    "admin"
                }
            }

            function main() -> string {
                greet(User { name: "alice", age: 30 })
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("alice".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn if_let_else_if_let_chain_runs_correct_arm() -> anyhow::Result<()> {
    // Three-way chain: first arm fails, second succeeds — second branch's
    // body runs.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Ok { value string }
            class Err { message string }
            class Empty {}

            function pick(r: Ok | Err | Empty) -> string {
                if let o: Ok = r {
                    o.value
                } else if let e: Err = r {
                    e.message
                } else {
                    "empty"
                }
            }

            function main() -> string {
                pick(Err { message: "second" })
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("second".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn if_let_chain_falls_through_to_final_else() -> anyhow::Result<()> {
    // All preceding arms fail → final else runs.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Ok { value string }
            class Err { message string }
            class Empty {}

            function pick(r: Ok | Err | Empty) -> string {
                if let o: Ok = r {
                    o.value
                } else if let e: Err = r {
                    e.message
                } else {
                    "empty"
                }
            }

            function main() -> string {
                pick(Empty {})
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("empty".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn if_let_or_pattern_dispatches_to_either_alternative() -> anyhow::Result<()> {
    // `Ok | Warn` — both alternatives bind `s.value`. Same body runs for
    // either class.
    let cases = [
        (r#"Ok { value: "ok!" }"#, "ok!"),
        (r#"Warn { value: "warn!" }"#, "warn!"),
        (r#"Err { message: "boom" }"#, "boom"),
    ];
    for (ctor, expected) in cases {
        let source = format!(
            r#"
            class Ok {{ value string }}
            class Warn {{ value string }}
            class Err {{ message string }}

            function describe(r: Ok | Warn | Err) -> string {{
                if let s: Ok | Warn = r {{
                    s.value
                }} else {{
                    r.message
                }}
            }}

            function main() -> string {{
                describe({ctor})
            }}
            "#
        );
        assert_engine_executes(EngineProgram {
            source: Box::leak(source.into_boxed_str()),
            entry: "main",
            expected: Ok(BexExternalValue::String(expected.to_string())),
            ..Default::default()
        })
        .await?;
    }
    Ok(())
}

#[tokio::test]
async fn if_let_nested_shadowing_restores_outer_binding() -> anyhow::Result<()> {
    // Inner if-let shadows `w` from the outer if-let. When the inner else
    // runs, the outer `w` must be visible again (carrying the outer
    // payload).
    assert_engine_executes(EngineProgram {
        source: r#"
            class Wrapper { payload string }
            class Other {}

            function pick(outer: Wrapper, inner: Wrapper | Other) -> string {
                if let w: Wrapper = outer {
                    if let w: Wrapper = inner {
                        // Inner `w` shadows outer; we read the inner payload.
                        w.payload
                    } else {
                        // Inner scope ended; outer `w` is visible again.
                        w.payload
                    }
                } else {
                    "none"
                }
            }

            function main() -> string {
                // Inner is `Other`, so the inner-else branch runs and we
                // see the OUTER's payload, not the inner's.
                pick(Wrapper { payload: "outer" }, Other {})
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("outer".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn if_let_composite_scrutinee_binds_call_result() -> anyhow::Result<()> {
    // Scrutinee is a function call (not a path local). No scrutinee
    // narrowing applies, but the pattern binding must still capture the
    // call's result value.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Ok { value string }
            class Err { message string }

            function maybe() -> Ok | Err {
                Ok { value: "from call" }
            }

            function main() -> string {
                if let o: Ok = maybe() {
                    o.value
                } else {
                    "no match"
                }
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("from call".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn if_let_optional_scrutinee_some() -> anyhow::Result<()> {
    // Nullable scrutinee where the value is present.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Item { name string }

            function pick(item: Item?) -> string {
                if let i: Item = item {
                    i.name
                } else {
                    "none"
                }
            }

            function main() -> string {
                pick(Item { name: "alice" })
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("alice".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn if_let_optional_scrutinee_null() -> anyhow::Result<()> {
    // Nullable scrutinee passing `null` — pattern fails, else branch runs.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Item { name string }

            function pick(item: Item?) -> string {
                if let i: Item = item {
                    i.name
                } else {
                    "none"
                }
            }

            function main() -> string {
                pick(null)
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("none".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn if_let_statement_form_with_return_in_then() -> anyhow::Result<()> {
    // No-else statement form: returns from inside the matched branch;
    // unmatched falls through to the tail expression.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Ok { value string }
            class Err { message string }

            function first_value(r: Ok | Err) -> string {
                if let o: Ok = r {
                    return o.value;
                }
                "fell through"
            }

            function main() -> string {
                first_value(Err { message: "ignored" })
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("fell through".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn let_else_binds_when_pattern_matches() -> anyhow::Result<()> {
    // Pattern matches → bindings flow into the enclosing scope; the rest
    // of the function body can use them.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Ok { value string }
            class Err { message string }

            function get_value(r: Ok | Err) -> string {
                let o: Ok = r else { return "fallback"; };
                o.value
            }

            function main() -> string {
                get_value(Ok { value: "hit" })
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("hit".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn let_else_takes_else_branch_when_pattern_does_not_match() -> anyhow::Result<()> {
    // Pattern fails → else branch runs and diverges (here via `return`),
    // so the tail expression past the binding never executes.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Ok { value string }
            class Err { message string }

            function get_value(r: Ok | Err) -> string {
                let o: Ok = r else { return "fallback"; };
                o.value
            }

            function main() -> string {
                get_value(Err { message: "ignored" })
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("fallback".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn let_else_destructure_binds_fields_at_runtime() -> anyhow::Result<()> {
    // Destructure binding: the matched class's fields are bound into the
    // enclosing scope, available to all later statements.
    assert_engine_executes(EngineProgram {
        source: r#"
            class User { name string, age int }
            class Admin { handle string }

            function greet(u: User | Admin) -> string {
                let User { name, age } = u else { return "admin"; };
                name
            }

            function main() -> string {
                greet(User { name: "alice", age: 30 })
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("alice".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn let_else_or_pattern_matches_either_alternative() -> anyhow::Result<()> {
    // Or-pattern: either alternative produces a binding of the joined
    // type. Here both Ok and Warn carry `value: string`.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Ok { value string }
            class Warn { value string }
            class Err { message string }

            function pick(r: Ok | Warn | Err) -> string {
                let s: Ok | Warn = r else { return "err"; };
                s.value
            }

            function main() -> string {
                pick(Warn { value: "warn-val" })
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("warn-val".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn let_else_break_in_else_inside_loop() -> anyhow::Result<()> {
    // `break` is a diverging form (`Ty::Never`). When the pattern fails,
    // the else branch breaks out of the enclosing loop instead of running
    // the rest of the loop body — proving the unreachable terminator on
    // the miss block doesn't fall through.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Ok { value string }
            class Err { message string }

            function pick_first_ok(items: (Ok | Err)[]) -> string {
                let result = "no-ok";
                let i = 0;
                while (i < 3) {
                    let item = items[i];
                    let o: Ok = item else { break; };
                    result = o.value;
                    i += 1;
                }
                result
            }

            function main() -> string {
                pick_first_ok([Ok { value: "first" }, Err { message: "stop" }, Ok { value: "third" }])
            }
        "#,
        entry: "main",
        // Iteration 0: Ok → result = "first". Iteration 1: Err → break.
        // Iteration 2 never runs; result stays "first".
        expected: Ok(BexExternalValue::String("first".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn let_else_continue_in_else_inside_loop() -> anyhow::Result<()> {
    // `continue` is a diverging form: skip to the next iteration when the
    // pattern fails. Aggregates only the matching items.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Ok { value string }
            class Err { message string }

            function concat_oks(items: (Ok | Err)[]) -> string {
                let acc = "";
                let i = 0;
                while (i < 3) {
                    let item = items[i];
                    i += 1;
                    let o: Ok = item else { continue; };
                    acc = acc + o.value;
                }
                acc
            }

            function main() -> string {
                concat_oks([Ok { value: "a" }, Err { message: "skip" }, Ok { value: "c" }])
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("ac".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn let_else_multiple_consecutive_bindings() -> anyhow::Result<()> {
    // Two let-else statements back-to-back: both bindings should be live
    // in the tail, locals must not collide, and a miss in the second one
    // diverges past the first binding cleanly.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Ok { value string }
            class Err { message string }

            function chain(a: Ok | Err, b: Ok | Err) -> string {
                let oa: Ok = a else { return "a-failed"; };
                let ob: Ok = b else { return "b-failed"; };
                oa.value + "-" + ob.value
            }

            function main() -> string {
                chain(Ok { value: "x" }, Ok { value: "y" })
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("x-y".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn let_else_second_miss_skips_first_binding() -> anyhow::Result<()> {
    // First let-else succeeds, second fails. Else of the second runs and
    // returns; the rest of the function never executes.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Ok { value string }
            class Err { message string }

            function chain(a: Ok | Err, b: Ok | Err) -> string {
                let oa: Ok = a else { return "a-failed"; };
                let ob: Ok = b else { return "b-failed"; };
                oa.value + "-" + ob.value
            }

            function main() -> string {
                chain(Ok { value: "x" }, Err { message: "boom" })
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("b-failed".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn let_else_throw_in_else_is_caught_by_outer_catch() -> anyhow::Result<()> {
    // A thrown value from the else branch participates in the surrounding
    // throws contract — an enclosing `catch` block must receive it. This
    // checks that throws-analysis on `Stmt::Let.else_branch` correctly
    // surfaces the thrown type.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Ok { value string }
            class Err { message string }
            class NoMatch {}

            function inner(r: Ok | Err) -> string throws NoMatch {
                let o: Ok = r else { throw NoMatch {}; };
                o.value
            }

            function main() -> string {
                inner(Err { message: "ignored" }) catch (e) {
                    NoMatch => "caught"
                }
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("caught".to_string())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn let_else_throw_in_else_propagates_when_uncaught() -> anyhow::Result<()> {
    // `throw` in the else branch is a diverging form. When uncaught, the
    // thrown value escapes the function and the engine reports the error.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Ok { value string }
            class Err { message string }
            class NoMatch {}

            function get_or_throw(r: Ok | Err) -> string throws NoMatch {
                let o: Ok = r else { throw NoMatch {}; };
                o.value
            }

            function main() -> string throws NoMatch {
                get_or_throw(Err { message: "boom" })
            }
        "#,
        entry: "main",
        // Uncaught NoMatch throw escapes — the engine reports a runtime
        // error rather than producing a value.
        expected: Err("NoMatch"),
        ..Default::default()
    })
    .await
}
