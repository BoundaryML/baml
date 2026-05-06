//! Unified tests for watch functionality and viz headers.
//!
//! TODO: Notification assertions are documented as comments only. Once
//! `BexEngine` plumbs `VmExecState::Notify` through to callers, revisit
//! these tests to assert on `output.notifications`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ============================================================================
// Watch primitive
// ============================================================================

#[tokio::test]
async fn watch_primitive() {
    // Expected notifications: [["value"]]
    // (one notification event: channel "value" fires when value = 1)
    let output = baml_test!(
        r#"
        function main() -> int {
            watch let value = 0;
            value = 1;
            value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 0
        store_var value
        load_const "value"
        load_const null
        watch value
        load_const 1
        store_var value
        load_var value
        unwatch value
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn watch_primitive_nested_scope() {
    // Expected notifications: [["value"]]
    let output = baml_test!(
        r#"
        function main() -> int {
            watch let value = 0;
            if (true) {
                value = 1;
            }
            value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 0
        store_var value
        load_const "value"
        load_const null
        watch value
        load_const true
        pop_jump_if_false L0
        load_const 1
        store_var value

      L0:
        load_var value
        unwatch value
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn watch_class_destructure_initializes_and_unwatches_binding() {
    let output = baml_test!(
        r#"
        class Point {
            x int
            y int
        }

        function main() -> int {
            watch let Point { x } = Point { x: 1, y: 2 };
            x
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        alloc_instance user.Point
        load_const 1
        init_field .x
        load_const 2
        init_field .y
        load_field .x
        store_var x
        load_const "x"
        load_const null
        watch x
        load_var x
        unwatch x
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn watch_default_filter() {
    // Expected notifications: [["value"]]
    // (value = 0 is no-op (same value), value = 6 triggers notification)
    let output = baml_test!(
        r#"
        function main() -> int {
            watch let value = 0;
            value = 0;
            value = 6;
            value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 0
        store_var value
        load_const "value"
        load_const null
        watch value
        load_const 0
        store_var value
        load_const 6
        store_var value
        load_var value
        unwatch value
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

#[tokio::test]
#[ignore = "compiler2: $watch accessor not resolved for primitive types (unresolved member: int.$watch)"]
async fn watch_user_filter() {
    // Expected notifications: [["value"]]
    // (value = 1 filtered out by greater_than_five, value = 6 passes)
    let output = baml_test!(
        r#"
        function greater_than_five(value: int) -> bool {
            value > 5
        }

        function main() -> int {
            watch let value = 0;
            value.$watch.options(baml.WatchOptions { when: greater_than_five });
            value = 1;
            value = 6;
            value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function greater_than_five(value: int) -> bool {
        load_var value
        load_const 5
        cmp_op >
        return
    }

    function main() -> int {
        load_const 0
        store_var value
        load_const "value"
        load_const null
        watch value
        load_const "value"
        load_global greater_than_five
        watch value
        load_const 1
        store_var value
        load_const 6
        store_var value
        load_var value
        unwatch value
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

#[tokio::test]
#[ignore = "compiler2: $watch accessor not resolved for primitive types (unresolved member: int.$watch)"]
async fn watch_manual_notify() {
    // Expected notifications: [["value"]]
    // (assignments don't notify in manual mode, only explicit $watch.notify() does)
    let output = baml_test!(
        r#"
        function main() -> int {
            watch let value = 0;
            value.$watch.options(baml.WatchOptions { when: "manual" });
            value = 1;
            value = 2;
            value = 3;
            value.$watch.notify();
            value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 0
        store_var value
        load_const "value"
        load_const null
        watch value
        load_const "value"
        load_const "manual"
        watch value
        load_const 1
        store_var value
        load_const 2
        store_var value
        load_const 3
        store_var value
        notify value
        load_var value
        unwatch value
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ============================================================================
// Watch with aliases and scope exit
// ============================================================================

#[tokio::test]
async fn watch_alias() {
    // Expected notifications: [["point"]]
    // (alias.x = 1 notifies on channel "point")
    let output = baml_test!(
        r#"
        class Point { x int  y int }

        function main() -> int {
            watch let point = Point { x: 0, y: 0 };
            let alias = point;
            alias.x = 1;
            point.x
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        alloc_instance user.Point
        load_const 0
        init_field .x
        load_const 0
        init_field .y
        store_var point
        load_const "point"
        load_const null
        watch point
        load_var point
        load_const 1
        store_field .x
        load_var point
        load_field .x
        unwatch point
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn watch_alias_nested_scope() {
    // Expected notifications: [["point"]]
    let output = baml_test!(
        r#"
        class Point { x int  y int }

        function main() -> int {
            watch let point = Point { x: 0, y: 0 };
            if (true) {
                let alias = point;
                alias.x = 1;
            }
            point.x
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        alloc_instance user.Point
        load_const 0
        init_field .x
        load_const 0
        init_field .y
        store_var point
        load_const "point"
        load_const null
        watch point
        load_const true
        pop_jump_if_false L0
        load_var point
        load_const 1
        store_field .x

      L0:
        load_var point
        load_field .x
        unwatch point
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn watch_scope_exit() {
    // Expected notifications: [["point"]]
    // (point.x = 1 inside block notifies, outter_point.x = 2 after scope exit does not)
    let output = baml_test!(
        r#"
        class Point { x int  y int }

        function main() -> int {
            let outter_point = {
                watch let point = Point { x: 0, y: 0 };
                point.x = 1;
                point
            };
            outter_point.x = 2;
            outter_point.x
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        alloc_instance user.Point
        load_const 0
        init_field .x
        load_const 0
        init_field .y
        store_var point
        load_const "point"
        load_const null
        watch point
        load_var point
        load_const 1
        store_field .x
        load_var point
        store_var outter_point
        unwatch point
        load_var outter_point
        load_const 2
        store_field .x
        load_var outter_point
        load_field .x
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

// ============================================================================
// Watch teardown across abnormal exits
// ============================================================================
//
// These tests pin the `unwatch` emission for `break`, `continue`, and early
// `return` so the helper that consolidates them (folding the inline
// loops into `emit_unwatch_to_depth`) cannot regress behavior. They also
// document the per-iteration semantic for `continue`: the watch is re-issued
// at the top of the next iteration, not held for the whole loop.

#[tokio::test]
async fn watch_break_unwatches() {
    // Expected notifications: [["x"]]
    // (iter 1 assigns x = 10 → notify; iter 2 hits break before any assignment.)
    //
    // unwatch x must precede the goto to the loop exit so the watcher is
    // torn down before iteration ends.
    let output = baml_test!(
        r#"
        function main() -> int {
            let total = 0;
            for (let i in [1, 2, 3]) {
                watch let x = i;
                if (x > 1) {
                    break;
                }
                x = x + 9;
                total = total + x;
            }
            total
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 0
        store_var total
        load_const 1
        load_const 2
        load_const 3
        alloc_array 3
        store_var _2
        load_const 0
        store_var __for_idx

      L0:
        load_var __for_idx
        load_var _2
        call baml.Array.length
        cmp_op <
        pop_jump_if_false L3
        load_var _2
        load_var __for_idx
        load_array_element
        store_var x
        load_const "x"
        load_const null
        watch x
        load_var x
        load_const 1
        cmp_op >
        pop_jump_if_false L1
        jump L2

      L1:
        load_var x
        load_const 9
        add_int
        store_var x
        load_var total
        load_var x
        add_int
        store_var total
        unwatch x
        load_var __for_idx
        load_const 1
        add_int
        store_var __for_idx
        jump L0

      L2:
        unwatch x

      L3:
        load_var total
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

#[tokio::test]
async fn watch_continue_unwatches() {
    // Expected notifications: [["x"], ["x"]]
    // (iter 1 assigns x = 11; iter 2 hits continue before assigning; iter 3
    // assigns x = 13. Each `watch let x = i` re-issues the watcher at the top
    // of its iteration, so unwatch on continue is per-iteration, not
    // permanent for the loop.)
    //
    // unwatch x must precede the goto to the continue target (the increment
    // step), AND must also fire on normal fallthrough at end of body.
    let output = baml_test!(
        r#"
        function main() -> int {
            let total = 0;
            for (let i in [1, 2, 3]) {
                watch let x = i;
                if (x == 2) {
                    continue;
                }
                x = x + 10;
                total = total + x;
            }
            total
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 0
        store_var total
        load_const 1
        load_const 2
        load_const 3
        alloc_array 3
        store_var _2
        load_const 0
        store_var __for_idx

      L0:
        load_var __for_idx
        load_var _2
        call baml.Array.length
        cmp_op <
        pop_jump_if_false L1
        jump L2

      L1:
        load_var total
        return

      L2:
        load_var _2
        load_var __for_idx
        load_array_element
        store_var x
        load_const "x"
        load_const null
        watch x
        load_var x
        load_const 2
        cmp_op ==
        pop_jump_if_false L3
        jump L4

      L3:
        load_var x
        load_const 10
        add_int
        store_var x
        load_var total
        load_var x
        add_int
        store_var total
        unwatch x
        jump L5

      L4:
        unwatch x

      L5:
        load_var __for_idx
        load_const 1
        add_int
        store_var __for_idx
        jump L0
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(24)));
}

#[tokio::test]
async fn watch_early_return_unwatches() {
    // Expected notifications: [["x"]]
    // (x = 42 notifies; the return path then unwatches before exiting.)
    //
    // unwatch x must precede the goto to the function's exit block.
    let output = baml_test!(
        r#"
        function main() -> int {
            watch let x = 0;
            x = 42;
            if (true) {
                return x;
            }
            x = 99;
            x
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 0
        store_var x
        load_const "x"
        load_const null
        watch x
        load_const 42
        store_var x
        load_const true
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 99
        store_var x
        load_var x
        unwatch x
        jump L2

      L1:
        load_var x
        unwatch x

      L2:
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// ============================================================================
// Watch teardown across throw and arm-body fallthrough
// ============================================================================
//
// `throw` and match/catch arm-body fallthrough are exit paths that previously
// did not emit `unwatch` ops:
//   - `Stmt::Throw` in MIR went straight to a dead block (lower.rs:3884-3889).
//   - Match arm bodies (lower.rs:4769-4775, 4785-4797) and catch arm bodies
//     (lower.rs:5343-5354) only restored locals; they did not unwatch
//     arm-declared `watch let`s before the goto-to-join.
//
// These tests pin the corrected behavior via bytecode snapshots and verify
// the function still produces the expected runtime result.

#[tokio::test]
async fn watch_throw_unwatches() {
    // Expected notifications: [["x"]]
    // (x = 5 notifies before throw; the unwatch then runs before the throw
    // terminator so the watcher is torn down on the divergent path.)
    let output = baml_test!(
        r#"
        function fails() -> int {
            watch let x = 0;
            x = 5;
            throw "boom";
        }

        function main() -> int {
            fails() catch (e) {
                "boom" => 99,
                _ => -1,
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function fails() -> int {
        load_const 0
        store_var x
        load_const "x"
        load_const null
        watch x
        load_const 5
        store_var x
        unwatch x
        load_const "boom"
        throw
    }

    function main() -> int {
        call user.fails
        jump L2
        load_var e
        load_const "boom"
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw_if_panic
        load_const 1
        unary_op -
        jump L2

      L1:
        load_const 99

      L2:
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn watch_for_throw_unwatches() {
    // Expected notifications: [["x"]]
    // (iter 1 assigns x = 10 → notify; iter 2 throws — the throw must unwatch
    // x before the throw terminator. The watch is also re-issued each iteration.)
    let output = baml_test!(
        r#"
        function fails() -> int {
            for (let i in [1, 2, 3]) {
                watch let x = i;
                if (x == 2) {
                    throw "boom";
                }
                x = x + 9;
            }
            0
        }

        function main() -> int {
            fails() catch (e) {
                "boom" => 99,
                _ => -1,
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function fails() -> int {
        load_const 1
        load_const 2
        load_const 3
        alloc_array 3
        store_var _1
        load_const 0
        store_var __for_idx

      L0:
        load_var __for_idx
        load_var _1
        call baml.Array.length
        cmp_op <
        pop_jump_if_false L1
        jump L2

      L1:
        load_const 0
        return

      L2:
        load_var _1
        load_var __for_idx
        load_array_element
        store_var x
        load_const "x"
        load_const null
        watch x
        load_var x
        load_const 2
        cmp_op ==
        pop_jump_if_false L3
        jump L4

      L3:
        load_var x
        load_const 9
        add_int
        store_var x
        unwatch x
        load_var __for_idx
        load_const 1
        add_int
        store_var __for_idx
        jump L0

      L4:
        unwatch x
        load_const "boom"
        throw
    }

    function main() -> int {
        call user.fails
        jump L2
        load_var e
        load_const "boom"
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw_if_panic
        load_const 1
        unary_op -
        jump L2

      L1:
        load_const 99

      L2:
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn watch_while_throw_unwatches() {
    // Expected notifications: [["x"]]
    // (Same shape as `watch_for_throw_unwatches` but with a `while` loop.
    // The MIR's while-body lowering pushes a Block expression for the body,
    // so the `watch let x` snapshot/teardown is anchored at the MIR layer.)
    let output = baml_test!(
        r#"
        function fails() -> int {
            let i = 0;
            while (i < 3) {
                watch let x = i;
                if (x == 1) {
                    throw "boom";
                }
                x = x + 10;
                i = i + 1;
            }
            0
        }

        function main() -> int {
            fails() catch (e) {
                "boom" => 99,
                _ => -1,
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function fails() -> int {
        load_const 0
        store_var i

      L0:
        load_var i
        load_const 3
        cmp_op <
        pop_jump_if_false L1
        jump L2

      L1:
        load_const 0
        return

      L2:
        load_var i
        store_var x
        load_const "x"
        load_const null
        watch x
        load_var x
        load_const 1
        cmp_op ==
        pop_jump_if_false L3
        jump L4

      L3:
        load_var x
        load_const 10
        add_int
        store_var x
        load_var i
        load_const 1
        add_int
        store_var i
        unwatch x
        jump L0

      L4:
        unwatch x
        load_const "boom"
        throw
    }

    function main() -> int {
        call user.fails
        jump L2
        load_var e
        load_const "boom"
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw_if_panic
        load_const 1
        unary_op -
        jump L2

      L1:
        load_const 99

      L2:
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn watch_match_arm_throw_unwatches() {
    // Expected notifications: [["x"]]
    // (The match arm declares a watch and assigns to it before throwing.
    // The throw path must unwatch x before the throw terminator.)
    let output = baml_test!(
        r#"
        function fails(input: int) -> int {
            match (input) {
                1 => {
                    watch let x = 0;
                    x = 5;
                    throw "boom"
                }
                _ => 0
            }
        }

        function main() -> int {
            fails(1) catch (e) {
                "boom" => 99,
                _ => -1,
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function fails(input: int) -> int {
        load_var input
        load_const 1
        cmp_int_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 0
        return

      L1:
        load_const 0
        store_var x
        load_const "x"
        load_const null
        watch x
        load_const 5
        store_var x
        unwatch x
        load_const "boom"
        throw
    }

    function main() -> int {
        load_const 1
        call user.fails
        jump L2
        load_var e
        load_const "boom"
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw_if_panic
        load_const 1
        unary_op -
        jump L2

      L1:
        load_const 99

      L2:
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn watch_catch_arm_throw_unwatches() {
    // Expected notifications: [["x"]]
    // (The catch arm body declares a watch, assigns to it, then re-throws.
    // The throw path must unwatch x before re-throwing — otherwise the
    // arm-scoped watch leaks past the function.)
    let output = baml_test!(
        r#"
        function inner() -> int {
            throw "first";
        }

        function fails() -> int {
            inner() catch (e) {
                _ => {
                    watch let x = 0;
                    x = 5;
                    throw "boom"
                }
            }
        }

        function main() -> int {
            fails() catch (e) {
                "boom" => 99,
                _ => -1,
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function fails() -> int {
        call user.inner
        jump L0
        load_var e
        throw_if_panic
        load_const 0
        store_var x
        load_const "x"
        load_const null
        watch x
        load_const 5
        store_var x
        unwatch x
        load_const "boom"
        throw

      L0:
        return
    }

    function inner() -> int {
        load_const "first"
        throw
    }

    function main() -> int {
        call user.fails
        jump L2
        load_var e
        load_const "boom"
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw_if_panic
        load_const 1
        unary_op -
        jump L2

      L1:
        load_const 99

      L2:
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn watch_match_arm_fallthrough_unwatches() {
    // Expected notifications: [["x"]]
    // (The match arm declares a watch let, assigns to it, and falls through
    // to the join. `unwatch x` must precede the goto to the join, otherwise
    // the arm-scoped watch leaks for the rest of the function.)
    //
    // After the match expression returns, a subsequent assignment to a
    // distinct outer var must NOT notify on channel "x".
    let output = baml_test!(
        r#"
        function entry(input: int) -> int {
            let result = match (input) {
                1 => {
                    watch let x = 0;
                    x = 5;
                    x
                }
                _ => 0
            };
            // If the arm-scoped watch leaked, this assignment would be
            // observed by an `x` watcher. After the arm, x must already be
            // unwatched.
            let result2 = result + 1;
            result2
        }

        function main() -> int {
            entry(1)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function entry(input: int) -> int {
        load_var input
        load_const 1
        cmp_int_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 0
        store_var result
        jump L2

      L1:
        load_const 0
        store_var x
        load_const "x"
        load_const null
        watch x
        load_const 5
        store_var x
        load_var x
        store_var result
        unwatch x

      L2:
        load_var result
        load_const 1
        bin_op +
        return
    }

    function main() -> int {
        load_const 1
        call user.entry
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

#[tokio::test]
async fn watch_switch_arm_fallthrough_unwatches() {
    // Expected notifications: [["x"]]
    //
    // Four dense int arms drive `try_lower_as_switch` (lower.rs:4351), which
    // emits a Switch terminator and lowers the matching arm body in
    // `try_lower_as_switch` itself rather than `lower_match_chain`. The arm
    // body declares a `watch let x` and falls through to the join — the
    // watcher must be torn down before the goto-to-join so it does not leak
    // past the arm. After the match returns, an assignment to a distinct
    // outer variable must NOT be observed by an `x` watcher.
    let output = baml_test!(
        r#"
        function entry(input: int) -> int {
            let result = match (input) {
                0 => 100,
                1 => {
                    watch let x = 0;
                    x = 5;
                    x
                }
                2 => 102,
                3 => 103,
                _ => 999
            };
            // If the arm-scoped watch leaked past the arm, this assignment
            // would be observed by an `x` watcher. After the arm, x must
            // already be unwatched.
            let result2 = result + 1;
            result2
        }

        function main() -> int {
            entry(1)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function entry(input: int) -> int {
        load_var input
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        store_var result
        jump L5

      L1: 3
        load_const 103
        store_var result
        jump L5

      L2: 2
        load_const 102
        store_var result
        jump L5

      L3: 1
        load_const 0
        store_var x
        load_const "x"
        load_const null
        watch x
        load_const 5
        store_var x
        load_var x
        store_var result
        unwatch x
        jump L5

      L4: 0
        load_const 100
        store_var result

      L5:
        load_var result
        load_const 1
        bin_op +
        return
    }

    function main() -> int {
        load_const 1
        call user.entry
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

#[tokio::test]
async fn watch_catch_arm_fallthrough_unwatches() {
    // Expected notifications: [["x"]]
    // (Same shape as watch_match_arm_fallthrough_unwatches, but the watch
    // is declared inside a catch arm body that falls through to the join.)
    let output = baml_test!(
        r#"
        function fails() -> int {
            throw "boom";
        }

        function main() -> int {
            let result = fails() catch (e) {
                _ => {
                    watch let x = 0;
                    x = 5;
                    x
                }
            };
            let result2 = result + 1;
            result2
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function fails() -> int {
        load_const "boom"
        throw
    }

    function main() -> int {
        call user.fails
        store_var result
        jump L0
        load_var e
        throw_if_panic
        load_const 0
        store_var x
        load_const "x"
        load_const null
        watch x
        load_const 5
        store_var x
        load_var x
        store_var result
        unwatch x

      L0:
        load_var result
        load_const 1
        bin_op +
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

// ============================================================================
// Watch with function calls and nested objects
// ============================================================================

#[tokio::test]
async fn watch_function_call_modifications() {
    // Expected notifications: [["point"], ["point"]]
    // (self.x = x and self.y = y each trigger a notification)
    let output = baml_test!(
        r#"
        class Point {
            x int
            y int

            function set(self, x: int, y: int) -> Point {
                self.x = x;
                self.y = y;
                self
            }
        }

        function main() -> int {
            watch let point = Point { x: 0, y: 0 };
            point.set(1, 2);
            point.x + point.y
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function Point.set(self: null, x: int, y: int) -> Point {
        load_var self
        load_var x
        store_field .x
        load_var self
        load_var y
        store_field .y
        load_var self
        return
    }

    function main() -> int {
        alloc_instance user.Point
        load_const 0
        init_field .x
        load_const 0
        init_field .y
        store_var point
        load_const "point"
        load_const null
        watch point
        load_var point
        load_const 1
        load_const 2
        call user.Point.set
        pop 1
        load_var point
        load_field .x
        load_var point
        load_field .y
        add_int
        unwatch point
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn watch_nested_object_added() {
    // Expected notifications: [["vec"], ["vec"]]
    // (vec.p = p notifies, then p.x.value = 2 also notifies because p is now part of vec)
    let output = baml_test!(
        r#"
        class Value { value int }
        class Point { x Value  y Value }
        class Vec2D { p Point  q Point }

        function main() -> int {
            watch let vec = Vec2D {
                p: Point { x: Value { value: 0 }, y: Value { value: 0 } },
                q: Point { x: Value { value: 0 }, y: Value { value: 0 } },
            };
            let p = Point { x: Value { value: 1 }, y: Value { value: 1 } };
            vec.p = p;
            p.x.value = 2;
            vec.p.x.value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        alloc_instance user.Vec2D
        alloc_instance user.Point
        alloc_instance user.Value
        load_const 0
        init_field .value
        init_field .x
        alloc_instance user.Value
        load_const 0
        init_field .value
        init_field .y
        init_field .p
        alloc_instance user.Point
        alloc_instance user.Value
        load_const 0
        init_field .value
        init_field .x
        alloc_instance user.Value
        load_const 0
        init_field .value
        init_field .y
        init_field .q
        store_var vec
        load_const "vec"
        load_const null
        watch vec
        alloc_instance user.Point
        alloc_instance user.Value
        load_const 1
        init_field .value
        init_field .x
        alloc_instance user.Value
        load_const 1
        init_field .value
        init_field .y
        store_var p
        load_var vec
        load_var p
        store_field .p
        load_var p
        load_field .x
        load_const 2
        store_field .value
        load_var vec
        load_field .p
        load_field .x
        load_field .value
        unwatch vec
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn watch_nested_object_removed() {
    // Expected notifications: [["vec"]]
    // (vec.p = <new Point> notifies, then p.x.value = 2 does NOT notify because p
    //  was detached from vec)
    let output = baml_test!(
        r#"
        class Value { value int }
        class Point { x Value  y Value }
        class Vec2D { p Point  q Point }

        function main() -> int {
            watch let vec = Vec2D {
                p: Point { x: Value { value: 0 }, y: Value { value: 0 } },
                q: Point { x: Value { value: 0 }, y: Value { value: 0 } },
            };
            let p = vec.p;
            vec.p = Point { x: Value { value: 1 }, y: Value { value: 1 } };
            p.x.value = 2;
            vec.p.x.value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        alloc_instance user.Vec2D
        alloc_instance user.Point
        alloc_instance user.Value
        load_const 0
        init_field .value
        init_field .x
        alloc_instance user.Value
        load_const 0
        init_field .value
        init_field .y
        init_field .p
        alloc_instance user.Point
        alloc_instance user.Value
        load_const 0
        init_field .value
        init_field .x
        alloc_instance user.Value
        load_const 0
        init_field .value
        init_field .y
        init_field .q
        store_var vec
        load_const "vec"
        load_const null
        watch vec
        load_var vec
        load_field .p
        store_var p
        load_var vec
        alloc_instance user.Point
        alloc_instance user.Value
        load_const 1
        init_field .value
        init_field .x
        alloc_instance user.Value
        load_const 1
        init_field .value
        init_field .y
        store_field .p
        load_var p
        load_field .x
        load_const 2
        store_field .value
        load_var vec
        load_field .p
        load_field .x
        load_field .value
        unwatch vec
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// ============================================================================
// Cyclic graph
// ============================================================================

#[tokio::test]
async fn watch_cyclic_graph() {
    // Expected notifications:
    //   v2.edges = [v3]       -> [["v2"]]
    //   v3.edges = [v4]       -> [["v2"]]
    //   v4.edges = [v1]       -> [["v2", "v4"]]
    //   v2.value = 20         -> [["v2", "v4"]]
    //   v1.value = 10         -> [["v2", "v4"]]
    //   v3.value = 30         -> [["v2", "v4"]]
    let output = baml_test!(
        r#"
        class Vertex {
            edges Vertex[]
            value int
        }

        function main() -> int {
            let v1 = Vertex { value: 1, edges: [] };
            watch let v2 = Vertex { value: 2, edges: [] };
            let v3 = Vertex { value: 3, edges: [] };
            watch let v4 = Vertex { value: 4, edges: [] };

            v1.edges = [v2];
            v2.edges = [v3];
            v3.edges = [v4];
            v4.edges = [v1];

            v2.value = 20;
            v1.value = 10;
            v3.value = 30;

            0
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        alloc_instance user.Vertex
        alloc_array 0
        init_field .edges
        load_const 1
        init_field .value
        store_var v1
        alloc_instance user.Vertex
        alloc_array 0
        init_field .edges
        load_const 2
        init_field .value
        store_var v2
        load_const "v2"
        load_const null
        watch v2
        alloc_instance user.Vertex
        alloc_array 0
        init_field .edges
        load_const 3
        init_field .value
        store_var v3
        alloc_instance user.Vertex
        alloc_array 0
        init_field .edges
        load_const 4
        init_field .value
        store_var v4
        load_const "v4"
        load_const null
        watch v4
        load_var v1
        load_var v2
        alloc_array 1
        store_field .edges
        load_var v2
        load_var v3
        alloc_array 1
        store_field .edges
        load_var v3
        load_var v4
        alloc_array 1
        store_field .edges
        load_var v4
        load_var v1
        alloc_array 1
        store_field .edges
        load_var v2
        load_const 20
        store_field .value
        load_var v1
        load_const 10
        store_field .value
        load_var v3
        load_const 30
        store_field .value
        unwatch v4
        unwatch v2
        load_const 0
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

// ============================================================================
// Block notifications (//# headers)
// ============================================================================

#[tokio::test]
async fn block_notification_basic() {
    // Expected notifications: [Block("test_blocks", "entering_computation", Statement, enter)]
    let output = baml_test!(
        r#"
        function main() -> int {
            //# entering_computation
            let x = 1;
            let y = 2;
            x + y
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        notify_block entering_computation
        load_const 1
        load_const 2
        bin_op +
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn block_notification_multiple() {
    // Expected notifications:
    //   [Block("test_multiple_blocks", "first_block", Statement, enter)]
    //   [Block("test_multiple_blocks", "second_block", Statement, enter)]
    let output = baml_test!(
        r#"
        function main() -> int {
            //# first_block
            let x = 1;
            //# second_block
            let y = 2;
            x + y
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        notify_block first_block
        notify_block second_block
        load_const 1
        load_const 2
        bin_op +
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ============================================================================
// VizEnter/VizExit (//# header before control flow)
// ============================================================================

#[tokio::test]
async fn viz_header_before_if() {
    // Expected notifications:
    //   Block("header_before_if", "MyHeader", Statement, enter)
    //   Viz("header_before_if", "MyHeader", enter)
    //   Viz("header_before_if", "MyHeader", exit)
    let output = baml_test! {
        baml: r#"
            function header_before_if() -> int {
                //# MyHeader
                if (true) {
                    1
                } else {
                    2
                }
            }
        "#,
        entry: "header_before_if",
    };

    insta::assert_snapshot!(output.bytecode, @"
    function header_before_if() -> int {
        notify_block MyHeader
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
async fn viz_header_before_while() {
    // Expected notifications:
    //   Block("header_before_while", "LoopHeader", Statement, enter)
    //   Viz("header_before_while", "LoopHeader", enter)
    //   Viz("header_before_while", "LoopHeader", exit)
    let output = baml_test! {
        baml: r#"
            function header_before_while() -> int {
                let x = 0;
                //# LoopHeader
                while (x < 1) {
                    x = x + 1;
                }
                x
            }
        "#,
        entry: "header_before_while",
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function header_before_while() -> int {
        load_const 0
        store_var x
        notify_block LoopHeader

      L0:
        load_var x
        load_const 1
        cmp_op <
        pop_jump_if_false L1
        jump L2

      L1:
        load_var x
        return

      L2:
        load_var x
        load_const 1
        add_int
        store_var x
        jump L0
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn viz_standalone_header_no_viz_events() {
    // Expected notifications: [Block("standalone_header", "JustAHeader", Statement, enter)]
    // (no VizEnter/VizExit because header is not before control flow)
    let output = baml_test! {
        baml: r#"
            function standalone_header() -> int {
                //# JustAHeader
                let x = 5;
                x
            }
        "#,
        entry: "standalone_header",
    };

    insta::assert_snapshot!(output.bytecode, @"
    function standalone_header() -> int {
        notify_block JustAHeader
        load_const 5
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn viz_multiple_headers_only_one_before_if() {
    // Expected notifications:
    //   Block("multiple_headers", "FirstHeader", Statement, enter)
    //   Block("multiple_headers", "SecondHeader", Statement, enter)
    //   Viz("multiple_headers", "SecondHeader", enter)   (only SecondHeader precedes if)
    //   Viz("multiple_headers", "SecondHeader", exit)
    let output = baml_test! {
        baml: r#"
            function multiple_headers() -> int {
                //# FirstHeader
                let x = 1;
                //# SecondHeader
                if (x > 0) {
                    2
                } else {
                    3
                }
            }
        "#,
        entry: "multiple_headers",
    };

    insta::assert_snapshot!(output.bytecode, @"
    function multiple_headers() -> int {
        notify_block FirstHeader
        notify_block SecondHeader
        load_const 1
        load_const 0
        cmp_op >
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 3
        jump L2

      L1:
        load_const 2

      L2:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn viz_if_without_header_no_viz() {
    // Expected notifications: [] (no header, no viz events)
    let output = baml_test! {
        baml: r#"
            function if_no_header() -> int {
                if (true) {
                    1
                } else {
                    2
                }
            }
        "#,
        entry: "if_no_header",
    };

    insta::assert_snapshot!(output.bytecode, @"
    function if_no_header() -> int {
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
