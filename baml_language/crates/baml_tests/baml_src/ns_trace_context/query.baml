function query_context_leaf(n: int) -> int {
    assert.equal(trace.current_context().distinct_id, "query-user");
    assert.equal(trace.current_context().metadata["root"], "inherited");
    assert.equal(trace.current_context().metadata["phase"], "entry");
    (
        () -> {
            assert.equal(trace.current_context().metadata["phase"], "nested");
        }
    )($trace = trace.context(metadata = { "phase": "nested" }));
    n
}

function query_context_main(n: int) -> int {
    (
        () -> {
            let options = trace.context(
                metadata = { "phase": "entry", "count": 42, "ratio": 1.5, "enabled": true },
            ).span();
            let child = spawn {
                query_context_leaf(n, $trace = options)
            };
            query_context_leaf(n, $trace = options) + (await child)
        }
    )($trace = trace.context(
        distinct_id = "query-user",
        metadata = { "root": "inherited" },
    ))
}

test "query context native" {
    assert.equal(query_context_main(7), 14);
}
