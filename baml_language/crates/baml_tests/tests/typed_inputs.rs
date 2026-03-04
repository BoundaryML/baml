//! Tests for function input argument handling via BexExternalValue.

use baml_tests::baml_test;
use bex_engine::{BexExternalValue, Ty};

#[tokio::test]
async fn int_input() {
    let output = baml_test! {
        baml: r#"
            function double(x: int) -> int {
                x * 2
            }
        "#,
        entry: "double",
        args: { "x" => BexExternalValue::Int(21) },
    };
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn string_input() {
    let output = baml_test! {
        baml: r#"
            function greet(name: string) -> string {
                "Hello, " + name + "!"
            }
        "#,
        entry: "greet",
        args: { "name" => BexExternalValue::String("World".to_string()) },
    };
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hello, World!".to_string()))
    );
}

#[tokio::test]
async fn multiple_inputs() {
    let output = baml_test! {
        baml: r#"
            function add(a: int, b: int) -> int {
                a + b
            }
        "#,
        entry: "add",
        args: { "a" => BexExternalValue::Int(10), "b" => BexExternalValue::Int(32) },
    };
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn bool_input() {
    let output = baml_test! {
        baml: r#"
            function negate(b: bool) -> bool {
                !b
            }
        "#,
        entry: "negate",
        args: { "b" => BexExternalValue::Bool(true) },
    };
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn float_input() {
    let output = baml_test! {
        baml: r#"
            function half(x: float) -> float {
                x / 2.0
            }
        "#,
        entry: "half",
        args: { "x" => BexExternalValue::Float(10.0) },
    };
    assert_eq!(output.result, Ok(BexExternalValue::Float(5.0)));
}

#[tokio::test]
async fn array_input() {
    let output = baml_test! {
        baml: r#"
            function sum(arr: int[]) -> int {
                let total = 0;
                for (let x in arr) {
                    total += x;
                }
                total
            }
        "#,
        entry: "sum",
        args: {
            "arr" => BexExternalValue::Array {
                element_type: Ty::int(),
                items: vec![
                    BexExternalValue::Int(1),
                    BexExternalValue::Int(2),
                    BexExternalValue::Int(3),
                    BexExternalValue::Int(4),
                ],
            }
        },
    };
    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

#[tokio::test]
async fn class_input() {
    let output = baml_test! {
        baml: r#"
            class Point {
                x int
                y int
            }

            function magnitude_squared(p: Point) -> int {
                p.x * p.x + p.y * p.y
            }
        "#,
        entry: "magnitude_squared",
        args: {
            "p" => BexExternalValue::Instance {
                class_name: "Point".to_string(),
                fields: indexmap::indexmap! {
                    "x".to_string() => BexExternalValue::Int(3),
                    "y".to_string() => BexExternalValue::Int(4),
                },
            }
        },
    };
    assert_eq!(output.result, Ok(BexExternalValue::Int(25)));
}

#[tokio::test]
async fn map_input() {
    let output = baml_test! {
        baml: r#"
            function get_value(m: map<string, int>, key: string) -> int {
                m[key]
            }
        "#,
        entry: "get_value",
        args: {
            "m" => BexExternalValue::Map {
                key_type: Ty::string(),
                value_type: Ty::int(),
                entries: indexmap::indexmap! {
                    "foo".to_string() => BexExternalValue::Int(42),
                    "bar".to_string() => BexExternalValue::Int(100),
                },
            },
            "key" => BexExternalValue::String("foo".to_string())
        },
    };
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn null_input() {
    let output = baml_test! {
        baml: r#"
            function is_null(x: int?) -> bool {
                x == null
            }
        "#,
        entry: "is_null",
        args: { "x" => BexExternalValue::Null },
    };
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn mixed_types_inputs() {
    let output = baml_test! {
        baml: r#"
            function concat(a: string, b: string, c: string) -> string {
                a + " " + b + " " + c
            }
        "#,
        entry: "concat",
        args: {
            "a" => BexExternalValue::String("Hello".to_string()),
            "b" => BexExternalValue::String("from".to_string()),
            "c" => BexExternalValue::String("BAML".to_string())
        },
    };
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hello from BAML".to_string()))
    );
}
