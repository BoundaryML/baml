function success(value: string) -> string {
    value + ":output"
}

function failure(value: string) -> string throws string {
    throw value + ":error";
}

function current() -> trace.SpanId? {
    trace.current_span_id()
}

function capture_success(value: string) -> string {
    let reserved = trace.span().reserve();
    assert.equal(current($trace = reserved) == reserved.id(), true);
    success(value, $trace = trace.span(inputs = true, output = true, error = true))
}

function capture_failure(value: string) -> string throws string {
    let reserved = trace.span(inputs = true, output = true, error = true).reserve();
    failure(value, $trace = reserved)
}

function no_capture(value: string) -> string {
    success(value, $trace = trace.span(inputs = false, output = false, error = false))
}

test "trace capture preserves program behavior" {
    assert.equal(capture_success("input"), "input:output");
    assert.equal(no_capture("private"), "private:output");
    let caught = capture_failure("input") catch (error) {
        string => error,
    };
    assert.equal(caught, "input:error");
}
