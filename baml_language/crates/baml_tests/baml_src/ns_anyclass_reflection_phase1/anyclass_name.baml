// `reflect.AnyClass.name()` reports the class declaration's name without
// type arguments; `type().to_string()` keeps them.

class NamedBox<V> {
    v: V,
}

class NamedPair<A, B> {
    a: A,
    b: B,
}

class NamedPlain {
    x: int,
}

function anyclass_name_of(value: unknown) -> string {
    match (value) {
        let c: reflect.AnyClass => c.name(),
        _ => "?",
    }
}

function anyclass_type_of(value: unknown) -> string {
    match (value) {
        let c: reflect.AnyClass => c.type().to_string(),
        _ => "?",
    }
}

test "anyclass_name_omits_type_arguments" {
    assert.equal(anyclass_name_of(NamedBox { v: 1 }), "anyclass_reflection_phase1.NamedBox");
    assert.equal(
        anyclass_name_of(NamedBox<string> { v: "a" }),
        "anyclass_reflection_phase1.NamedBox",
    );
    assert.equal(
        anyclass_name_of(NamedPair<int, NamedBox<int>> { a: 1, b: NamedBox { v: 2 } }),
        "anyclass_reflection_phase1.NamedPair",
    );
    assert.equal(anyclass_name_of(NamedPlain { x: 1 }), "anyclass_reflection_phase1.NamedPlain");
}

test "anyclass_type_keeps_type_arguments" {
    assert.equal(
        anyclass_type_of(NamedBox<string> { v: "a" }),
        "anyclass_reflection_phase1.NamedBox<string>",
    );
    assert.equal(anyclass_type_of(NamedPlain { x: 1 }), "anyclass_reflection_phase1.NamedPlain");
}
