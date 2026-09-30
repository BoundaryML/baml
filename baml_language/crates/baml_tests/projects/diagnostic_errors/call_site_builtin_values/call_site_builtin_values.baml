// A few builtins are compiled into their call sites (`log.info` into a log
// statement, `baml.spawn.__spawn` into the task launch, ...) and have no
// function value. Any reference to one that is not a direct call is refused,
// rather than compiling to a reference to nothing.

function apply(f: (unknown) -> void throws never) -> void {
    f("value")
}

function bound_to_a_local() -> void {
    let l = log.info;
    l("value")
}

function passed_as_an_argument() -> void {
    apply(log.info)
}

function returned() -> (unknown) -> void throws never {
    log.info
}

function as_a_parameter_default(f: (unknown) -> void throws never = log.warn) -> void {
    f("value")
}

function inside_a_lambda() -> void {
    let g = () -> void {
        let l = log.debug;
        l("value")
    };
    g()
}

// An optional call evaluates its callee as a value first.
function optional_call() -> void {
    log.info?.("value");
}

function specialized_spawn() -> baml.future.Future<int, never> {
    let launch = baml.spawn.__spawn<int, never>;
    launch(baml.spawn.Plan.new(body = () -> int { 1 }))
}

function specialized_await_any(futures: baml.future.Future<int, never>[]) -> int {
    let first = baml.future.__await_any<int, never>;
    first(futures)
}

function current_package() -> reflect.Package {
    let current = reflect.Package.current;
    current()
}

// Direct calls are what these builtins are for; parentheses around the callee
// do not change that.
function direct_calls() -> reflect.Package {
    log.info("direct");
    (log.info)("parenthesized");
    reflect.Package.current()
}
