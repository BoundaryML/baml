//! `obj.to_string()` sugar: a 0-arg `to_string` call on a value whose type has no
//! real `to_string` method lowers to `string.from(obj)`, so any value can be
//! rendered with `.to_string()` regardless of whether its type implements
//! `baml.ToString`. Operator-style (like `==` -> `baml.ops.equals_equals`): TIR
//! types the fallback as `string`, MIR lowers it; the AST stays a method call, so
//! diagnostics keep showing `.to_string()` and a real `to_string` (from any
//! interface impl) is dispatched normally — never hijacked.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// Compile errors raised in the user file, as `[CODE] message`.
fn compile_errors(source: &str) -> Vec<String> {
    use baml_compiler_diagnostics::Severity;
    use baml_project::{collect_diagnostics, testing::setup_test_db};
    let db = setup_test_db(source);
    let project = db.get_project().expect("project");
    let files = db.get_source_files();
    collect_diagnostics(&db, project, &files)
        .into_iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .map(|d| format!("[{}] {}", d.code(), d.message))
        .collect()
}

async fn expect_string(src: &str) -> String {
    match baml_test!(src).result.unwrap() {
        BexExternalValue::String(s) => s.to_string(),
        other => panic!("expected a string, got {other:?}"),
    }
}

#[tokio::test]
async fn primitive_to_string() {
    assert_eq!(
        expect_string(r#"function main() -> string { return (5).to_string() }"#).await,
        "5"
    );
    assert_eq!(
        expect_string(r#"function main() -> string { return true.to_string() }"#).await,
        "true"
    );
}

#[tokio::test]
async fn string_to_string_is_bare() {
    assert_eq!(
        expect_string(r#"function main() -> string { return "hi".to_string() }"#).await,
        "hi"
    );
}

#[tokio::test]
async fn array_to_string() {
    assert_eq!(
        expect_string(r#"function main() -> string { return [1, 2, 3].to_string() }"#).await,
        "[1, 2, 3]"
    );
}

#[tokio::test]
async fn plain_class_to_string_is_structural() {
    let out = expect_string(
        r#"
        class Point { x int  y int }
        function main() -> string {
            let p = Point { x: 1, y: 2 }
            return p.to_string()
        }
    "#,
    )
    .await;
    assert_eq!(out, "Point { x: 1, y: 2 }");
}

#[tokio::test]
async fn implementor_to_string_uses_override() {
    let out = expect_string(
        r#"
        class Point {
            x int
            y int
            implements baml.ToString {
                function to_string(self) -> string throws never {
                    "(" + string.from(self.x) + ", " + string.from(self.y) + ")"
                }
            }
        }
        function main() -> string {
            let p = Point { x: 1, y: 2 }
            return p.to_string()
        }
    "#,
    )
    .await;
    assert_eq!(out, "(1, 2)");
}

#[tokio::test]
async fn nested_override_honored_via_sugar() {
    // `.to_string()` on a container routes through `string.from`, so nested
    // overrides are honored.
    let out = expect_string(
        r#"
        class Point {
            x int
            y int
            implements baml.ToString {
                function to_string(self) -> string throws never {
                    "(" + string.from(self.x) + ", " + string.from(self.y) + ")"
                }
            }
        }
        function main() -> string {
            return [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }].to_string()
        }
    "#,
    )
    .await;
    assert_eq!(out, "[(1, 2), (3, 4)]");
}

#[tokio::test]
async fn nested_override_in_field_honored_via_sugar() {
    // Parity with `string.from`: `.to_string()` on a non-implementor whose FIELDS
    // are override-bearing honors those overrides at depth (same walker as
    // `string.from`, since the sugar lowers to it).
    let out = expect_string(
        r#"
        class Point {
            x int
            y int
            implements baml.ToString {
                function to_string(self) -> string throws never {
                    "(" + string.from(self.x) + ", " + string.from(self.y) + ")"
                }
            }
        }
        class Line { start Point  end Point }
        function main() -> string {
            let l = Line { start: Point { x: 1, y: 2 }, end: Point { x: 3, y: 4 } }
            return l.to_string()
        }
    "#,
    )
    .await;
    assert_eq!(out, "Line { start: (1, 2), end: (3, 4) }");
}

#[tokio::test]
async fn sugar_matches_string_from_exactly() {
    // `x.to_string()` and `string.from(x)` must produce identical output — the
    // sugar respects overrides in exactly the same way.
    let src = |call: &str| {
        format!(
            r#"
        class Point {{
            x int
            y int
            implements baml.ToString {{
                function to_string(self) -> string throws never {{
                    "(" + string.from(self.x) + ", " + string.from(self.y) + ")"
                }}
            }}
        }}
        class Line {{ start Point  end Point }}
        function main() -> string {{
            let l = Line {{ start: Point {{ x: 1, y: 2 }}, end: Point {{ x: 3, y: 4 }} }}
            return {call}
        }}
    "#
        )
    };
    let via_sugar = expect_string(&src("l.to_string()")).await;
    let via_from = expect_string(&src("string.from(l)")).await;
    assert_eq!(via_sugar, via_from);
}

#[tokio::test]
async fn chained_method_result_to_string() {
    // `expr.to_string()` where the receiver is itself a call expression.
    let out = expect_string(
        r#"function main() -> string { return baml.time.Instant.epoch().to_string() }"#,
    )
    .await;
    assert_eq!(out, "1970-01-01T00:00:00Z");
}

#[tokio::test]
async fn generic_receiver_to_string() {
    // `x.to_string()` on an unbounded type variable falls back to `string.from`,
    // which dispatches on the runtime class.
    let out = expect_string(
        r#"
        function show<T>(x: T) -> string { x.to_string() }
        function main() -> string { return show<int>(42) }
    "#,
    )
    .await;
    assert_eq!(out, "42");
}

#[test]
fn receiver_error_is_not_swallowed() {
    // The fallback rolls back only its own `to_string` member error; a real error
    // in the receiver expression (here: a bad argument) must still be reported.
    let errors = compile_errors(
        r#"
        function takes_int(n: int) -> int { n }
        function main() -> string {
            return takes_int("not an int").to_string()
        }
    "#,
    );
    assert!(
        errors.iter().any(|e| e.contains("not an int")
            || e.to_lowercase().contains("type mismatch")
            || e.contains("int")),
        "expected the bad-argument error to survive the to_string fallback; got:\n  {}",
        errors.join("\n  ")
    );
}

#[tokio::test]
async fn user_interface_to_string_is_not_hijacked() {
    // A `to_string` from a *user* interface (not `baml.ToString`) must dispatch to
    // that impl, not be replaced by `string.from`. This is the correctness win of
    // the operator-style (resolution-aware) lowering over a blind AST rewrite.
    let out = expect_string(
        r#"
        interface MyShow {
            function to_string(self) -> string throws never
        }
        class Widget {
            id int
            implements MyShow {
                function to_string(self) -> string throws never { "widget!" }
            }
        }
        function main() -> string {
            let w = Widget { id: 1 }
            return w.to_string()
        }
    "#,
    )
    .await;
    assert_eq!(out, "widget!");
}

#[tokio::test]
async fn zzz_throws_never2() {
    let out = expect_string(
        r#"
        function show<T>(x: T) -> string throws never { x.to_string() }
        function main() -> string { return show<int>(42) }
    "#,
    )
    .await;
    assert_eq!(out, "42");
}
