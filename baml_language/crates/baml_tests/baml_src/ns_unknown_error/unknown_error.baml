class UeKnownError {
    message: string,
}

function ue_from_known_preserves_identity() -> bool {
    let source = UeKnownError { message: "known" };
    let converted = baml.errors.UnknownError.from<UeKnownError>(source);
    match (converted) {
        let known: UeKnownError => {
            known.message = "changed";
            source.message == "changed"
        },
        _ => false,
    }
}

test "unknown_error_from_known_preserves_identity" {
    assert.is_true(ue_from_known_preserves_identity())
}

function ue_from_recovers_wrapped_known() -> bool {
    let source = UeKnownError { message: "known" };
    let wrapper = baml.errors.UnknownError { data: source, message: ["boundary"] };
    let converted = baml.errors.UnknownError.from<UeKnownError>(wrapper);
    match (converted) {
        let known: UeKnownError => {
            known.message = "changed";
            source.message == "changed"
        },
        _ => false,
    }
}

test "unknown_error_from_recovers_wrapped_known" {
    assert.is_true(ue_from_recovers_wrapped_known())
}

function ue_from_does_not_double_wrap() -> bool {
    let source = baml.errors.UnknownError { data: "raw", message: ["inner"] };
    let converted = baml.errors.UnknownError.from<UeKnownError>(source);
    match (converted) {
        let wrapper: baml.errors.UnknownError => {
            wrapper.message.push("outer");
            source.message.length() == 2 && source.message.at(1) == "outer"
        },
        _ => false,
    }
}

test "unknown_error_from_does_not_double_wrap" {
    assert.is_true(ue_from_does_not_double_wrap())
}

function ue_with_message_wraps_once() -> bool {
    let first = baml.errors.UnknownError.with_message<UeKnownError>("raw", "inner");
    let second = baml.errors.UnknownError.with_message<UeKnownError>(first, "outer");
    match (second) {
        let wrapper: baml.errors.UnknownError => {
            wrapper.message.length() == 2
                && wrapper.message.at(0) == "inner"
                && wrapper.message.at(1) == "outer"
                && wrapper.data.to_string() == "raw"
        },
        _ => false,
    }
}

test "unknown_error_with_message_wraps_once" {
    assert.is_true(ue_with_message_wraps_once())
}

function ue_with_message_recovers_wrapped_known() -> bool {
    let source = UeKnownError { message: "known" };
    let wrapper = baml.errors.UnknownError { data: source, message: ["boundary"] };
    let converted = baml.errors.UnknownError.with_message<UeKnownError>(wrapper, "outer");
    match (converted) {
        let known: UeKnownError => {
            known.message = "changed";
            source.message == "changed"
        },
        _ => false,
    }
}

test "unknown_error_with_message_recovers_wrapped_known" {
    assert.is_true(ue_with_message_recovers_wrapped_known())
}

function ue_to_string_is_stable() -> string {
    baml.errors.UnknownError { data: "raw", message: ["provider failed", "generation failed"] }
        .to_string()
}

test "unknown_error_to_string_is_stable" {
    assert.equal(ue_to_string_is_stable(), "UnknownError: provider failed: generation failed: raw")
}

function ue_origin() -> bool {
    throw "origin"
}

function ue_normalized() -> bool {
    ue_origin() catch_all (error) {
        _ => throw baml.errors.UnknownError.with_message<never>(error, "boundary failed"),
    }
}

function ue_preserves_origin_trace() -> bool {
    ue_normalized() catch_all (error, context) {
        baml.errors.UnknownError => {
            context.stack_trace.frames.some((frame: baml.errors.StackFrame) -> bool {
                frame.function_name.includes("ue_origin")
            })
        },
    }
}

test "unknown_error_preserves_origin_trace" {
    assert.is_true(ue_preserves_origin_trace())
}

function ue_chained_origin() -> bool {
    throw "root"
}

function ue_chained_middle() -> bool {
    ue_chained_origin() catch (error, context) {
        _ => throw "middle",
    }
}

function ue_chained_normalized() -> bool {
    ue_chained_middle() catch_all (error) {
        _ => throw baml.errors.UnknownError.with_message<never>(error, "outer boundary"),
    }
}

