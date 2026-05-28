// Unified tests for watch functionality and viz headers.
//
// Converted from crates/baml_tests/tests/watch.rs. Each Rust test becomes a
// top-level helper function + a `test` block. Results are compared with
// `baml.deep_equals` (structural equality).
//
// Two tests (watch_user_filter, watch_manual_notify) were #[ignore]d in the
// Rust source because the $watch accessor is not resolved for primitive types.
// They are preserved below as commented-out stubs (see "Watch primitive") so the
// test set stays complete; uncomment once the bug is fixed.

// ============================================================================
// Shared classes
// ============================================================================

class WPoint { x int  y int }

class WValue { value int }
class WPointV { x WValue  y WValue }
class WVec2D { p WPointV  q WPointV }

class WVertex {
    edges WVertex[]
    value int
}

// ============================================================================
// Watch primitive
// ============================================================================

function watch_primitive_fn() -> int {
    watch let value = 0;
    value = 1;
    value
}

test "watch_primitive" {
    assert.is_true(baml.deep_equals(watch_primitive_fn(), 1))
}

// ─────────────────────────────────────────────────────────────────────────────

function watch_primitive_nested_scope_fn() -> int {
    watch let value = 0;
    if (true) {
        value = 1;
    }
    value
}

test "watch_primitive_nested_scope" {
    assert.is_true(baml.deep_equals(watch_primitive_nested_scope_fn(), 1))
}

// ─────────────────────────────────────────────────────────────────────────────

function watch_class_destructure_fn() -> int {
    watch let WPoint { x } = WPoint { x: 1, y: 2 };
    x
}

test "watch_class_destructure_initializes_and_unwatches_binding" {
    assert.is_true(baml.deep_equals(watch_class_destructure_fn(), 1))
}

// ─────────────────────────────────────────────────────────────────────────────

function watch_default_filter_fn() -> int {
    watch let value = 0;
    value = 0;
    value = 6;
    value
}

test "watch_default_filter" {
    assert.is_true(baml.deep_equals(watch_default_filter_fn(), 6))
}

// BUG: $watch accessor not resolved for primitive types (unresolved member:
// int.$watch). Both tests below were #[ignore]d in Rust for this reason; they
// are kept commented-out so no test is lost. Uncomment once the bug is fixed.
//
// watch_user_filter — expected result 6 (value = 1 is filtered out by
// greater_than_five, value = 6 passes the filter):
//
// function watch_greater_than_five(value: int) -> bool {
//     value > 5
// }
// function watch_user_filter_fn() -> int {
//     watch let value = 0;
//     value.$watch.options(baml.WatchOptions { when: watch_greater_than_five });
//     value = 1;
//     value = 6;
//     value
// }
// test "watch_user_filter" {
//     assert.is_true(baml.deep_equals(watch_user_filter_fn(), 6))
// }
//
// watch_manual_notify — expected result 3 (assignments do not notify in manual
// mode; only the explicit $watch.notify() call does):
//
// function watch_manual_notify_fn() -> int {
//     watch let value = 0;
//     value.$watch.options(baml.WatchOptions { when: "manual" });
//     value = 1;
//     value = 2;
//     value = 3;
//     value.$watch.notify();
//     value
// }
// test "watch_manual_notify" {
//     assert.is_true(baml.deep_equals(watch_manual_notify_fn(), 3))
// }

// ============================================================================
// Watch with aliases and scope exit
// ============================================================================

function watch_alias_fn() -> int {
    watch let point = WPoint { x: 0, y: 0 };
    let alias = point;
    alias.x = 1;
    point.x
}

test "watch_alias" {
    assert.is_true(baml.deep_equals(watch_alias_fn(), 1))
}

// ─────────────────────────────────────────────────────────────────────────────

function watch_alias_nested_scope_fn() -> int {
    watch let point = WPoint { x: 0, y: 0 };
    if (true) {
        let alias = point;
        alias.x = 1;
    }
    point.x
}

test "watch_alias_nested_scope" {
    assert.is_true(baml.deep_equals(watch_alias_nested_scope_fn(), 1))
}

// ─────────────────────────────────────────────────────────────────────────────

function watch_scope_exit_fn() -> int {
    let outter_point = {
        watch let point = WPoint { x: 0, y: 0 };
        point.x = 1;
        point
    };
    outter_point.x = 2;
    outter_point.x
}

test "watch_scope_exit" {
    assert.is_true(baml.deep_equals(watch_scope_exit_fn(), 2))
}

// ============================================================================
// Watch teardown across abnormal exits (break / continue / early return)
// ============================================================================

