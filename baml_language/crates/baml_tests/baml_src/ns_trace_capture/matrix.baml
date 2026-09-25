function matrix_echo(value: unknown) -> unknown {
    value
}

function matrix_fail(value: unknown) -> unknown throws unknown {
    throw value;
}

function matrix_case(
    value: unknown,
    inputs: bool?,
    output: bool?,
    error: bool?,
    reserved: bool,
    fail: bool,
) -> unknown throws unknown {
    let options = trace.span(inputs = inputs, output = output, error = error);
    let attachment = if (reserved) { options.reserve() } else { options };
    if (fail) {
        matrix_fail(value, $trace = attachment)
    } else {
        matrix_echo(value, $trace = attachment)
    }
}

function matrix_observed(value: string, expected_span: bool) -> string {
    assert.equal(trace.current_span_id() != null, expected_span);
    value
}

function matrix_mode(mode: string, expected_span: bool, reserved: bool) -> string {
    let options = trace.span(inputs = true, output = true, error = true);
    let configured = match (mode) {
        "hidden" => options.mode(trace.Mode.Hidden),
        "timing" => options.mode(trace.Mode.Timing),
        "default" => options.mode(null),
        _ => options,
    };
    let attachment = if (reserved) { configured.reserve() } else { configured };
    matrix_observed(mode, expected_span, $trace = attachment)
}

function matrix_default(value: string? = "default") -> string? {
    value
}

function matrix_omitted() -> string? {
    matrix_default($trace = trace.span(inputs = true, output = true))
}

function matrix_explicit_null() -> string? {
    matrix_default(value = null, $trace = trace.span(inputs = true, output = true))
}

function matrix_named_target(first: string, second: string) -> string {
    first + second
}

function matrix_named() -> string {
    matrix_named_target(
        second = "second",
        $trace = trace.span(inputs = true, output = true),
        first = "first",
    )
}

function matrix_no_args_target() -> int {
    42
}

function matrix_no_args() -> int {
    matrix_no_args_target($trace = trace.span(inputs = true, output = true))
}

function matrix_caught_target(value: string) -> unknown {
    let caught = matrix_fail(value) catch (error) {
        _ => "caught",
    };
    caught
}

function matrix_caught() -> unknown {
    matrix_caught_target("inside", $trace = trace.span(inputs = true, output = true, error = true))
}

function matrix_parent(value: string) -> unknown {
    matrix_echo("child", $trace = trace.span(inputs = true, output = true))
}

function matrix_nested(hidden: bool) -> unknown {
    let options = if (hidden) { trace.hidden() } else { trace.span(inputs = true, output = true) };
    matrix_parent("parent", $trace = options)
}

function matrix_spawned() -> unknown {
    let reserved = trace.span(inputs = true, output = true).reserve();
    let child = spawn { matrix_echo("spawned", $trace = reserved) };
    await child
}

function matrix_reused(deep_copy: bool) -> unknown {
    let reserved = trace.span(inputs = true, output = true).reserve();
    let alias = if (deep_copy) { baml.deep_copy(reserved) } else { reserved };
    matrix_echo("first", $trace = reserved);
    let rejected = {
        matrix_echo("rejected", $trace = alias);
        false
    } catch_all_panics (error) {
        _ => true,
    };
    assert.equal(rejected, true);
    assert.equal(matrix_echo("untraced after reuse"), "untraced after reuse");
    matrix_echo("later", $trace = trace.span(inputs = true, output = true))
}

class MatrixNode {
    value: int,
    next: MatrixNode?,
}

function matrix_mutate(node: MatrixNode) -> MatrixNode {
    node.value = 2;
    node
}

function matrix_cycle(cycle: bool) -> int {
    let node = MatrixNode { value: 1, next: null };
    if (cycle) {
        node.next = node;
    }
    let result = matrix_mutate(node, $trace = trace.span(inputs = true, output = true));
    result.value = 3;
    result.value
}

