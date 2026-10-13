class OffCounts {
    hooks: int,
    defaults: int,
    bodies: int,
}

function off_default(counts: OffCounts) -> int {
    counts.defaults += 1;
    17
}

function off_hook(
    counts: OffCounts,
    value: int,
    settings: trace.Settings,
) -> trace.Options throws never {
    counts.hooks += 1;
    baml.sys.panic("telemetry-off engine executed a declared hook")
}

/// baml:$trace=off_hook
function off_target(counts: OffCounts, value: int = off_default(counts)) -> int {
    counts.bodies += 1;
    value
}

function off_generic_hook<T>(
    counts: OffCounts,
    value: T,
    settings: trace.Settings,
) -> trace.Options throws never {
    counts.hooks += 1;
    baml.sys.panic("telemetry-off engine executed a generic hook")
}

/// baml:$trace=off_generic_hook
function off_generic<T>(counts: OffCounts, value: T) -> T {
    counts.bodies += 1;
    value
}

class OffReceiver {
    counts: OffCounts,
    /// baml:$trace=off_method_hook
    function run(self, value: int) -> int {
        self.counts.bodies += 1;
        value
    }
}

function off_method_hook(
    receiver: OffReceiver,
    value: int,
    settings: trace.Settings,
) -> trace.Options throws never {
    receiver.counts.hooks += 1;
    baml.sys.panic("telemetry-off engine executed a receiver hook")
}

function off_main() -> int {
    let counts = OffCounts { hooks: 0, defaults: 0, bodies: 0 };
    assert.equal(off_target(counts), 17);
    let callback = off_target;
    assert.equal(callback(counts, value = 18), 18);
    let receiver = OffReceiver { counts };
    let bound = receiver.run;
    assert.equal(bound(19), 19);
    assert.equal(off_generic(counts, "value"), "value");
    let child = spawn { off_target(counts, value = 20) };
    assert.equal(await child, 20);
    assert.equal(off_builtin(counts), 21);
    assert.equal(counts.hooks, 0);
    assert.equal(counts.defaults, 1);
    assert.equal(counts.bodies, 6);
    42
}

/// baml:$trace=trace.empty_span
function off_builtin(counts: OffCounts) -> int {
    counts.bodies += 1;
    assert.equal(trace.current_span_id(), null);
    21
}

test "disabled telemetry skips declared hooks and preserves argument defaults" {
    assert.equal(off_main(), 42);
}
