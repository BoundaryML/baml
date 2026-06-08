mod common;

use bex_engine::BexExternalValue;
use common::{EngineProgram, assert_engine_executes};

#[tokio::test]
async fn array_constructor_preallocates_and_set_updates_by_index() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> int {
                let width = 3;
                let dp = int[](6, 0);
                dp.set(0, 4);
                dp.set(1 * width + 2, 9);
                (dp.at(0) ?? 0) + (dp.at(5) ?? 0) + dp.length()
            }
        "#,
        expected: Ok(BexExternalValue::Int(19)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn array_set_out_of_bounds_throws_invalid_argument() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> int {
                {
                    let xs = int[](2, 0);
                    xs.set(2, 1);
                    0
                } catch (e) {
                    baml.errors.InvalidArgument => 1
                }
            }
        "#,
        expected: Ok(BexExternalValue::Int(1)),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn array_constructor_negative_size_throws_invalid_argument() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> int {
                {
                    let xs = int[](-1, 0);
                    xs.length()
                } catch (e) {
                    baml.errors.InvalidArgument => 1
                }
            }
        "#,
        expected: Ok(BexExternalValue::Int(1)),
        ..Default::default()
    })
    .await
}
