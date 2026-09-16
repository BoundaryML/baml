function greet(name: string, excited: bool = false) -> string throws never {
    return name;
}

function main() -> int throws never {
    let f: reflect.AnyFunction<Returns = string, Throws = never> = greet;
    let missing = reflect.call_any(f, {}) catch (e) {
        reflect.InvalidArgumentError => 1,
        reflect.errors.CompilationError => 100,
    };
    let ty = reflect.call_any(f, { "name": 42 }) catch (e) {
        reflect.InvalidArgumentError => 1,
        reflect.errors.CompilationError => 100,
    };
    let key = reflect.call_any(f, { "name": "x", "volume": 11 }) catch (e) {
        reflect.InvalidArgumentError => 1,
        reflect.errors.CompilationError => 100,
    };
    let opt_ty = reflect.call_any(f, { "name": "x", "excited": "yes" }) catch (e) {
        reflect.InvalidArgumentError => 1,
        reflect.errors.CompilationError => 100,
    };
    let failures = 0;
    if (missing is int) {
        failures = failures + missing
    }
    if (ty is int) {
        failures = failures + ty
    }
    if (key is int) {
        failures = failures + key
    }
    if (opt_ty is int) {
        failures = failures + opt_ty
    }
    return failures;
}

test "call_any_rejects_missing_key_and_type_mismatches" {
    assert.equal(main(), 4)
}
