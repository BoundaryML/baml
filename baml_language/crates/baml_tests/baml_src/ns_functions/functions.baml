// Unified tests for function calls, parameters, and returns.
//
// Converted from crates/baml_tests/tests/functions.rs. Each Rust test becomes a
// `test` block. Tests using `baml_test!{ entry: "fn", args: {...} }` form keep
// the function as a top-level definition and call it.
//
// Scalar/primitive return values and instance results both use `assert.equal`.

function literal_int() -> int { 42 }
function literal_bool() -> bool { true }
function literal_string() -> string { "hello" }

test "return_literal_int" {
    assert.equal(literal_int(), 42)
}

test "return_literal_bool" {
    assert.is_true(literal_bool())
}

test "return_literal_string" {
    assert.equal(literal_string(), "hello")
}

function one() -> int {
    1
}

test "return_function_call" {
    assert.equal(one(), 1)
}

function two() -> int {
    2
}

function two_plus_one() -> int {
    let a = two();
    a + 1
}

test "call_function_assign_to_variable" {
    assert.equal(two_plus_one(), 3)
}

function mutable_variables_fn() -> int {
    let y = 3;
    y = 5;
    y
}

test "mutable_variables" {
    assert.equal(mutable_variables_fn(), 5)
}

function one_of(a: int, b: int) -> int {
    a
}

function call_with_arguments_fn() -> int {
    let v = one_of(1, 2);
    v
}

test "call_with_arguments" {
    assert.equal(call_with_arguments_fn(), 1)
}

function get_greeting() -> string {
    "Hello"
}

function unused_variable_fn() -> string {
    let greeting = get_greeting();
    let name = "World";
    greeting
}

test "unused_variable_does_not_affect_result" {
    assert.equal(unused_variable_fn(), "Hello")
}

function early_return(x: int) -> int {
    if (x == 42) { return 1; }
    x + 5
}

test "early_return" {
    assert.equal(early_return(42), 1)
}

function early_return_nested() -> int {
    let a = 1;

    if (a == 0) { return 0; }

    {
        let b = 1;
        if (a != b) {
            return 0;
        }
    }

    {
        let c = 2;
        let b = 3;
        while (b != c) {
            if (true) {
                return 0;
            }
        }
    }

    7
}

test "early_return_from_nested_scopes" {
    assert.equal(early_return_nested(), 0)
}

function fib(n: int) -> int {
    if (n <= 1) {
        n
    } else {
        fib(n - 1) + fib(n - 2)
    }
}

function fib_main() -> int {
    fib(3)
}

test "recursion" {
    assert.equal(fib_main(), 2)
}

function add(a: int, b: int) -> int {
    a + b
}

function call_twice(f: (int, int) -> int, x: int, y: int) -> int {
    f(x, y) + f(x, y)
}

function function_as_value_main() -> int {
    let f = add;
    call_twice(f, 20, 1)
}

test "function_as_value" {
    assert.equal(function_as_value_main(), 42)
}
