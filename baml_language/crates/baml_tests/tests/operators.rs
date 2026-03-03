//! Unified tests for operators (arithmetic, bitwise, unary, comparison, logical, compound assignment).

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ── Arithmetic ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn add() {
    let output = baml_test!(
        "
        function main() -> int {
            1 + 2
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        load_const 2
        bin_op +
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn subtract() {
    let output = baml_test!(
        "
        function main() -> int {
            1 - 2
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        load_const 2
        bin_op -
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn multiply() {
    let output = baml_test!(
        "
        function main() -> int {
            1 * 2
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        load_const 2
        bin_op *
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn divide() {
    let output = baml_test!(
        "
        function main() -> int {
            10 / 2
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 10
        load_const 2
        bin_op /
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn modulo() {
    let output = baml_test!(
        "
        function main() -> int {
            10 % 3
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 10
        load_const 3
        bin_op %
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// ── Bitwise ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn bitwise_and() {
    let output = baml_test!(
        "
        function main() -> int {
            10 & 3
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 10
        load_const 3
        bin_op &
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn bitwise_or() {
    let output = baml_test!(
        "
        function main() -> int {
            10 | 3
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 10
        load_const 3
        bin_op |
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
}

#[tokio::test]
async fn bitwise_xor() {
    let output = baml_test!(
        "
        function main() -> int {
            10 ^ 3
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 10
        load_const 3
        bin_op ^
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(9)));
}

#[tokio::test]
async fn shift_left() {
    let output = baml_test!(
        "
        function main() -> int {
            10 << 3
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 10
        load_const 3
        bin_op <<
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(80)));
}

#[tokio::test]
async fn shift_right() {
    let output = baml_test!(
        "
        function main() -> int {
            10 >> 3
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 10
        load_const 3
        bin_op >>
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// ── Unary ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn unary_negate() {
    let output = baml_test!(
        "
        function main() -> int {
            -1
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        unary_op -
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn unary_not() {
    let output = baml_test!(
        "
        function main() -> bool {
            !true
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        load_const true
        unary_op !
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn negative_int_in_let() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = -5;
            x
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 5
        unary_op -
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(-5)));
}

#[tokio::test]
async fn negative_int_arithmetic() {
    let output = baml_test!(
        "
        function main() -> int {
            -5 + 3
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 5
        unary_op -
        load_const 3
        bin_op +
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(-2)));
}

#[tokio::test]
async fn negative_int_comparison() {
    let output = baml_test!(
        "
        function main() -> bool {
            -5 < 0
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        load_const 5
        unary_op -
        load_const 0
        cmp_op <
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn negative_float_in_let() {
    let output = baml_test!(
        "
        function main() -> float {
            let x = -2.5;
            x
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> float {
        load_const 2.5
        unary_op -
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Float(-2.5)));
}

#[tokio::test]
async fn double_negation() {
    let output = baml_test!(
        "
        function main() -> int {
            --5
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 5
        unary_op -
        unary_op -
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn double_negation_variable() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = -10;
            --x
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 10
        unary_op -
        unary_op -
        unary_op -
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(-10)));
}

// ── Comparison ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn equal() {
    let output = baml_test!(
        "
        function main() -> bool {
            1 == 2
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        load_const 1
        load_const 2
        cmp_op ==
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn not_equal() {
    let output = baml_test!(
        "
        function main() -> bool {
            1 != 2
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        load_const 1
        load_const 2
        cmp_op !=
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn greater_than() {
    let output = baml_test!(
        "
        function main() -> bool {
            1 > 2
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        load_const 1
        load_const 2
        cmp_op >
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn greater_than_or_equal() {
    let output = baml_test!(
        "
        function main() -> bool {
            1 >= 2
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        load_const 1
        load_const 2
        cmp_op >=
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn less_than() {
    let output = baml_test!(
        "
        function main() -> bool {
            1 < 2
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        load_const 1
        load_const 2
        cmp_op <
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn less_than_or_equal() {
    let output = baml_test!(
        "
        function main() -> bool {
            1 <= 2
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        load_const 1
        load_const 2
        cmp_op <=
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

// ── Logical ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn logical_and() {
    let output = baml_test!(
        "
        function main() -> bool {
            true && false
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        load_const true
        pop_jump_if_false L0
        jump L1

      L0:
        load_const false
        jump L2

      L1:
        load_const false

      L2:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn logical_or() {
    let output = baml_test!(
        "
        function main() -> bool {
            true || false
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        load_const true
        pop_jump_if_false L0
        jump L1

      L0:
        load_const false
        jump L2

      L1:
        load_const true

      L2:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn short_circuit_and() {
    let output = baml_test!(
        r#"
        function ret_bool() -> bool {
            true
        }

        function main() -> bool {
            true && ret_bool()
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        load_const true
        pop_jump_if_false L0
        jump L1

      L0:
        load_const false
        jump L2

      L1:
        call ret_bool

      L2:
        return
    }

    function ret_bool() -> bool {
        load_const true
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn short_circuit_or() {
    let output = baml_test!(
        r#"
        function ret_bool() -> bool {
            true
        }

        function main() -> bool {
            true || ret_bool()
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        load_const true
        pop_jump_if_false L0
        jump L1

      L0:
        call ret_bool
        jump L2

      L1:
        load_const true

      L2:
        return
    }

    function ret_bool() -> bool {
        load_const true
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

// ── Compound assignment ─────────────────────────────────────────────────────

#[tokio::test]
async fn assign_add() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 1;
            x += 2;
            x
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        store_var x
        load_var x
        load_const 2
        bin_op +
        store_var x
        load_var x
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn assign_subtract() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 1;
            x -= 2;
            x
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        store_var x
        load_var x
        load_const 2
        bin_op -
        store_var x
        load_var x
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn assign_multiply() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 1;
            x *= 2;
            x
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        store_var x
        load_var x
        load_const 2
        bin_op *
        store_var x
        load_var x
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn assign_divide() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 10;
            x /= 2;
            x
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 10
        store_var x
        load_var x
        load_const 2
        bin_op /
        store_var x
        load_var x
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn assign_modulo() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 10;
            x %= 3;
            x
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 10
        store_var x
        load_var x
        load_const 3
        bin_op %
        store_var x
        load_var x
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn assign_bitwise_and() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 10;
            x &= 3;
            x
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 10
        store_var x
        load_var x
        load_const 3
        bin_op &
        store_var x
        load_var x
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn assign_bitwise_or() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 10;
            x |= 3;
            x
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 10
        store_var x
        load_var x
        load_const 3
        bin_op |
        store_var x
        load_var x
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
}

#[tokio::test]
async fn assign_bitwise_xor() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 10;
            x ^= 3;
            x
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 10
        store_var x
        load_var x
        load_const 3
        bin_op ^
        store_var x
        load_var x
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(9)));
}
