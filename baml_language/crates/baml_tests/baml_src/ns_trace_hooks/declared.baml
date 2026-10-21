class Counts {
    hooks: int,
    bodies: int,
    defaults: int,
}

function resolved_default(counts: Counts) -> int {
    counts.defaults += 1;
    7
}

function decide(
    counts: Counts,
    value: int,
    settings: trace.Settings,
    unused: int = 9,
) -> trace.Options? throws never {
    counts.hooks += 1;
    assert.equal(value, 7);
    assert.equal(unused, 9);
    assert.equal(settings.mode, trace.Mode.Span);
    assert.equal(settings.capture.inputs, true);
    assert.equal(settings.context.metadata["caller"], "kept");
    settings.context.metadata["snapshot_only"] = "changed";
    assert.equal(trace.current_context().metadata["root"], "inherited");
    assert.equal(trace.current_context().metadata.length(), 1);
    (
        () -> {
            assert.equal(trace.current_context().metadata["hook_local"], "isolated");
        }
    )($trace = trace.context(metadata = { "hook_local": "isolated" }));
    trace.hidden().context(metadata = { "decision": "hidden" })
}

/// baml:$trace=decide
function defaulted(counts: Counts, value: int = resolved_default(counts)) -> int {
    counts.bodies += 1;
    assert.equal(trace.current_context().metadata["decision"], "hidden");
    assert.equal(trace.current_context().metadata["caller"], "kept");
    assert.equal(trace.current_context().metadata["root"], "inherited");
    assert.equal(trace.current_context().metadata.length(), 3);
    value
}

test "declared hooks bind defaults once and isolate context" {
    let counts = Counts { hooks: 0, bodies: 0, defaults: 0 };
    (
        () -> {
            assert.equal(
                defaulted(
                    counts,
                    $trace = trace.span(inputs = true).context(metadata = { "caller": "kept" }),
                ),
                7,
            );
            assert.equal(
                defaulted(
                    value = 7,
                    counts = counts,
                    $trace = trace.span(inputs = true).context(metadata = { "caller": "kept" }),
                ),
                7,
            );
            assert.equal(trace.current_context().metadata.length(), 1);
        }
    )($trace = trace.context(metadata = { "root": "inherited" }));
    assert.equal(counts.hooks, 2);
    assert.equal(counts.bodies, 2);
    assert.equal(counts.defaults, 1);
}

function forbidden_hook(
    counts: Counts,
    n: int,
    original: trace.Settings,
    settings: trace.Settings,
) -> trace.Options? throws never {
    counts.defaults += 1;
    null
}

/// baml:$trace=forbidden_hook
function cycle_hook(
    counts: Counts,
    n: int,
    settings: trace.Settings,
) -> trace.Options? throws never {
    counts.hooks += 1;
    cycle_helper(counts);
    null
}

/// baml:$trace=cycle_hook
function recursive(counts: Counts, n: int) -> int {
    counts.bodies += 1;
    if (n > 0) {
        recursive(counts, n - 1);
    }
    n
}

function cycle_helper(counts: Counts) -> void {
    recursive(counts, 0);
}

test "hook suppression is transitive but ordinary target recursion selects again" {
    let counts = Counts { hooks: 0, bodies: 0, defaults: 0 };
    assert.equal(recursive(counts, 3), 3);
    assert.equal(counts.hooks, 4);
    assert.equal(counts.bodies, 8);
    assert.equal(counts.defaults, 0);
}

function panic_hook(counts: Counts, settings: trace.Settings) -> trace.Options? throws never {
    counts.hooks += 1;
    baml.sys.panic("hook failure")
}

function exit_hook(counts: Counts, settings: trace.Settings) -> trace.Options? throws never {
    counts.hooks += 1;
    baml.sys.exit(7)
}

/// baml:$trace=panic_hook
function after_panic(counts: Counts) -> int {
    counts.bodies += 1;
    11
}

/// baml:$trace=exit_hook
function after_exit(counts: Counts) -> int {
    counts.bodies += 1;
    12
}

test "panic and exit hooks fall back and restore selection" {
    let counts = Counts { hooks: 0, bodies: 0, defaults: 0 };
    assert.equal(after_panic(counts), 11);
    assert.equal(after_exit(counts), 12);
    assert.equal(after_panic(counts), 11);
    assert.equal(counts.hooks, 3);
    assert.equal(counts.bodies, 3);
}

function zero_hook() -> trace.Options throws never {
    trace.hidden().context(metadata = { "zero": true })
}

/// baml:$trace=zero_hook
function zero_target() -> int {
    assert.equal(trace.current_context().metadata["zero"], true);
    1
}

/// baml:$trace=trace.span
function all_default_target() -> int {
    2
}

function optional_hook(marker: string = "default") -> trace.Options throws never {
    trace.context(metadata = { "optional_hook": marker })
}

/// baml:$trace=optional_hook
function optional_target() -> int {
    assert.equal(trace.current_context().metadata["optional_hook"], "default");
    3
}

function generic_hook<T>(
    counts: Counts,
    value: T,
    settings: trace.Settings,
) -> trace.Options? throws never {
    counts.hooks += 1;
    null
}

/// baml:$trace=generic_hook
function generic_target<T>(counts: Counts, value: T) -> T {
    counts.bodies += 1;
    value
}