function ue_preserves_cause_chain() -> bool {
    ue_chained_normalized() catch_all (error, context) {
        baml.errors.UnknownError => {
            match (context.root_cause().error) {
                let root: string => root == "root",
                _ => false,
            }
        },
    }
}

test "unknown_error_preserves_cause_chain" {
    assert.is_true(ue_preserves_cause_chain())
}

class UeRootCause {
    message: string,
}

function ue_gc_root() -> bool {
    throw UeRootCause { message: "root" }
}

function ue_gc_middle() -> bool {
    ue_gc_root() catch (error, context) {
        _ => throw "middle",
    }
}

function ue_gc_normalized() -> bool {
    ue_gc_middle() catch_all (error) {
        _ => {
            let normalized = baml.errors.UnknownError.with_message<never>(error, "outer");
            baml.sys.collect_garbage();
            throw normalized
        },
    }
}

function ue_context_survives_gc() -> bool {
    ue_gc_normalized() catch_all (error, context) {
        baml.errors.UnknownError => {
            match (context.root_cause().error) {
                let root: UeRootCause => root.message == "root",
                _ => false,
            }
        },
    }
}

test "unknown_error_context_survives_gc" {
    assert.is_true(ue_context_survives_gc())
}

interface UeRunner {
    function run(self) -> bool throws string
}

class UeRunnerImpl {
    implements UeRunner {
        function run(self) -> bool {
            ue_origin()
        }
    }
}

function ue_interface_boundary(runner: UeRunner) -> bool throws baml.errors.UnknownError {
    runner.run() catch_all (error) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            error,
            "interface dispatch failed",
        )
        },
    }
}

function ue_interface_dispatch_preserves_context() -> bool {
    ue_interface_boundary(UeRunnerImpl {  }) catch_all (error, context) {
        baml.errors.UnknownError => {
            context.stack_trace.frames.some((frame: baml.errors.StackFrame) -> bool {
                frame.function_name.includes("ue_origin")
            })
        },
    }
}

test "unknown_error_interface_dispatch_preserves_context" {
    assert.is_true(ue_interface_dispatch_preserves_context())
}

function ue_callback_origin() -> bool {
    throw "callback origin"
}

function ue_invoke_callback(callback: () -> bool throws unknown) -> bool {
    callback()
}

function ue_callback_boundary(
    callback: () -> bool throws unknown,
) -> bool throws baml.errors.UnknownError {
    ue_invoke_callback(callback) catch_all (error) {
        _ => throw baml.errors.UnknownError.with_message<never>(error, "callback failed"),
    }
}

function ue_callback_preserves_context() -> bool {
    ue_callback_boundary(ue_callback_origin) catch_all (error, context) {
        baml.errors.UnknownError => {
            context.stack_trace.frames.some((frame: baml.errors.StackFrame) -> bool {
                frame.function_name.includes("ue_invoke_callback")
            })
        },
    }
}

test "unknown_error_callback_preserves_context" {
    assert.is_true(ue_callback_preserves_context())
}

function ue_reflect_origin() -> bool {
    throw "reflect origin"
}

function ue_call_reflect(
    target: reflect.AnyFunction<Returns = bool, Throws = unknown>,
) -> bool {
    reflect.call_any(target, {})
}

function ue_reflect_boundary(
    target: reflect.AnyFunction<Returns = bool, Throws = unknown>,
) -> bool throws baml.errors.UnknownError {
    ue_call_reflect(target) catch_all (error) {
        _ => throw baml.errors.UnknownError.with_message<never>(error, "reflect call failed"),
    }
}

function ue_reflect_preserves_context() -> bool {
    let target: reflect.AnyFunction<Returns = bool, Throws = unknown> = ue_reflect_origin;
    ue_reflect_boundary(target) catch_all (error, context) {
        baml.errors.UnknownError => {
            context.stack_trace.frames.some((frame: baml.errors.StackFrame) -> bool {
                frame.function_name.includes("ue_call_reflect")
            })
        },
    }
}

test "unknown_error_reflect_preserves_context" {
    assert.is_true(ue_reflect_preserves_context())
}
