class InherentEquality {
    value int

    function eq(self, other: Self) -> bool throws never {
        false
    }

    function neq(self, other: Self) -> bool throws never {
        false
    }
}

class EqualityHolder {
    item InherentEquality
}

function make_equality(calls: int[]) -> InherentEquality {
    calls[0] += 1;
    InherentEquality { value: 1 }
}

function optional_equality(value: InherentEquality?) -> bool? throws never {
    value?.eq(InherentEquality { value: 1 })
}

test "inherent methods win across call spellings" {
    let value = InherentEquality { value: 1 };
    let other = InherentEquality { value: 2 };
    let holder = EqualityHolder { item: value };
    assert.equal(value.eq(value), false)
    assert.equal((value).eq(value), false)
    assert.equal(holder.item.eq(value), false)
    assert.equal((holder.item).eq(value), false)
    assert.equal(value.neq(other), false)
    assert.equal((value).neq(other), false)
    assert.equal(InherentEquality.eq(value, value), false)
    assert.equal(value == value, true)
    assert.equal(value != other, true)
}

test "inherent method references retain their selected body" {
    let value = InherentEquality { value: 1 };
    let path_method = value.eq;
    let expression_method = (value).eq;
    let unbound_method = InherentEquality.eq;
    assert.equal(path_method(value), false)
    assert.equal(expression_method(value), false)
    assert.equal(unbound_method(value, value), false)
}

test "inherent calls evaluate receivers once and preserve optional chains" {
    let calls = [0];
    assert.equal(make_equality(calls).eq(InherentEquality { value: 1 }), false)
    assert.equal(calls[0], 1)
    assert.equal(optional_equality(InherentEquality { value: 1 }), false)
    assert.equal(optional_equality(null), null)
}

class StructuralNameFields {
    eq int
    neq string
    to_string string
    to_json int
    from_json bool
}

function optional_field(value: StructuralNameFields?) -> int? throws never {
    value?.eq
}

test "fields win over implicit structural methods" {
    let value = StructuralNameFields {
        eq: 7,
        neq: "field",
        to_string: "text",
        to_json: 9,
        from_json: true,
    };
    assert.equal(value.eq, 7)
    assert.equal((value).eq, 7)
    assert.equal((value).neq, "field")
    assert.equal((value).to_string, "text")
    assert.equal((value).to_json, 9)
    assert.equal((value).from_json, true)
    assert.equal(optional_field(value), 7)
    assert.equal(optional_field(null), null)
}

interface StructuralCallbackView {
    eq (other: int) -> string throws never
    to_json () -> int throws never
}

class StructuralCallbackFields {
    eq (other: int) -> string throws never
    to_json () -> int throws never

    implements StructuralCallbackView {}
}

function call_callback_view(value: StructuralCallbackView) -> string throws never {
    let callback = (value).eq;
    assert.equal(callback(7), "callback")
    assert.equal(value.to_json(), 42)
    value.eq(7)
}

test "function fields retain their values and call signatures" {
    let value = StructuralCallbackFields {
        eq: (other: int) -> string throws never { "callback" },
        to_json: () -> int throws never { 42 },
    };
    let path_method = value.eq;
    let expression_method = (value).eq;
    let json_method = (value).to_json;
    assert.equal(path_method(7), "callback")
    assert.equal(expression_method(7), "callback")
    assert.equal(json_method(), 42)
    assert.equal(value.eq(7), "callback")
    assert.equal((value).eq(7), "callback")
    assert.equal(value.to_json(), 42)
    assert.equal(call_callback_view(value), "callback")
}
