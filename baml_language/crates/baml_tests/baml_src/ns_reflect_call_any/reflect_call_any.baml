class ToolError {
    message: string,
}

class Counter {
    base: int,

    function bump(self, amount: int) -> int throws never {
        self.base + amount
    }
}

function add(x: int, y: int) -> int throws never {
    x + y
}

function scale(x: int, factor: int = x + 1) -> int throws never {
    x * factor
}

function greet(name: string, excited: bool = false) -> string throws never {
    name
}

function name_only(name: string) -> string throws never {
    name
}

function fail_search(q: string) -> string throws ToolError {
    throw ToolError { message: "no results for " + q }
}

function shout(s: string) -> string throws never {
    s + "!"
}

function fail(s: string) -> string throws ToolError {
    throw ToolError { message: s }
}

function dispatch(
    tools: map<string, baml.AnyFunction<Returns = string, Throws = ToolError>>,
    name: string,
    args: map<string, unknown>,
) -> string throws never {
    let f = tools.get(name);
    if (f == null) {
        return "no-such-tool";
    }
    reflect.call_any(f, args) catch (e) {
        ToolError => "err:" + e.message,
        reflect.InvalidArgumentError => "iae",
    }
}

/// Searches the index.
function search(q: string, limit: int = 10) -> string[] throws ToolError {
    if (q == "boom") {
        throw ToolError { message: "x" }
    }
    return [q];
}

function ident<T>(x: T) -> T throws never {
    x
}

function call_any_dispatches_named_args_fn() -> int throws never {
    let f: baml.AnyFunction<Returns = int, Throws = never> = add;
    reflect.call_any(f, { "x": 20, "y": 22 }) catch (e) {
        _ => -1,
    }
}

test "call_any_dispatches_named_args" {
    assert.equal(call_any_dispatches_named_args_fn(), 42)
}

function call_any_absent_optional_fires_callee_default_fn() -> int throws never {
    let f: baml.AnyFunction<Returns = int, Throws = never> = scale;
    let defaulted = reflect.call_any(f, { "x": 5 }) catch (e) {
        _ => -1,
    };
    let supplied = reflect.call_any(f, { "x": 5, "factor": 10 }) catch (e) {
        _ => -1,
    };
    defaulted * 1000 + supplied
}

test "call_any_absent_optional_fires_callee_default" {
    assert.equal(call_any_absent_optional_fires_callee_default_fn(), 30050)
}

function call_any_rejects_missing_key_and_type_mismatches_fn() -> int throws never {
    let f: baml.AnyFunction<Returns = string, Throws = never> = greet;
    let missing = reflect.call_any(f, {}) catch (e) {
        reflect.InvalidArgumentError => 1,
    };
    let ty = reflect.call_any(f, { "name": 42 }) catch (e) {
        reflect.InvalidArgumentError => 1,
    };
    let key = reflect.call_any(f, { "name": "x", "volume": 11 }) catch (e) {
        reflect.InvalidArgumentError => 1,
    };
    let opt_ty = reflect.call_any(f, { "name": "x", "excited": "yes" }) catch (e) {
        reflect.InvalidArgumentError => 1,
    };
    let failures = 0;
    if (missing is int) {
        failures += missing
    }
    if (ty is int) {
        failures += ty
    }
    if (key is int) {
        failures += key
    }
    if (opt_ty is int) {
        failures += opt_ty
    }
    failures
}

test "call_any_rejects_missing_key_and_type_mismatches" {
    assert.equal(call_any_rejects_missing_key_and_type_mismatches_fn(), 4)
}

function call_any_invalid_argument_error_carries_types_fn() -> string throws never {
    let f: baml.AnyFunction<Returns = string, Throws = never> = name_only;
    let bad_value = reflect.call_any(f, { "name": 42 }) catch (e) {
        reflect.InvalidArgumentError => {
            e.argument + ":" + e.expected.to_string() + "|" + e.got.to_string()
        },
    };
    let missing = reflect.call_any(f, {}) catch (e) {
        reflect.InvalidArgumentError => {
            e.argument + ":" + e.expected.to_string() + "|" + e.got.to_string()
        },
    };
    let out = "?";
    if (bad_value is string) {
        out = bad_value
    }
    if (missing is string) {
        out = out + "/" + missing
    }
    out
}

test "call_any_invalid_argument_error_carries_types" {
    assert
        .equal(
            call_any_invalid_argument_error_carries_types_fn(),
            "name:string|int/name:string|never",
        )
}

function call_any_propagates_callee_typed_throw_fn() -> string throws never {
    let f: baml.AnyFunction<Returns = string, Throws = ToolError> = fail_search;
    reflect.call_any(f, { "q": "cats" }) catch (e) {
        ToolError => e.message,
        reflect.InvalidArgumentError => "iae",
    }
}

