class SrTag {
    id: int
    note: string

    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never {
            self.id == other.id
        }
    }

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            SrTag { id: baml.json.to<int>(j), note: "decoded" }
        }
    }
}

class SrNode {
    next: SrNode?
    tag: SrTag
}

function sr_cycle(id: int, note: string) -> SrNode {
    let node = SrNode { next: null, tag: SrTag { id: id, note: note } };
    node.next = node;
    node
}

function sr_cycles() -> bool {
    let a = sr_cycle(1, "left");
    let b = sr_cycle(1, "right");
    let c = sr_cycle(2, "left");
    a.eq(b) && a == b && !a.eq(c) && a.eq(a)
}

test "synthesized_equals_keeps_cycles_and_nested_overrides" {
    assert.is_true(sr_cycles())
}

class SrNeverEqual {
    value: int
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { false }
    }
}

function sr_reflexive_operator() -> bool {
    let a = SrNeverEqual { value: 1 };
    a == a && !(a == SrNeverEqual { value: 1 })
}

test "same_reference_operator_precedes_explicit_override" {
    assert.is_true(sr_reflexive_operator())
    assert.is_true(!baml.ops.equals_equals(1, "1"))
}

class SrEnvelope<T> {
    item: T
    items: T[]
    by_name: map<string, T>
    optional: T?
}

function sr_decode_nested() -> bool throws baml.json.DecodeError {
    let j: baml.json.json = { "item": 1, "items": [2], "by_name": { "x": 3 }, "optional": null };
    let a = SrEnvelope<SrTag>.from_json(j);
    let b = baml.json.to<SrEnvelope<SrTag>>(j);
    a.item.id == 1 && a.item.note == "decoded"
        && a.items[0].id == 2 && a.by_name["x"].id == 3
        && a.optional == null && a.eq(b)
}

test "synthesized_from_json_bypasses_only_root" {
    assert.is_true(sr_decode_nested())
}

function sr_callable_defaults<T>(value: T, other: T) -> bool throws baml.json.SerializationError | baml.json.DecodeError {
    let render = value.to_string;
    let encode = value.to_json;
    let decode = T.from_json;
    let equal = value.eq;
    let unequal = value.neq;
    let restored = decode(encode());
    render() == restored.to_string() && equal(restored) && unequal(other)
}

test "structural_first_class_methods_keep_self_frames" {
    assert.is_true(sr_callable_defaults(
        SrEnvelope<int> { item: 1, items: [2], by_name: { "x": 3 }, optional: null },
        SrEnvelope<int> { item: 2, items: [2], by_name: { "x": 3 }, optional: null }
    ))
}
