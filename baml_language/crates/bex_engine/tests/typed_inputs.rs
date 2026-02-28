//! Tests for function input argument handling.
//!
//! These tests verify that `call_function` properly passes external arguments
//! to BAML functions via `BexExternalValue`.

mod common;

use baml_type::TyAttr;
use bex_engine::{BexExternalValue, Ty};
use common::{EngineProgram, assert_engine_executes};
use indexmap::indexmap;

#[tokio::test]
async fn int_input() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function double(x: int) -> int {
                x * 2
            }
        "#,
        entry: "double",
        inputs: vec![BexExternalValue::Int(21)],
        expected: Ok(BexExternalValue::Int(42)),
    })
    .await
}

#[tokio::test]
async fn string_input() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function greet(name: string) -> string {
                "Hello, " + name + "!"
            }
        "#,
        entry: "greet",
        inputs: vec![BexExternalValue::String("World".to_string())],
        expected: Ok(BexExternalValue::String("Hello, World!".to_string())),
    })
    .await
}

#[tokio::test]
async fn multiple_inputs() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function add(a: int, b: int) -> int {
                a + b
            }
        "#,
        entry: "add",
        inputs: vec![BexExternalValue::Int(10), BexExternalValue::Int(32)],
        expected: Ok(BexExternalValue::Int(42)),
    })
    .await
}

#[tokio::test]
async fn bool_input() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function negate(b: bool) -> bool {
                !b
            }
        "#,
        entry: "negate",
        inputs: vec![BexExternalValue::Bool(true)],
        expected: Ok(BexExternalValue::Bool(false)),
    })
    .await
}

#[tokio::test]
async fn float_input() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function half(x: float) -> float {
                x / 2.0
            }
        "#,
        entry: "half",
        inputs: vec![BexExternalValue::Float(10.0)],
        expected: Ok(BexExternalValue::Float(5.0)),
    })
    .await
}

#[tokio::test]
async fn array_input() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function sum(arr: int[]) -> int {
                let total = 0;
                for (let x in arr) {
                    total += x;
                }
                total
            }
        "#,
        entry: "sum",
        inputs: vec![BexExternalValue::Array {
            element_type: Ty::Int {
                attr: TyAttr::default(),
            },
            items: vec![
                BexExternalValue::Int(1),
                BexExternalValue::Int(2),
                BexExternalValue::Int(3),
                BexExternalValue::Int(4),
            ],
        }],
        expected: Ok(BexExternalValue::Int(10)),
    })
    .await
}

// TODO: Enable when Instance allocation from BexExternalValue is implemented
// #[tokio::test]
// async fn class_input() -> anyhow::Result<()> {
//     assert_engine_executes(EngineProgram {
//         fs: indexmap! {},
//         source: r#"
//             class Point {
//                 x int
//                 y int
//             }
//
//             function magnitude_squared(p: Point) -> int {
//                 p.x * p.x + p.y * p.y
//             }
//         "#,
//         entry: "magnitude_squared",
//         inputs: vec![BexExternalValue::Instance {
//             class_name: "Point".to_string(),
//             fields: indexmap! {
//                 "x".to_string() => BexExternalValue::Int(3),
//                 "y".to_string() => BexExternalValue::Int(4),
//             },
//         }],
//         expected: Ok(BexExternalValue::Int(25)),
//     })
//     .await
// }

#[tokio::test]
async fn map_input() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function get_value(m: map<string, int>, key: string) -> int {
                m[key]
            }
        "#,
        entry: "get_value",
        inputs: vec![
            BexExternalValue::Map {
                key_type: Ty::String {
                    attr: TyAttr::default(),
                },
                value_type: Ty::Int {
                    attr: TyAttr::default(),
                },
                entries: indexmap! {
                    "foo".to_string() => BexExternalValue::Int(42),
                    "bar".to_string() => BexExternalValue::Int(100),
                },
            },
            BexExternalValue::String("foo".to_string()),
        ],
        expected: Ok(BexExternalValue::Int(42)),
    })
    .await
}

#[tokio::test]
async fn null_input() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function is_null(x: int?) -> bool {
                x == null
            }
        "#,
        entry: "is_null",
        inputs: vec![BexExternalValue::Null],
        expected: Ok(BexExternalValue::Bool(true)),
    })
    .await
}

#[tokio::test]
async fn mixed_types_inputs() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function concat(a: string, b: string, c: string) -> string {
                a + " " + b + " " + c
            }
        "#,
        entry: "concat",
        inputs: vec![
            BexExternalValue::String("Hello".to_string()),
            BexExternalValue::String("from".to_string()),
            BexExternalValue::String("BAML".to_string()),
        ],
        expected: Ok(BexExternalValue::String("Hello from BAML".to_string())),
    })
    .await
}
