// Tests for basic `spawn { body }` + `await` end-to-end wiring.
//
// Converted from crates/baml_tests/tests/spawn_basic.rs. Each Rust test becomes
// a top-level `function` (to avoid the test-block local-boxing VM bug) plus a
// `test` block that calls it with `assert.equal(...)`.

function spawn_returning_int_literal() -> int {
    let f = spawn { 42 };
    await f
}

test "spawn_returning_int_literal" {
    assert.equal(spawn_returning_int_literal(), 42)
}

function spawn_returning_string() -> string {
    let f = spawn { "hello from spawn" };
    await f
}

test "spawn_returning_string" {
    assert.equal(spawn_returning_string(), "hello from spawn")
}

function spawn_with_name_literal() -> int {
    let f = spawn "answer" { 42 };
    await f
}

test "spawn_with_name_literal" {
    assert.equal(spawn_with_name_literal(), 42)
}