test "zero all default generic and indirect hooks execute" {
    assert.equal(zero_target(), 1);
    let indirect = zero_target;
    assert.equal(indirect(), 1);
    assert.equal(all_default_target(), 2);
    assert.equal(optional_target(), 3);
    let counts = Counts { hooks: 0, bodies: 0, defaults: 0 };
    assert.equal(generic_target(counts, "text"), "text");
    assert.equal(generic_target(counts, 42), 42);
    let typed = generic_target<string>;
    assert.equal(typed(counts, "indirect"), "indirect");
    assert.equal(counts.hooks, 3);
    assert.equal(counts.bodies, 3);
}

interface Worker {
    function work(self, value: int) -> int throws never;
}

class Desk {
    counts: Counts,

    /// baml:$trace=method_hook
    function direct(self, value: int) -> int {
        self.counts.bodies += 1;
        value
    }

    implements Worker {
        /// baml:$trace=method_hook
        function work(self, value: int) -> int {
            self.counts.bodies += 1;
            value
        }
    }
}

function method_hook(
    desk: Desk,
    value: int,
    settings: trace.Settings,
) -> trace.Options? throws never {
    desk.counts.hooks += 1;
    null
}

function through_interface(worker: Worker) -> int {
    worker.work(3)
}

function through_generic<T extends Worker>(worker: T) -> int {
    worker.work(4)
}

test "receiver interface virtual and native callback calls execute one hook" {
    let counts = Counts { hooks: 0, bodies: 0, defaults: 0 };
    let desk = Desk { counts };
    assert.equal(desk.direct(1), 1);
    let bound = desk.direct;
    assert.equal(bound(2), 2);
    assert.equal(through_interface(desk), 3);
    assert.equal(through_generic(desk), 4);
    assert.equal([5, 6].map(bound), [5, 6]);
    assert.equal(counts.hooks, 6);
    assert.equal(counts.bodies, 6);
}

class SpawnState {
    counts: Counts,
    release: bool,
    done: bool,
}

function delayed_child(state: SpawnState) -> void {
    while (!state.release) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(1));
    }
    recursive(state.counts, 0);
    state.done = true;
}

function async_hook(state: SpawnState, settings: trace.Settings) -> trace.Options? throws never {
    state.counts.hooks += 1;
    let awaited = spawn {
        cycle_helper(state.counts);
    };
    await awaited;
    let detached = spawn {
        delayed_child(state);
    };
    null
}

/// baml:$trace=async_hook
function async_target(state: SpawnState) -> int {
    state.release = true;
    while (!state.done) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(1));
    }
    state.counts.bodies += 1;
    19
}

test "hook suppression survives await and children outliving the hook" {
    let counts = Counts { hooks: 0, bodies: 0, defaults: 0 };
    let state = SpawnState { counts, release: false, done: false };
    let unrelated_counts = Counts { hooks: 0, bodies: 0, defaults: 0 };
    let unrelated = spawn { recursive(unrelated_counts, 0) };
    assert.equal(async_target(state), 19);
    assert.equal(await unrelated, 0);
    assert.equal(counts.hooks, 1);
    assert.equal(counts.bodies, 3);
    assert.equal(unrelated_counts.hooks, 1);
    assert.equal(unrelated_counts.bodies, 2);
    assert.equal(recursive(counts, 0), 0);
    assert.equal(counts.hooks, 2);
}

function context_hook(value: string, settings: trace.Settings) -> trace.Options? throws never {
    if (value == "null") {
        return null;
    }
    trace.hidden().context(
        distinct_id = "hook",
        metadata = { "winner": "hook", "removed": null, "added": true },
    )
}

/// baml:$trace=context_hook
function context_target(value: string) -> trace.Context {
    trace.current_context()
}

test "hook options preserve caller context and reservation context wins" {
    (
        () -> {
            let caller = trace.span(inputs = true).context(
                distinct_id = "caller",
                metadata = { "winner": "caller", "caller": true },
            );
            let ordinary = context_target("options", $trace = caller);
            assert.equal(ordinary.distinct_id, "hook");
            assert.equal(
                ordinary.metadata,
                { "winner": "hook", "caller": true, "added": true, "root": true },
            );
            let unchanged = context_target("null", $trace = caller);
            assert.equal(unchanged.distinct_id, "caller");
            assert.equal(
                unchanged.metadata,
                { "winner": "caller", "caller": true, "root": true, "removed": true },
            );
            let reserved = context_target("options", $trace = caller.reserve());
            assert.equal(reserved.distinct_id, "caller");
            assert.equal(
                reserved.metadata,
                { "winner": "caller", "caller": true, "added": true, "root": true },
            );
        }
    )($trace = trace.context(metadata = { "root": true, "removed": true }));
}

function failing_default(counts: Counts) -> int throws string {
    counts.defaults += 1;
    throw "argument preparation";
}

function preparation_hook(
    counts: Counts,
    value: int,
    settings: trace.Settings,
) -> trace.Options? throws never {
    counts.hooks += 1;
    null
}

/// baml:$trace=preparation_hook
function failing_target(counts: Counts, value: int = failing_default(counts)) -> int {
    counts.bodies += 1;
    value
}

test "failed argument defaults do not execute a hook or target body" {
    let counts = Counts { hooks: 0, bodies: 0, defaults: 0 };
    let result = failing_target(counts) catch (error) {
        string => 99
    };
    assert.equal(result, 99);
    assert.equal(counts.defaults, 1);
    assert.equal(counts.hooks, 0);
    assert.equal(counts.bodies, 0);
    assert.equal(recursive(counts, 0), 0);
    assert.equal(counts.hooks, 1);
}
