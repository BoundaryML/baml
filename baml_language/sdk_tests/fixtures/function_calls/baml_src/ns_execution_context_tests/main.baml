// These are ordinary BAML calls, not bridge context helpers or trace hooks.
function internal_context(callback: (int) -> int, value: int) -> int {
    let result = leaf(
        callback,
        value,
        $trace = trace.hidden().context(
            distinct_id = "internal-id",
            metadata = { "phase": "internal", "remove": null },
        ),
    );
    assert.equal(trace.current_context().metadata["phase"], "root");
    result
}

function leaf(callback: (int) -> int, value: int) -> int {
    callback(value)
}

function current_context() -> trace.Context {
    trace.current_context()
}

function child_span_under_hidden_parent() -> bool {
    leaf_has_span($trace = trace.span())
}

function leaf_has_span() -> bool {
    trace.current_span_id() != null
}
