// Tests for spawn body throws → `await` re-throws.
//
// Converted from crates/baml_tests/tests/spawn_throws.rs.
//
// `await_uncaught_bubbles_to_host`: the Rust test asserts on the host-level
// EngineError debug string (class name + message field). In BAML we cannot
// observe the host error object, but we can faithfully verify the substance:
// that `baml.errors.Io { message: "boom" }` propagates through the await
// and is catchable with the correct fields. The catch-arm assertion replaces
// the Rust `format!("{err:?}").contains(...)` checks.
//
// `await_rethrows_caught_by_user_catch`: direct value test → convertible.

function boom_io() -> int throws baml.errors.Io {
    throw baml.errors.Io { message: "boom" }
}

// The Rust test asserts on the host-level EngineError debug string, which we
// cannot observe in BAML. We verify the substance instead: that the error
// propagates through the await and the message field is intact. Because `await f`
// returns `int` on success and the catch arm returns `string`, we catch from
// the test block directly (avoids the mixed-type branch problem).
function await_uncaught_spawns_io_fn() -> int throws baml.errors.Io {
    let f = spawn { boom_io() };
    await f
}

test "await_uncaught_bubbles_to_host" {
    assert.is_true(baml.deep_equals(
        await_uncaught_spawns_io_fn() catch (e) {
            baml.errors.Io => e.message
        },
        "boom"
    ))
}

function boom_io_x() -> int throws baml.errors.Io {
    throw baml.errors.Io { message: "x" }
}

function await_rethrows_caught_by_user_catch_fn() -> int {
    let f = spawn { boom_io_x() };
    (await f) catch (e) { baml.errors.Io => 99 }
}

test "await_rethrows_caught_by_user_catch" {
    assert.is_true(baml.deep_equals(await_rethrows_caught_by_user_catch_fn(), 99))
}
