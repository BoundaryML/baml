class ToolError {
    message: string,
}

function fail_search(q: string) -> string throws ToolError {
    throw ToolError { message: "no results for " + q }
}

function main() -> string throws never {
    let f: reflect.AnyFunction<Returns = string, Throws = ToolError> = fail_search;
    // Exhaustive without a wildcard: all declared channels are named.
    return reflect.call_any(f, { "q": "cats" }) catch (e) {
        ToolError => e.message,
        reflect.InvalidArgumentError => "iae",
        reflect.errors.CompilationError => "compilation-error",
    };
}

test "call_any_propagates_callee_typed_throw" {
    assert.equal(main(), "no results for cats")
}
