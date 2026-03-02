//! Tests for environment variable operations (`env.get`, `env.get_or_panic`).

#![allow(unsafe_code)]

mod common;

use baml_type::TyAttr;
use bex_engine::BexExternalValue;
use common::{EngineProgram, assert_engine_executes};
use indexmap::indexmap;

#[tokio::test]
async fn env_get_or_panic_existing_var() -> anyhow::Result<()> {
    unsafe { std::env::set_var("BAML_TEST_ENV_PANIC", "panic_value") };
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> string {
                env.get_or_panic("BAML_TEST_ENV_PANIC")
            }
        "#,
        entry: "main",
        inputs: vec![],
        expected: Ok(BexExternalValue::String("panic_value".to_string())),
    })
    .await
}

#[tokio::test]
async fn env_get_or_panic_missing_var() -> anyhow::Result<()> {
    unsafe { std::env::remove_var("BAML_TEST_MISSING_PANIC") };
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> string {
                env.get_or_panic("BAML_TEST_MISSING_PANIC")
            }
        "#,
        entry: "main",
        inputs: vec![],
        expected: Err("not found"),
    })
    .await
}

#[tokio::test]
async fn env_get_existing_var() -> anyhow::Result<()> {
    unsafe { std::env::set_var("BAML_TEST_ENV_GET", "hello_env") };
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> string? {
                env.get("BAML_TEST_ENV_GET")
            }
        "#,
        entry: "main",
        inputs: vec![],
        // env.get returns string? — the engine wraps in Union metadata
        expected: Ok(BexExternalValue::Union {
            value: Box::new(BexExternalValue::String("hello_env".to_string())),
            metadata: bex_engine::UnionMetadata::new(
                bex_engine::Ty::Optional(
                    Box::new(bex_engine::Ty::String {
                        attr: TyAttr::default(),
                    }),
                    TyAttr::default(),
                ),
                bex_engine::Ty::String {
                    attr: TyAttr::default(),
                },
            ),
        }),
    })
    .await
}

#[tokio::test]
async fn env_get_missing_var_returns_null() -> anyhow::Result<()> {
    unsafe { std::env::remove_var("BAML_TEST_NONEXISTENT_VAR") };
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> string? {
                env.get("BAML_TEST_NONEXISTENT_VAR")
            }
        "#,
        entry: "main",
        inputs: vec![],
        expected: Ok(BexExternalValue::Union {
            value: Box::new(BexExternalValue::Null),
            metadata: bex_engine::UnionMetadata::new(
                bex_engine::Ty::Optional(
                    Box::new(bex_engine::Ty::String {
                        attr: TyAttr::default(),
                    }),
                    TyAttr::default(),
                ),
                bex_engine::Ty::Null {
                    attr: TyAttr::default(),
                },
            ),
        }),
    })
    .await
}

#[tokio::test]
async fn env_sugar_existing_var() -> anyhow::Result<()> {
    unsafe { std::env::set_var("BAML_TEST_SUGAR_VAR", "sugar_value") };
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> string {
                env.BAML_TEST_SUGAR_VAR
            }
        "#,
        entry: "main",
        inputs: vec![],
        expected: Ok(BexExternalValue::String("sugar_value".to_string())),
    })
    .await
}

#[tokio::test]
async fn env_sugar_missing_var() -> anyhow::Result<()> {
    unsafe { std::env::remove_var("BAML_TEST_SUGAR_MISSING") };
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> string {
                env.BAML_TEST_SUGAR_MISSING
            }
        "#,
        entry: "main",
        inputs: vec![],
        expected: Err("not found"),
    })
    .await
}
