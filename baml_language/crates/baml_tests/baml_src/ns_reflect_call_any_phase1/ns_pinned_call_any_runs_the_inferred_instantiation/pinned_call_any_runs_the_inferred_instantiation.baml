// The pins of the expected `AnyFunction` (`Returns = string`) determine
// `ident`'s `T`, so the reference is the realized value `ident<string>`, not
// an unspecialized generic: reflection dispatches it like the written
// spelling. An unspecialized generic is refused by the `get_function` tests.
function ident<T>(x: T) -> T throws never {
    return x;
}

function main() -> string throws never {
    let f: reflect.AnyFunction<Returns = string, Throws = never> = ident;
    reflect.call_any(f, { "x": "value" }) catch (e) {
        reflect.errors.CompilationError => "unexpected compilation error",
        _ => "wrong error",
    }
}

test "pinned_call_any_runs_the_inferred_instantiation" {
    assert.equal(main(), "value")
}