enum MatrixChoice {
    First,
    Second,
}

function matrix_enum() -> unknown {
    matrix_echo(MatrixChoice.Second, $trace = trace.span(inputs = true, output = true))
}

function matrix_aliases() -> unknown {
    let shared: int[] = [1, 2];
    let values: int[][] = [shared, shared];
    matrix_echo(values, $trace = trace.span(inputs = true, output = true))
}

function matrix_map(empty: bool) -> unknown {
    let values: map<string, int> = {};
    if (!empty) {
        values["first"] = 1;
        values["second"] = 2;
    }
    matrix_echo(values, $trace = trace.span(inputs = true, output = true))
}

function matrix_panic_target(value: string) -> string {
    baml.sys.panic(value)
}

function matrix_panic(error: bool?) -> string {
    matrix_panic_target("captured panic", $trace = trace.span(inputs = true, error = error))
}

function matrix_cancel_target(value: string) -> string {
    log.info("ready to cancel");
    baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n));
    value
}

function matrix_cancel(error: bool?) -> string {
    matrix_cancel_target("cancelled input", $trace = trace.span(inputs = true, error = error))
}

class MatrixMethod {
    prefix: string,

    function echo(self, value: string) -> string {
        self.prefix + value
    }
}

function matrix_method(bound: bool) -> string {
    let receiver = MatrixMethod { prefix: "method:" };
    if (bound) {
        let call = receiver.echo;
        call("input", $trace = trace.span(inputs = true, output = true))
    } else {
        receiver.echo("input", $trace = trace.span(inputs = true, output = true))
    }
}

function matrix_closure() -> string {
    let prefix = "closure:";
    let call = (value: string) -> string { prefix + value };
    call("input", $trace = trace.span(inputs = true, output = true))
}

function matrix_optional_target(value: string) -> string {
    value
}

function matrix_optional_call(call: ((value: string) -> string)?) -> string? {
    call?.("optional", $trace = trace.span(inputs = true, output = true))
}

function matrix_optional(present: bool) -> string? {
    let call = if (present) { matrix_optional_target } else { null };
    matrix_optional_call(call)
}

function matrix_empty_array() -> unknown {
    let values: int[] = [];
    matrix_echo(values, $trace = trace.span(inputs = true, output = true))
}

class MatrixBox<T> {
    value: T,
}

function matrix_generic() -> unknown {
    let value = MatrixBox<int> { value: 7 };
    matrix_echo(value, $trace = trace.span(inputs = true, output = true))
}

function matrix_union_target(value: int | string) -> int | string {
    value
}

function matrix_union(use_string: bool) -> int | string {
    let value: int | string = if (use_string) { "union" } else { 7 };
    matrix_union_target(value, $trace = trace.span(inputs = true, output = true))
}

function matrix_opaque() -> int {
    matrix_echo(trace.options(), $trace = trace.span(inputs = true, output = true));
    1
}

function matrix_llm(value: string) -> string {
    client: "openai-chat/test"
    prompt: `${value}`
}

function matrix_llm_policy(base_url: string, capture: bool?) -> string {
    let cl = openai.ChatClient.new(api_key = "test", base_url = base_url);
    matrix_llm(
        "llm input",
        client = cl,
        $trace = trace.span(inputs = capture, output = capture, error = capture),
    )
}

function matrix_native_rejected(call: () -> trace.SpanId?) -> unknown {
    let rejected = {
        call($trace = trace.span(inputs = true, output = true));
        false
    } catch_all_panics (error) {
        _ => true,
    };
    assert.equal(rejected, true);
    assert.equal(matrix_echo("untraced after native"), "untraced after native");
    matrix_echo("native rejected", $trace = trace.span(inputs = true, output = true))
}

function matrix_native_cleanup() -> unknown {
    matrix_native_rejected(trace.current_span_id)
}
