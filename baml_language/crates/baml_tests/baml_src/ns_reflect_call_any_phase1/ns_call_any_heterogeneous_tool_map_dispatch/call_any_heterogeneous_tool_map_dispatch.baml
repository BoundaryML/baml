class ToolError {
    message: string,
}

function shout(s: string) -> string throws never {
    return s + "!";
}

function fail(s: string) -> string throws ToolError {
    throw ToolError { message: s }
}

function dispatch(
    tools: map<string, reflect.AnyFunction<Returns = string, Throws = ToolError>>,
    name: string,
    args: map<string, unknown>,
) -> string throws never {
    let f = tools.get(name);
    if (f is null) {
        return "no-such-tool";
    }
    return reflect.call_any(f, args) catch (e) {
        ToolError => "err:" + e.message,
        reflect.InvalidArgumentError => "iae",
        reflect.errors.CompilationError => "compilation-error",
    };
}

function main() -> string throws never {
    let tools: map<string, reflect.AnyFunction<Returns = string, Throws = ToolError>> = { "shout": shout, "fail": fail };
    return dispatch(tools, "shout", { "s": "hi" })
        + "/"
        + dispatch(tools, "fail", { "s": "down" })
        + "/"
        + dispatch(tools, "nope", {});
}

test "call_any_heterogeneous_tool_map_dispatch" {
    assert.equal(main(), "hi!/err:down/no-such-tool")
}
