// Tests for function input argument handling.
//
// Converted from crates/baml_tests/tests/typed_inputs.rs.
//
// All Rust tests inject host args via BexExternalValue. Here we call the same
// functions with BAML literals in test blocks instead.

function double(x: int) -> int {
    x * 2
}

test "int_input" {
    assert.equal(double(21), 42)
}

function greet(name: string) -> string {
    "Hello, " + name + "!"
}

test "string_input" {
    assert.equal(greet("World"), "Hello, World!")
}

function add(a: int, b: int) -> int {
    a + b
}

test "multiple_inputs" {
    assert.equal(add(10, 32), 42)
}

function negate(b: bool) -> bool {
    !b
}

test "bool_input" {
    assert.is_true(!negate(true))
}

function half(x: float) -> float {
    x / 2.0
}

test "float_input" {
    assert.equal(half(10.0), 5.0)
}

// array_input: sum uses a for-loop accumulator local; moved to a helper so
// locals are not boxed.
function sum(arr: int[]) -> int {
    let total = 0;
    for (let x in arr) {
        total += x;
    }
    total
}

test "array_input" {
    assert.equal(sum([1, 2, 3, 4]), 10)
}

class Point {
    x int
    y int
}

function magnitude_squared(p: Point) -> int {
    p.x * p.x + p.y * p.y
}

test "class_input" {
    assert.equal(magnitude_squared(Point { x: 3, y: 4 }), 25)
}

// map_input: call get_value with a literal map and a literal key.

function get_value(m: map<string, int>, key: string) -> int {
    m[key]
}

test "map_input" {
    assert.equal(get_value({"foo": 42, "bar": 100}, "foo"), 42)
}

function is_null(x: int?) -> bool {
    x == null
}

test "null_input" {
    assert.is_true(is_null(null))
}

function concat(a: string, b: string, c: string) -> string {
    a + " " + b + " " + c
}

test "mixed_types_inputs" {
    assert.equal(concat("Hello", "from", "BAML"), "Hello from BAML")
}
