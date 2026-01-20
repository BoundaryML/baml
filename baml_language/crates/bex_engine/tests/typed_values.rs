//! Tests for `TypedSnapshot` with union types.
//!
//! These tests verify that `call_function` returns proper type information,
//! especially for union return types where the declared type differs from
//! the actual runtime value's type.

mod common;

use bex_engine::{Snapshot, Ty, TypedSnapshot};
use common::{TypedEngineProgram, assert_engine_typed};
use indexmap::indexmap;

#[tokio::test]
async fn test_primitive_int() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> int {
                42
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::Int(42),
            declared_type: Ty::Int,
        }),
    })
    .await
}

#[tokio::test]
async fn test_primitive_string() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> string {
                "hello"
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::String("hello".to_string()),
            declared_type: Ty::String,
        }),
    })
    .await
}

#[tokio::test]
async fn test_array_of_ints() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> int[] {
                [1, 2, 3]
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::Array(vec![
                TypedSnapshot {
                    value: Snapshot::Int(1),
                    declared_type: Ty::Int,
                },
                TypedSnapshot {
                    value: Snapshot::Int(2),
                    declared_type: Ty::Int,
                },
                TypedSnapshot {
                    value: Snapshot::Int(3),
                    declared_type: Ty::Int,
                },
            ]),
            declared_type: Ty::List(Box::new(Ty::Int)),
        }),
    })
    .await
}

#[tokio::test]
async fn test_union_int_or_string_returns_int() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> int | string {
                42
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::Int(42),
            declared_type: Ty::Union(vec![Ty::Int, Ty::String]),
        }),
    })
    .await
}

#[tokio::test]
async fn test_union_int_or_string_returns_string() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> int | string {
                "hello"
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::String("hello".to_string()),
            declared_type: Ty::Union(vec![Ty::Int, Ty::String]),
        }),
    })
    .await
}

#[tokio::test]
async fn test_optional_int_returns_value() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> int? {
                42
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::Int(42),
            declared_type: Ty::Optional(Box::new(Ty::Int)),
        }),
    })
    .await
}

#[tokio::test]
async fn test_optional_int_returns_null() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> int? {
                null
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::Null,
            declared_type: Ty::Optional(Box::new(Ty::Int)),
        }),
    })
    .await
}

#[tokio::test]
async fn test_map_string_to_int() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> map<string, int> {
                {"a": 1, "b": 2}
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::Map(indexmap! {
                "a".to_string() => TypedSnapshot { value: Snapshot::Int(1), declared_type: Ty::Int },
                "b".to_string() => TypedSnapshot { value: Snapshot::Int(2), declared_type: Ty::Int },
            }),
            declared_type: Ty::Map {
                key: Box::new(Ty::String),
                value: Box::new(Ty::Int),
            },
        }),
    })
    .await
}

#[tokio::test]
async fn test_class_with_union_field() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            class Response {
                data int | string
            }

            function main() -> Response {
                Response { data: 42 }
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::Instance {
                class_name: "Response".to_string(),
                fields: indexmap! {
                    "data".to_string() => TypedSnapshot {
                        value: Snapshot::Int(42),
                        declared_type: Ty::Union(vec![Ty::Int, Ty::String]),
                    },
                },
            },
            declared_type: Ty::Class("Response".to_string()),
        }),
    })
    .await
}

#[tokio::test]
async fn test_union_of_classes_returns_success() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            class Success {
                value int
            }

            class Failure {
                error string
            }

            function main() -> Success | Failure {
                Success { value: 42 }
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::Instance {
                class_name: "Success".to_string(),
                fields: indexmap! {
                    "value".to_string() => TypedSnapshot {
                        value: Snapshot::Int(42),
                        declared_type: Ty::Int,
                    },
                },
            },
            declared_type: Ty::Union(vec![
                Ty::Class("Success".to_string()),
                Ty::Class("Failure".to_string()),
            ]),
        }),
    })
    .await
}

#[tokio::test]
async fn test_union_of_classes_returns_failure() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            class Success {
                value int
            }

            class Failure {
                error string
            }

            function main() -> Success | Failure {
                Failure { error: "something went wrong" }
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::Instance {
                class_name: "Failure".to_string(),
                fields: indexmap! {
                    "error".to_string() => TypedSnapshot {
                        value: Snapshot::String("something went wrong".to_string()),
                        declared_type: Ty::String,
                    },
                },
            },
            declared_type: Ty::Union(vec![
                Ty::Class("Success".to_string()),
                Ty::Class("Failure".to_string()),
            ]),
        }),
    })
    .await
}

#[tokio::test]
async fn test_union_of_arrays() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> int[] | string[] {
                [1, 2, 3]
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::Array(vec![
                TypedSnapshot {
                    value: Snapshot::Int(1),
                    declared_type: Ty::Int,
                },
                TypedSnapshot {
                    value: Snapshot::Int(2),
                    declared_type: Ty::Int,
                },
                TypedSnapshot {
                    value: Snapshot::Int(3),
                    declared_type: Ty::Int,
                },
            ]),
            declared_type: Ty::Union(vec![
                Ty::List(Box::new(Ty::Int)),
                Ty::List(Box::new(Ty::String)),
            ]),
        }),
    })
    .await
}

