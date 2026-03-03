//! Unified tests for if/else expressions, statements, and block expressions.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ============================================================================
// Basic if/else
// ============================================================================

#[tokio::test]
async fn if_else_true_branch() {
    let output = baml_test!(
        "
        function main() -> int {
            if (true) { 1 } else { 2 }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const true
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 2
        jump L2

      L1:
        load_const 1

      L2:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn if_else_false_branch() {
    let output = baml_test!(
        "
        function main() -> int {
            if (false) { 1 } else { 2 }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const false
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 2
        jump L2

      L1:
        load_const 1

      L2:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn if_else_comparison() {
    let output = baml_test!(
        "
        function main() -> int {
            if (1 < 2) { 10 } else { 20 }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        load_const 2
        cmp_op <
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 20
        jump L2

      L1:
        load_const 10

      L2:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

#[tokio::test]
async fn if_else_equality() {
    let output = baml_test!(
        "
        function main() -> int {
            if (5 == 5) { 100 } else { 200 }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 5
        load_const 5
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 200
        jump L2

      L1:
        load_const 100

      L2:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

#[tokio::test]
async fn if_else_assign_to_variable() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = if (true) { 42 } else { 0 };
            x
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const true
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 0
        jump L2

      L1:
        load_const 42

      L2:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn if_else_with_local_in_branches() {
    let output = baml_test!(
        "
        function main() -> int {
            if (true) {
                let a = 1;
                a
            } else {
                let b = 2;
                b
            }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const true
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 2
        jump L2

      L1:
        load_const 1

      L2:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn if_else_with_parameter() {
    let output = baml_test! {
        baml: "
            function main(b: bool) -> int {
                if (b) { 1 } else { 2 }
            }
        ",
        args: { "b" => BexExternalValue::Bool(true) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function main(b: bool) -> int {
        load_var b
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 2
        jump L2

      L1:
        load_const 1

      L2:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn if_else_return_expr_with_locals() {
    let output = baml_test! {
        baml: "
            function main(b: bool) -> int {
                if (b) {
                    let a = 1;
                    a
                } else {
                    let a = 2;
                    a
                }
            }
        ",
        args: { "b" => BexExternalValue::Bool(true) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function main(b: bool) -> int {
        load_var b
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 2
        jump L2

      L1:
        load_const 1

      L2:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn if_else_assignment_with_param() {
    let output = baml_test! {
        baml: "
            function main(b: bool) -> int {
                let i = if (b) { 1 } else { 2 };
                i
            }
        ",
        args: { "b" => BexExternalValue::Bool(true) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function main(b: bool) -> int {
        load_var b
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 2
        jump L2

      L1:
        load_const 1

      L2:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn if_else_assignment_with_locals() {
    let output = baml_test! {
        baml: "
            function main(b: bool) -> int {
                let i = if (b) {
                    let a = 1;
                    a
                } else {
                    let a = 2;
                    a
                };

                i
            }
        ",
        args: { "b" => BexExternalValue::Bool(true) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function main(b: bool) -> int {
        load_var b
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 2
        jump L2

      L1:
        load_const 1

      L2:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// ============================================================================
// Nested / else-if
// ============================================================================

#[tokio::test]
async fn if_else_nested() {
    let output = baml_test!(
        "
        function main() -> int {
            if (true) {
                if (false) { 1 } else { 2 }
            } else {
                3
            }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const true
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 3
        jump L4

      L1:
        load_const false
        pop_jump_if_false L2
        jump L3

      L2:
        load_const 2
        jump L4

      L3:
        load_const 1

      L4:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn else_if_chain() {
    let output = baml_test!(
        "
        function main() -> int {
            if (false) {
                1
            } else if (false) {
                2
            } else {
                3
            }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const false
        pop_jump_if_false L0
        jump L3

      L0:
        load_const false
        pop_jump_if_false L1
        jump L2

      L1:
        load_const 3
        jump L4

      L2:
        load_const 2
        jump L4

      L3:
        load_const 1

      L4:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn else_if_with_comparisons() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 5;
            if (x < 0) {
                0
            } else if (x < 10) {
                1
            } else {
                2
            }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 5
        load_const 0
        cmp_op <
        pop_jump_if_false L0
        jump L3

      L0:
        load_const 5
        load_const 10
        cmp_op <
        pop_jump_if_false L1
        jump L2

      L1:
        load_const 2
        jump L4

      L2:
        load_const 1
        jump L4

      L3:
        load_const 0

      L4:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn else_if_with_parameter() {
    let output = baml_test! {
        baml: "
            function main(a: bool, b: bool) -> int {
                if (a) {
                    1
                } else if (b) {
                    2
                } else {
                    3
                }
            }
        ",
        args: { "a" => BexExternalValue::Bool(false), "b" => BexExternalValue::Bool(true) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function main(a: bool, b: bool) -> int {
        load_var a
        pop_jump_if_false L0
        jump L3

      L0:
        load_var b
        pop_jump_if_false L1
        jump L2

      L1:
        load_const 3
        jump L4

      L2:
        load_const 2
        jump L4

      L3:
        load_const 1

      L4:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn else_if_assignment() {
    let output = baml_test! {
        baml: "
            function main(a: bool, b: bool) -> int {
                let result = if (a) {
                    1
                } else if (b) {
                    2
                } else {
                    3
                };

                result
            }
        ",
        args: { "a" => BexExternalValue::Bool(false), "b" => BexExternalValue::Bool(true) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function main(a: bool, b: bool) -> int {
        load_var a
        pop_jump_if_false L0
        jump L3

      L0:
        load_var b
        pop_jump_if_false L1
        jump L2

      L1:
        load_const 3
        store_var result
        jump L4

      L2:
        load_const 2
        store_var result
        jump L4

      L3:
        load_const 1
        store_var result

      L4:
        load_var result
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn else_if_return_expr_with_locals() {
    let output = baml_test! {
        baml: "
            function main(a: bool, b: bool) -> int {
                if (a) {
                    let x = 1;
                    x
                } else if (b) {
                    let y = 2;
                    y
                } else {
                    let z = 3;
                    z
                }
            }
        ",
        args: { "a" => BexExternalValue::Bool(false), "b" => BexExternalValue::Bool(true) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function main(a: bool, b: bool) -> int {
        load_var a
        pop_jump_if_false L0
        jump L3

      L0:
        load_var b
        pop_jump_if_false L1
        jump L2

      L1:
        load_const 3
        jump L4

      L2:
        load_const 2
        jump L4

      L3:
        load_const 1

      L4:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn else_if_assignment_with_locals() {
    let output = baml_test! {
        baml: "
            function main(a: bool, b: bool) -> int {
                let result = if (a) {
                    let x = 1;
                    x
                } else if (b) {
                    let y = 2;
                    y
                } else {
                    let z = 3;
                    z
                };

                result
            }
        ",
        args: { "a" => BexExternalValue::Bool(false), "b" => BexExternalValue::Bool(true) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function main(a: bool, b: bool) -> int {
        load_var a
        pop_jump_if_false L0
        jump L3

      L0:
        load_var b
        pop_jump_if_false L1
        jump L2

      L1:
        load_const 3
        store_var result
        jump L4

      L2:
        load_const 2
        store_var result
        jump L4

      L3:
        load_const 1
        store_var result

      L4:
        load_var result
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

// ============================================================================
// Complex conditions
// ============================================================================

#[tokio::test]
async fn if_else_function_call_in_branch() {
    let output = baml_test!(
        "
        function get_value() -> int {
            42
        }

        function main() -> int {
            if (true) { get_value() } else { 0 }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function get_value() -> int {
        load_const 42
        return
    }

    function main() -> int {
        load_const true
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 0
        jump L2

      L1:
        call get_value

      L2:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn if_else_arithmetic_condition() {
    let output = baml_test!(
        "
        function main() -> int {
            if (1 + 1 == 2) { 100 } else { 0 }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        load_const 1
        bin_op +
        load_const 2
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 0
        jump L2

      L1:
        load_const 100

      L2:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

#[tokio::test]
async fn if_else_logical_and() {
    let output = baml_test!(
        "
        function main() -> int {
            if (true && true) { 1 } else { 0 }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const true
        pop_jump_if_false L0
        jump L1

      L0:
        load_const false
        jump L2

      L1:
        load_const true

      L2:
        pop_jump_if_false L3
        jump L4

      L3:
        load_const 0
        jump L5

      L4:
        load_const 1

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn if_else_logical_or() {
    let output = baml_test!(
        "
        function main() -> int {
            if (false || true) { 1 } else { 0 }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const false
        pop_jump_if_false L0
        jump L1

      L0:
        load_const true
        jump L2

      L1:
        load_const true

      L2:
        pop_jump_if_false L3
        jump L4

      L3:
        load_const 0
        jump L5

      L4:
        load_const 1

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// ============================================================================
// If-else as expression
// ============================================================================

#[tokio::test]
async fn if_else_in_arithmetic() {
    let output = baml_test!(
        "
        function main() -> int {
            1 + if (true) { 2 } else { 3 }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const true
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 3
        store_var _2
        jump L2

      L1:
        load_const 2
        store_var _2

      L2:
        load_const 1
        load_var _2
        bin_op +
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn if_else_as_function_arg() {
    let output = baml_test!(
        "
        function identity(x: int) -> int { x }

        function main() -> int {
            identity(if (false) { 10 } else { 20 })
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function identity(x: int) -> int {
        load_var x
        return
    }

    function main() -> int {
        load_const false
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 20
        jump L2

      L1:
        load_const 10

      L2:
        call identity
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(20)));
}

#[tokio::test]
async fn if_else_assigned_then_passed_to_call() {
    let output = baml_test!(
        "
        function identity(x: int) -> int { x }

        function main() -> int {
            let tmp = if (false) { 10 } else { 20 };
            identity(tmp)
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function identity(x: int) -> int {
        load_var x
        return
    }

    function main() -> int {
        load_const false
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 20
        jump L2

      L1:
        load_const 10

      L2:
        call identity
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(20)));
}

#[tokio::test]
async fn parenthesized_if_else_in_arithmetic() {
    let output = baml_test!(
        "
        function main() -> int {
            (if (true) { 1 } else { 2 }) + (if (false) { 3 } else { 4 })
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const true
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 2
        store_var _1
        jump L2

      L1:
        load_const 1
        store_var _1

      L2:
        load_const false
        pop_jump_if_false L3
        jump L4

      L3:
        load_const 4
        store_var _3
        jump L5

      L4:
        load_const 3
        store_var _3

      L5:
        load_var _1
        load_var _3
        bin_op +
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn chained_if_else_in_arithmetic() {
    let output = baml_test!(
        "
        function main() -> int {
            if (true) { 1 } else { 2 } + if (false) { 3 } else { 4 }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const true
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 2
        store_var _1
        jump L2

      L1:
        load_const 1
        store_var _1

      L2:
        load_const false
        pop_jump_if_false L3
        jump L4

      L3:
        load_const 4
        store_var _3
        jump L5

      L4:
        load_const 3
        store_var _3

      L5:
        load_var _1
        load_var _3
        bin_op +
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

// ============================================================================
// If without else
// ============================================================================

#[tokio::test]
async fn if_without_else() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 0;
            if (true) {
                x = 5;
            }
            x
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 0
        store_var x
        load_const true
        pop_jump_if_false L0
        load_const 5
        store_var x

      L0:
        load_var x
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn if_without_else_with_local() {
    let output = baml_test!(
        "
        function main() -> int {
            let result = 0;
            if (true) {
                let temp = 10;
            }
            result
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const true
        pop_jump_if_false L0
        load_const 10
        store_var temp

      L0:
        load_const 0
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn consecutive_if_without_else() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 0;
            if (true) { x = 1; }
            if (false) { x = 2; }
            x
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 0
        store_var x
        load_const true
        pop_jump_if_false L0
        load_const 1
        store_var x

      L0:
        load_const false
        pop_jump_if_false L1
        load_const 2
        store_var x

      L1:
        load_var x
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// ============================================================================
// Block expressions
// ============================================================================

#[tokio::test]
async fn block_expression() {
    let output = baml_test!(
        "
        function main() -> int {
            let a = {
                let b = 1;
                b
            };

            a
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// ============================================================================
// If-else as statement (with side effects)
// ============================================================================

#[tokio::test]
async fn if_else_statement() {
    let output = baml_test! {
        baml: "
            function identity(i: int) -> int {
                i
            }

            function main(b: bool) -> int {
                let a = 1;

                if (b) {
                    let x = 1;
                    let y = 2;
                    identity(x);
                } else {
                    let x = 3;
                    let y = 4;
                    identity(y);
                }

                a
            }
        ",
        args: { "b" => BexExternalValue::Bool(true) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function identity(i: int) -> int {
        load_var i
        return
    }

    function main(b: bool) -> int {
        load_var b
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 3
        store_var x
        load_const 4
        call identity
        pop 1
        jump L2

      L1:
        load_const 2
        store_var y
        load_const 1
        call identity
        pop 1

      L2:
        load_const 1
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn nested_block_with_if() {
    let output = baml_test!(
        "
        function main() -> int {
            let a = 1;

            {
                let b = 2;
                let c = 3;
                a = b + c;

                if (a == 5) {
                    a = 10;
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
        load_const 2
        load_const 3
        bin_op +
        store_var a
        load_var a
        load_const 5
        cmp_op ==
        pop_jump_if_false L0
        load_const 10
        store_var a

      L0:
        load_var a
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}
