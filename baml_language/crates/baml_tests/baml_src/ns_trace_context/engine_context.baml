function engine_context_check(expected: string?) -> bool {
    assert.equal(trace.current_context().distinct_id, expected);
    if (expected != null) {
        assert.equal(trace.current_context().metadata["owner"], expected);
    } else {
        assert.equal(trace.current_context().metadata.length(), 0);
    }
    let child = spawn {
        assert.equal(trace.current_context().distinct_id, expected);
        (
            () -> {
                assert.equal(trace.current_context().distinct_id, "nested");
                assert.equal(trace.current_context().metadata["owner"], "nested");
            }
        )($trace = trace.context(distinct_id = "nested", metadata = { "owner": "nested" }));
        assert.equal(trace.current_context().distinct_id, expected);
        true
    };
    (
        () -> {
            assert.equal(trace.current_context().distinct_id, "override");
            assert.equal(trace.current_context().metadata["owner"], "override");
        }
    )($trace = trace.context(distinct_id = "override", metadata = { "owner": "override" }));
    assert.equal(trace.current_context().distinct_id, expected);
    await child
}

test "engine context inheritance and override isolation" {
    assert.equal(engine_context_check("native", $trace = trace.context(
        distinct_id = "native",
        metadata = { "owner": "native" },
    )), true);
}
