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
                function from_json(j: baml.json.json) -> Self throws baml.json.JsonParseError | baml.json.JsonDecodeError {
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
                function from_json(j: baml.json.json) -> Self throws baml.json.JsonParseError | baml.json.JsonDecodeError {
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
                function from_json(j: baml.json.json) -> Self throws baml.json.JsonParseError | baml.json.JsonDecodeError {
                    Temp { celsius: baml.json.to<float>(baml.json.field(j, "celsius")) }
                }
            }
        }
        function main() -> bool throws baml.json.JsonSerializationError | baml.json.JsonParseError | baml.json.JsonDecodeError {
            let original: Temp = Temp { celsius: 36.6 }
            let j: baml.json.json = baml.json.from(original)
            let decoded: Temp = baml.json.to<Temp>(j)
            decoded.celsius == 36.6
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
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
