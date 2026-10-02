function own_id() -> trace.SpanId? {
    trace.current_span_id()
}

test "disabled tracing never exposes an invocation identity" {
    assert.equal(own_id(), null);
    assert.equal(own_id($trace = trace.span()), null);
    assert.equal(own_id($trace = trace.timing()), null);
    assert.equal(own_id($trace = trace.hidden()), null);
    let reserved = trace.span(inputs = true, output = true, error = true).reserve();
    let before = reserved.id();
    assert.equal(own_id($trace = reserved), null);
    assert.equal(reserved.id() == before, true);
    assert.equal(reserved.inspect().mode, trace.Mode.Span);
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

test "disabled tracing still enforces single use reservations" {
    assert.equal(reservation_reuse_fails(), true);
}
