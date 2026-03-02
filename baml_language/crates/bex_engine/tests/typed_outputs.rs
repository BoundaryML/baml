//! Tests for union type handling in `BexExternalValue`.
//!
//! These tests verify that `call_function` properly wraps union return types
//! in `BexExternalValue::Union { value, metadata }` with correct metadata.

mod common;

use baml_type::TyAttr;
use bex_engine::{BexExternalValue, Ty, UnionMetadata};
use common::{EngineProgram, assert_engine_executes};
use indexmap::indexmap;

#[tokio::test]
async fn union_int_or_string_returns_int() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> int | string {
                42
            }
        "#,
        entry: "main",
        inputs: vec![],
        expected: Ok(BexExternalValue::Union {
            value: Box::new(BexExternalValue::Int(42)),
            metadata: UnionMetadata::new(
                Ty::Union(
                    vec![
                        Ty::Int {
                            attr: TyAttr::default(),
                        },
                        Ty::String {
                            attr: TyAttr::default(),
                        },
                    ],
                    TyAttr::default(),
                ),
                Ty::Int {
                    attr: TyAttr::default(),
                },
            ),
        }),
    })
    .await
}

#[tokio::test]
async fn union_int_or_string_returns_string() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> int | string {
                "hello"
            }
        "#,
        entry: "main",
        inputs: vec![],
        expected: Ok(BexExternalValue::Union {
            value: Box::new(BexExternalValue::String("hello".to_string())),
            metadata: UnionMetadata::new(
                Ty::Union(
                    vec![
                        Ty::Int {
                            attr: TyAttr::default(),
                        },
                        Ty::String {
                            attr: TyAttr::default(),
                        },
                    ],
                    TyAttr::default(),
                ),
                Ty::String {
                    attr: TyAttr::default(),
                },
            ),
        }),
    })
    .await
}

#[tokio::test]
async fn optional_int_returns_value() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> int? {
                42
            }
        "#,
        entry: "main",
        inputs: vec![],
        expected: Ok(BexExternalValue::Union {
            value: Box::new(BexExternalValue::Int(42)),
            metadata: UnionMetadata::new(
                Ty::Optional(
                    Box::new(Ty::Int {
                        attr: TyAttr::default(),
                    }),
                    TyAttr::default(),
                ),
                Ty::Int {
                    attr: TyAttr::default(),
                },
            ),
        }),
    })
    .await
}

#[tokio::test]
async fn optional_int_returns_null() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> int? {
                null
            }
        "#,
        entry: "main",
        inputs: vec![],
        expected: Ok(BexExternalValue::Union {
            value: Box::new(BexExternalValue::Null),
            metadata: UnionMetadata::new(
                Ty::Optional(
                    Box::new(Ty::Int {
                        attr: TyAttr::default(),
                    }),
                    TyAttr::default(),
                ),
                Ty::Null {
                    attr: TyAttr::default(),
                },
            ),
        }),
    })
    .await
}

#[tokio::test]
async fn class_with_union_field() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
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
        inputs: vec![],
        expected: Ok(BexExternalValue::Instance {
            class_name: "Response".to_string(),
            fields: indexmap! {
                "data".to_string() => BexExternalValue::Union {
                    value: Box::new(BexExternalValue::Int(42)),
                    metadata: UnionMetadata::new(Ty::Union(vec![Ty::Int { attr: TyAttr::default() }, Ty::String { attr: TyAttr::default() }], TyAttr::default()), Ty::Int { attr: TyAttr::default() }),
                },
            },
        }),
    })
    .await
}

#[tokio::test]
async fn union_of_classes_returns_success() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
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
        inputs: vec![],
        expected: Ok(BexExternalValue::Union {
            value: Box::new(BexExternalValue::Instance {
                class_name: "Success".to_string(),
                fields: indexmap! {
                    "value".to_string() => BexExternalValue::Int(42),
                },
            }),
            metadata: UnionMetadata::new(
                Ty::Union(
                    vec![
                        Ty::Class(
                            bex_engine::TypeName::local("Success".into()),
                            TyAttr::default(),
                        ),
                        Ty::Class(
                            bex_engine::TypeName::local("Failure".into()),
                            TyAttr::default(),
                        ),
                    ],
                    TyAttr::default(),
                ),
                Ty::Class(
                    bex_engine::TypeName::local("Success".into()),
                    TyAttr::default(),
                ),
            ),
        }),
    })
    .await
}

#[tokio::test]
async fn union_of_classes_returns_failure() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
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
        inputs: vec![],
        expected: Ok(BexExternalValue::Union {
            value: Box::new(BexExternalValue::Instance {
                class_name: "Failure".to_string(),
                fields: indexmap! {
                    "error".to_string() => BexExternalValue::String("something went wrong".to_string()),
                },
            }),
            metadata: UnionMetadata::new(
                Ty::Union(vec![
                    Ty::Class(bex_engine::TypeName::local("Success".into()), TyAttr::default()),
                    Ty::Class(bex_engine::TypeName::local("Failure".into()), TyAttr::default()),
                ], TyAttr::default()),
                Ty::Class(bex_engine::TypeName::local("Failure".into()), TyAttr::default()),
            ),
        }),
    })
    .await
}

