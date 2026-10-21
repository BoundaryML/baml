class Box { value: int }

function and_inline(a: bool, box: Box) -> int {
    a && { box.value = box.value + 1; true }
    box.value
}

function or_inline(a: bool, box: Box) -> int {
    a || { box.value = box.value + 1; true }
    box.value
}

function and_index(a: bool, values: int[]) -> int {
    a && { values[0] = values[0] + 1; true }
    values[0]
}

function and_capture(a: bool) -> int {
    let value = 0
    let read = () => { value }
    a && { value = 1; true }
    read()
}

function unused_division(x: int) -> int {
    { let unused = 1 / x; 42 } catch (e) {
        baml.panics.DivisionByZero => 7
    }
}

function unused_index(values: int[], index: int) -> int {
    { let unused = values[index]; 42 } catch (e) {
        baml.panics.IndexOutOfBounds => 7
    }
}

test "discarded logical expressions preserve conditional effects" {
    assert.equal(and_inline(true, Box { value: 0 }), 1)
    assert.equal(and_inline(false, Box { value: 0 }), 0)
    assert.equal(or_inline(false, Box { value: 0 }), 1)
    assert.equal(or_inline(true, Box { value: 0 }), 0)
    assert.equal(and_index(true, [0]), 1)
    assert.equal(and_index(false, [0]), 0)
    assert.equal(and_capture(true), 1)
    assert.equal(and_capture(false), 0)
}

test "unused evaluations preserve catchable panics" {
    assert.equal(unused_division(0), 7)
    assert.equal(unused_division(1), 42)
    assert.equal(unused_index([1], 4), 7)
    assert.equal(unused_index([1], 0), 42)
}
