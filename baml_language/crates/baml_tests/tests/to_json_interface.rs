//! Tests for the `baml.ToJson` interface and the `baml.json.from<T>` driver.
//!
//! `baml.json.from(value)` resolves `to_json` on `value`'s *runtime* class:
//!   - if that class has an `implements baml.ToJson` override, dispatch to it;
//!   - otherwise render structurally via `baml._to_json_default`, which is also
//!     the interface's default `to_json` body.
//!
//! Runtime-class dispatch (`baml._to_json_shim`) is used instead of a static
//! `match (value) { baml.ToJson => ... }` for the same package-boundary reason
//! `string.from` uses `_to_string_shim` — see `baml.json.from`. Each test pins
//! the serialized text (`baml.json.stringify(baml.json.from(v))`) so the json
//! shape is asserted exactly.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// Run `main` (which must return `string`) and return the produced text.
async fn expect_json(src: &str) -> String {
    let output = baml_test!(src);
    match output.result.unwrap() {
        BexExternalValue::String(s) => s.to_string(),
        other => panic!("expected a string, got {other:?}"),
    }
}

/// Compile errors raised in the user file, as `[CODE] message`.
fn compile_errors(source: &str) -> Vec<String> {
    use baml_compiler_diagnostics::Severity;
    use baml_project::{collect_diagnostics, testing::setup_test_db};
    let db = setup_test_db(source);
    collect_diagnostics(&db)
        .into_iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .map(|d| format!("[{}] {}", d.code(), d.message))
        .collect()
}

