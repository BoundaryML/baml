// `assert.is_type<T>(value)` checks a type and returns nothing. There is one
// way to narrow a value, and it is `is`.

class A {
    a: int,
}

class B {
    b: string,
}

// The call has type `void`: its result has no member to read.
function read_member_of_the_result(x: A | B) -> int {
    assert.is_type<A>(x).a
}

// The call has type `void`: a `let` cannot take its result.
function bind_the_result(x: A | B) -> int {
    let y = assert.is_type<A>(x);
    1
}

// The call does not narrow its argument: `x` is `A | B` after it.
function read_member_after_the_call(x: A | B) -> int {
    assert.is_type<A>(x);
    x.a
}

// The same for a local.
function read_member_of_a_local_after_the_call(first: bool) -> int {
    let x: A | B = if (first) {
        A { a: 1 }
    } else {
        B { b: "bee" }
    };
    assert.is_type<A>(x);
    x.a
}

// No error: `is` narrows.
function narrow_with_is(x: A | B) -> int {
    assert.is_type<A>(x);
    if (x is A) {
        x.a
    } else {
        0
    }
}

// No error today: a void result is accepted where `unknown` is expected. It
// reads as `null` there, so this assertion fails at run time.
function result_where_unknown_is_expected(x: string | int) -> void {
    assert.equal(assert.is_type<string>(x), "text");
}
