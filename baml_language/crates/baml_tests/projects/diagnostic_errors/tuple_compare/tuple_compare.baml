// A tuple implements `baml.ops.Compare` only when every slot type does.

// `uint8array` has no `Compare`, so neither does a tuple holding one.
function bytes_slot(a: (int, uint8array), b: (int, uint8array)) -> bool {
    a < b
}

// Nested: the inner tuple's `uint8array` slot disqualifies the outer tuple.
function nested_bytes_slot(a: (int, (string, uint8array)), b: (int, (string, uint8array))) -> bool {
    a >= b
}

// A union slot is not orderable even when every member is.
function union_slot(a: (int | string, int), b: (int | string, int)) -> bool {
    a <= b
}

function needs_compare<T extends baml.ops.Compare>(x: T) -> T {
    x
}

function bound_rejects_bytes_slot(x: (int, uint8array)) -> (int, uint8array) {
    needs_compare(x)
}

// An unbounded slot proves nothing.
function unbounded_slot<A>(a: (A, int), b: (A, int)) -> bool {
    a > b
}

class NoOrder {
    n: int
}

function class_slot(a: (NoOrder,), b: (NoOrder,)) -> baml.ops.Ordering {
    a.cmp(b)
}
