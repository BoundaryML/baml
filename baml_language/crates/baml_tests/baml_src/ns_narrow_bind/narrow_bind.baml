class Foo {
    field: int,
}

function pick(x: Foo | int) -> int {
    match (x) {
        let foo: Foo => foo.field,
        let n: int => n,
    }
}

function typed_pattern_tests_and_binds_the_same_value_fn() -> int {
    pick(Foo { field: 1 }) + pick(2)
}

test "typed_pattern_tests_and_binds_the_same_value" {
    assert.equal(typed_pattern_tests_and_binds_the_same_value_fn(), 3)
}
