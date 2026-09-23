function observed_id() -> trace.SpanId? {
    trace.current_span_id()
}

function matches_id(expected: trace.SpanId) -> bool {
    let before = trace.current_span_id();
    let hidden = observed_id($trace = trace.hidden());
    let timing = observed_id($trace = trace.timing());
    let nested = observed_id($trace = trace.span());
    before == expected
        && hidden == null
        && timing == null
        && nested != null
        && nested != before
        && trace.current_span_id() == before
}

test "reserved identity is used and child overrides do not leak" {
    let span = trace.span().reserve();
    assert.equal(matches_id(span.id(), $trace = span), true);
    assert.equal(span.inspect().mode, trace.Mode.Span)
}

test "reusable options create distinct spans" {
    let options = trace.span();
    let first = observed_id($trace = options);
    let second = observed_id($trace = options);
    assert.equal(first != null && second != null && first != second, true)
}

test "reservation aliases cannot attach twice" {
    let span = trace.span().reserve();
    let alias = span;
    let first = observed_id($trace = span);
    assert.equal(first, span.id());
    let rejected = {
        observed_id($trace = alias);
        false
    } catch (error) {
        baml.panics.UserPanic => true,
    };
    assert.equal(rejected, true);
    assert.equal(span.id(), alias.id())
}

function observed_generic<T>(value: T, expected: trace.SpanId) -> T {
    assert.equal(trace.current_span_id(), expected);
    value
}

test "generic traced calls keep their type arguments" {
    let span = trace.span().reserve();
    assert.equal(observed_generic<int>(42, span.id(), $trace = span), 42)
}

function observed_defaults(a: int, b: int = 7) -> int {
    assert.equal(trace.current_span_id() != null, true);
    a * 10 + b
}

test "traced calls preserve named and default arguments" {
    assert.equal(observed_defaults(2, $trace = trace.span()), 27);
    assert.equal(observed_defaults(2, b = 4, $trace = trace.span()), 24);
    assert.equal(observed_defaults(2, $trace = trace.span(), b = 5), 25)
}

test "traced closures use their own span" {
    let span = trace.span().reserve();
    let run = () -> {
        trace.current_span_id()
    };
    assert.equal(run($trace = span), span.id())
}

function recursive_span(depth: int) -> bool {
    let own = trace.current_span_id();
    if (depth == 0) {
        return own != null;
    }
    let nested = recursive_span(depth - 1, $trace = trace.span());
    nested && trace.current_span_id() == own
}

test "recursive spans restore the outer identity" {
    assert.equal(recursive_span(3, $trace = trace.span()), true)
}

function traced_failure() -> void throws baml.errors.InvalidArgument {
    throw baml.errors.InvalidArgument { message: "expected" }
}

function catches_traced_failure() -> bool {
    let own = trace.current_span_id();
    traced_failure($trace = trace.span(error = true)) catch (error) {
        baml.errors.InvalidArgument => {},
    };
    trace.current_span_id() == own
}

test "error unwinding restores the caller span" {
    assert.equal(catches_traced_failure($trace = trace.span()), true)
}

test "reservations may attach in a spawned task" {
    let span = trace.span().reserve();
    let task = spawn { observed_id($trace = span) };
    assert.equal(await task, span.id())
}

function capture_leaf(value: int) -> int {
    value + 1
}

function capture_main() -> int {
    let base = trace.span(inputs = true, output = true);
    let first = capture_leaf(41, $trace = base);
    let second = capture_leaf(42, $trace = base.span(inputs = false, output = false));
    first + second
}

function off_main() -> bool {
    let span = trace.span().reserve();
    observed_id($trace = span) == null
}
