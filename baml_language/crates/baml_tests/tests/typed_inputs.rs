//! Unified tests for function input argument handling.

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

    insta::assert_snapshot!(output.bytecode, @r"
    function double(x: int) -> int {
        load_var x
        load_const 2
        bin_op *
        return
    }
    ");
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

    insta::assert_snapshot!(output.bytecode, @r#"
    function greet(name: string) -> string {
        load_const "Hello, "
        load_var name
        bin_op +
        load_const "!"
        bin_op +
        return
    }
    "#);
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

    insta::assert_snapshot!(output.bytecode, @r"
    function add(a: int, b: int) -> int {
        load_var a
        load_var b
        bin_op +
        return
    }
    ");
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

    insta::assert_snapshot!(output.bytecode, @r"
    function negate(b: bool) -> bool {
        load_var b
        unary_op !
        return
    }
    ");
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

    insta::assert_snapshot!(output.bytecode, @r"
    function half(x: float) -> float {
        load_var x
        load_const 2
        bin_op /
        return
    }
    ");
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

    insta::assert_snapshot!(output.bytecode, @r"
    function sum(arr: int[]) -> int {
        load_const 0
        store_var total
        load_var arr
        call baml.Array.length
        store_var _len
        load_const 0
        store_var _i

      L0:
        load_var _i
        load_var _len
        cmp_op <
        pop_jump_if_false L1
        jump L2

      L1:
        load_var total
        return

      L2:
        load_var arr
        load_var _i
        load_array_element
        store_var x
        load_var _i
        load_const 1
        bin_op +
        store_var _i
        load_var total
        load_var x
        bin_op +
        store_var total
        jump L0
    }
    ");
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

    insta::assert_snapshot!(output.bytecode, @r"
    function magnitude_squared(p: Point) -> int {
        load_var p
        load_field .x
        load_var p
        load_field .x
        bin_op *
        load_var p
        load_field .y
        load_var p
        load_field .y
        bin_op *
        bin_op +
        return
    }
    ");
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

    insta::assert_snapshot!(output.bytecode, @r"
    function get_value(m: map<string, int>, key: string) -> int {
        load_var m
        load_var key
        load_map_element
        return
    }
    ");
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

    insta::assert_snapshot!(output.bytecode, @r"
    function is_null(x: int?) -> bool {
        load_var x
        load_const null
        cmp_op ==
        return
    }
    ");
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

    insta::assert_snapshot!(output.bytecode, @r#"
    function concat(a: string, b: string, c: string) -> string {
        load_var a
        load_const " "
        bin_op +
        load_var b
        bin_op +
        load_const " "
        bin_op +
        load_var c
        bin_op +
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hello from BAML".to_string()))
    );
}
