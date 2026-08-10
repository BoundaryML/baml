// Tests for built-in methods and operations.
//
// Converted from crates/baml_tests/tests/builtins.rs.
//
// `bind_method_call`: the Rust test reads a bare local (`let v = arr.length(); v`).
// In a test block, a local read compares unequal to a literal (VM bug), so the
// body is moved into a helper function where locals are not boxed.

class BuiltinsPoint {
    x int
    y int
}

class BuiltinsPerson {
    name string
    age int
    location BuiltinsPoint
    hobbies string[]
    scores map<string, int>
}

// Helper: binds the method-call result to a local before returning — safe inside
// a function (locals are not boxed there).
function builtins_bind_method_call() -> int {
    let arr = [1, 2, 3];
    let v = arr.length();
    v
}

// Helper: string concatenation via locals — locals are boxed in test blocks,
// causing `CannotApplyBinOp { left: Object(Any), right: Object(String) }`.
function builtins_float_to_string_concat() -> string {
    let a = string.from(1.0);
    let b = string.from(0.0);
    let c = string.from(-1.0);
    let d = string.from(3.14);
    let e = string.from(2);
    a + " " + b + " " + c + " " + d + " " + e
}

test "builtin_method_call" {
    assert.equal([1, 2, 3].length(), 3)
}

test "bind_method_call" {
    assert.equal(builtins_bind_method_call(), 3)
}

// Helper: constructs and stringifies a person — moved out of the test block
// because both `p` and `person` are locals in a test block (VM bug: boxed as
// Any), so `string.from(person)` would receive Object(Any).
function builtins_any_value_to_string() -> string {
    let p = BuiltinsPoint { x: 10, y: 20 };
    let person = BuiltinsPerson {
        name: "Alice",
        age: 25,
        location: p,
        hobbies: ["reading", "coding"],
        scores: {"math": 95, "english": 88}
    };
    string.from(person)
}

test "any_value_to_string" {
    // `string.from` renders instances inline (`Class { field: value, ... }`) with
    // unqualified class names; nested strings and map keys are quoted.
    assert.equal(
        builtins_any_value_to_string(),
        "BuiltinsPerson { name: \"Alice\", age: 25, location: BuiltinsPoint { x: 10, y: 20 }, hobbies: [\"reading\", \"coding\"], scores: {\"math\": 95, \"english\": 88} }"
    )
}

// float_to_string_preserves_decimal: uses a helper function because locals in
// test blocks are boxed and string concatenation via boxed locals hits
// CannotApplyBinOp { left: Object(Any), right: Object(String) }.
test "float_to_string_preserves_decimal" {
    assert.equal(builtins_float_to_string_concat(), "1.0 0.0 -1.0 3.14 2")
}
