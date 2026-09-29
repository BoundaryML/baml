// A `test` body must be `void` (E0176). Its value used to be discarded, so a
// test written as a bare condition, like `test "x" { 1 == 2 }`, passed
// without checking anything.

function is_even(n: int) -> bool {
    n % 2 == 0
}

function answer() -> int {
    42
}

function log_it() -> void {
    1 == 2
}

test "bare comparison" {
    1 == 2
}

test "call returning bool" {
    is_even(3)
}

test "call returning a non-bool value" {
    answer()
}

test "statements then a bool tail" {
    let xs = [1, 2, 3];
    xs.length() == 4
}

testset "grouped" {
    test "bare literal in a testset" {
        false
    }

    // OK: an assertion is `void`.
    test "assertion" {
        assert.is_true(is_even(4))
    }
}

// OK: `;` discards the value.
test "discarded value" {
    answer();
}

// OK: a `void` call as the tail.
test "void call" {
    log_it()
}

// OK: no tail expression.
test "statements only" {
    let x = answer();
}
