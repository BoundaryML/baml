function greet(name: string) -> string throws never {
    return name;
}

function main() -> string throws never {
    let f: reflect.AnyFunction<Returns = string, Throws = never> = greet;
    let bad_value = reflect.call_any(f, { "name": 42 }) catch (e) {
        reflect.InvalidArgumentError => {
            e.argument + ":" + e.expected.to_string() + "|" + e.got.to_string()
        },
        reflect.errors.CompilationError => "unexpected-compilation-error",
    };
    let missing = reflect.call_any(f, {}) catch (e) {
        reflect.InvalidArgumentError => {
            e.argument + ":" + e.expected.to_string() + "|" + e.got.to_string()
        },
        reflect.errors.CompilationError => "unexpected-compilation-error",
    };
    let out = "?";
    if (bad_value is string) {
        out = bad_value
    }
    if (missing is string) {
        out = out + "/" + missing
    }
    return out;
}

test "call_any_invalid_argument_error_carries_types" {
    assert.equal(main(), "name:string|int/name:string|never")
}
