// `assert.is_type<T>(value)` checks a type. It returns nothing and it narrows
// nothing: `is` is the one way to narrow a value.
//
// The compile errors that go with this file are in the diagnostics project
// `diagnostic_errors/assert_is_type_checks_only`: a read of the result
// (`assert.is_type<A>(x).a`, `let y = assert.is_type<A>(x)`), and a read of
// `x.a` after the call.
//
// Every case body is a top-level function; a `test` block only asserts.

class AitA {
    a: int,
}

class AitB {
    b: string,
}

function ait_pick(first: bool) -> AitA | AitB {
    if (first) {
        AitA { a: 7 }
    } else {
        AitB { b: "bee" }
    }
}

function ait_statement_passes() -> string {
    let x = ait_pick(true);
    assert.is_type<AitA>(x);
    "passed"
}

function ait_statement_panics() -> string {
    let x = ait_pick(false);
    assert.is_type<AitA>(x);
    "passed"
}

function ait_panic_message() -> string {
    ait_statement_panics() catch (e) {
        let failed: baml.panics.AssertionFailed => failed.message,
    }
}

function ait_checks_a_union_member() -> string {
    let x: int | string | null = "text";
    assert.is_type<string>(x);
    assert.is_type<int | string>(x);
    "passed"
}

function ait_checks_null_against_an_optional() -> string {
    let x: int? = null;
    assert.is_type<int?>(x);
    assert.is_type<int>(x) catch (e) {
        baml.panics.AssertionFailed => {
            return "null is not an int";
        },
    };
    "passed"
}

// The value is used as an `AitA` through `is`, not through the assertion.
function ait_narrow_with_is() -> int {
    let x = ait_pick(true);
    assert.is_type<AitA>(x);
    if (x is AitA) {
        x.a
    } else {
        -1
    }
}

// A void result is accepted where `unknown` is expected, and it reads as
// `null` there. The assertion does not hand the checked value on.
function ait_result_as_unknown() -> string {
    let result: unknown = assert.is_type<string>("text");
    `${result}`
}

test "is_type as a statement passes for a value of the type" {
    assert.equal(ait_statement_passes(), "passed");
    assert.equal(ait_checks_a_union_member(), "passed");
}

test "is_type panics with AssertionFailed for a value of another type" {
    assert.is_true(ait_panic_message().starts_with("assertion failed: expected type"));
    assert.equal(ait_checks_null_against_an_optional(), "null is not an int");
}

test "a value that is_type checked is narrowed with is" {
    assert.equal(ait_narrow_with_is(), 7);
}

test "the result of is_type is not the checked value" {
    assert.equal(ait_result_as_unknown(), "null");
}
