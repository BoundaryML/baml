//! Tests for filesystem operations (baml.fs.open, file.read).

mod common;

use bex_engine::BexExternalValue;
use common::{EngineProgram, assert_engine_executes};
use indexmap::indexmap;

/// Test that just `open()` returns something (without read)
#[tokio::test]
async fn fs_open_only() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {
            "hello.txt" => "Hello from BAML!",
        },
        source: r#"
            function main() -> int {
                let file = baml.fs.open("{ROOT}/hello.txt");
                42
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(42)),
    })
    .await
}

#[tokio::test]
async fn fs_open_and_read() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {
            "hello.txt" => "Hello from BAML!",
        },
        source: r#"
            function main() -> string {
                let file = baml.fs.open("{ROOT}/hello.txt");
                file.read()
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("Hello from BAML!".to_string())),
    })
    .await
}

#[tokio::test]
async fn fs_open_nonexistent_file() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> string {
                let file = baml.fs.open("{ROOT}/nonexistent.txt");
                file.read()
            }
        "#,
        entry: "main",
        expected: Err("Failed to open file"),
    })
    .await
}
