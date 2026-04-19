//! Unified tests for while loops, break, and continue.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ============================================================================
// While loops
// ============================================================================

#[tokio::test]
async fn while_loop_gcd() {
    let output = baml_test! {
        baml: r#"
            function gcd(a: int, b: int) -> int {
                while (a != b) {
                    if (a > b) {
                        a = a - b;
                    } else {
                        b = b - a;
                    }
                }

                a
            }
        "#,
        entry: "gcd",
        args: { "a" => BexExternalValue::Int(21), "b" => BexExternalValue::Int(15) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function gcd(a: int, b: int) -> int {
      L0:
        load_var a
        load_var b
        cmp_op !=
        pop_jump_if_false L1
        jump L2

      L1:
        load_var a
        return

      L2:
        load_var a
        load_var b
        cmp_op >
        pop_jump_if_false L3
        jump L4

      L3:
        load_var b
        load_var a
        bin_op -
        store_var b
        jump L0

      L4:
        load_var a
        load_var b
        bin_op -
        store_var a
        jump L0
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn while_with_scope() {
    let output = baml_test! {
        baml: r#"
            function fib(n: int) -> int {
                let a = 0;
                let b = 1;

                while (n > 0) {
                    n -= 1;
                    let t = a + b;
                    b = a;
                    a = t;
                }

                a
            }
        "#,
        entry: "fib",
        args: { "n" => BexExternalValue::Int(5) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function fib(n: int) -> int {
        load_const 0
        store_var a
        load_const 1
        store_var b

      L0:
        load_var n
        load_const 0
        cmp_op >
        pop_jump_if_false L1
        jump L2

      L1:
        load_var a
        return

      L2:
        load_var n
        load_const 1
        bin_op -
        store_var n
        load_var a
        load_var b
        bin_op +
        store_var t
        load_var a
        store_var b
        load_var t
        store_var a
        jump L0
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn while_with_break() {
    let output = baml_test!(
        "
        function main() -> int {
            let a = 1;

            while (a < 5) {
                a += 1;

                if (a == 2) {
                    break;
                }
            }

            a
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        store_var a

      L0:
        load_var a
        load_const 5
        cmp_op <
        pop_jump_if_false L1
        load_var a
        load_const 1
        bin_op +
        store_var a
        load_var a
        load_const 2
        cmp_op ==
        pop_jump_if_false L0

      L1:
        load_var a
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

// ============================================================================
// Break
// ============================================================================

#[tokio::test]
async fn break_factorial() {
    let output = baml_test! {
        baml: r#"
            function factorial(limit: int) -> int {
                let result = 1;

                while (true) {
                    if (limit == 0) {
                        break;
                    }
                    result = result * limit;
                    limit = limit - 1;
                }

                result
            }
        "#,
        entry: "factorial",
        args: { "limit" => BexExternalValue::Int(5) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function factorial(limit: int) -> int {
        load_const 1
        store_var result

      L0:
        load_const true
        pop_jump_if_false L2
        load_var limit
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_var result
        load_var limit
        bin_op *
        store_var result
        load_var limit
        load_const 1
        bin_op -
        store_var limit
        jump L0

      L2:
        load_var result
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(120)));
}

#[tokio::test]
async fn break_nested() {
    let output = baml_test! {
        baml: r#"
            function nested() -> int {
                let a = 5;
                while (true) {
                    while (true) {
                        a = a + 1;
                        break;
                    }
                    a = a + 1;
                    break;
                }
                a
            }
        "#,
        entry: "nested",
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function nested() -> int {
        load_const 5
        store_var a
        load_const true
        pop_jump_if_false L1
        load_const true
        pop_jump_if_false L0
        load_var a
        load_const 1
        bin_op +
        store_var a

      L0:
        load_var a
        load_const 1
        bin_op +
        store_var a

      L1:
        load_var a
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn break_nested_with_variable_conditions() {
    let output = baml_test! {
        baml: r#"
            function nested(x: bool, y: bool) -> int {
                let a = 5;
                while (x) {
                    while (y) {
                        a = a + 1;
                        break;
                    }
                    a = a + 1;
                    break;
                }
                a
            }
        "#,
        entry: "nested",
        args: { "x" => BexExternalValue::Bool(true), "y" => BexExternalValue::Bool(true) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function nested(x: bool, y: bool) -> int {
        load_const 5
        store_var a
        load_var x
        pop_jump_if_false L1
        load_var y
        pop_jump_if_false L0
        load_var a
        load_const 1
        bin_op +
        store_var a

      L0:
        load_var a
        load_const 1
        bin_op +
        store_var a

      L1:
        load_var a
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn while_with_conditional_break() {
    let output = baml_test! {
        baml: r#"
            function count_down(n: int) -> int {
                let result = 0;
                while (true) {
                    result = result + n;
                    n = n - 1;
                    if (n == 0) {
                        break;
                    }
                }
                result
            }
        "#,
        entry: "count_down",
        args: { "n" => BexExternalValue::Int(3) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function count_down(n: int) -> int {
        load_const 0
        store_var result

      L0:
        load_const true
        pop_jump_if_false L1
        load_var result
        load_var n
        bin_op +
        store_var result
        load_var n
        load_const 1
        bin_op -
        store_var n
        load_var n
        load_const 0
        cmp_op ==
        pop_jump_if_false L0

      L1:
        load_var result
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

// ============================================================================
// Continue
// ============================================================================

#[tokio::test]
async fn continue_factorial() {
    let output = baml_test! {
        baml: r#"
            function factorial(limit: int) -> int {
                let result = 1;

                let should_continue = true;
                while (should_continue) {
                    result = result * limit;
                    limit = limit - 1;

                    if (limit != 0) {
                        continue;
                    } else {
                        should_continue = false;
                    }
                }

                result
            }
        "#,
        entry: "factorial",
        args: { "limit" => BexExternalValue::Int(5) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function factorial(limit: int) -> int {
        load_const 1
        store_var result
        load_const true
        store_var should_continue

      L0:
        load_var should_continue
        pop_jump_if_false L1
        jump L2

      L1:
        load_var result
        return

      L2:
        load_var result
        load_var limit
        bin_op *
        store_var result
        load_var limit
        load_const 1
        bin_op -
        store_var limit
        load_var limit
        load_const 0
        cmp_op !=
        pop_jump_if_false L3
        jump L0

      L3:
        load_const false
        store_var should_continue
        jump L0
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(120)));
}

#[tokio::test]
async fn continue_nested() {
    let output = baml_test! {
        baml: r#"
            function continue_nested() -> int {
                let execute = true;
                while (execute) {
                    while (false) {
                        continue;
                    }
                    if (false) {
                        continue;
                    }
                    execute = false;
                }
                5
            }
        "#,
        entry: "continue_nested",
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function continue_nested() -> int {
        load_const true
        store_var execute

      L0:
        load_var execute
        pop_jump_if_false L1
        jump L2

      L1:
        load_const 5
        return

      L2:
        load_const false
        pop_jump_if_false L3
        jump L2

      L3:
        load_const false
        pop_jump_if_false L4
        jump L0

      L4:
        load_const false
        store_var execute
        jump L0
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}
