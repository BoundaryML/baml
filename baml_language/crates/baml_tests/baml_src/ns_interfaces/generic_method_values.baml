interface GmvIdentity {
    function echo<T>(self, value: T) -> T throws never { value }
    function empty<T>(self) -> T[] throws never { [] }
}

implements GmvIdentity for int {}

class GmvHolder {
    value: GmvIdentity,
}

test "generic_interface_method_values_keep_separate_specializations" {
    let value: GmvIdentity = 7;
    let text = value.echo<string>;
    let number = value.echo<int>;
    assert.equal(text("Ada"), "Ada");
    assert.equal(number(42), 42);
    assert.equal(text("Grace"), "Grace")
}

test "generic_interface_method_value_retains_output_only_type_argument" {
    let value: GmvIdentity = 7;
    let strings = value.empty<string>;
    let ints = value.empty<int>;
    assert.is_true(strings() is string[]);
    assert.is_true(ints() is int[])
}

test "generic_interface_method_value_from_field_access_captures_receiver" {
    let holder = GmvHolder { value: 7 };
    let echo = holder.value.echo<string>;
    assert.equal(echo("Ada"), "Ada")
}
