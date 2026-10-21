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

function launch_context_leaf() -> int {
    assert.equal(trace.current_context().distinct_id, "launch-user");
    assert.equal(trace.current_context().metadata["root"], "launch");
    assert.equal(trace.current_context().metadata["count"], 42);
    assert.equal(trace.current_context().metadata["ratio"], 1.5);
    assert.equal(trace.current_context().metadata["enabled"], true);
    1
}

function launch_context_main() -> int {
    let child = spawn { launch_context_leaf() };
    let result = launch_context_leaf() + query_context_main(7) + (await child);
    assert.equal(trace.current_context().distinct_id, "launch-user");
    assert.equal(trace.current_context().metadata["root"], "launch");
    result
}

test "launch context inheritance and override isolation" {
    assert.equal(launch_context_main($trace = trace.context(
        distinct_id = "launch-user",
        metadata = { "root": "launch", "count": 42, "ratio": 1.5, "enabled": true },
    )), 16);
}
