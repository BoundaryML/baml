//! Tests for shell operations (baml.sys.shell).

mod common;

use baml_tests::vm::Value;
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
        function: "main",
        // Note: echo adds a newline
        expected: Ok(Value::string("Hello From Shell!\n")),
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
        function: "main",
        expected: Ok(Value::string("HELLO WORLD\n")),
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
        function: "main",
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
        function: "main",
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
        function: "main",
        expected: Ok(Value::string("dynamic\n")),
    })
    .await
}
