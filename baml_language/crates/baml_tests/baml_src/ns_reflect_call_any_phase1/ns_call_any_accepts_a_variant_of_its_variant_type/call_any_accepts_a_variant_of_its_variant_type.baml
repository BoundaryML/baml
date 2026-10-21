enum Kind {
    GARAGE,
    COMMERCIAL,
}

function only_garage(kind: Kind.GARAGE) -> string throws never {
    "garage"
}

// The types of the mismatch render with this file's namespace, so the test
// reads only which argument was rejected.
function describe(e: reflect.InvalidArgumentError) -> string throws never {
    "rejected:" + e.argument
}

function main() -> string throws never {
    let f: reflect.AnyFunction<Returns = string, Throws = never> = only_garage;
    let fits = reflect.call_any(f, { "kind": Kind.GARAGE }) catch (e) {
        reflect.InvalidArgumentError => describe(e),
        reflect.errors.CompilationError => "unexpected-compilation-error",
    };
    let other_variant = reflect.call_any(f, { "kind": Kind.COMMERCIAL }) catch (e) {
        reflect.InvalidArgumentError => describe(e),
        reflect.errors.CompilationError => "unexpected-compilation-error",
    };
    `${fits}/${other_variant}`
}

test "call_any_accepts_a_variant_of_its_variant_type" {
    assert.equal(main(), `garage/rejected:kind`)
}
