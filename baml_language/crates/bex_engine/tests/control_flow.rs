//! End-to-end control-flow tests through compiler2 + `BexEngine`.

mod common;

use bex_engine::BexExternalValue;
use common::{EngineProgram, assert_engine_executes};

#[tokio::test]
async fn match_arm_break_exits_infinite_while_loop() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function break_loop(x: int) -> int {
                let result = 0;
                while (true) {
                    match (x) {
                        1 => {
                            result = 40;
                            break;
                        },
                        _ => {
                            result = 2;
                            break;
                        },
                    };
                }

                result
            }

            function main() -> int {
                break_loop(1) + break_loop(2)
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(42)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn match_arm_continue_continues_loop_without_falling_through() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function continue_loop(x: int) -> int {
                let i = 0;
                while (i < 3) {
                    match (x) {
                        1 => {
                            i += 1;
                            continue;
                        },
                        _ => {
                            i += 10;
                            continue;
                        },
                    };

                    i = 100;
                }

                i
            }

            function main() -> int {
                continue_loop(1) + continue_loop(2)
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(13)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn match_arm_return_returns_without_falling_through() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function return_from_match(x: int) -> int {
                match (x) {
                    1 => {
                        return 40;
                    },
                    _ => {
                        return 2;
                    },
                };

                999
            }

            function main() -> int {
                return_from_match(1) + return_from_match(2)
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(42)),
        ..Default::default()
    })
    .await
}
