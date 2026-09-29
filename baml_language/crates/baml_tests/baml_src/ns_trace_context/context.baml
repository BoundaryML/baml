function read() -> trace.Context {
    trace.current_context()
}

function mutate() -> trace.Context {
    trace.update_context(metadata = { "child": true, "remove": null });
    trace.current_context()
}

function fail() -> void throws string {
    trace.update_context(metadata = { "failed_child": true });
    throw "failure";
}

test "context starts empty and native defaults do not create a context boundary" {
    trace.update_context();
    assert.equal(trace.context().inspect().context.metadata.length(), 0);
    assert.equal(trace.current_context().metadata.length(), 0);
    trace.update_context(distinct_id = "root");
    assert.equal(trace.current_context().distinct_id, "root");
    assert.equal(read().distinct_id, "root");
}

test "updates propagate downwards but children cannot change their parent" {
    trace.update_context(metadata = { "parent": 1, "remove": "yes" }, distinct_id = "parent");
    let child = mutate();
    assert.equal(child.metadata, { "parent": 1, "child": true });
    assert.equal(child.distinct_id, "parent");
    assert.equal(trace.current_context().metadata, { "parent": 1, "remove": "yes" });
}

test "builders own input maps and preserve context through mode and span changes" {
    let metadata: map<string, string | int | float | bool | null> = { "count": 1, "remove": null };
    let options = trace.context(metadata = metadata, distinct_id = "user")
        .context(metadata = { "flag": true })
        .mode(trace.Mode.Hidden)
        .span(inputs = false, output = false, error = false);
    metadata["count"] = 99;
    trace.update_context(metadata = { "remove": "inherited", "ratio": 1.5 });
    let result = read($trace = options);
    assert.equal(result.distinct_id, "user");
    assert.equal(result.metadata, { "count": 1, "flag": true, "ratio": 1.5 });
    assert.equal(options.inspect().context.metadata["count"], 1);
    assert.equal(options.inspect().context.metadata["remove"], null);
}

test "inspection is detached from both options and the current frame" {
    trace.update_context(metadata = { "live": 1 });
    let observed = trace.current_context();
    observed.metadata["live"] = 99;
    observed.distinct_id = "not live";
    assert.equal(trace.current_context().metadata["live"], 1);
    assert.equal(trace.current_context().distinct_id, null);
    let options = trace.context(metadata = { "value": 1 });
    let settings = options.inspect();
    settings.context.metadata["value"] = 99;
    assert.equal(read($trace = options).metadata["value"], 1);
}

test "reservations freeze patches but resolve ambient context when invoked" {
    trace.update_context(metadata = { "ambient": 1 });
    let reserved = trace.context(metadata = { "own": true }).reserve();
    trace.update_context(metadata = { "ambient": 2 });
    let result = read($trace = reserved);
    assert.equal(result.metadata, { "ambient": 2, "own": true });
    assert.equal(reserved.inspect().context.metadata, { "own": true });
}

test "unwind restores the caller and same-frame catch retains updates" {
    trace.update_context(metadata = { "parent": true });
    fail() catch (error) { _ => null };
    assert.equal(trace.current_context().metadata, { "parent": true });
    {
        trace.update_context(metadata = { "retained": true });
        throw "same-frame";
    } catch (error) { _ => null };
    assert.equal(trace.current_context().metadata, { "parent": true, "retained": true });
}

test "closures and optional calls inherit context without leaking updates" {
    trace.update_context(metadata = { "parent": true });
    let closure = () -> {
        trace.update_context(metadata = { "closure": true });
        trace.current_context()
    };
    assert.equal(closure($trace = trace.hidden()).metadata, { "parent": true, "closure": true });
    let maybe: (() -> trace.Context throws never)? = read;
    assert.equal(maybe?.($trace = trace.context(metadata = { "optional": true }))?.metadata,
        { "parent": true, "optional": true });
    assert.equal(trace.current_context().metadata, { "parent": true });
}

test "spawn snapshots context at launch and await does not import child changes" {
    trace.update_context(metadata = { "version": 1 });
    let child = spawn {
        trace.update_context(metadata = { "child": true });
        trace.current_context()
    };
    trace.update_context(metadata = { "version": 2 });
    let result = await child;
    assert.equal(result.metadata, { "version": 1, "child": true });
    assert.equal(trace.current_context().metadata, { "version": 2 });
}

test "null identity inherits and repeated patches replace or remove metadata keys" {
    trace.update_context(metadata = { "key": 1 }, distinct_id = "parent");
    let options = trace.context(metadata = { "key": 2 })
        .context(metadata = { "key": null, "missing": null });
    assert.equal(read($trace = options).metadata.length(), 0);
    assert.equal(read($trace = options).distinct_id, "parent");
}

class TraceContextModifier {}

implements<T, E> baml.spawn.Modifier<T, E> for TraceContextModifier {
    function apply(self, plan: baml.spawn.Plan<T, E>) -> baml.spawn.Plan<T, E> throws never {
        trace.update_context(metadata = { "phase": "modifier" });
        plan
    }
}

test "spawn modifier calls do not leak their local context into the launched task" {
    trace.update_context(metadata = { "phase": "parent" });
    let modifier = TraceContextModifier {};
    let child = spawn with modifier { trace.current_context() };
    assert.equal((await child).metadata["phase"], "parent");
    assert.equal(trace.current_context().metadata["phase"], "parent");
}

test "rooted spawns retain context even though their cancellation parent changes" {
    trace.update_context(metadata = { "version": 1 });
    let child = spawn with baml.spawn.Root.new() { trace.current_context() };
    trace.update_context(metadata = { "version": 2 });
    assert.equal((await child).metadata["version"], 1);
    assert.equal(trace.current_context().metadata["version"], 2);
}
