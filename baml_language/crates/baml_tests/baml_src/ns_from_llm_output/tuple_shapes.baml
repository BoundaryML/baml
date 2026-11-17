// Shape values nested in tuples must be built before heap allocation.
class LlmOutputTupleUnit {
    value: int,
    implements baml.FromLlmOutput {
        type Shape = int;
        function from_llm_output(shape: int) -> Self throws never {
            LlmOutputTupleUnit { value: shape }
        }
    }
}

function tuple_shape_first() -> int {
    let value = baml.sap.parse<(LlmOutputTupleUnit, int)>(`[7, 9]`);
    value.0.value + value.1
}

test "sap builds a shape in the first tuple slot" {
    assert.equal(tuple_shape_first(), 16)
}

function tuple_shape_last() -> int {
    baml.sap.parse<(int, LlmOutputTupleUnit)>(`[9, 7]`).1.value
}

test "sap builds a shape after a plain tuple slot" {
    assert.equal(tuple_shape_last(), 7)
}

function tuple_shape_nested() -> int {
    baml.sap.parse<((int, LlmOutputTupleUnit), int)>(`[[9, 7], 3]`).0.1.value
}

test "sap finds shapes through nested tuples" {
    assert.equal(tuple_shape_nested(), 7)
}

function tuple_shape_list() -> int {
    baml.sap.parse<(LlmOutputTupleUnit[], int)>(`[[7, 8], 9]`).0[1].value
}

test "sap finds a shaped list inside a tuple" {
    assert.equal(tuple_shape_list(), 8)
}

function tuple_shape_map() -> int {
    baml.sap.parse<(map<string, LlmOutputTupleUnit>, int)>(`[{"item": 7}, 9]`).0["item"].value
}

test "sap finds a shaped map inside a tuple" {
    assert.equal(tuple_shape_map(), 7)
}

class LlmOutputTupleBox {
    value: LlmOutputTupleUnit,
}

function tuple_shape_class() -> int {
    baml.sap.parse<(LlmOutputTupleBox, int)>(`[{"value": 7}, 9]`).0.value.value
}

test "sap finds a shaped class field inside a tuple" {
    assert.equal(tuple_shape_class(), 7)
}

function tuple_shape_singleton() -> int {
    baml.sap.parse<(LlmOutputTupleUnit,)>(`[7]`).0.value
}

test "sap builds a shape in a singleton tuple" {
    assert.equal(tuple_shape_singleton(), 7)
}

function tuple_shape_multiple() -> int {
    let value = baml.sap.parse<(LlmOutputTupleUnit, LlmOutputTupleUnit)>(`[7, 9]`);
    value.0.value + value.1.value
}

test "sap builds every shaped tuple slot" {
    assert.equal(tuple_shape_multiple(), 16)
}

function tuple_shape_outer_list() -> int {
    baml.sap.parse<(LlmOutputTupleUnit, int)[]>(`[[7, 9]]`)[0].0.value
}

test "sap finds a shaped tuple inside a list" {
    assert.equal(tuple_shape_outer_list(), 7)
}

function tuple_plain_control() -> int {
    let value = baml.sap.parse<(int, int)>(`[7, 9]`);
    value.0 + value.1
}

test "sap still reads tuples without shapes" {
    assert.equal(tuple_plain_control(), 16)
}
