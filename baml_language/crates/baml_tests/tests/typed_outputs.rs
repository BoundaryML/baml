//! Unified tests for union type handling in BexExternalValue.

use baml_tests::baml_test;
use bex_engine::{BexExternalValue, Ty};
use indexmap::indexmap;

#[tokio::test]
async fn union_int_or_string_returns_int() {
    let output = baml_test!(
        r#"
            function main() -> int | string {
                42
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int | string {
        load_const 42
        return
    }
    ");
    assert_eq!(
        output.result,
        Ok(BexExternalValue::union(
            BexExternalValue::Int(42),
            [Ty::int(), Ty::string()],
            Ty::int()
        ))
    );
}

#[tokio::test]
async fn union_int_or_string_returns_string() {
    let output = baml_test!(
        r#"
            function main() -> int | string {
                "hello"
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int | string {
        load_const "hello"
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::union(
            BexExternalValue::String("hello".to_string()),
            [Ty::int(), Ty::string()],
            Ty::string()
        ))
    );
}

#[tokio::test]
async fn optional_int_returns_value() {
    let output = baml_test!(
        r#"
            function main() -> int? {
                42
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int? {
        load_const 42
        return
    }
    ");
    assert_eq!(
        output.result,
        Ok(BexExternalValue::optional(
            BexExternalValue::Int(42),
            Ty::int()
        ))
    );
}

#[tokio::test]
async fn optional_int_returns_null() {
    let output = baml_test!(
        r#"
            function main() -> int? {
                null
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int? {
        load_const null
        return
    }
    ");
    assert_eq!(
        output.result,
        Ok(BexExternalValue::optional(
            BexExternalValue::Null,
            Ty::int()
        ))
    );
}

#[tokio::test]
async fn class_with_union_field() {
    let output = baml_test!(
        r#"
            class Response {
                data int | string
            }

            function main() -> Response {
                Response { data: 42 }
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> Response {
        alloc_instance Response
        copy 0
        load_const 42
        store_field .data
        return
    }
    ");
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Instance {
            class_name: "Response".to_string(),
            fields: indexmap! {
                "data".to_string() => BexExternalValue::union(BexExternalValue::Int(42), [Ty::int(), Ty::string()], Ty::int()),
            },
        })
    );
}

#[tokio::test]
async fn union_of_classes_returns_success() {
    let output = baml_test!(
        r#"
            class Success {
                value int
            }

            class Failure {
                error string
            }

            function main() -> Success | Failure {
                Success { value: 42 }
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> Success | Failure {
        alloc_instance Success
        copy 0
        load_const 42
        store_field .value
        return
    }
    ");
    assert_eq!(
        output.result,
        Ok(BexExternalValue::union(
            BexExternalValue::Instance {
                class_name: "Success".to_string(),
                fields: indexmap! { "value".to_string() => BexExternalValue::Int(42) },
            },
            [Ty::class("Success"), Ty::class("Failure")],
            Ty::class("Success"),
        ))
    );
}

#[tokio::test]
async fn union_of_classes_returns_failure() {
    let output = baml_test!(
        r#"
            class Success {
                value int
            }

            class Failure {
                error string
            }

            function main() -> Success | Failure {
                Failure { error: "something went wrong" }
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> Success | Failure {
        alloc_instance Failure
        copy 0
        load_const "something went wrong"
        store_field .error
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::union(
            BexExternalValue::Instance {
                class_name: "Failure".to_string(),
                fields: indexmap! { "error".to_string() => BexExternalValue::String("something went wrong".to_string()) },
            },
            [Ty::class("Success"), Ty::class("Failure")],
            Ty::class("Failure"),
        ))
    );
}

#[tokio::test]
async fn union_of_arrays() {
    let output = baml_test!(
        r#"
            function main() -> int[] | string[] {
                [1, 2, 3]
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int[] | string[] {
        load_const 1
        load_const 2
        load_const 3
        alloc_array 3
        return
    }
    ");
    assert_eq!(
        output.result,
        Ok(BexExternalValue::union(
            BexExternalValue::Array {
                element_type: Ty::int(),
                items: vec![
                    BexExternalValue::Int(1),
                    BexExternalValue::Int(2),
                    BexExternalValue::Int(3)
                ],
            },
            [Ty::list(Ty::int()), Ty::list(Ty::string())],
            Ty::list(Ty::int()),
        ))
    );
}

#[tokio::test]
async fn array_of_unions() {
    let output = baml_test!(
        r#"
            function main() -> (int | string)[] {
                [1, "two", 3]
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int | string[] {
        load_const 1
        load_const "two"
        load_const 3
        alloc_array 3
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::union([Ty::int(), Ty::string()]),
            items: vec![
                BexExternalValue::union(
                    BexExternalValue::Int(1),
                    [Ty::int(), Ty::string()],
                    Ty::int()
                ),
                BexExternalValue::union(
                    BexExternalValue::String("two".to_string()),
                    [Ty::int(), Ty::string()],
                    Ty::string()
                ),
                BexExternalValue::union(
                    BexExternalValue::Int(3),
                    [Ty::int(), Ty::string()],
                    Ty::int()
                ),
            ],
        })
    );
}

#[tokio::test]
async fn optional_class() {
    let output = baml_test!(
        r#"
            class Data {
                value int
            }

            function main() -> Data? {
                Data { value: 42 }
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> Data? {
        alloc_instance Data
        copy 0
        load_const 42
        store_field .value
        return
    }
    ");
    assert_eq!(
        output.result,
        Ok(BexExternalValue::optional(
            BexExternalValue::Instance {
                class_name: "Data".to_string(),
                fields: indexmap! { "value".to_string() => BexExternalValue::Int(42) },
            },
            Ty::class("Data"),
        ))
    );
}

#[tokio::test]
async fn optional_class_returns_null() {
    let output = baml_test!(
        r#"
            class Data {
                value int
            }

            function main() -> Data? {
                null
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> Data? {
        load_const null
        return
    }
    ");
    assert_eq!(
        output.result,
        Ok(BexExternalValue::optional(
            BexExternalValue::Null,
            Ty::class("Data")
        ))
    );
}

#[tokio::test]
async fn class_with_optional_field() {
    let output = baml_test!(
        r#"
            class Person {
                name string
                age int?
            }

            function main() -> Person {
                Person { name: "Alice", age: null }
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> Person {
        alloc_instance Person
        copy 0
        load_const "Alice"
        store_field .name
        copy 0
        load_const null
        store_field .age
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Instance {
            class_name: "Person".to_string(),
            fields: indexmap! {
                "name".to_string() => BexExternalValue::String("Alice".to_string()),
                "age".to_string() => BexExternalValue::optional(BexExternalValue::Null, Ty::int()),
            },
        })
    );
}

#[tokio::test]
async fn map_with_union_values() {
    let output = baml_test!(
        r#"
            function main() -> map<string, int | string> {
                {"count": 42, "name": "test"}
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> map<string, int | string> {
        load_const 42
        load_const "test"
        load_const "count"
        load_const "name"
        alloc_map 2
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Map {
            key_type: Ty::string(),
            value_type: Ty::union([Ty::int(), Ty::string()]),
            entries: indexmap! {
                "count".to_string() => BexExternalValue::union(BexExternalValue::Int(42), [Ty::int(), Ty::string()], Ty::int()),
                "name".to_string() => BexExternalValue::union(BexExternalValue::String("test".to_string()), [Ty::int(), Ty::string()], Ty::string()),
            },
        })
    );
}

#[tokio::test]
async fn union_of_array_with_union_elements_or_string() {
    // Tests that selected_option uses declared type, not inferred from values
    let output = baml_test!(
        r#"
            function main() -> (int | bool)[] | string {
                [1, true, 2]
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int | bool[] | string {
        load_const 1
        load_const true
        load_const 2
        alloc_array 3
        return
    }
    ");
    assert_eq!(
        output.result,
        Ok(BexExternalValue::union(
            BexExternalValue::Array {
                element_type: Ty::union([Ty::int(), Ty::bool()]),
                items: vec![
                    BexExternalValue::union(
                        BexExternalValue::Int(1),
                        [Ty::int(), Ty::bool()],
                        Ty::int()
                    ),
                    BexExternalValue::union(
                        BexExternalValue::Bool(true),
                        [Ty::int(), Ty::bool()],
                        Ty::bool()
                    ),
                    BexExternalValue::union(
                        BexExternalValue::Int(2),
                        [Ty::int(), Ty::bool()],
                        Ty::int()
                    ),
                ],
            },
            [Ty::list(Ty::union([Ty::int(), Ty::bool()])), Ty::string()],
            Ty::list(Ty::union([Ty::int(), Ty::bool()])),
        ))
    );
}
