//! Tests for the `baml.FromJson` interface and the `baml.json.to<T>` driver
//! (the deserialize counterpart of `baml.ToJson` / `baml.json.from`).
//!
//! `baml.json.to<T>(j)` decodes a `json` value into a `T`:
//!   - if `T` has an `implements baml.FromJson` override, construct via it;
//!   - otherwise decode structurally (per-field).
//!
//! F1 is additive: the magic auto-derived `from_json` still coexists, so the
//! structural path is still provided by it. These tests pin the decoded result.

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
fn from_json_interface_compiles() {
    // The no-`self` `from_json(j) -> Self` interface method + an in-body impl
    // must typecheck cleanly.
    let errors = compile_errors(
        r#"
        class Temp {
            celsius float
            implements baml.FromJson {
                function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError {
                    Temp { celsius: baml.json.to<float>(baml.json.field(j, "c")) }
                }
            }
        }
        function main() -> int { 1 }
        "#,
    );
    assert!(
        errors.is_empty(),
        "implementing baml.FromJson should compile clean; got:\n  {}",
        errors.join("\n  ")
    );
}

#[tokio::test]
async fn to_dispatches_fromjson_override() {
    // `baml.json.to<Temp>(j)` resolves Temp's `baml.FromJson` override, which
    // reads the `"c"` field rather than a structural `celsius` field.
    let output = baml_test!(
        r#"
        class Temp {
            celsius float
            implements baml.FromJson {
                function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError {
                    Temp { celsius: baml.json.to<float>(baml.json.field(j, "c")) }
                }
            }
        }
        function main() -> float throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j = baml.json.parse("{\"c\": 20.5}")
            let t: Temp = baml.json.to<Temp>(j)
            t.celsius
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Float(20.5));
}

#[tokio::test]
async fn to_decodes_non_implementor_structurally() {
    // A plain class (no FromJson impl) decodes structurally via `baml.json.to`.
    let output = baml_test!(
        r#"
        class User { name string  age int }
        function main() -> string throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j = baml.json.parse("{\"name\": \"Ada\", \"age\": 30}")
            let u: User = baml.json.to<User>(j)
            u.name
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Ada".into())
    );
}

#[tokio::test]
async fn to_decodes_primitive() {
    let output = baml_test!(
        r#"
        function main() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            baml.json.to<int>(baml.json.parse("42"))
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(42));
}

#[tokio::test]
async fn from_to_roundtrip_with_override() {
    // `json.to<T>(json.from(x))` round-trips, dispatching the FromJson override.
    let output = baml_test!(
        r#"
        class Temp {
            celsius float
            implements baml.FromJson {
                function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError {
                    Temp { celsius: baml.json.to<float>(baml.json.field(j, "celsius")) }
                }
            }
        }
        function main() -> bool throws baml.json.JsonSerializationError | baml.json.JsonDecodeError {
            let original: Temp = Temp { celsius: 36.6 }
            let j: baml.json.json = baml.json.from(original)
            let decoded: Temp = baml.json.to<Temp>(j)
            decoded.celsius == 36.6
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[tokio::test]
async fn nested_field_override_is_honored() {
    // `Outer` has no FromJson impl, so it decodes via the per-field Rust decoder;
    // its `Inner` field implements FromJson and must decode via its override
    // (which reads "wrapped", not a structural "v").
    let output = baml_test!(
        r#"
        class Inner {
            v int
            implements baml.FromJson {
                function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError {
                    Inner { v: baml.json.to<int>(baml.json.field(j, "wrapped")) }
                }
            }
        }
        class Outer { inner Inner  tag string }
        function main() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j = baml.json.parse("{\"inner\": {\"wrapped\": 7}, \"tag\": \"x\"}")
            let o: Outer = baml.json.to<Outer>(j)
            o.inner.v
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(7));
}

#[test]
fn empty_implementor_is_rejected() {
    // `from_json` is a required method (no default body), so an empty
    // `implements baml.FromJson {}` is a missing-required-method error. Non-
    // implementors get structural decode from `baml.json.to` instead.
    let errors = compile_errors(
        r#"
        class Point {
            x int
            y int
            implements baml.FromJson {}
        }
        function main() -> int { 1 }
        "#,
    );
    assert!(
        errors.iter().any(|e| e.contains("from_json")),
        "empty implements baml.FromJson should require from_json; got:\n  {}",
        errors.join("\n  ")
    );
}

// ── The `Type.from_json(j)` static-call sugar ──────────────────────────────
// These exercise the sugar form directly (the tests above use the `baml.json.to`
// driver). `Type.from_json(j)` desugars to `baml.json.to<Type>(j)`: there is no
// synthesized `from_json` method on the class.

#[tokio::test]
async fn static_from_json_decodes_non_implementor() {
    // `User.from_json(j)` (no FromJson impl) desugars to `baml.json.to<User>(j)`
    // and decodes structurally.
    let output = baml_test!(
        r#"
        class User { name string  age int }
        function main() -> string throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j = baml.json.parse("{\"name\": \"Ada\", \"age\": 30}")
            let u: User = User.from_json(j)
            u.name
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Ada".into())
    );
}

#[tokio::test]
async fn static_from_json_dispatches_override() {
    // An implementor's `Temp.from_json(j)` resolves Temp's own `baml.FromJson`
    // override (not the sugar) — it reads "c", not a structural "celsius".
    let output = baml_test!(
        r#"
        class Temp {
            celsius float
            implements baml.FromJson {
                function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError {
                    Temp { celsius: baml.json.to<float>(baml.json.field(j, "c")) }
                }
            }
        }
        function main() -> float throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j = baml.json.parse("{\"c\": 20.5}")
            let t: Temp = Temp.from_json(j)
            t.celsius
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Float(20.5));
}

#[tokio::test]
async fn static_from_json_threads_generic_type_arg() {
    // `Box<int>.from_json(j)` — the `<int>` parses as the call's type args and is
    // applied to the receiver, so it desugars to `baml.json.to<Box<int>>(j)` and
    // the field decodes as int (the case the `-> Self` delegate could not thread).
    let output = baml_test!(
        r#"
        class Box<T> { value T }
        function main() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j = baml.json.parse("{\"value\": 42}")
            let b: Box<int> = Box<int>.from_json(j)
            b.value
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(42));
}

#[test]
fn static_from_json_charges_only_json_decode_error() {
    // The sugar charges exactly `JsonDecodeError` (not the unaccounted-callee
    // `unknown`): a wrapper declaring only that throws compiles clean, and a
    // structural decode needs no `JsonParseError`.
    let errors = compile_errors(
        r#"
        class User { name string  age int }
        function decode(j: baml.json.json) -> User throws baml.json.JsonDecodeError {
            User.from_json(j)
        }
        function main() -> int { 1 }
        "#,
    );
    assert!(
        errors.is_empty(),
        "Type.from_json should charge only JsonDecodeError; got:\n  {}",
        errors.join("\n  ")
    );
}
