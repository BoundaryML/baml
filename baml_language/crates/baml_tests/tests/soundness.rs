//! Soundness regression tests.
//!
//! Tests that the compiler produces correct bytecode and execution results
//! for tricky cases: stack operand ordering, cross-block variable mutation,
//! register allocation, and value copy semantics.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// --- Stack operand ordering ---

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

// --- Cross-block variable mutation ---

#[tokio::test]
async fn cross_block_field_mutation() {
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

#[tokio::test]
async fn cross_block_let_mutation() {
    let output = baml_tests::baml_test! {
        baml: r#"
            function main(c: bool) -> int {
                let a = 1;
                let b = a;
                if (c) {
                    a = 2;
                }
                b
            }
        "#,
        args: { "c" => BexExternalValue::Bool(true) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function main(c: bool) -> int {
        load_const 1
        store_var a
        load_var a
        store_var b
        load_var c
        pop_jump_if_false L0
        load_const 2
        store_var a

      L0:
        load_var b
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn cross_block_let_mutation_false_branch() {
    let output = baml_tests::baml_test! {
        baml: r#"
            function main(c: bool) -> int {
                let a = 1;
                let b = a;
                if (c) {
                    a = 2;
                }
                b
            }
        "#,
        args: { "c" => BexExternalValue::Bool(false) },
    };
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn cross_block_param_mutation() {
    let output = baml_tests::baml_test! {
        baml: r#"
            function main(c: bool, p: int) -> int {
                let x = p;
                if (c) {
                    p = 2;
                }
                x
            }
        "#,
        args: {
            "c" => BexExternalValue::Bool(true),
            "p" => BexExternalValue::Int(10)
        },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function main(c: bool, p: int) -> int {
        load_var p
        store_var x
        load_var c
        pop_jump_if_false L0
        load_const 2
        store_var p

      L0:
        load_var x
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

#[tokio::test]
async fn copy_of_mutable_param() {
    let output = baml_tests::baml_test! {
        baml: r#"
            function main(x: int) -> int {
                let y = x;
                x = 2;
                y
            }
        "#,
        args: { "x" => BexExternalValue::Int(5) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function main(x: int) -> int {
        load_var x
        store_var y
        load_const 2
        store_var x
        load_var y
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn transitive_param_copy_mutation() {
    let output = baml_tests::baml_test! {
        baml: r#"
            function main(c: bool, p: int) -> int {
                let t = p;
                let x = t;
                if (c) {
                    p = 2;
                }
                x
            }
        "#,
        args: {
            "c" => BexExternalValue::Bool(true),
            "p" => BexExternalValue::Int(7)
        },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function main(c: bool, p: int) -> int {
        load_var p
        store_var x
        load_var c
        pop_jump_if_false L0
        load_const 2
        store_var p

      L0:
        load_var x
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn multiple_defs_preserve_side_effects() {
    let output = baml_tests::baml_test! {
        baml: r#"
            function fail() -> int {
                assert(false);
                1
            }

            function main() -> int {
                let x = fail();
                x = 2;
                x
            }
        "#,
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function fail() -> int {
        load_const false
        assert
        load_const 1
        return
    }

    function main() -> int {
        call fail
        store_var x
        load_const 2
        store_var x
        load_var x
        return
    }
    ");
    insta::assert_snapshot!(output.result.unwrap_err().to_string(), @"VM error: assertion failed");
}