test "call_any_propagates_callee_typed_throw" {
    assert.equal(call_any_propagates_callee_typed_throw_fn(), "no results for cats")
}

function call_any_heterogeneous_tool_map_dispatch_fn() -> string throws never {
    let tools: map<string, baml.AnyFunction<Returns = string, Throws = ToolError>> = { "shout": shout, "fail": fail };
    dispatch(tools, "shout", { "s": "hi" })
        + "/"
        + dispatch(tools, "fail", { "s": "down" })
        + "/"
        + dispatch(tools, "nope", {})
}

test "call_any_heterogeneous_tool_map_dispatch" {
    assert.equal(call_any_heterogeneous_tool_map_dispatch_fn(), "hi!/err:down/no-such-tool")
}

function call_any_lambda_callee_fn() -> int throws never {
    let double = (x: int) -> int throws never {
        x * 2
    };
    let f: baml.AnyFunction<Returns = int, Throws = never> = double;
    reflect.call_any(f, { "x": 21 }) catch (e) {
        _ => -1,
    }
}

test "call_any_lambda_callee" {
    assert.equal(call_any_lambda_callee_fn(), 42)
}

function call_any_bound_method_callee_fn() -> int throws never {
    let c = Counter { base: 40 };
    let m: baml.AnyFunction<Returns = int, Throws = never> = c.bump;
    reflect.call_any(m, { "amount": 2 }) catch (e) {
        _ => -1,
    }
}

test "call_any_bound_method_callee" {
    assert.equal(call_any_bound_method_callee_fn(), 42)
}

function signature_reports_types_and_param_split_fn() -> string throws never {
    let f: baml.AnyFunction = search;
    let sig = reflect.signature(f);
    let limit = sig.opts.get("limit");
    let limit_str = "absent";
    if (limit != null) {
        limit_str = limit.type.to_string()
    }
    let fn_name = "unnamed";
    let signature_name = sig.name;
    if (signature_name != null) {
        fn_name = signature_name
    }
    let doc = "no-doc";
    let signature_docstring = sig.docstring;
    if (signature_docstring != null) {
        doc = signature_docstring
    }
    fn_name
        + "|"
        + sig.returns.to_string()
        + "|"
        + sig.errors.to_string()
        + "|"
        + sig.args.length().to_string()
        + "|"
        + sig.args[0].type.to_string()
        + "|"
        + sig.args[0].name
        + "|"
        + limit_str
        + "|"
        + doc
}

test "signature_reports_types_and_param_split" {
    assert
        .equal(
            signature_reports_types_and_param_split_fn(),
            "search|string[]|reflect_call_any.ToolError|1|string|q|int|Searches the index.",
        )
}

function signature_bound_method_drops_receiver_fn() -> string throws never {
    let c = Counter { base: 40 };
    let m: baml.AnyFunction = c.bump;
    let sig = reflect.signature(m);
    let method_name = "unnamed";
    let signature_name = sig.name;
    if (signature_name != null) {
        method_name = signature_name
    }
    let lambda_name = "some";
    if (
        reflect
            .signature((x: int) -> int throws never {
                x
            })
            .name
            == null
    ) {
        lambda_name = "null"
    }
    method_name
        + "|"
        + lambda_name
        + "|"
        + sig.args.length().to_string()
        + "|"
        + sig.returns.to_string()
        + "|"
        + sig.errors.to_string()
}

test "signature_bound_method_drops_receiver" {
    assert.equal(signature_bound_method_drops_receiver_fn(), "bump|null|1|int|never")
}

function generic_function_reflects_coarsely_and_dispatches_fn() -> string throws never {
    let f: baml.AnyFunction = ident;
    let sig = reflect.signature(f);
    let result = reflect.call_any(f, { "x": 42 }) catch (e) {
        _ => -1,
    };
    let called = "no";
    if (result is int) {
        called = result.to_string()
    }
    sig.returns.to_string() + "|" + sig.args.length().to_string() + "|" + called
}

test "generic_function_reflects_coarsely_and_dispatches" {
    assert.equal(generic_function_reflects_coarsely_and_dispatches_fn(), "unknown|1|42")
}

function signature_object_literal_construction_fn() -> string throws never {
    let manual = reflect.Signature {
        args: [],
        opts: {},
        returns: reflect.type_of<int>(),
        errors: reflect.type_of<never>(),
        docstring: null,
    };
    manual.returns.to_string() + "|" + manual.errors.to_string()
}

test "signature_object_literal_construction" {
    assert.equal(signature_object_literal_construction_fn(), "int|never")
}