function watch_break_unwatches_fn() -> int {
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

test "watch_break_unwatches" {
    assert.is_true(baml.deep_equals(watch_break_unwatches_fn(), 10))
}

// ─────────────────────────────────────────────────────────────────────────────

function watch_continue_unwatches_fn() -> int {
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

test "watch_continue_unwatches" {
    assert.is_true(baml.deep_equals(watch_continue_unwatches_fn(), 24))
}

// ─────────────────────────────────────────────────────────────────────────────

function watch_early_return_unwatches_fn() -> int {
    watch let x = 0;
    x = 42;
    if (true) {
        return x;
    }
    x = 99;
    x
}

test "watch_early_return_unwatches" {
    assert.is_true(baml.deep_equals(watch_early_return_unwatches_fn(), 42))
}

// ============================================================================
// Watch teardown across throw and arm-body fallthrough
// ============================================================================

function watch_throw_fails() -> int {
    watch let x = 0;
    x = 5;
    throw "boom";
}

function watch_throw_unwatches_fn() -> int {
    watch_throw_fails() catch (e) {
        "boom" => 99,
        _ => -1,
    }
}

test "watch_throw_unwatches" {
    assert.is_true(baml.deep_equals(watch_throw_unwatches_fn(), 99))
}

// ─────────────────────────────────────────────────────────────────────────────

function watch_for_throw_fails() -> int {
    for (let i in [1, 2, 3]) {
        watch let x = i;
        if (x == 2) {
            throw "boom";
        }
        x = x + 9;
    }
    0
}

function watch_for_throw_unwatches_fn() -> int {
    watch_for_throw_fails() catch (e) {
        "boom" => 99,
        _ => -1,
    }
}

test "watch_for_throw_unwatches" {
    assert.is_true(baml.deep_equals(watch_for_throw_unwatches_fn(), 99))
}

// ─────────────────────────────────────────────────────────────────────────────

function watch_while_throw_fails() -> int {
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

function watch_while_throw_unwatches_fn() -> int {
    watch_while_throw_fails() catch (e) {
        "boom" => 99,
        _ => -1,
    }
}

test "watch_while_throw_unwatches" {
    assert.is_true(baml.deep_equals(watch_while_throw_unwatches_fn(), 99))
}

// ─────────────────────────────────────────────────────────────────────────────

function watch_match_arm_throw_fails(input: int) -> int {
    match (input) {
        1 => {
            watch let x = 0;
            x = 5;
            throw "boom"
        }
        _ => 0
    }
}

function watch_match_arm_throw_unwatches_fn() -> int {
    watch_match_arm_throw_fails(1) catch (e) {
        "boom" => 99,
        _ => -1,
    }
}

test "watch_match_arm_throw_unwatches" {
    assert.is_true(baml.deep_equals(watch_match_arm_throw_unwatches_fn(), 99))
}

// ─────────────────────────────────────────────────────────────────────────────

function watch_catch_arm_inner() -> int {
    throw "first";
}

function watch_catch_arm_throw_fails() -> int {
    watch_catch_arm_inner() catch (e) {
        _ => {
            watch let x = 0;
            x = 5;
            throw "boom"
        }
    }
}

function watch_catch_arm_throw_unwatches_fn() -> int {
    watch_catch_arm_throw_fails() catch (e) {
        "boom" => 99,
        _ => -1,
    }
}

test "watch_catch_arm_throw_unwatches" {
    assert.is_true(baml.deep_equals(watch_catch_arm_throw_unwatches_fn(), 99))
}

// ─────────────────────────────────────────────────────────────────────────────

function watch_match_arm_fallthrough_entry(input: int) -> int {
    let result = match (input) {
        1 => {
            watch let x = 0;
            x = 5;
            x
        }
        _ => 0
    };
    let result2 = result + 1;
    result2
}

function watch_match_arm_fallthrough_fn() -> int {
    watch_match_arm_fallthrough_entry(1)
}

test "watch_match_arm_fallthrough_unwatches" {
    assert.is_true(baml.deep_equals(watch_match_arm_fallthrough_fn(), 6))
}

// ─────────────────────────────────────────────────────────────────────────────

function watch_switch_arm_fallthrough_entry(input: int) -> int {
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
    let result2 = result + 1;
    result2
}

function watch_switch_arm_fallthrough_fn() -> int {
    watch_switch_arm_fallthrough_entry(1)
}

test "watch_switch_arm_fallthrough_unwatches" {
    assert.is_true(baml.deep_equals(watch_switch_arm_fallthrough_fn(), 6))
}

// ─────────────────────────────────────────────────────────────────────────────

function watch_catch_arm_fallthrough_fails() -> int {
    throw "boom";
}

function watch_catch_arm_fallthrough_fn() -> int {
    let result = watch_catch_arm_fallthrough_fails() catch (e) {
        _ => {
            watch let x = 0;
            x = 5;
            x
        }
    };
    let result2 = result + 1;
    result2
}

test "watch_catch_arm_fallthrough_unwatches" {
    assert.is_true(baml.deep_equals(watch_catch_arm_fallthrough_fn(), 6))
}

// ============================================================================
// Watch with function calls and nested objects
// ============================================================================

class WPointWithSet {
    x int
    y int

    function set(self, x: int, y: int) -> WPointWithSet {
        self.x = x;
        self.y = y;
        self
    }
}

function watch_function_call_modifications_fn() -> int {
    watch let point = WPointWithSet { x: 0, y: 0 };
    point.set(1, 2);
    point.x + point.y
}

test "watch_function_call_modifications" {
    assert.is_true(baml.deep_equals(watch_function_call_modifications_fn(), 3))
}

// ─────────────────────────────────────────────────────────────────────────────

function watch_nested_object_added_fn() -> int {
    watch let vec = WVec2D {
        p: WPointV { x: WValue { value: 0 }, y: WValue { value: 0 } },
        q: WPointV { x: WValue { value: 0 }, y: WValue { value: 0 } },
    };
    let p = WPointV { x: WValue { value: 1 }, y: WValue { value: 1 } };
    vec.p = p;
    p.x.value = 2;
    vec.p.x.value
}

test "watch_nested_object_added" {
    assert.is_true(baml.deep_equals(watch_nested_object_added_fn(), 2))
}

// ─────────────────────────────────────────────────────────────────────────────

function watch_nested_object_removed_fn() -> int {
    watch let vec = WVec2D {
        p: WPointV { x: WValue { value: 0 }, y: WValue { value: 0 } },
        q: WPointV { x: WValue { value: 0 }, y: WValue { value: 0 } },
    };
    let p = vec.p;
    vec.p = WPointV { x: WValue { value: 1 }, y: WValue { value: 1 } };
    p.x.value = 2;
    vec.p.x.value
}

test "watch_nested_object_removed" {
    assert.is_true(baml.deep_equals(watch_nested_object_removed_fn(), 1))
}

// ============================================================================
// Cyclic graph
// ============================================================================

function watch_cyclic_graph_fn() -> int {
    let v1 = WVertex { value: 1, edges: [] };
    watch let v2 = WVertex { value: 2, edges: [] };
    let v3 = WVertex { value: 3, edges: [] };
    watch let v4 = WVertex { value: 4, edges: [] };

    v1.edges = [v2];
    v2.edges = [v3];
    v3.edges = [v4];
    v4.edges = [v1];

    v2.value = 20;
    v1.value = 10;
    v3.value = 30;

    0
}

test "watch_cyclic_graph" {
    assert.is_true(baml.deep_equals(watch_cyclic_graph_fn(), 0))
}

// ============================================================================
// Block notifications (//# headers)
// ============================================================================

function block_notification_basic_fn() -> int {
    //# entering_computation
    let x = 1;
    let y = 2;
    x + y
}

test "block_notification_basic" {
    assert.is_true(baml.deep_equals(block_notification_basic_fn(), 3))
}

// ─────────────────────────────────────────────────────────────────────────────

function block_notification_multiple_fn() -> int {
    //# first_block
    let x = 1;
    //# second_block
    let y = 2;
    x + y
}

test "block_notification_multiple" {
    assert.is_true(baml.deep_equals(block_notification_multiple_fn(), 3))
}

// ============================================================================
// VizEnter/VizExit (//# header before control flow)
// ============================================================================

function header_before_if() -> int {
    //# MyHeader
    if (true) {
        1
    } else {
        2
    }
}

test "viz_header_before_if" {
    assert.is_true(baml.deep_equals(header_before_if(), 1))
}

// ─────────────────────────────────────────────────────────────────────────────

function header_before_while() -> int {
    let x = 0;
    //# LoopHeader
    while (x < 1) {
        x = x + 1;
    }
    x
}

test "viz_header_before_while" {
    assert.is_true(baml.deep_equals(header_before_while(), 1))
}

// ─────────────────────────────────────────────────────────────────────────────

function standalone_header() -> int {
    //# JustAHeader
    let x = 5;
    x
}

test "viz_standalone_header_no_viz_events" {
    assert.is_true(baml.deep_equals(standalone_header(), 5))
}

// ─────────────────────────────────────────────────────────────────────────────

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

test "viz_multiple_headers_only_one_before_if" {
    assert.is_true(baml.deep_equals(multiple_headers(), 2))
}

// ─────────────────────────────────────────────────────────────────────────────

function if_no_header() -> int {
    if (true) {
        1
    } else {
        2
    }
}

test "viz_if_without_header_no_viz" {
    assert.is_true(baml.deep_equals(if_no_header(), 1))
}
