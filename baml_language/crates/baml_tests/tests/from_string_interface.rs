//! Tests for the `baml.FromString` interface and the `string.to<T>(s)` driver
//! (the inverse of `baml.ToString` / `string.from`).
//!
//! `string.to<T>(s)` parses the primitive types directly and dispatches every
//! other type to its `baml.FromString` `from_string` override, resolved on the
//! reified type argument `T` (mirroring `baml.json.from_json<T>`). Failure
//! throws `baml.StringParseError`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// A `Point` implementing `baml.FromString`. Its `from_string` parses `s` as an
/// int into `x` (and re-throws `StringParseError` on bad input), exercising both
/// dispatch and nested `string.to`.
const POINT_FROMSTRING: &str = r#"
    class Point {
        x int
        y int
        implements baml.FromString {
            function from_string(s: string) -> Self throws baml.errors.ParseError {
                Point { x: string.to<int>(s), y: 0 }
            }
        }
    }
"#;

async fn expect_int(src: &str) -> i64 {
    match baml_test!(src).result.unwrap() {
        BexExternalValue::Int(n) => n,
        other => panic!("expected int, got {other:?}"),
    }
}

async fn expect_string(src: &str) -> String {
    match baml_test!(src).result.unwrap() {
        BexExternalValue::String(s) => s.to_string(),
        other => panic!("expected string, got {other:?}"),
    }
}

#[tokio::test]
async fn to_parses_primitives() {
    assert_eq!(
        expect_int(r#"function main() -> int { return string.to<int>("42") }"#).await,
        42
    );
    assert_eq!(
        expect_int(r#"function main() -> int { return string.to<int>("-7") }"#).await,
        -7
    );
    assert!(matches!(
        baml_test!(r#"function main() -> bool { return string.to<bool>("true") }"#)
            .result
            .unwrap(),
        BexExternalValue::Bool(true)
    ));
    assert!(matches!(
        baml_test!(r#"function main() -> float { return string.to<float>("3.5") }"#)
            .result
            .unwrap(),
        BexExternalValue::Float(f) if (f - 3.5).abs() < 1e-9
    ));
}

#[tokio::test]
async fn to_string_is_identity() {
    assert_eq!(
        expect_string(r#"function main() -> string { return string.to<string>("hi") }"#).await,
        "hi"
    );
}

#[tokio::test]
async fn to_int_parse_failure_throws() {
    let output = baml_test!(r#"function main() -> int { return string.to<int>("abc") }"#);
    let Err(err) = &output.result else {
        panic!("expected a parse error, got: {:?}", output.result);
    };
    let msg = format!("{err:?}");
    assert!(
        msg.contains("ParseError"),
        "expected a ParseError, got: {msg}"
    );
}

#[tokio::test]
async fn implementor_from_string_is_dispatched() {
    let out = expect_int(&format!(
        r#"
        {POINT_FROMSTRING}
        function main() -> int {{
            let p: Point = string.to<Point>("42")
            return p.x
        }}
    "#
    ))
    .await;
    assert_eq!(out, 42);
}

#[tokio::test]
async fn implementor_from_string_propagates_parse_error() {
    let output = baml_test!(&format!(
        r#"
        {POINT_FROMSTRING}
        function main() -> int {{
            let p: Point = string.to<Point>("notanint")
            return p.x
        }}
    "#
    ));
    assert!(
        output.result.is_err(),
        "expected the nested int parse to throw, got: {:?}",
        output.result
    );
}

#[tokio::test]
async fn class_without_fromstring_throws() {
    let output = baml_test!(
        r#"
        class Bare { x int }
        function main() -> int {
            let b: Bare = string.to<Bare>("nope")
            return b.x
        }
    "#
    );
    let Err(err) = &output.result else {
        panic!("expected a parse error, got: {:?}", output.result);
    };
    let msg = format!("{err:?}");
    assert!(
        msg.contains("ParseError"),
        "expected a ParseError for a type with no FromString impl, got: {msg}"
    );
}

#[tokio::test]
async fn generic_driver_dispatches_by_typearg() {
    // The real use case: `string.to<T>` inside a generic function, T resolved at
    // runtime from the call's reified type arg.
    let out = expect_int(&format!(
        r#"
        {POINT_FROMSTRING}
        function parse_it<T extends baml.FromString>(s: string) -> T throws baml.errors.ParseError {{
            string.to<T>(s)
        }}
        function main() -> int {{
            let p: Point = parse_it<Point>("13")
            return p.x
        }}
    "#
    ))
    .await;
    assert_eq!(out, 13);
}

#[tokio::test]
async fn nested_class_override_is_dispatched() {
    // A class whose `from_string` parses a *nested* class via `string.to<Inner>`:
    // each `string.to` re-enters dispatch, so the inner override fires. (FromString
    // has no native structural parser to bypass, unlike ToString's renderer.)
    let out = expect_int(&format!(
        r#"
        {POINT_FROMSTRING}
        class Boxed {{
            inner Point
            implements baml.FromString {{
                function from_string(s: string) -> Self throws baml.errors.ParseError {{
                    Boxed {{ inner: string.to<Point>(s) }}
                }}
            }}
        }}
        function main() -> int {{
            let b: Boxed = string.to<Boxed>("55")
            return b.inner.x
        }}
    "#
    ))
    .await;
    assert_eq!(out, 55);
}

#[tokio::test]
async fn round_trips_with_string_from() {
    // string.to<Point>(string.from(int)) — the int renders to its digits, which
    // Point.from_string parses straight back.
    let out = expect_int(&format!(
        r#"
        {POINT_FROMSTRING}
        function main() -> int {{
            let p: Point = string.to<Point>(string.from(99))
            return p.x
        }}
    "#
    ))
    .await;
    assert_eq!(out, 99);
}