#[tokio::test]
async fn test_array_of_unions() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> (int | string)[] {
                [1, "two", 3]
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::Array(vec![
                TypedSnapshot {
                    value: Snapshot::Int(1),
                    declared_type: Ty::Union(vec![Ty::Int, Ty::String]),
                },
                TypedSnapshot {
                    value: Snapshot::String("two".to_string()),
                    declared_type: Ty::Union(vec![Ty::Int, Ty::String]),
                },
                TypedSnapshot {
                    value: Snapshot::Int(3),
                    declared_type: Ty::Union(vec![Ty::Int, Ty::String]),
                },
            ]),
            declared_type: Ty::List(Box::new(Ty::Union(vec![Ty::Int, Ty::String]))),
        }),
    })
    .await
}

#[tokio::test]
async fn test_nested_class() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            class Inner {
                value int
            }

            class Outer {
                inner Inner
            }

            function main() -> Outer {
                Outer { inner: Inner { value: 42 } }
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::Instance {
                class_name: "Outer".to_string(),
                fields: indexmap! {
                    "inner".to_string() => TypedSnapshot {
                        value: Snapshot::Instance {
                            class_name: "Inner".to_string(),
                            fields: indexmap! {
                                "value".to_string() => TypedSnapshot {
                                    value: Snapshot::Int(42),
                                    declared_type: Ty::Int,
                                },
                            },
                        },
                        declared_type: Ty::Class("Inner".to_string()),
                    },
                },
            },
            declared_type: Ty::Class("Outer".to_string()),
        }),
    })
    .await
}

#[tokio::test]
async fn test_optional_class() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            class Data {
                value int
            }

            function main() -> Data? {
                Data { value: 42 }
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::Instance {
                class_name: "Data".to_string(),
                fields: indexmap! {
                    "value".to_string() => TypedSnapshot {
                        value: Snapshot::Int(42),
                        declared_type: Ty::Int,
                    },
                },
            },
            declared_type: Ty::Optional(Box::new(Ty::Class("Data".to_string()))),
        }),
    })
    .await
}

#[tokio::test]
async fn test_optional_class_returns_null() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            class Data {
                value int
            }

            function main() -> Data? {
                null
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::Null,
            declared_type: Ty::Optional(Box::new(Ty::Class("Data".to_string()))),
        }),
    })
    .await
}

#[tokio::test]
async fn test_class_with_optional_field() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            class Person {
                name string
                age int?
            }

            function main() -> Person {
                Person { name: "Alice", age: null }
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::Instance {
                class_name: "Person".to_string(),
                fields: indexmap! {
                    "name".to_string() => TypedSnapshot {
                        value: Snapshot::String("Alice".to_string()),
                        declared_type: Ty::String,
                    },
                    "age".to_string() => TypedSnapshot {
                        value: Snapshot::Null,
                        declared_type: Ty::Optional(Box::new(Ty::Int)),
                    },
                },
            },
            declared_type: Ty::Class("Person".to_string()),
        }),
    })
    .await
}

#[tokio::test]
async fn test_array_of_classes() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            class Item {
                id int
            }

            function main() -> Item[] {
                [Item { id: 1 }, Item { id: 2 }]
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::Array(vec![
                TypedSnapshot {
                    value: Snapshot::Instance {
                        class_name: "Item".to_string(),
                        fields: indexmap! {
                            "id".to_string() => TypedSnapshot {
                                value: Snapshot::Int(1),
                                declared_type: Ty::Int,
                            },
                        },
                    },
                    declared_type: Ty::Class("Item".to_string()),
                },
                TypedSnapshot {
                    value: Snapshot::Instance {
                        class_name: "Item".to_string(),
                        fields: indexmap! {
                            "id".to_string() => TypedSnapshot {
                                value: Snapshot::Int(2),
                                declared_type: Ty::Int,
                            },
                        },
                    },
                    declared_type: Ty::Class("Item".to_string()),
                },
            ]),
            declared_type: Ty::List(Box::new(Ty::Class("Item".to_string()))),
        }),
    })
    .await
}

#[tokio::test]
async fn test_map_with_union_values() -> anyhow::Result<()> {
    assert_engine_typed(TypedEngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> map<string, int | string> {
                {"count": 42, "name": "test"}
            }
        "#,
        entry: "main",
        expected: Ok(TypedSnapshot {
            value: Snapshot::Map(indexmap! {
                "count".to_string() => TypedSnapshot {
                    value: Snapshot::Int(42),
                    declared_type: Ty::Union(vec![Ty::Int, Ty::String]),
                },
                "name".to_string() => TypedSnapshot {
                    value: Snapshot::String("test".to_string()),
                    declared_type: Ty::Union(vec![Ty::Int, Ty::String]),
                },
            }),
            declared_type: Ty::Map {
                key: Box::new(Ty::String),
                value: Box::new(Ty::Union(vec![Ty::Int, Ty::String])),
            },
        }),
    })
    .await
}
