// Regression tests: spawn name expression must not consume the body's
// brace as an object-literal constructor.
//
// Converted from crates/baml_tests/tests/spawn_name_object.rs. The spawn
// name is a variable expression; the body contains a struct literal or
// map literal whose opening brace would have been mis-consumed by the
// broken parser.

class B { x int }

function spawn_name_then_struct_body_parses_correctly_fn() -> int {
    let nm = "task";
    let f = spawn nm { B { x: 7 } };
    (await f).x
}

test "spawn_name_then_struct_body_parses_correctly" {
    assert.equal(spawn_name_then_struct_body_parses_correctly_fn(), 7)
}

function spawn_name_then_map_literal_body_fn() -> int {
    let nm = "task";
    let f = spawn nm { { "y": 1 }["y"] };
    await f
}

test "spawn_name_then_map_literal_body" {
    assert.equal(spawn_name_then_map_literal_body_fn(), 1)
}
