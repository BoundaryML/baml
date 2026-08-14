// Runtime narrowing of the `type` metatype — `is`, `if let`, and `match`,
// standalone, inside unions, optionals, and containers.
//
// Regression coverage for the emitter bug where the `type` leaf had no arm in
// the `is`-lowering dispatch and fell into the tagless-leaf fallback of
// `is_type`, compiling every `type` test to constant-FALSE: `v is type` was
// false for an actual `reflect.type_of<...>()` value, and in a `match` over
// `type | int` no `type` arm ever fired (the last arm swallowed the value, so
// a `type` came out through the `int` arm). The `type` leaf now reports its
// coarse type tag (`typetag::TYPE`) — the same tag the MIR switch path
// already assigned it, so the `is`-chain and the tag-switch agree.
//
// The `is` operator inspects a value's runtime type, so — as in
// `ns_is_operator` — every check lives inside a function rather than directly
// in a test block.

// ── Group A: `is type` on a bare `type` value ─────────────────────────────────

function type_is_type() -> string {
    let v = reflect.type_of<int>()
    `${v is type}|${v is int}|${v is string}`
}

test "type_is_type_positive_and_negatives" {
    assert.equal(type_is_type(), "true|false|false")
}

// A non-`type` value is not a `type`.
function non_type_is_not_type() -> string {
    let i = 7
    let s = "int"
    let xs = [1, 2]
    `${i is type}|${s is type}|${xs is type}`
}

test "non_type_values_are_not_type" {
    assert.equal(non_type_is_not_type(), "false|false|false")
}

// A `type` value stays a `type` whichever type it reflects.
function type_of_various_is_type() -> string {
    let a = reflect.type_of<string>()
    let b = reflect.type_of<int[]>()
    let c = reflect.type_of<map<string, int>>()
    `${a is type}|${b is type}|${c is type}`
}

test "type_of_any_argument_is_type" {
    assert.equal(type_of_various_is_type(), "true|true|true")
}

// ── Group B: `type | int` union, both directions ──────────────────────────────

function union_holding_type_is() -> string {
    let v: type | int = reflect.type_of<int>()
    `${v is type}|${v is int}`
}

test "union_holding_type_narrows_to_type" {
    assert.equal(union_holding_type_is(), "true|false")
}

function union_holding_int_is() -> string {
    let v: type | int = 42
    `${v is type}|${v is int}`
}

test "union_holding_int_narrows_to_int" {
    assert.equal(union_holding_int_is(), "false|true")
}

// `if let` over the union, both inhabitants.
function union_if_let_type() -> string {
    let v: type | int = reflect.type_of<bool>()
    if let t: type = v {
        `type:${t.to_string()}`
    } else {
        "not-a-type"
    }
}

test "union_if_let_binds_type_arm" {
    assert.equal(union_if_let_type(), "type:bool")
}

function union_if_let_int() -> string {
    let v: type | int = 9
    if let t: type = v {
        `type:${t.to_string()}`
    } else {
        "not-a-type"
    }
}

test "union_if_let_skips_type_arm_for_int" {
    assert.equal(union_if_let_int(), "not-a-type")
}

// `match` over the union — `type` arm first.
function union_match_type_first(v: type | int) -> string {
    match (v) {
        let t: type => `type:${t.to_string()}`,
        let i: int => `int:${i}`,
    }
}

test "union_match_type_arm_fires_for_type" {
    assert.equal(union_match_type_first(reflect.type_of<string>()), "type:string")
}

test "union_match_int_arm_fires_for_int" {
    assert.equal(union_match_type_first(5), "int:5")
}

// Same `match` with the arms reversed — the `type` arm is now the exhaustive
// final arm, so its test is elided; the `int` arm carries the refutable test.
function union_match_int_first(v: type | int) -> string {
    match (v) {
        let i: int => `int:${i}`,
        let t: type => `type:${t.to_string()}`,
    }
}

test "reversed_match_type_arm_fires_for_type" {
    assert.equal(union_match_int_first(reflect.type_of<int>()), "type:int")
}

test "reversed_match_int_arm_fires_for_int" {
    assert.equal(union_match_int_first(11), "int:11")
}

// A three-member union: `type` must not be swallowed by a sibling leaf.
function union_three_way(v: type | int | string) -> string {
    match (v) {
        let t: type => `type:${t.to_string()}`,
        let s: string => `string:${s}`,
        let i: int => `int:${i}`,
    }
}

test "three_way_union_dispatches_type" {
    assert.equal(union_three_way(reflect.type_of<bool>()), "type:bool")
}

test "three_way_union_dispatches_string" {
    assert.equal(union_three_way("hi"), "string:hi")
}

test "three_way_union_dispatches_int" {
    assert.equal(union_three_way(3), "int:3")
}

// ── Group C: `type?` optionals ────────────────────────────────────────────────

function optional_type_present() -> string {
    let v: type? = reflect.type_of<float>()
    let is_t = v is type
    if let t: type = v {
        `${is_t}|${t.to_string()}`
    } else {
        `${is_t}|none`
    }
}

test "optional_type_present_narrows" {
    assert.equal(optional_type_present(), "true|float")
}

function optional_type_absent() -> string {
    let v: type? = null
    let is_t = v is type
    if let t: type = v {
        `${is_t}|${t.to_string()}`
    } else {
        `${is_t}|none`
    }
}

test "optional_type_absent_does_not_narrow" {
    assert.equal(optional_type_absent(), "false|none")
}

// ── Group D: `(type | string)[]` iteration ────────────────────────────────────

function classify_mixed_list() -> string {
    let items: (type | string)[] = [
        reflect.type_of<int>(),
        "plain",
        reflect.type_of<bool[]>(),
    ]
    let out: string[] = []
    for (let item in items) {
        if (item is type) {
            out.push("T")
        } else {
            out.push("S")
        }
    }
    out.join(",")
}

test "mixed_list_classifies_each_element" {
    assert.equal(classify_mixed_list(), "T,S,T")
}

function render_mixed_list() -> string {
    let items: (type | string)[] = [
        reflect.type_of<int>(),
        "plain",
        reflect.type_of<bool[]>(),
    ]
    let out: string[] = []
    for (let item in items) {
        match (item) {
            let t: type => out.push(t.to_string()),
            let s: string => out.push(s),
        }
    }
    out.join(",")
}

test "mixed_list_match_renders_each_element" {
    assert.equal(render_mixed_list(), "int,plain,bool[]")
}

// ── Group E: `reflect.type_of` equality is unaffected ─────────────────────────

function type_equality() -> string {
    let a = reflect.type_of<int>()
    let b = reflect.type_of<int>()
    let c = reflect.type_of<string>()
    `${a == b}|${a == c}|${a != c}`
}

test "type_values_compare_structurally" {
    assert.equal(type_equality(), "true|false|true")
}

function type_to_string_after_narrowing() -> string {
    let v: type | int = reflect.type_of<map<string, int>>()
    if let t: type = v {
        t.to_string()
    } else {
        "unreachable"
    }
}

test "narrowed_type_value_keeps_its_reflection" {
    assert.equal(type_to_string_after_narrowing(), "map<string, int>")
}
