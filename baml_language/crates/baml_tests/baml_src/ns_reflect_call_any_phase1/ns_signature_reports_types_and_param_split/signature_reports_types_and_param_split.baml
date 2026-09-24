class ToolError {
    message: string,
}

/// Searches the index.
function search(q: string, limit: int = 10) -> string[] throws ToolError {
    if (q == "boom") {
        throw ToolError { message: "x" }
    }
    return [q];
}

function main() -> string throws never {
    let f: reflect.AnyFunction = search;
    let sig = reflect.signature(f);
    let limit = sig.opts.get("limit");
    let limit_str = "absent";
    if (limit != null) {
        limit_str = limit.type.to_string()
    }
    let fn_name = "unnamed";
    let sn = sig.name;
    if (sn != null) {
        fn_name = sn
    }
    let q_name = sig.args[0].name;
    let doc = "no-doc";
    let d = sig.docstring;
    if (d != null) {
        doc = d
    }
    return fn_name
        + "|"
        + sig.returns.to_string()
        + "|"
        + sig.errors.to_string()
        + "|"
        + sig.args.length().to_string()
        + "|"
        + sig.args[0].type.to_string()
        + "|"
        + q_name
        + "|"
        + limit_str
        + "|"
        + doc;
}

test "signature_reports_types_and_param_split" {
    let result = main();
    assert.is_true(result.starts_with("search|string[]|"));
    assert.contains(result, "ToolError|1|string|q|int|Searches the index.")
}
