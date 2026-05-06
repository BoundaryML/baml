//! Unified tests for for-in loops and C-style for loops.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ============================================================================
// For-in loops
// ============================================================================

#[tokio::test]
async fn for_loop_sum() {
    let output = baml_test!(
        r#"
        function sum(xs: int[]) -> int {
            let result = 0;

            for (let x in xs) {
                result += x;
            }

            result
        }

        function main() -> int {
            sum([1, 2, 3, 4])
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        load_const 2
        load_const 3
        load_const 4
        alloc_array 4
        call user.sum
        return
    }

    function sum(xs: int[]) -> int {
        load_const 0
        store_var result
        load_const 0
        store_var __for_idx

      L0:
        load_var __for_idx
        load_var xs
        call baml.Array.length
        cmp_op <
        pop_jump_if_false L1
        jump L2

      L1:
        load_var result
        return

      L2:
        load_var result
        load_var xs
        load_var __for_idx
        load_array_element
        add_int
        store_var result
        load_var __for_idx
        load_const 1
        add_int
        store_var __for_idx
        jump L0
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

#[tokio::test]
async fn for_loop_with_break() {
    let output = baml_test!(
        r#"
        function for_with_break(xs: int[]) -> int {
            let result = 0;

            for (let x in xs) {
                if (x > 10) {
                    break;
                }
                result += x;
            }

            result
        }

        function main() -> int {
            for_with_break([3, 4, 11, 100])
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function for_with_break(xs: int[]) -> int {
        load_const 0
        store_var result
        load_const 0
        store_var __for_idx

      L0:
        load_var __for_idx
        load_var xs
        call baml.Array.length
        cmp_op <
        pop_jump_if_false L2
        load_var xs
        load_var __for_idx
        load_array_element
        store_var x
        load_var x
        load_const 10
        cmp_op >
        pop_jump_if_false L1
        jump L2

      L1:
        load_var result
        load_var x
        add_int
        store_var result
        load_var __for_idx
        load_const 1
        add_int
        store_var __for_idx
        jump L0

      L2:
        load_var result
        return
    }

    function main() -> int {
        load_const 3
        load_const 4
        load_const 11
        load_const 100
        alloc_array 4
        call user.for_with_break
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn for_loop_with_continue() {
    let output = baml_test!(
        r#"
        function for_with_continue(xs: int[]) -> int {
            let result = 0;

            for (let x in xs) {
                if (x > 10) {
                    continue;
                }
                result += x;
            }

            result
        }

        function main() -> int {
            for_with_continue([5, 20, 6])
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function for_with_continue(xs: int[]) -> int {
        load_const 0
        store_var result
        load_const 0
        store_var __for_idx

      L0:
        load_var __for_idx
        load_var xs
        call baml.Array.length
        cmp_op <
        pop_jump_if_false L1
        jump L2

      L1:
        load_var result
        return

      L2:
        load_var xs
        load_var __for_idx
        load_array_element
        store_var x
        load_var x
        load_const 10
        cmp_op >
        pop_jump_if_false L3
        jump L4

      L3:
        load_var result
        load_var x
        add_int
        store_var result

      L4:
        load_var __for_idx
        load_const 1
        add_int
        store_var __for_idx
        jump L0
    }

    function main() -> int {
        load_const 5
        load_const 20
        load_const 6
        alloc_array 3
        call user.for_with_continue
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
}

#[tokio::test]
async fn for_loop_nested() {
    let output = baml_test!(
        r#"
        function nested_for(arr_a: int[], arr_b: int[]) -> int {
            let result = 0;

            for (let a in arr_a) {
                for (let b in arr_b) {
                    result += a * b;
                }
            }

            result
        }

        function main() -> int {
            nested_for([1, 2], [3, 4])
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        load_const 2
        alloc_array 2
        load_const 3
        load_const 4
        alloc_array 2
        call user.nested_for
        return
    }

    function nested_for(arr_a: int[], arr_b: int[]) -> int {
        load_const 0
        store_var result
        load_const 0
        store_var __for_idx

      L0:
        load_var __for_idx
        load_var arr_a
        call baml.Array.length
        cmp_op <
        pop_jump_if_false L1
        jump L2

      L1:
        load_var result
        return

      L2:
        load_var arr_a
        load_var __for_idx
        load_array_element
        store_var a
        load_const 0
        store_var __for_idx_1

      L3:
        load_var __for_idx_1
        load_var arr_b
        call baml.Array.length
        cmp_op <
        pop_jump_if_false L4
        jump L5

      L4:
        load_var __for_idx
        load_const 1
        add_int
        store_var __for_idx
        jump L0

      L5:
        load_var result
        load_var a
        load_var arr_b
        load_var __for_idx_1
        load_array_element
        bin_op *
        add_int
        store_var result
        load_var __for_idx_1
        load_const 1
        add_int
        store_var __for_idx_1
        jump L3
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(21)));
}

// ============================================================================
// C-style for loops
// ============================================================================

#[tokio::test]
async fn c_for_sum_to_ten() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let s = 0;

            for (let i = 1; i <= 10; i += 1) {
                s += i;
            }

            s
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 0
        store_var s
        load_const 1
        store_var i

      L0:
        load_var i
        load_const 10
        cmp_op <=
        pop_jump_if_false L1
        jump L2

      L1:
        load_var s
        return

      L2:
        load_var s
        load_var i
        add_int
        store_var s
        load_var i
        load_const 1
        add_int
        store_var i
        jump L0
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(55)));
}

#[tokio::test]
#[ignore = "compiler2: C-style for condition evaluated with TypeError (expected Bool, got Int)"]
async fn c_for_with_break_continue() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let s = 0;

            for (let i = 0; ; s += i) {
                i += 1;
                if (i > 10) {
                    let x = 0;
                    break;
                }
                if (i == 5) {
                    continue;
                }
            }

            s
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 0
        store_var i

      L0:
        load_const 0
        load_var i
        add_int
        pop_jump_if_false L2
        load_var i
        load_const 1
        add_int
        store_var i
        load_var i
        load_const 10
        cmp_op >
        pop_jump_if_false L1
        jump L2

      L1:
        load_var i
        load_const 5
        cmp_op ==
        pop_jump_if_false L0
        jump L0

      L2:
        load_const 0
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(55)));
}

#[tokio::test]
#[ignore = "compiler2: C-style for loop uses SysOp where Callable expected"]
async fn c_for_only_condition() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let s = 0;

            for (; false;) {
            }

            s
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const "missing statement"
        call baml.sys.panic
        pop 1
        jump L0

      L0:
        unreachable
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
#[ignore = "compiler2: C-style for loop uses SysOp where Callable expected"]
async fn c_for_endless_break() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let s = 0;

            for (;;) {
                break;
            }

            s
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const "missing statement"
        call baml.sys.panic
        pop 1
        jump L0

      L0:
        unreachable
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

// ============================================================================
// For-in loops over let-bound variables
// ============================================================================

/// Regression test: iterating over an array stored in a `let` variable
/// should work the same as iterating over an inline array literal.
#[tokio::test]
async fn for_loop_over_let_variable() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let xs = [1, 2, 3];
            let sum = 0;

            for (let x in xs) {
                sum += x;
            }

            sum
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

/// Same as above but without parenthesized syntax.
#[tokio::test]
async fn for_loop_over_let_variable_no_parens() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let xs = [10, 20, 30];
            let sum = 0;

            for let x in xs {
                sum += x;
            }

            sum
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(60)));
}

#[tokio::test]
async fn for_loop_final_if_without_semicolon_can_return() {
    let output = baml_test!(
        r#"
        function first_even(xs: int[]) -> int {
            for (let x in xs) {
                if (x % 2 == 0) {
                    return x;
                }
            }

            -1
        }

        function main() -> int {
            first_even([1, 3, 4, 7])
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(4)));
}

#[tokio::test]
async fn for_loop_final_if_else_without_semicolon_can_mutate() {
    let output = baml_test!(
        r#"
        function render(xs: string[]) -> string {
            let result = "";

            for (let x in xs) {
                if (x == "") {
                    result += ".";
                } else {
                    result += x;
                }
            }

            result
        }

        function main() -> string {
            render(["x", "", "o"])
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("x.o".to_string()))
    );
}

#[tokio::test]
async fn nested_for_loop_final_if_without_semicolon_can_return() {
    let output = baml_test!(
        r#"
        function has_empty_cell(grid: string[][]) -> bool {
            for (let row in grid) {
                for (let cell in row) {
                    if (cell == "") {
                        return true;
                    }
                }
            }

            false
        }

        function main() -> bool {
            has_empty_cell([["x", "o"], ["", "x"]])
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
