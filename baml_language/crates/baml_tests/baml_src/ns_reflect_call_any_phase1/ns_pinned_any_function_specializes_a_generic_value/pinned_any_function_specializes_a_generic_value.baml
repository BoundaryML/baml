// A pinned `AnyFunction` is a context that determines a generic function
// value's type parameter (`Returns = string` pins `T`), so `f` IS
// `ident<string>`: it carries its instantiation and `call_any` runs it. An
// unspecialized generic reaching reflection is refused instead (see
// `get_function_refuses_an_unspecialized_generic_*`).

function ident<T>(x: T) -> T throws never {
    return x;
}

function main() -> string throws never {
    let f: reflect.AnyFunction<Returns = string, Throws = never> = ident;
    reflect.call_any(f, { "x": "value" }) catch (e) {
        _ => "unexpected error",
    }
}

test "pinned_any_function_specializes_a_generic_value" {
    assert.equal(main(), "value")
}
