function integration_dependency() -> reflect.Package {
    return reflect.Package.compile(
        {
            "dependency.baml": `
                class Counts { hooks int defaults int bodies int }
                interface Runner { function run(self, value: int) -> int throws never; }
                function default_value(counts: Counts) -> int throws never {
                    counts.defaults += 1;
                    17
                }
                function choose(counts: Counts, value: int, settings: trace.Settings) -> trace.Options throws never {
                    counts.hooks += 1;
                    trace.context(metadata = { "selected": true })
                }
                /// baml:$trace=choose
                function target(counts: Counts, value: int = default_value(counts)) -> int throws never {
                    assert.equal(trace.current_context().metadata["selected"], true);
                    assert.equal(counts.hooks, counts.bodies + 1);
                    counts.bodies += 1;
                    value
                }
                function choose_generic<T>(counts: Counts, value: T, settings: trace.Settings) -> trace.Options throws never {
                    counts.hooks += 1;
                    trace.span(inputs = true).context(metadata = { "selected": true })
                }
                /// baml:$trace=choose_generic
                function generic<T>(counts: Counts, value: T) -> T throws never {
                    assert.equal(trace.current_context().metadata["selected"], true);
                    assert.equal(counts.hooks, counts.bodies + 1);
                    counts.bodies += 1;
                    value
                }
                class Worker {
                    counts Counts
                    /// baml:$trace=choose_method
                    function run(self, value: int) -> int throws never {
                        assert.equal(trace.current_context().metadata["selected"], true);
                        assert.equal(self.counts.hooks, self.counts.bodies + 1);
                        self.counts.bodies += 1;
                        value
                    }
                    implements Runner {
                        /// baml:$trace=choose_method
                        function run(self, value: int) -> int throws never {
                            assert.equal(trace.current_context().metadata["selected"], true);
                            assert.equal(self.counts.hooks, self.counts.bodies + 1);
                            self.counts.bodies += 1;
                            value
                        }
                    }
                }
                function choose_method(receiver: Worker, value: int, settings: trace.Settings) -> trace.Options throws never {
                    receiver.counts.hooks += 1;
                    trace.context(metadata = { "selected": true })
                }
                /// baml:$trace=trace.hidden
                function hidden() -> bool throws never { trace.current_span_id() != null }
                /// baml:$trace=trace.timing
                function timing() -> bool throws never { trace.current_span_id() != null }
                /// baml:$trace=trace.span
                function span() -> bool throws never { trace.current_span_id() != null }
                /// baml:$trace=trace.empty_span
                function rich() -> bool throws never { trace.current_span_id() != null }
            `,
        },
    );
}

function integration_consumer(dependency: reflect.Package) -> reflect.Package {
    return reflect.Package.compile(
        {
            "consumer.baml": `
                function call_mode(target: () -> bool throws never, options: trace.Options) -> bool throws never {
                    target($trace = options)
                }
                function run() -> int {
                    let counts = dep.Counts { hooks: 0, defaults: 0, bodies: 0 };
                    assert.equal(dep.target(counts), 17);
                    let indirect = dep.target;
                    assert.equal(indirect(counts, value = 18), 18);
                    assert.equal(dep.generic(counts, "generic"), "generic");
                    let worker = dep.Worker { counts };
                    assert.equal(worker.run(19), 19);
                    let bound = worker.run;
                    assert.equal(bound(20), 20);
                    let virtual: dep.Runner = worker;
                    assert.equal(virtual.run(21), 21);
                    let child = spawn { dep.generic(counts, "child") };
                    assert.equal(await child, "child");
                    assert.equal(counts.hooks, 7);
                    assert.equal(counts.bodies, 7);
                    assert.equal(counts.defaults, 1);
                    assert.equal(dep.hidden($trace = trace.span(inputs = true)), false);
                    assert.equal(dep.timing($trace = trace.span(inputs = true)), false);
                    assert.equal(dep.span($trace = trace.hidden()), true);
                    assert.equal(dep.rich($trace = trace.hidden()), true);
                    assert.equal(call_mode(dep.hidden, trace.span(inputs = true)), false);
                    assert.equal(call_mode(dep.timing, trace.span(inputs = true)), false);
                    assert.equal(call_mode(dep.span, trace.hidden()), true);
                    assert.equal(call_mode(dep.rich, trace.hidden()), true);
                    42
                }
            `,
        },
        packages = { "dep": dependency },
    );
}

test "real mounted packages select hooks for direct generic receiver virtual and spawned calls" {
    let dependency = integration_dependency();
    let consumer = integration_consumer(dependency);
    let run = consumer.get_function<() -> int>("root.run") ?? baml.sys.panic("missing mounted entry");
    assert.equal(run(), 42);
    assert.equal(run(), 42);
}

type IntegrationCallable = () -> bool throws never;

function integration_mount_in_hook() -> trace.Options throws never {
    let package = reflect.Package.compile(
        {
            "late.baml": `
                function choose() -> trace.Options throws never { trace.hidden() }
                /// baml:$trace=choose
                function custom() -> bool throws never { trace.current_span_id() != null }
                /// baml:$trace=trace.hidden
                function builtin() -> bool throws never { trace.current_span_id() != null }
            `,
        },
    ) catch (error) {
        _ => baml.sys.panic("runtime compile failed inside hook")
    };
    let custom = package.get_function<IntegrationCallable>("root.custom") catch (error) {
        _ => baml.sys.panic("custom lookup failed inside hook")
    }
        ?? baml.sys.panic("missing custom");
    let builtin = package.get_function<IntegrationCallable>("root.builtin") catch (error) {
        _ => baml.sys.panic("builtin lookup failed inside hook")
    }
        ?? baml.sys.panic("missing builtin");
    assert.equal(custom($trace = trace.span(inputs = true)), true);
    assert.equal(builtin($trace = trace.span(inputs = true)), true);
    trace.context(metadata = { "late_compiled": true })
}

/// baml:$trace=integration_mount_in_hook
function integration_target_after_mount() -> int {
    assert.equal(trace.current_context().metadata["late_compiled"], true);
    42
}

test "hooks remain suppressed across runtime compilation and newly installed callables" {
    assert.equal(integration_target_after_mount(), 42);
}
