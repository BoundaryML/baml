interface B1470Shift {
    offset: int
    function shift(self, n: int) -> int throws never
}

class B1470Adder {
    offset: int
    implements B1470Shift {
        function shift(self, n: int) -> int {
            n + self.offset
        }
    }
}

class B1470Holder {
    value: B1470Shift
}

class B1470Outer {
    inner: B1470Holder
}

function b1470_mapped(value: B1470Shift, items: int[]) -> int[] {
    items.map((n) -> { value.shift(n) })
}

function b1470_nested(value: B1470Shift) -> int[][] {
    [1, 2].map((n) -> {
        [3, 4].map((m) -> { value.shift(n + m) })
    })
}

function b1470_bounded<T extends B1470Shift>(value: T) -> int[] {
    [1, 2].map((n) -> { value.shift(n) })
}

test "b1470_direct_and_captured_interface_receivers" {
    let value: B1470Shift = B1470Adder { offset: 10 }
    assert.equal(value.shift(1), 11)
    let result: int[] = []
    for (let n in [1, 2, 3]) {
        result.push(value.shift(n))
    }
    assert.equal(result, [11, 12, 13])
    assert.equal(b1470_mapped(value, [1, 2, 3]), [11, 12, 13])
    assert.equal([1, 2, 3].map((n) -> { value.shift(n) }), [11, 12, 13])
    assert.equal(b1470_nested(value), [[14, 15], [15, 16]])
    assert.equal(b1470_bounded(B1470Adder { offset: 20 }), [21, 22])
}

test "b1470_captured_interface_field_chains" {
    let holder = B1470Outer {
        inner: B1470Holder { value: B1470Adder { offset: 10 } }
    }
    assert.equal([1, 2].map((n) -> { holder.inner.value.shift(n) }), [11, 12])
}

test "b1470_captured_interface_bound_method" {
    let value: B1470Shift = B1470Adder { offset: 10 }
    assert.equal([1, 2].map((n) -> {
        let shift = value.shift
        shift(n)
    }), [11, 12])
}

test "b1470_captured_interface_field_assignment" {
    let value: B1470Shift = B1470Adder { offset: 10 }
    assert.equal([1, 2].map((n) -> {
        value.offset = n
        value.shift(10)
    }), [11, 12])
    assert.equal(value.offset, 2)
}

interface B1470Transform<T> {
    function transform(self, value: T) -> T throws never
}

implements B1470Transform<int> for B1470Adder {
    function transform(self, value: int) -> int {
        self.offset + value
    }
}

function b1470_generic<T>(value: B1470Transform<T>, items: T[]) -> T[] {
    items.map((item) -> { value.transform(item) })
}

test "b1470_captured_generic_interface" {
    let value: B1470Transform<int> = B1470Adder { offset: 30 }
    assert.equal([1, 2].map((n) -> { value.transform(n) }), [31, 32])
    assert.equal(b1470_generic(value, [3, 4]), [33, 34])
}
