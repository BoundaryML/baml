function record_value(order: int[], value: int) -> int {
    order.push(value);
    value
}

function record_options(order: int[]) -> trace.Options {
    order.push(2);
    trace.span()
}

function traced_sum(first: int, second: int) -> int {
    assert.equal(trace.current_span_id() != null, true);
    first + second
}

function invoke_optional(callee: ((first: int, second: int) -> int)?, order: int[]) -> int? {
    callee?.(
        second = record_value(order, 1),
        $trace = record_options(order),
        first = record_value(order, 3),
    )
}

test "optional BAML trace call preserves argument evaluation order" {
    let order: int[] = [];
    assert.equal(invoke_optional(traced_sum, order), 4);
    assert.equal(order, [1, 2, 3]);
}

test "null optional BAML call skips trace and ordinary arguments" {
    let order: int[] = [];
    assert.equal(invoke_optional(null, order), null);
    assert.equal(order.length(), 0);
}
