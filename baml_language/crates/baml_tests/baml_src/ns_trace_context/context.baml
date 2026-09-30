function read() -> trace.Context {
    trace.current_context()
}

function fail() -> void throws string {
    assert.equal(trace.current_context().metadata["failed_child"], true);
    throw "failure";
}

test "context starts empty and IIFEs restore it on return" {
    assert.equal(trace.context().inspect().context.metadata.length(), 0);
    assert.equal(trace.current_context().metadata.length(), 0);
    (
        () -> {
            assert.equal(trace.current_context().distinct_id, "root");
            assert.equal(read().distinct_id, "root");
        }
    )($trace = trace.context(distinct_id = "root"));
    assert.equal(trace.current_context().distinct_id, null);
}

test "nested scopes inherit without changing their parent" {
    (
        () -> {
            let child = read($trace = trace.context(metadata = { "child": true, "remove": null }));
            assert.equal(child.metadata, { "parent": 1, "child": true });
            assert.equal(child.distinct_id, "parent");
            assert.equal(trace.current_context().metadata, { "parent": 1, "remove": "yes" });
        }
    )($trace = trace.context(metadata = { "parent": 1, "remove": "yes" }, distinct_id = "parent"));
    assert.equal(trace.current_context().metadata.length(), 0);
}

test "builders own input maps and preserve context through mode and span changes" {
    let metadata: map<string, string | int | float | bool | null> = { "count": 1, "remove": null };
    let options = trace.context(metadata = metadata, distinct_id = "user")
        .context(metadata = { "flag": true })
        .mode(trace.Mode.Hidden)
        .span(inputs = false, output = false, error = false);
    metadata["count"] = 99;
    let result = (
        () -> {
            read($trace = options)
        }
    )($trace = trace.context(metadata = { "remove": "inherited", "ratio": 1.5 }));
    assert.equal(result.distinct_id, "user");
    assert.equal(result.metadata, { "count": 1, "flag": true, "ratio": 1.5 });
    assert.equal(options.inspect().context.metadata["count"], 1);
    assert.equal(options.inspect().context.metadata["remove"], null);
}

test "inspection is detached from both options and the current frame" {
    (
        () -> {
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
    )($trace = trace.context(metadata = { "live": 1 }));
}

test "reservations freeze patches but resolve ambient context when invoked" {
    let reserved = (
        () -> {
            trace.context(metadata = { "own": true }).reserve()
        }
    )($trace = trace.context(metadata = { "ambient": 1 }));
    let result = (
        () -> {
            read($trace = reserved)
        }
    )($trace = trace.context(metadata = { "ambient": 2 }));
    assert.equal(result.metadata, { "ambient": 2, "own": true });
    assert.equal(reserved.inspect().context.metadata, { "own": true });
}

test "unwind restores the caller context across IIFEs" {
    (
        () -> {
            fail($trace = trace.context(metadata = { "failed_child": true })) catch (error) {
                _ => null
            };
            assert.equal(trace.current_context().metadata, { "parent": true });
            (
                () -> {
                    throw "nested";
                }
            )($trace = trace.context(metadata = { "nested": true })) catch (error) {
                _ => null
            };
            assert.equal(trace.current_context().metadata, { "parent": true });
        }
    )($trace = trace.context(metadata = { "parent": true }));
    assert.equal(trace.current_context().metadata.length(), 0);
}

test "closures and optional calls inherit isolated contexts" {
    (
        () -> {
            let closure = () -> {
                trace.current_context()
            };
            assert.equal(
                closure($trace = trace.hidden().context(metadata = { "closure": true })).metadata,
                { "parent": true, "closure": true },
            );
            let maybe: (() -> trace.Context throws never)? = read;
            assert.equal(
                maybe?.($trace = trace.context(metadata = { "optional": true }))?.metadata,
                { "parent": true, "optional": true },
            );
            assert.equal(trace.current_context().metadata, { "parent": true });
        }
    )($trace = trace.context(metadata = { "parent": true }));
}

test "spawn retains launch context after its IIFE returns" {
    let child = (
        () -> {
            spawn { read($trace = trace.context(metadata = { "child": true })) }
        }
    )($trace = trace.context(metadata = { "version": 1 }));
    (
        () -> {
            let result = await child;
            assert.equal(result.metadata, { "version": 1, "child": true });
            assert.equal(trace.current_context().metadata, { "version": 2 });
        }
    )($trace = trace.context(metadata = { "version": 2 }));
    assert.equal(trace.current_context().metadata.length(), 0);
}

test "null identity inherits and repeated patches replace or remove metadata keys" {
    (
        () -> {
            let options = trace.context(metadata = { "key": 2 }).context(
                metadata = { "key": null, "missing": null },
            );
            assert.equal(read($trace = options).metadata.length(), 0);
            assert.equal(read($trace = options).distinct_id, "parent");
        }
    )($trace = trace.context(metadata = { "key": 1 }, distinct_id = "parent"));
}

class TraceContextModifier {
}

implements<T, E> baml.spawn.Modifier<T, E> for TraceContextModifier {
    function apply(self, plan: baml.spawn.Plan<T, E>) -> baml.spawn.Plan<T, E> throws never {
        (() -> {
            assert.equal(trace.current_context().metadata["phase"], "modifier");
            plan
        })($trace = trace.context(metadata = { "phase": "modifier" }))
    }
}

test "spawn modifier calls do not leak their local context into the launched task" {
    (
        () -> {
            let modifier = TraceContextModifier {};
            let child = spawn with modifier { trace.current_context() };
            assert.equal((await child).metadata["phase"], "parent");
            assert.equal(trace.current_context().metadata["phase"], "parent");
        }
    )($trace = trace.context(metadata = { "phase": "parent" }));
}

test "rooted spawns retain context even though their cancellation parent changes" {
    let child = (
        () -> {
            spawn with baml.spawn.Root.new() { trace.current_context() }
        }
    )($trace = trace.context(metadata = { "version": 1 }));
    (
        () -> {
            assert.equal((await child).metadata["version"], 1);
            assert.equal(trace.current_context().metadata["version"], 2);
        }
    )($trace = trace.context(metadata = { "version": 2 }));
}
