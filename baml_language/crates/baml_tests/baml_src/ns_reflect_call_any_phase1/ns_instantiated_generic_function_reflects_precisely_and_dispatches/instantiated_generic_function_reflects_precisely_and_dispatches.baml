function ident<T>(x: T) -> T throws never {
    return x;
}

function main() -> string throws never {
    // An instantiated generic callable carries its realized frame, so its
    // signature reconstructs at that instantiation rather than coarsely.
    let f: reflect.AnyFunction = ident<int>;
    let sig = reflect.signature(f);
    let r = reflect.call_any(f, { "x": 42 }) catch (e) {
        _ => -1
    };
    let called = "no";
    if (r is int) {
        called = r.to_string()
    }
    return sig.returns.to_string() + "|" + sig.args.length().to_string() + "|" + called;
}

test "instantiated_generic_function_reflects_precisely_and_dispatches" {
    assert.equal(main(), "int|1|42")
}
