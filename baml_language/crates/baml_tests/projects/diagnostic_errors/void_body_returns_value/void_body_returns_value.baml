// A `-> void` function or lambda must not evaluate to a value (E0176). The
// value used to be silently discarded (and still reached the caller at
// runtime), so `function f() -> void { 5 }` compiled, and a `test` body, which
// is a `() -> void` lambda, passed when it evaluated to `false`.

function int_tail() -> void {
    5
}

function bool_tail(a: int, b: int) -> void {
    a == b
}

function branch_values(c: bool) -> void {
    if (c) {
        1
    } else {
        2
    }
}

// `push` returns the new length, so it must be discarded explicitly.
function push_tail(xs: int[]) -> void {
    xs.push(1)
}

function lambda_tail() -> void {
    let check = () -> void {
        1 == 2
    };
    check();
}

test "bare comparison" {
    1 == 2
}

testset "grouped" {
    test "bare literal in a testset" {
        false
    }

    // OK: an assertion is `void`.
    test "assertion" {
        assert.is_true(true)
    }
}

// OK: `;` discards the value.
function discarded(xs: int[]) -> void {
    xs.push(1);
}

// OK: a `void` call as the tail.
function void_call(xs: int[]) -> void {
    discarded(xs)
}

// OK: `null` is the unit value.
function null_tail() -> void {
    null
}

// OK: an `if` without `else` whose branch is `void`.
function if_without_else(c: bool, xs: int[]) -> void {
    if (c) {
        discarded(xs)
    }
}

// OK: no tail expression.
function empty() -> void {}

// OK: a statement-position expression keeps discarding its value.
function statement_position(xs: int[]) -> void {
    for (let x in [1, 2]) {
        xs.push(x)
    }
}