#[tokio::test]
async fn union_of_arrays() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> int[] | string[] {
                [1, 2, 3]
            }
        "#,
        entry: "main",
        inputs: vec![],
        expected: Ok(BexExternalValue::Union {
            value: Box::new(BexExternalValue::Array {
                element_type: Ty::Int {
                    attr: TyAttr::default(),
                },
                items: vec![
                    BexExternalValue::Int(1),
                    BexExternalValue::Int(2),
                    BexExternalValue::Int(3),
                ],
            }),
            metadata: UnionMetadata::new(
                Ty::Union(
                    vec![
                        Ty::List(
                            Box::new(Ty::Int {
                                attr: TyAttr::default(),
                            }),
                            TyAttr::default(),
                        ),
                        Ty::List(
                            Box::new(Ty::String {
                                attr: TyAttr::default(),
                            }),
                            TyAttr::default(),
                        ),
                    ],
                    TyAttr::default(),
                ),
                Ty::List(
                    Box::new(Ty::Int {
                        attr: TyAttr::default(),
                    }),
                    TyAttr::default(),
                ),
            ),
        }),
    })
    .await
}

#[tokio::test]
async fn array_of_unions() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> (int | string)[] {
                [1, "two", 3]
            }
        "#,
        entry: "main",
        inputs: vec![],
        expected: Ok(BexExternalValue::Array {
            element_type: Ty::Union(
                vec![
                    Ty::Int {
                        attr: TyAttr::default(),
                    },
                    Ty::String {
                        attr: TyAttr::default(),
                    },
                ],
                TyAttr::default(),
            ),
            items: vec![
                BexExternalValue::Union {
                    value: Box::new(BexExternalValue::Int(1)),
                    metadata: UnionMetadata::new(
                        Ty::Union(
                            vec![
                                Ty::Int {
                                    attr: TyAttr::default(),
                                },
                                Ty::String {
                                    attr: TyAttr::default(),
                                },
                            ],
                            TyAttr::default(),
                        ),
                        Ty::Int {
                            attr: TyAttr::default(),
                        },
                    ),
                },
                BexExternalValue::Union {
                    value: Box::new(BexExternalValue::String("two".to_string())),
                    metadata: UnionMetadata::new(
                        Ty::Union(
                            vec![
                                Ty::Int {
                                    attr: TyAttr::default(),
                                },
                                Ty::String {
                                    attr: TyAttr::default(),
                                },
                            ],
                            TyAttr::default(),
                        ),
                        Ty::String {
                            attr: TyAttr::default(),
                        },
                    ),
                },
                BexExternalValue::Union {
                    value: Box::new(BexExternalValue::Int(3)),
                    metadata: UnionMetadata::new(
                        Ty::Union(
                            vec![
                                Ty::Int {
                                    attr: TyAttr::default(),
                                },
                                Ty::String {
                                    attr: TyAttr::default(),
                                },
                            ],
                            TyAttr::default(),
                        ),
                        Ty::Int {
                            attr: TyAttr::default(),
                        },
                    ),
                },
            ],
        }),
    })
    .await
}

#[tokio::test]
async fn optional_class() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
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
        inputs: vec![],
        expected: Ok(BexExternalValue::Union {
            value: Box::new(BexExternalValue::Instance {
                class_name: "Data".to_string(),
                fields: indexmap! {
                    "value".to_string() => BexExternalValue::Int(42),
                },
            }),
            metadata: UnionMetadata::new(
                Ty::Optional(
                    Box::new(Ty::Class(
                        bex_engine::TypeName::local("Data".into()),
                        TyAttr::default(),
                    )),
                    TyAttr::default(),
                ),
                Ty::Class(
                    bex_engine::TypeName::local("Data".into()),
                    TyAttr::default(),
                ),
            ),
        }),
    })
    .await
}

#[tokio::test]
async fn optional_class_returns_null() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
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
        inputs: vec![],
        expected: Ok(BexExternalValue::Union {
            value: Box::new(BexExternalValue::Null),
            metadata: UnionMetadata::new(
                Ty::Optional(
                    Box::new(Ty::Class(
                        bex_engine::TypeName::local("Data".into()),
                        TyAttr::default(),
                    )),
                    TyAttr::default(),
                ),
                Ty::Null {
                    attr: TyAttr::default(),
                },
            ),
        }),
    })
    .await
}

