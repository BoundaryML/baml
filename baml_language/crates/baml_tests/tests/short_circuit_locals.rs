//! Regression tests for locals the emitter may carry on the operand stack.
//!
//! Both shapes here used to be classified as stack-carried even though one
//! definition of the local was never balanced by the single use, so each loop
//! iteration left an extra value on the operand stack.

use baml_tests::{baml_test, baml_test_optimized};
use bex_engine::BexExternalValue;

/// The short circuit's join is merged into the `if` join, which is also where
/// `x` is read — but the `if`'s false edge reaches that block without pushing
/// anything, and `let x = false` is a second definition.
const CONDITIONAL_SHORT_CIRCUIT: &str = r###"
        function count_while_short_circuiting(items: int[], a: bool, b: bool) -> int {
            let x = false
            let hits = 0
            for (let i in items) {
                if (i > 0) {
                    x = a && b
                }
                if (x) {
                    hits = hits + 1
                }
            }
            hits
        }

        function main() -> int {
            count_while_short_circuiting([0, 1, 2, 0, 3], true, true)
        }
        "###;

/// Both arms assign `x` and fall through to its use, but `let x = 0` defines it
/// once more outside them.
const INITIALIZED_LOCAL_IN_BRANCH: &str = r###"
        function sum_branch_local(items: int[]) -> int {
            let total = 0
            for (let i in items) {
                let x = 0
                if (i > 0) {
                    x = 1
                } else {
                    x = 2
                }
                total = x + total
            }
            total
        }

        function main() -> int {
            sum_branch_local([1, -1, 2, -2, 0])
        }
        "###;

#[tokio::test]
async fn short_circuit_under_a_conditional_in_a_loop() {
    let output = baml_test!(CONDITIONAL_SHORT_CIRCUIT);

    assert_eq!(
        output.result.map_err(|error| format!("{error:?}")),
        Ok(BexExternalValue::Int(4))
    );
}

#[tokio::test]
async fn short_circuit_under_a_conditional_in_a_loop_optimized() {
    let output = baml_test_optimized!(CONDITIONAL_SHORT_CIRCUIT);

    assert_eq!(
        output.result.map_err(|error| format!("{error:?}")),
        Ok(BexExternalValue::Int(4))
    );
}

#[tokio::test]
async fn initialized_local_assigned_in_both_branches_of_a_loop() {
    let output = baml_test!(INITIALIZED_LOCAL_IN_BRANCH);

    assert_eq!(
        output.result.map_err(|error| format!("{error:?}")),
        Ok(BexExternalValue::Int(8))
    );
}

#[tokio::test]
async fn initialized_local_assigned_in_both_branches_of_a_loop_optimized() {
    let output = baml_test_optimized!(INITIALIZED_LOCAL_IN_BRANCH);

    assert_eq!(
        output.result.map_err(|error| format!("{error:?}")),
        Ok(BexExternalValue::Int(8))
    );
}
