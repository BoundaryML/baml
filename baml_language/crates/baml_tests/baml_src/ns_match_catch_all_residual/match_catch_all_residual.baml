// A bare binding in a catch-all `match` arm binds what the earlier arms left.
//
// The scrutinee's own binding already read as the residual in such an arm
// (B-774). The arm's own binding did not: it kept the full scrutinee type, so
// `null => .., let hook => hook(n)` was E0006 on an optional function type,
// and `let rest => rest.b` was E0007 after arms for `null` and `A`. A function
// type made the gap visible, because no shorter arm pattern exists for it.
//
// Every case body is a top-level function; a `test` block only asserts.

class McrA {
    a: int,
}

class McrB {
    b: string,
}

class McrHolder {
    hook: ((int) -> string throws never)?,
    item: (McrA | McrB)?,

    function call(self, n: int) -> string {
        match (self.hook) {
            null => "none",
            let hook => hook(n),
        }
    }
}

function mcr_decimal(n: int) -> string throws never {
    n.to_string()
}

function mcr_call_with_hook() -> string {
    let holder = McrHolder { hook: mcr_decimal, item: null };
    holder.call(7)
}

function mcr_call_without_hook() -> string {
    let holder = McrHolder { hook: null, item: null };
    holder.call(7)
}

function mcr_parameter(item: McrA | McrB | null) -> string {
    match (item) {
        null => "none",
        let a: McrA => "a",
        let rest => rest.b,
    }
}

function mcr_local(n: int) -> int {
    let item: McrA? = if (n > 0) {
        McrA { a: n }
    } else {
        null
    };
    match (item) {
        null => 0,
        let found => found.a,
    }
}

function mcr_field_path(holder: McrHolder) -> string {
    match (holder.item) {
        null => "none",
        let a: McrA => "a",
        let rest => rest.b,
    }
}

function mcr_function_parameter(hook: ((int) -> string throws never)?, n: int) -> string {
    match (hook) {
        null => "none",
        let known => known(n),
    }
}

function mcr_string(text: string?) -> int {
    match (text) {
        null => -1,
        let known => known.length(),
    }
}

// A guarded arm subtracts nothing: its guard can send the value on, so the
// catch-all binding still holds every member.
function mcr_after_guard(item: McrA | McrB) -> int {
    match (item) {
        let a: McrA if a.a > 0 => 1,
        let rest => match (rest) {
            let a: McrA => 2,
            let b: McrB => 3,
        },
    }
}

// Every binding of a chain names the same value and has the same type.
function mcr_chain(item: McrA?) -> int {
    match (item) {
        null => 0,
        let outer: let inner => outer.a + inner.a,
    }
}

// The binding is a new local of the residual type: it can be assigned a
// value of that type, and the scrutinee is not changed.
function mcr_binding_is_a_local(item: McrA?) -> int {
    match (item) {
        null => 0,
        let found => {
            found = McrA { a: found.a + 1 };
            found.a
        },
    }
}

test "a catch-all binding after a null arm calls an optional function field" {
    assert.equal(mcr_call_with_hook(), "7");
    assert.equal(mcr_call_without_hook(), "none");
}

test "a catch-all binding of a parameter has the members that the earlier arms left" {
    assert.equal(mcr_parameter(McrB { b: "bee" }), "bee");
    assert.equal(mcr_parameter(McrA { a: 1 }), "a");
    assert.equal(mcr_parameter(null), "none");
}

test "a catch-all binding of a local is not null after a null arm" {
    assert.equal(mcr_local(4), 4);
    assert.equal(mcr_local(0), 0);
}

test "a catch-all binding of a field path has the members that the earlier arms left" {
    assert.equal(mcr_field_path(McrHolder { hook: null, item: McrB { b: "bee" } }), "bee");
    assert.equal(mcr_field_path(McrHolder { hook: null, item: McrA { a: 1 } }), "a");
    assert.equal(mcr_field_path(McrHolder { hook: null, item: null }), "none");
}

test "a catch-all binding after a null arm calls an optional function parameter" {
    assert.equal(mcr_function_parameter(mcr_decimal, 12), "12");
    assert.equal(mcr_function_parameter(null, 12), "none");
}

test "a catch-all binding of an optional string is a string" {
    assert.equal(mcr_string("four"), 4);
    assert.equal(mcr_string(null), -1);
}

test "a guarded arm leaves its members for the catch-all binding" {
    assert.equal(mcr_after_guard(McrA { a: 5 }), 1);
    assert.equal(mcr_after_guard(McrA { a: 0 }), 2);
    assert.equal(mcr_after_guard(McrB { b: "bee" }), 3);
}

test "every binding of a catch-all chain has the residual type" {
    assert.equal(mcr_chain(McrA { a: 21 }), 42);
    assert.equal(mcr_chain(null), 0);
}

test "a catch-all binding is a local of the residual type" {
    assert.equal(mcr_binding_is_a_local(McrA { a: 1 }), 2);
    assert.equal(mcr_binding_is_a_local(null), 0);
}
