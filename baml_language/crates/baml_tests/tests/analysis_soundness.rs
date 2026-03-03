//! Bytecode-shape regressions for analysis soundness.
//!
//! These tests verify that the compiler produces correct bytecode when variables
//! are copied across block boundaries and mutated — ensuring virtual/physical
//! register allocation preserves value semantics.

use bex_external_types::BexExternalValue;

#[tokio::test]
async fn virtual_cross_block_soundness_codegen() {
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
async fn virtual_cross_block_soundness_codegen_false() {
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
async fn virtual_cross_block_param_mutation_soundness_codegen() {
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
async fn copy_of_mutable_param_soundness_codegen() {
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
async fn virtual_cross_block_transitive_param_mutation_soundness_codegen() {
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
async fn virtual_multiple_defs_preserve_side_effects_codegen() {
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
