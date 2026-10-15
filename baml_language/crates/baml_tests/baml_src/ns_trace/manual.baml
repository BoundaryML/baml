function associated(value: int) -> int {
    value + 1
}

test "manual span first terminal call wins and target is not invoked" {
    let span = trace.start_span(associated, inputs = { "value": 4 });
    assert.equal(span.is_closed(), false);
    assert.equal(
        span.run(() -> {
            7
        }),
        7,
    );
    assert.equal(span.is_closed(), false);
    span.finish(5);
    span.fail("ignored");
    span.close();
    assert.equal(span.is_closed(), true);
}

test "manual span run restores context on return and throw" {
    let original = trace.current_context().metadata;
    let span = trace.start_span("operation", metadata = { "phase": "stream" });
    span.run(() -> {
        assert.equal(trace.current_context().metadata["phase"], "stream");
    });
    assert.equal(trace.current_context().metadata, original);
    let caught = false;
    span.run(() -> {
        throw "expected";
    }) catch_all (e) {
        _ => {
            caught = true;
        },
    };
    assert.equal(caught, true);
    assert.equal(trace.current_context().metadata, original);
    assert.equal(span.is_closed(), false);
    span.close();
}

test "manual span rejects invalid targets and closed activation" {
    let rejected = false;
    trace.start_span(12) catch_all (e) {
        _ => {
            rejected = true;
        }
    };
    assert.equal(rejected, true);
    let span = trace.start_span("operation");
    span.close();
    let called = false;
    span.run(() -> {
        called = true;
    }) catch_all (e) {
        _ => {}
    };
    assert.equal(called, false);
}

test "manual span explicit parent and nested scopes retain independent lifetimes" {
    let parent = trace.start_span("parent", metadata = { "key": "parent" });
    let child = trace.start_span("child", parent = parent, metadata = { "key": "child" });
    parent.run(() -> {
        assert.equal(trace.current_context().metadata["key"], "parent");
        child.run(() -> { assert.equal(trace.current_context().metadata["key"], "child"); });
        assert.equal(trace.current_context().metadata["key"], "parent");
    });
    parent.close();
    assert.equal(child.is_closed(), false);
    child.run(() -> { assert.equal(trace.current_context().metadata["key"], "child"); });
    child.finish(null);
}

test "pass-through internals reject ordinary and lambda invocations" {
    let rejected = 0;
    trace._passthrough() catch_all_panics (e) { _ => { rejected += 1; } };
    (() -> { trace._passthrough() })() catch_all_panics (e) { _ => { rejected += 1; } };
    trace._create_span(associated, inputs = {}, config = trace.span()) catch_all (e) {
        _ => { rejected += 1; },
    };
    assert.equal(rejected, 3);
}

test "manual failure accepts an existing context" {
    let span = trace.start_span("context_failure");
    assert.equal(span.is_closed(), false);
    (() -> { throw "original"; })() catch_all (e, context) {
        _ => { span.fail_with_context("replacement", context); },
    };
    assert.equal(span.is_closed(), true);
    span.finish("ignored");
}
