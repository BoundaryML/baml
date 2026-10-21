interface B1180Named {
  function name(self) -> string throws never
}

class B1180Person {
    value: string,
}

implements B1180Named for B1180Person {
  function name(self) -> string {
    self.value
  }
}

function b1180_optional_name(value: B1180Named?) -> string {
    value?.name() ?? "nobody"
}

test "optional_interface_method_call_dispatches" {
    assert.equal(b1180_optional_name(B1180Person { value: "Ada" }), "Ada");
    assert.equal(b1180_optional_name(null), "nobody")
}

interface B1180Getter<T> {
  function get(self) -> T throws never
}

class B1180IntValue {
    value: int,
}

implements B1180Getter<int> for B1180IntValue {
  function get(self) -> int {
    self.value
  }
}

function b1180_optional_get(value: B1180Getter<int>?) -> int {
    value?.get() ?? -1
}

test "optional_generic_interface_method_call_preserves_type_args" {
    assert.equal(b1180_optional_get(B1180IntValue { value: 42 }), 42);
    assert.equal(b1180_optional_get(null), -1)
}
