// Tests for spawn/await semantic invariants beyond the basic round-trip.
//
// Converted from crates/baml_tests/tests/spawn_semantics.rs.
//
// `parent_throw_cancels_running_children` is NOT converted: it asserts on
// wall-clock timing (elapsed < 5s) to verify cascade-cancel fires promptly.
// A test block can only assert a deterministic value, not scheduling/timing.
// It remains in the Rust file.

function compute() -> int { 42 }

function multiple_awaits_on_same_future_return_cached_value_fn() -> int {
    let f = spawn { compute() };
    let a = await f;
    let b = await f;
    let c = await f;
    a + b + c
}

test "multiple_awaits_on_same_future_return_cached_value" {
    assert.equal(multiple_awaits_on_same_future_return_cached_value_fn(), 126)
}

function leaf() -> int { 1 }

function mid() -> int {
    let l = spawn { leaf() };
    (await l) + 10
}

function top() -> int {
    let m = spawn { mid() };
    (await m) + 100
}

function three_level_nested_spawn_sum_fn() -> int {
    let t = spawn { top() };
    await t
}

test "three_level_nested_spawn_sum" {
    assert.equal(three_level_nested_spawn_sum_fn(), 111)
}

function spawn_child() -> baml.future.Future<int, never> {
    spawn { 99 }
}

function function_can_return_future_typed_value_fn() -> int {
    let f = spawn_child();
    await f
}

test "function_can_return_future_typed_value" {
    assert.equal(function_can_return_future_typed_value_fn(), 99)
}
