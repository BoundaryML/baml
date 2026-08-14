// Tests for `const` bindings executing identically to `let` bindings.
//
// Entry functions have unique prefixes: all baml_src files compile to the same
// `user` package. Locals and compound assignments are in helper functions to
// work around the test-block local-boxing VM bug.

function const_bindings_with_let() -> int {
    let sum = 0;
    for (let i = 0; i < 4; i += 1) {
        sum += i;
    }
    sum
}

function const_bindings_with_const() -> int {
    const sum = 0;
    for (const i = 0; i < 4; i += 1) {
        sum += i;
    }
    sum
}

test "let_and_const_bindings_execute_the_same" {
    assert.equal(const_bindings_with_let(), 6);
    assert.equal(const_bindings_with_const(), 6)
}
