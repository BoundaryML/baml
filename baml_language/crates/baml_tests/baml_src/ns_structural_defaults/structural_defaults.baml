class Record {
    value int
}

class Box<T> {
    value T
}

enum Color {
    Red
    Blue
}

function render<T extends baml.ToString>(value: T) -> string throws never {
    value.to_string()
}

function encode<T extends baml.ToJson>(value: T) -> baml.json.json throws baml.json.SerializationError {
    value.to_json()
}

function decode<T extends baml.FromJson>(value: baml.json.json) -> T throws baml.json.DecodeError {
    T.from_json(value)
}

function equal<T extends baml.ops.Equals>(left: T, right: T) -> bool throws never {
    left.eq(right)
}

test "structural_defaults_satisfy_bounds" {
    let record = Record { value: 7 };
    assert.equal(render(record), string.from(record))
    assert.equal(encode(record), baml.json.from(record))
    assert.equal(decode<Record>(encode(record)).value, 7)
    assert.equal(equal(record, Record { value: 7 }), true)
    assert.equal(record.eq(Record { value: 8 }), false)
    assert.equal(record.neq(Record { value: 8 }), true)
}

test "structural_defaults_cover_containers_and_generic_classes" {
    let values = [1, 2];
    let entries = { "value": 3 };
    let box = Box<Record> { value: Record { value: 4 } };
    assert.equal(render(values), string.from(values))
    assert.equal(encode(entries), baml.json.from(entries))
    assert.equal(equal(values, [1, 2]), true)
    assert.equal(equal(entries, { "value": 3 }), true)
    assert.equal(decode<Box<Record>>(encode(box)).value.value, 4)
}

test "structural_defaults_cover_enums_and_optional_types" {
    let value: Record? = null;
    assert.equal(render(value), "null")
    assert.equal(encode(value), null)
    assert.equal(decode<Record?>(null), null)
    assert.equal(Color.from_json("Red"), Color.Red)
    assert.equal(decode<Color>("Blue"), Color.Blue)
    assert.equal(equal(Color.Red, Color.Red), true)
}

class Custom {
    value int

    implements baml.ToString {
        function to_string(self) -> string throws never {
            "custom"
        }
    }

    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws never {
            "custom"
        }
    }

    implements baml.FromJson {
        function from_json(value: baml.json.json) -> Self throws never {
            Custom { value: 42 }
        }
    }

    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never {
            true
        }
    }
}

test "explicit_implementations_win_over_synthesized_defaults" {
    let value = Custom { value: 1 };
    assert.equal(render(value), "custom")
    assert.equal(encode(value), "custom")
    assert.equal(decode<Custom>(null).value, 42)
    let optional = decode<Custom?>("custom");
    assert.equal(optional?.value, 42)
    assert.equal(decode<Custom?>(null), null)
    assert.equal(equal(value, Custom { value: 2 }), true)
    assert.equal(value == Custom { value: 2 }, true)
}

function erased_render(value: unknown) -> string throws never {
    value.to_string()
}

function erased_encode(value: unknown) -> baml.json.json throws baml.json.SerializationError {
    value.to_json()
}

function unconstrained<T>(value: T) -> string throws baml.json.SerializationError | baml.json.DecodeError {
    let decoded = T.from_json(value.to_json());
    decoded.to_string()
}

test "erased_values_use_the_same_implementations" {
    let value = Custom { value: 1 };
    assert.equal(erased_render(value), "custom")
    assert.equal(erased_encode(value), "custom")
    assert.equal(erased_encode(Record { value: 7 }), encode(Record { value: 7 }))
    assert.equal(unconstrained(Record { value: 7 }), render(Record { value: 7 }))
}

test "json_failures_remain_runtime_failures" {
    let result = decode<Record>(null) catch (error) {
        let err: baml.json.DecodeError => Record { value: 99 }
    };
    assert.equal(result.value, 99)
}

interface HasItem {
    type Item
}

interface HasOrderedItem {
    type Item extends baml.ops.Compare
}

class RecordItem {
    implements HasItem {
        type Item = Record
    }
}

class IntItem {
    implements HasOrderedItem {
        type Item = int
    }
}

function decode_item<T extends HasItem>(value: baml.json.json) -> T.Item throws baml.json.DecodeError {
    T.Item.from_json(value)
}

function decode_ordered_item<T extends HasOrderedItem>(value: baml.json.json) -> T.Item throws baml.json.DecodeError {
    T.Item.from_json(value)
}

test "associated_types_receive_structural_defaults" {
    assert.equal(decode_item<RecordItem>({ "value": 7 }).value, 7)
    assert.equal(decode_ordered_item<IntItem>(9), 9)
}

interface CustomRender {
    function to_string(self) -> string throws never
}

class Left {
    implements CustomRender {
        function to_string(self) -> string throws never {
            "left"
        }
    }
}

class Right {
    implements CustomRender {
        function to_string(self) -> string throws never {
            "right"
        }
    }
}

function render_union(value: Left | Right) -> string throws never {
    value.to_string()
}

function render_custom<T extends CustomRender>(value: T) -> string throws never {
    value.to_string()
}

function render_erased_custom(value: CustomRender) -> string throws never {
    value.to_string()
}

test "explicit_interface_members_take_precedence_in_unions_and_bounds" {
    assert.equal(render_union(Left {}), "left")
    assert.equal(render_union(Right {}), "right")
    assert.equal(render_custom(Left {}), "left")
    assert.equal(render_erased_custom(Right {}), "right")
    assert.equal(Left {}.to_string(), "left")
}

test "explicit_to_string_sources_remain_ambiguous" {
    let declarations = `
        interface CustomRender {
            function to_string(self) -> string throws never
        }
        class Left {
            implements CustomRender {
                function to_string(self) -> string throws never { "left" }
            }
            implements baml.ToString {
                function to_string(self) -> string throws never { "explicit" }
            }
        }
        class Right {
            implements CustomRender {
                function to_string(self) -> string throws never { "right" }
            }
        }
    `;
    for (let source in [
        "function render<T extends CustomRender & baml.ToString>(value: T) -> string throws never { value.to_string() }",
        "function render<T extends baml.ToString & CustomRender>(value: T) -> string throws never { value.to_string() }",
        "function render(value: Left) -> string throws never { value.to_string() }",
        "function render(value: Left | Right) -> string throws never { value.to_string() }",
        "interface Both requires CustomRender, baml.ToString {}\nfunction render(value: Both) -> string throws never { value.to_string() }",
    ]) {
        root.compiler.AssertHasCode({ "main.baml": declarations + source }, "E0121")
    }
}
