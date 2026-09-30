// Invocation identity tests run with BAML_TELEMETRY=medium.
test "defaults preserve automatic capture" {
    let settings = trace.options().inspect();
    assert.equal(settings.mode, null);
    assert.equal(settings.capture.inputs, null);
    assert.equal(settings.capture.output, null);
    assert.equal(settings.capture.error, null);
    assert.equal(trace.options(mode = trace.Mode.Hidden).inspect().mode, trace.Mode.Hidden);
    assert.equal(trace.hidden().inspect().mode, trace.Mode.Hidden);
    assert.equal(trace.timing().inspect().mode, trace.Mode.Timing);
    assert.equal(trace.span().inspect().mode, trace.Mode.Span);
}

test "builders are immutable and mode preserves capture" {
    let base = trace.span(inputs = false, output = true, error = null);
    let hidden = base.mode(trace.Mode.Hidden);
    let restored = hidden.mode(null);
    let replaced = base.span(error = true);
    base.mode(trace.Mode.Timing);
    assert.equal(base.inspect().mode, trace.Mode.Span);
    assert.equal(hidden.inspect().mode, trace.Mode.Hidden);
    assert.equal(restored.inspect().mode, null);
    assert.equal(restored.inspect().capture.inputs, false);
    assert.equal(restored.inspect().capture.output, true);
    assert.equal(restored.inspect().capture.error, null);
    assert.equal(replaced.inspect().capture.inputs, null);
    assert.equal(replaced.inspect().capture.output, null);
    assert.equal(replaced.inspect().capture.error, true);
    assert.equal(base.inspect().capture.error, null);
}

test "inspection snapshots are detached at every level" {
    let options = trace.span(inputs = true, output = false);
    let first = options.inspect();
    let second = options.inspect();
    first.mode = trace.Mode.Hidden;
    first.capture.inputs = false;
    first.capture.output = true;
    assert.equal(second.mode, trace.Mode.Span);
    assert.equal(second.capture.inputs, true);
    assert.equal(second.capture.output, false);
    assert.equal(options.inspect().capture.inputs, true);
    assert.equal(options.inspect().capture.output, false);
}

test "reservation snapshots force span without changing options" {
    let options = trace.span(error = true).mode(trace.Mode.Hidden);
    let reserved = options.reserve();
    let inspected = reserved.inspect();
    inspected.capture.error = false;
    inspected.mode = null;
    assert.equal(options.inspect().mode, trace.Mode.Hidden);
    assert.equal(reserved.inspect().mode, trace.Mode.Span);
    assert.equal(reserved.inspect().capture.error, true);
    let first = reserved.id();
    let alias = first;
    assert.equal(first == alias, true);
    assert.equal(first == reserved.id(), true);
    assert.equal(first != options.reserve().id(), true);
}

function own_id() -> trace.SpanId? {
    let first = trace.current_span_id();
    assert.equal(first == trace.current_span_id(), true);
    first
}

function non_span_children() -> bool {
    let parent = trace.current_span_id();
    assert.equal(parent != null, true);
    let hidden = own_id($trace = trace.hidden());
    let timing = own_id($trace = trace.timing());
    assert.equal(hidden, null);
    assert.equal(timing, null);
    assert.equal(parent == trace.current_span_id(), true);
    true
}

test "hidden and timing children never expose ancestor identity" {
    assert.equal(non_span_children($trace = trace.span()), true);
}

function check_reserved(reserved: trace.ReservedSpan) -> bool {
    let actual = own_id($trace = reserved);
    assert.equal(actual == reserved.id(), true);
    assert.equal(reserved.id() == reserved.id(), true);
    assert.equal(reserved.inspect().mode, trace.Mode.Span);
    true
}

test "reservations retain identity and inspection after attachment" {
    assert.equal(check_reserved(trace.span(output = true).reserve()), true);
    let shared = trace.span().reserve();
    let task = spawn { check_reserved(shared) };
    assert.equal(await task, true);
}

function reservation_reuse_fails() -> bool {
    let reserved = trace.span().reserve();
    let alias = reserved;
    own_id($trace = reserved);
    {
        own_id($trace = alias);
        false
    } catch_all_panics (error) {
        _ => true,
    }
}

test "reservation aliases cannot attach twice" {
    assert.equal(reservation_reuse_fails(), true);
}

function copied_reservation_reuse_fails() -> bool {
    let reserved = trace.span().reserve();
    let copied = baml.deep_copy(reserved);
    own_id($trace = reserved);
    {
        own_id($trace = copied);
        false
    } catch_all_panics (error) {
        _ => true,
    }
}

test "deep copy cannot duplicate reservation attachment authority" {
    assert.equal(copied_reservation_reuse_fails(), true);
}

function reusable_options() -> bool {
    let options = trace.span();
    let first = own_id($trace = options);
    let second = own_id($trace = options);
    first != null && second != null && first != second
}

test "reusable options produce independent invocation identities" {
    assert.equal(reusable_options(), true);
}

function record_value(order: int[], value: int) -> int {
    order.push(value);
    value
}

function record_options(order: int[]) -> trace.Options {
    order.push(2);
    trace.span()
}

function sum(first: int, second: int) -> int {
    first + second
}

test "reserved arguments retain source evaluation order" {
    let order: int[] = [];
    assert.equal(sum(
        second = record_value(order, 1),
        $trace = record_options(order),
        first = record_value(order, 3),
    ), 4);
    assert.equal(order, [1, 2, 3]);
}

class Counter {
    value: int,

    function add(self, amount: int, expected: trace.SpanId) -> int {
        assert.equal(trace.current_span_id() == expected, true);
        self.value + amount
    }
}

test "attachments work on methods closures and nested calls" {
    let counter = Counter { value: 10 };
    let direct = trace.span().reserve();
    assert.equal(counter.add(3, direct.id(), $trace = direct), 13);
    let method = counter.add;
    let bound = trace.span().reserve();
    assert.equal(method(4, bound.id(), $trace = bound), 14);
    let closure = (value: int, expected: trace.SpanId) -> int {
        assert.equal(trace.current_span_id() == expected, true);
        value + 1
    };
    let closed = trace.span().reserve();
    assert.equal(closure(5, closed.id(), $trace = closed), 6);
    let nested = trace.span().reserve();
    assert.equal(sum(
        if (own_id($trace = trace.hidden()) == null) { 1 } else { 0 },
        closure(2, nested.id(), $trace = nested),
        $trace = trace.span(),
    ), 4);
}

function unsupported_native_does_not_leak(native: () -> trace.SpanId?) -> bool {
    let rejected = {
        native($trace = trace.span());
        false
    } catch_all_panics (error) {
        _ => true,
    };
    assert.equal(own_id(), null);
    rejected
}

test "native attachment rejection does not leak pending options" {
    let native = trace.current_span_id;
    assert.equal(unsupported_native_does_not_leak(native), true);
}
