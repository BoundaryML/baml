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
        call sum
        return
    }

    function sum(xs: int[]) -> int {
        load_const 0
        store_var result
        load_var xs
        call baml.Array.length
        store_var _len
        load_const 0
        store_var _i

      L0:
        load_var _i
        load_var _len
        cmp_op <
        pop_jump_if_false L1
        jump L2

      L1:
        load_var result
        return

      L2:
        load_var xs
        load_var _i
        load_array_element
        store_var x
        load_var _i
        load_const 1
        bin_op +
        store_var _i
        load_var result
        load_var x
        bin_op +
        store_var result
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
        load_var xs
        call baml.Array.length
        store_var _len
        load_const 0
        store_var _i

      L0:
        load_var _i
        load_var _len
        cmp_op <
        pop_jump_if_false L2
        load_var xs
        load_var _i
        load_array_element
        store_var x
        load_var _i
        load_const 1
        bin_op +
        store_var _i
        load_var x
        load_const 10
        cmp_op >
        pop_jump_if_false L1
        jump L2

      L1:
        load_var result
        load_var x
        bin_op +
        store_var result
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
        call for_with_break
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
        load_var xs
        call baml.Array.length
        store_var _len
        load_const 0
        store_var _i

      L0:
        load_var _i
        load_var _len
        cmp_op <
        pop_jump_if_false L1
        jump L2

      L1:
        load_var result
        return

      L2:
        load_var xs
        load_var _i
        load_array_element
        store_var x
        load_var _i
        load_const 1
        bin_op +
        store_var _i
        load_var x
        load_const 10
        cmp_op >
        pop_jump_if_false L3
        jump L0

      L3:
        load_var result
        load_var x
        bin_op +
        store_var result
        jump L0
    }

    function main() -> int {
        load_const 5
        load_const 20
        load_const 6
        alloc_array 3
        call for_with_continue
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
        call nested_for
        return
    }

    function nested_for(arr_a: int[], arr_b: int[]) -> int {
        load_const 0
        store_var result
        load_var arr_a
        call baml.Array.length
        store_var _len
        load_const 0
        store_var _i

      L0:
        load_var _i
        load_var _len
        cmp_op <
        pop_jump_if_false L1
        jump L2

      L1:
        load_var result
        return

      L2:
        load_var arr_a
        load_var _i
        load_array_element
        store_var a
        load_var _i
        load_const 1
        bin_op +
        store_var _i
        load_var arr_b
        call baml.Array.length
        store_var _len1
        load_const 0
        store_var _i1

      L3:
        load_var _i1
        load_var _len1
        cmp_op <
        pop_jump_if_false L0
        load_var arr_b
        load_var _i1
        load_array_element
        store_var b
        load_var _i1
        load_const 1
        bin_op +
        store_var _i1
        load_var result
        load_var a
        load_var b
        bin_op *
        bin_op +
        store_var result
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
        bin_op +
        store_var s
        load_var i
        load_const 1
        bin_op +
        store_var i
        jump L0
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(55)));
}

#[tokio::test]
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
        store_var s
        load_const 0
        store_var i

      L0:
        load_const true
        pop_jump_if_false L5
        load_var i
        load_const 1
        bin_op +
        store_var i
        load_var i
        load_const 10
        cmp_op >
        pop_jump_if_false L1
        jump L4

      L1:
        load_var i
        load_const 5
        cmp_op ==
        pop_jump_if_false L2
        jump L3

      L2:
        load_var s
        load_var i
        bin_op +
        store_var s
        jump L0

      L3:
        load_var s
        load_var i
        bin_op +
        store_var s
        jump L0

      L4:
        load_const 0
        store_var x

      L5:
        load_var s
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(55)));
}

#[tokio::test]
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
      L0:
        load_const false
        pop_jump_if_false L1
        jump L0

      L1:
        load_const 0
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const true
        pop_jump_if_false L0

      L0:
        load_const 0
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}
