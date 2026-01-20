//! Tests for shell operations (baml.sys.shell).

mod common;

use bex_engine::BexExternalValue;
use common::{EngineProgram, assert_engine_executes};
use indexmap::indexmap;

#[tokio::test]
async fn shell_echo() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> string {
                baml.sys.shell("echo 'Hello From Shell!'")
            }
        "#,
        entry: "main",
        // Note: echo adds a newline
        expected: Ok(BexExternalValue::String("Hello From Shell!\n".to_string())),
    })
    .await
}

#[tokio::test]
async fn shell_with_pipe() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> string {
                baml.sys.shell("echo 'hello world' | tr 'a-z' 'A-Z'")
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("HELLO WORLD\n".to_string())),
    })
    .await
}

#[tokio::test]
async fn shell_failing_command() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> string {
                baml.sys.shell("exit 1")
            }
        "#,
        entry: "main",
        expected: Err("failed with exit code 1"),
    })
    .await
}

#[tokio::test]
async fn shell_nonexistent_command() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> string {
                baml.sys.shell("nonexistent_command_12345")
            }
        "#,
        entry: "main",
        expected: Err("not found"),
    })
    .await
}

#[tokio::test]
async fn shell_with_variable() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> string {
                let cmd = "echo 'dynamic'";
                baml.sys.shell(cmd)
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("dynamic\n".to_string())),
    })
    .await
}
