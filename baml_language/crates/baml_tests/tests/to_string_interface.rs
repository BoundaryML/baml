//! Proof-of-concept tests for the `baml.ToString` interface and the
//! `string.from<T>` driver that replaces the hardcoded to_string magic.
//!
//! `string.from(value)` resolves `to_string` on `value`'s *runtime* class:
//!   - if that class has an `implements baml.ToString` override, dispatch to it;
//!   - otherwise render structurally via `baml._to_string_default`, which is
//!     also the interface's default `to_string` body.
//!
//! Runtime-class dispatch (`baml._to_string_shim`) is used instead of a static
//! `match (value) { baml.ToString => ... }` because an interface match in the
//! stdlib package can't see downstream user implementors — see `string.from`.

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

#[test]
fn direct_to_string_method_is_banned() {
    let errors = compile_errors(
        r#"
        class Point {
            x int
            function to_string(self) -> string throws never { "p" }
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("to_string") && e.contains("baml.ToString")),
        "expected a to_string ban error recommending baml.ToString; got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn to_string_via_interface_is_allowed() {
    // The same method inside an `implements baml.ToString` block is fine.
    let errors = compile_errors(
        r#"
        class Point {
            x int
            implements baml.ToString {
                function to_string(self) -> string throws never { "p" }
            }
        }
    "#,
    );
    assert!(
        !errors.iter().any(|e| e.contains("to_string")),
        "implementing baml.ToString must not be flagged; got:\n  {}",
        errors.join("\n  ")
    );
}

async fn expect_string(src: &str) -> String {
    let output = baml_test!(src);
    match output.result.unwrap() {
        BexExternalValue::String(s) => s.to_string(),
        other => panic!("expected a string, got {other:?}"),
    }
}

#[tokio::test]
async fn default_renders_primitives() {
    assert_eq!(
        expect_string(r#"function main() -> string { return string.from(42) }"#).await,
        "42"
    );
    assert_eq!(
        expect_string(r#"function main() -> string { return string.from(true) }"#).await,
        "true"
    );
    assert_eq!(
        expect_string(r#"function main() -> string { return string.from(3.5) }"#).await,
        "3.5"
    );
}

#[tokio::test]
async fn default_top_level_string_is_bare() {
    // A bare `string` does not implement ToString, so it hits the `_` arm; the
    // top-level rendering is unquoted.
    assert_eq!(
        expect_string(r#"function main() -> string { return string.from("hi") }"#).await,
        "hi"
    );
}

#[tokio::test]
async fn default_renders_array_with_quoted_nested_strings() {
    assert_eq!(
        expect_string(r#"function main() -> string { return string.from([1, 2, 3]) }"#).await,
        "[1, 2, 3]"
    );
    assert_eq!(
        expect_string(r#"function main() -> string { return string.from(["a", "b"]) }"#).await,
        r#"["a", "b"]"#
    );
}

#[tokio::test]
async fn default_renders_class_instance_structurally() {
    // A class that does NOT implement ToString renders structurally via the
    // `_` arm.
    let out = expect_string(
        r#"
        class Point { x int  y int }
        function main() -> string {
            return string.from(Point { x: 1, y: 2 })
        }
    "#,
    )
    .await;
    assert_eq!(out, "Point { x: 1, y: 2 }");
}

#[tokio::test]
async fn implementor_override_is_used() {
    // A class implementing ToString with a custom body: `string.from` resolves
    // `to_string` on the value's runtime class and dispatches to the override.
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
            return string.from(Point { x: 1, y: 2 })
        }
    "#,
    )
    .await;
    assert_eq!(out, "(1, 2)");
}

#[tokio::test]
async fn implementor_with_default_body_matches_nonimplementor() {
    // `implements baml.ToString {}` with no override uses the interface's
    // default body, which is the same structural rendering as the `_` arm.
    let with_default = expect_string(
        r#"
        class Point {
            x int
            y int
            implements baml.ToString {}
        }
        function main() -> string {
            return string.from(Point { x: 1, y: 2 })
        }
    "#,
    )
    .await;
    let without = expect_string(
        r#"
        class Point { x int  y int }
        function main() -> string {
            return string.from(Point { x: 1, y: 2 })
        }
    "#,
    )
    .await;
    assert_eq!(with_default, without);
    assert_eq!(with_default, "Point { x: 1, y: 2 }");
}

#[tokio::test]
async fn builtin_instant_dispatches_through_interface() {
    // `Instant` was migrated from a magic `to_string` method to
    // `implements baml.ToString`; both a direct call and `string.from` must
    // dispatch to it (and the formatter is infallible for an in-range value).
    assert_eq!(
        expect_string(
            r#"function main() -> string { return baml.time.Instant.epoch().to_string() }"#
        )
        .await,
        "1970-01-01T00:00:00Z"
    );
    assert_eq!(
        expect_string(
            r#"function main() -> string { return string.from(baml.time.Instant.epoch()) }"#
        )
        .await,
        "1970-01-01T00:00:00Z"
    );
}

/// A `Point` with a `to_string` override, reused by the nested-override tests.
const POINT_OVERRIDE: &str = r#"
    class Point {
        x int
        y int
        implements baml.ToString {
            function to_string(self) -> string throws never {
                "(" + string.from(self.x) + ", " + string.from(self.y) + ")"
            }
        }
    }
"#;

#[tokio::test]
async fn nested_override_in_array_is_honored() {
    // Elements whose runtime class overrides `to_string` render via the override,
    // not structurally — the whole point of honoring overrides at depth.
    let out = expect_string(&format!(
        r#"
        {POINT_OVERRIDE}
        function main() -> string {{
            return string.from([Point {{ x: 1, y: 2 }}, Point {{ x: 3, y: 4 }}])
        }}
    "#
    ))
    .await;
    assert_eq!(out, "[(1, 2), (3, 4)]");
}

#[tokio::test]
async fn nested_override_in_class_field_is_honored() {
    // The outer class does NOT implement ToString (renders structurally), but its
    // fields whose class overrides `to_string` are rendered via the override.
    let out = expect_string(&format!(
        r#"
        {POINT_OVERRIDE}
        class Line {{ start Point  end Point }}
        function main() -> string {{
            return string.from(Line {{ start: Point {{ x: 1, y: 2 }}, end: Point {{ x: 3, y: 4 }} }})
        }}
    "#
    ))
    .await;
    assert_eq!(out, "Line { start: (1, 2), end: (3, 4) }");
}

#[tokio::test]
async fn override_and_structural_siblings_stay_aligned() {
    // Mixed override / non-override fields exercise the pre-order counter: the
    // structural `n` must not consume an override result meant for `p` / `q`.
    let out = expect_string(&format!(
        r#"
        {POINT_OVERRIDE}
        class Mixed {{ p Point  n int  q Point }}
        function main() -> string {{
            return string.from(Mixed {{ p: Point {{ x: 1, y: 2 }}, n: 99, q: Point {{ x: 3, y: 4 }} }})
        }}
    "#
    ))
    .await;
    assert_eq!(out, "Mixed { p: (1, 2), n: 99, q: (3, 4) }");
}

#[tokio::test]
async fn nested_override_in_map_is_honored() {
    let out = expect_string(&format!(
        r#"
        {POINT_OVERRIDE}
        function main() -> string {{
            let m: map<string, Point> = {{ "a": Point {{ x: 1, y: 2 }} }}
            return string.from(m)
        }}
    "#
    ))
    .await;
    assert_eq!(out, "{\"a\": (1, 2)}");
}

#[tokio::test]
async fn nested_override_inside_array_inside_class() {
    // Override two levels down: a non-overriding class holding an array of
    // overriding elements.
    let out = expect_string(&format!(
        r#"
        {POINT_OVERRIDE}
        class Path {{ points Point[] }}
        function main() -> string {{
            return string.from(Path {{ points: [Point {{ x: 1, y: 2 }}, Point {{ x: 3, y: 4 }}] }})
        }}
    "#
    ))
    .await;
    assert_eq!(out, "Path { points: [(1, 2), (3, 4)] }");
}

#[tokio::test]
async fn direct_method_call_on_implementor() {
    // The interface method is also directly callable on a concrete implementor.
    let out = expect_string(
        r#"
        class Greeter {
            name string
            implements baml.ToString {
                function to_string(self) -> string throws never {
                    "Hello, " + self.name
                }
            }
        }
        function main() -> string {
            let g = Greeter { name: "world" }
            return g.to_string()
        }
    "#,
    )
    .await;
    assert_eq!(out, "Hello, world");
}