#[tokio::test]
async fn class_with_optional_field() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
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
        inputs: vec![],
        expected: Ok(BexExternalValue::Instance {
            class_name: "Person".to_string(),
            fields: indexmap! {
                "name".to_string() => BexExternalValue::String("Alice".to_string()),
                "age".to_string() => BexExternalValue::Union {
                    value: Box::new(BexExternalValue::Null),
                    metadata: UnionMetadata::new(Ty::Optional(Box::new(Ty::Int { attr: TyAttr::default() }), TyAttr::default()), Ty::Null { attr: TyAttr::default() }),
                },
            },
        }),
    })
    .await
}

#[tokio::test]
async fn map_with_union_values() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> map<string, int | string> {
                {"count": 42, "name": "test"}
            }
        "#,
        entry: "main",
        inputs: vec![],
        expected: Ok(BexExternalValue::Map {
            key_type: Ty::String { attr: TyAttr::default() },
            value_type: Ty::Union(vec![Ty::Int { attr: TyAttr::default() }, Ty::String { attr: TyAttr::default() }], TyAttr::default()),
            entries: indexmap! {
                "count".to_string() => BexExternalValue::Union {
                    value: Box::new(BexExternalValue::Int(42)),
                    metadata: UnionMetadata::new(Ty::Union(vec![Ty::Int { attr: TyAttr::default() }, Ty::String { attr: TyAttr::default() }], TyAttr::default()), Ty::Int { attr: TyAttr::default() }),
                },
                "name".to_string() => BexExternalValue::Union {
                    value: Box::new(BexExternalValue::String("test".to_string())),
                    metadata: UnionMetadata::new(Ty::Union(vec![Ty::Int { attr: TyAttr::default() }, Ty::String { attr: TyAttr::default() }], TyAttr::default()), Ty::String { attr: TyAttr::default() }),
                },
            },
        }),
    })
    .await
}

#[tokio::test]
async fn union_of_array_with_union_elements_or_string() -> anyhow::Result<()> {
    // Tests that selected_option uses declared type, not inferred from values
    // The array element type is (int | bool), not just int
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> (int | bool)[] | string {
                [1, true, 2]
            }
        "#,
        entry: "main",
        inputs: vec![],
        expected: Ok(BexExternalValue::Union {
            value: Box::new(BexExternalValue::Array {
                element_type: Ty::Union(
                    vec![
                        Ty::Int {
                            attr: TyAttr::default(),
                        },
                        Ty::Bool {
                            attr: TyAttr::default(),
                        },
                    ],
                    TyAttr::default(),
                ),
                items: vec![
                    BexExternalValue::Union {
                        value: Box::new(BexExternalValue::Int(1)),
                        metadata: UnionMetadata::new(
                            Ty::Union(
                                vec![
                                    Ty::Int {
                                        attr: TyAttr::default(),
                                    },
                                    Ty::Bool {
                                        attr: TyAttr::default(),
                                    },
                                ],
                                TyAttr::default(),
                            ),
                            Ty::Int {
                                attr: TyAttr::default(),
                            },
                        ),
                    },
                    BexExternalValue::Union {
                        value: Box::new(BexExternalValue::Bool(true)),
                        metadata: UnionMetadata::new(
                            Ty::Union(
                                vec![
                                    Ty::Int {
                                        attr: TyAttr::default(),
                                    },
                                    Ty::Bool {
                                        attr: TyAttr::default(),
                                    },
                                ],
                                TyAttr::default(),
                            ),
                            Ty::Bool {
                                attr: TyAttr::default(),
                            },
                        ),
                    },
                    BexExternalValue::Union {
                        value: Box::new(BexExternalValue::Int(2)),
                        metadata: UnionMetadata::new(
                            Ty::Union(
                                vec![
                                    Ty::Int {
                                        attr: TyAttr::default(),
                                    },
                                    Ty::Bool {
                                        attr: TyAttr::default(),
                                    },
                                ],
                                TyAttr::default(),
                            ),
                            Ty::Int {
                                attr: TyAttr::default(),
                            },
                        ),
                    },
                ],
            }),
            // Key assertion: selected_option is the full declared type (int | bool)[]
            // not Ty::List(Ty::Int { attr: TyAttr::default() }) inferred from first element
            metadata: UnionMetadata::new(
                Ty::Union(
                    vec![
                        Ty::List(
                            Box::new(Ty::Union(
                                vec![
                                    Ty::Int {
                                        attr: TyAttr::default(),
                                    },
                                    Ty::Bool {
                                        attr: TyAttr::default(),
                                    },
                                ],
                                TyAttr::default(),
                            )),
                            TyAttr::default(),
                        ),
                        Ty::String {
                            attr: TyAttr::default(),
                        },
                    ],
                    TyAttr::default(),
                ),
                Ty::List(
                    Box::new(Ty::Union(
                        vec![
                            Ty::Int {
                                attr: TyAttr::default(),
                            },
                            Ty::Bool {
                                attr: TyAttr::default(),
                            },
                        ],
                        TyAttr::default(),
                    )),
                    TyAttr::default(),
                ),
            ),
        }),
    })
    .await
}