#[test]
fn direct_to_json_method_is_banned() {
    let errors = compile_errors(
        r#"
        class Point {
            x int
            function to_json(self) -> baml.json.json throws never { 1 }
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("to_json") && e.contains("baml.ToJson")),
        "expected a to_json ban error recommending baml.ToJson; got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn to_json_via_interface_is_allowed() {
    let errors = compile_errors(
        r#"
        class Point {
            x int
            implements baml.ToJson {
                function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError { 1 }
            }
        }
    "#,
    );
    assert!(
        !errors.iter().any(|e| e.contains("to_json")),
        "implementing baml.ToJson must not be flagged; got:\n  {}",
        errors.join("\n  ")
    );
}

#[tokio::test]
async fn operator_style_to_json_desugars_to_from() {
    // `obj.to_json()` on a non-implementor has no real method; it desugars to
    // `baml.json.from(obj)` (structural), the json analog of `obj.to_string()`.
    let out = expect_json(&program(
        "class User { name string  age int }",
        r#"User { name: "Ada", age: 30 }"#,
    ))
    .await;
    let via_method = expect_json(
        r#"
        class User { name string  age int }
        function main() -> string {
            let u = User { name: "Ada", age: 30 }
            return baml.json.stringify(u.to_json())
        }
        "#,
    )
    .await;
    assert_eq!(via_method, out);
    assert_eq!(via_method, r#"{"name":"Ada","age":30}"#);
}

#[tokio::test]
async fn operator_style_to_json_dispatches_override() {
    // `obj.to_json()` on an implementor resolves the real interface method.
    let out = expect_json(
        r#"
        class Point {
            x int
            y int
            implements baml.ToJson {
                function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
                    { "pt": [baml.json.from(self.x), baml.json.from(self.y)] }
                }
            }
        }
        function main() -> string {
            let p = Point { x: 1, y: 2 }
            return baml.json.stringify(p.to_json())
        }
        "#,
    )
    .await;
    assert_eq!(out, r#"{"pt":[1,2]}"#);
}

/// `baml.json.stringify(baml.json.from(<expr>))` wrapped in a `main`.
fn program(decls: &str, expr: &str) -> String {
    format!(
        r#"
        {decls}
        function main() -> string {{
            return baml.json.stringify(baml.json.from({expr}))
        }}
        "#
    )
}

#[tokio::test]
async fn default_renders_primitives() {
    assert_eq!(expect_json(&program("", "42")).await, "42");
    assert_eq!(expect_json(&program("", "true")).await, "true");
    assert_eq!(expect_json(&program("", "null")).await, "null");
    assert_eq!(expect_json(&program("", r#""hi""#)).await, r#""hi""#);
    assert_eq!(expect_json(&program("", "3.5")).await, "3.5");
}

#[tokio::test]
async fn default_renders_containers() {
    assert_eq!(expect_json(&program("", "[1, 2, 3]")).await, "[1,2,3]");
    assert_eq!(
        expect_json(&program("", r#"{"a": 1, "b": 2}"#)).await,
        r#"{"a":1,"b":2}"#
    );
}

#[tokio::test]
async fn default_renders_class_instance_structurally() {
    // A class that does NOT implement ToJson renders as a field map.
    let out = expect_json(&program(
        "class User { name string  age int }",
        r#"User { name: "Ada", age: 30 }"#,
    ))
    .await;
    assert_eq!(out, r#"{"name":"Ada","age":30}"#);
}

#[tokio::test]
async fn empty_implementor_matches_nonimplementor() {
    // `implements baml.ToJson {}` with no override uses the interface's default
    // body — the same structural rendering a non-implementor gets.
    let with_default = expect_json(&program(
        r#"
        class Point {
            x int
            y int
            implements baml.ToJson {}
        }
        "#,
        "Point { x: 1, y: 2 }",
    ))
    .await;
    let without = expect_json(&program(
        "class Point { x int  y int }",
        "Point { x: 1, y: 2 }",
    ))
    .await;
    assert_eq!(with_default, without);
    assert_eq!(with_default, r#"{"x":1,"y":2}"#);
}

#[tokio::test]
async fn implementor_override_is_used() {
    // A class implementing ToJson with a custom body: `baml.json.from` resolves
    // `to_json` on the value's runtime class and dispatches to the override.
    let out = expect_json(&program(
        r#"
        class Point {
            x int
            y int
            implements baml.ToJson {
                function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
                    { "pt": [baml.json.from(self.x), baml.json.from(self.y)] }
                }
            }
        }
        "#,
        "Point { x: 1, y: 2 }",
    ))
    .await;
    assert_eq!(out, r#"{"pt":[1,2]}"#);
}

#[tokio::test]
async fn nested_override_is_honored_at_depth() {
    // An override-bearing instance nested inside a structurally-rendered class
    // must render via its override, not structurally.
    let out = expect_json(&program(
        r#"
        class Inner {
            v int
            implements baml.ToJson {
                function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
                    { "wrapped": baml.json.from(self.v) }
                }
            }
        }
        class Outer { inner Inner  tag string }
        "#,
        r#"Outer { inner: Inner { v: 7 }, tag: "x" }"#,
    ))
    .await;
    assert_eq!(out, r#"{"inner":{"wrapped":7},"tag":"x"}"#);
}

#[tokio::test]
async fn override_inside_container_is_honored() {
    // Override-bearing instances inside a list each render via their override.
    let out = expect_json(&program(
        r#"
        class Tag {
            name string
            implements baml.ToJson {
                function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
                    baml.json.from(self.name)
                }
            }
        }
        "#,
        r#"[Tag { name: "a" }, Tag { name: "b" }]"#,
    ))
    .await;
    assert_eq!(out, r#"["a","b"]"#);
}

#[tokio::test]
async fn enum_renders_as_variant_name() {
    let out = expect_json(&program("enum Color { Red  Green  Blue }", "Color.Green")).await;
    assert_eq!(out, r#""Green""#);
}

// ─── B-728: enum & string-literal-union fields in the value→JSON dispatch ──────
//
// A class field of `enum` type or a string-literal union (`"X" | "Y"`) must
// serialize to a plain JSON string — the variant name / the string — through
// both the structural `baml.json.from` / `.to_json()` path and the typed
// `baml.json.to_string<T>` path. The regression these lock in: a field whose
// declared type is a *union containing an enum* (including the optional enum
// `E?` == `E | null`) serialized as `null` through the typed walker, whose
// `RuntimeTy::Union` arm delegates to the untyped `value_to_serde`, which
// mapped enum variants to `null` instead of the variant-name string.

#[tokio::test]
async fn enum_field_serializes_as_variant_name() {
    // Primary ticket case: `class C { e E }` round-trips through `.to_json()`.
    let out = expect_json(&program("enum E { A  B }  class C { e E }", "C { e: E.A }")).await;
    assert_eq!(out, r#"{"e":"A"}"#);
}

#[tokio::test]
async fn string_literal_union_field_serializes_as_string() {
    // Primary ticket case: a `"X" | "Y"` field round-trips through `.to_json()`
    // as the plain string.
    let out = expect_json(&program(r#"class D { f "X" | "Y" }"#, r#"D { f: "X" }"#)).await;
    assert_eq!(out, r#"{"f":"X"}"#);
}

#[tokio::test]
async fn optional_enum_field_serializes_via_typed_to_string() {
    // Regression: `E?` == `E | null` is a union, so the typed `to_string<T>`
    // walker rendered it via the untyped converter, which produced `null` for
    // the enum variant. It must be the variant name.
    let out = expect_json(
        r#"
        enum E { A  B }
        class OptEnum { e E? }
        function main() -> string {
            return baml.json.to_string(OptEnum { e: E.A })
        }
        "#,
    )
    .await;
    assert_eq!(out, r#"{"e":"A"}"#);
}

#[tokio::test]
async fn enum_union_field_serializes_via_typed_to_string() {
    // Regression: a field typed as a union of two enums (`E | F`) serialized the
    // held variant as `null` through the typed walker. It must be the name.
    let out = expect_json(
        r#"
        enum E { A  B }
        enum F { X  Y }
        class EnumUnion { v E | F }
        function main() -> string {
            return baml.json.to_string(EnumUnion { v: F.Y })
        }
        "#,
    )
    .await;
    assert_eq!(out, r#"{"v":"Y"}"#);
}
