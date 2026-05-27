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
        expected: Ok(BexExternalValue::String("hit".into())),
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
        expected: Ok(BexExternalValue::String("boom".into())),
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
        expected: Ok(BexExternalValue::String("alice".into())),
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
        expected: Ok(BexExternalValue::String("second".into())),
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
        expected: Ok(BexExternalValue::String("empty".into())),
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
            expected: Ok(BexExternalValue::String(expected.into())),
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
        expected: Ok(BexExternalValue::String("outer".into())),
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
        expected: Ok(BexExternalValue::String("from call".into())),
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
        expected: Ok(BexExternalValue::String("alice".into())),
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
        expected: Ok(BexExternalValue::String("none".into())),
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
        expected: Ok(BexExternalValue::String("fell through".into())),
        ..Default::default()
    })
    .await
}
