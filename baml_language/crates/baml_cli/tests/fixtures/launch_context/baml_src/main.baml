function Initialize() -> string? {
    assert.equal(trace.current_context().distinct_id, "pipe-user");
    assert.equal(trace.current_context().metadata["phase"], "launch");
    trace.current_context().distinct_id
}

function Check(value: string) -> string {
    assert.equal(Initialize(), "pipe-user");
    assert.equal(trace.current_context().distinct_id, "pipe-user");
    assert.equal(trace.current_context().metadata["phase"], "launch");
    let child = spawn {
        assert.equal(trace.current_context().distinct_id, "pipe-user");
        value
    };
    await child
}

function Args() -> string[] {
    assert.equal(Check("ok"), "ok");
    baml.sys.argv()
}
