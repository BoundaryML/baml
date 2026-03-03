//! Unified tests for stack-discipline soundness regressions.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn call_result_immediate_right_operand_subtraction() {
    let output = baml_test!(
        r#"
            function id(x: int) -> int { x }

            function main() -> int {
                1 - id(2)
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function id(x: int) -> int {
        load_var x
        return
    }

    function main() -> int {
        load_const 2
        call id
        store_var _2
        load_const 1
        load_var _2
        bin_op -
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn phi_like_right_operand_subtraction() {
    let output = baml_test!(
        r#"
            function main() -> int {
                100 - if (2 > 1) { 7 } else { 3 }
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 2
        load_const 1
        cmp_op >
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 3
        store_var _2
        jump L2

      L1:
        load_const 7
        store_var _2

      L2:
        load_const 100
        load_var _2
        bin_op -
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(93)));
}

#[tokio::test]
async fn cross_block_virtual_misses_statement0_side_effect() {
    let output = baml_test!(
        r#"
            class Box {
                v int
            }

            function main() -> int {
                let b = Box { v: 1 };
                let t = b.v;
                if (1 == 1) {
                }
                b.v = 2;
                t
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Box
        copy 0
        load_const 1
        store_field .v
        store_var b
        load_var b
        load_field .v
        store_var t
        load_const 1
        load_const 1
        cmp_op ==
        pop_jump_if_false L0

      L0:
        load_var b
        load_const 2
        store_field .v
        load_var t
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}
