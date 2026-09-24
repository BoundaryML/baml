function ident<T>(x: T) -> T throws never {
    return x;
}

function main() -> string throws never {
    let f: reflect.AnyFunction<Returns = string, Throws = never> = ident;
    let _ = reflect.call_any(f, { "x": "value" }) catch (e) {
        reflect.errors.CompilationError => {
            return e.diagnostics[0].code + "|" + e.diagnostics[0].message;
        },
        _ => {
            return "wrong error";
        },
    };
    return "generic call unexpectedly succeeded";
}

test "pinned_call_any_declares_unspecialized_generic_compilation_error" {
    assert.equal(
        main(),
        "E0165|generic function `ident` cannot be extracted through reflection: its signature still mentions its own type parameters",
    )
}
