function pick(mode: "auto" | "manual", level: 1 | 2, on: true) -> string throws never {
    `${mode}|${level}|${on}`
}

function describe(e: reflect.InvalidArgumentError) -> string throws never {
    e.argument + ":" + e.expected.to_string() + "|" + e.got.to_string()
}

function main() -> string throws never {
    let f: reflect.AnyFunction<Returns = string, Throws = never> = pick;
    let fits = reflect.call_any(f, { "mode": "manual", "level": 2, "on": true }) catch (e) {
        reflect.InvalidArgumentError => describe(e),
        reflect.errors.CompilationError => "unexpected-compilation-error",
    };
    let other_string = reflect.call_any(f, { "mode": "sometimes", "level": 2, "on": true }) catch (e) {
        reflect.InvalidArgumentError => describe(e),
        reflect.errors.CompilationError => "unexpected-compilation-error",
    };
    let other_int = reflect.call_any(f, { "mode": "auto", "level": 3, "on": true }) catch (e) {
        reflect.InvalidArgumentError => describe(e),
        reflect.errors.CompilationError => "unexpected-compilation-error",
    };
    `${fits}/${other_string}/${other_int}`
}

test "call_any_accepts_a_value_of_a_literal_type" {
    assert.equal(main(), `manual|2|true/mode:"auto" | "manual"|string/level:1 | 2|int`)
}
