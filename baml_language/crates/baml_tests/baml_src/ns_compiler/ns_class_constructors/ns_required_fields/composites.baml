// Nested classes, containers, and aliases.

test "missing required nested class becomes null" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Inner { name: string }
                class Outer { id: int, inner: Inner }
                function run() -> string { baml.json.to_string(Outer { id: 1 }) }
            `,
        ),
        "{\"id\":1,\"inner\":null}",
    )
}

test "missing field inside present nested class becomes null" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Inner { name: string, count: int }
                class Outer { id: int, inner: Inner }
                function run() -> string {
                    baml.json.to_string(Outer { id: 1, inner: Inner { name: "present" } })
                }
            `,
        ),
        "{\"id\":1,\"inner\":{\"name\":\"present\",\"count\":null}}",
    )
}

test "missing list and map fields become null not empty" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Collections { numbers: int[], lookup: map<string, bool> }
                function run() -> string { baml.json.to_string(Collections {}) }
            `,
        ),
        "{\"numbers\":null,\"lookup\":null}",
    )
}

test "nullable alias and non nullable alias omissions look identical" {
    assert.equal(
        RunCompiledConstructor(
            `
                type MaybeText = string?
                type RequiredText = string
                class Aliased { maybe: MaybeText, required: RequiredText }
                function run() -> string { baml.json.to_string(Aliased {}) }
            `,
        ),
        "{\"maybe\":null,\"required\":null}",
    )
}

test "nullable nested class is the valid control" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Inner { name: string }
                class Outer { id: int, inner: Inner? }
                function run() -> string { baml.json.to_string(Outer { id: 1 }) }
            `,
        ),
        "{\"id\":1,\"inner\":null}",
    )
}

test "required null field may be omitted" {
    assert.equal(
        RunCompiledConstructor(
            `
                class NullHolder { value: null }
                function run() -> string { baml.json.to_string(NullHolder {}) }
            `,
        ),
        "{\"value\":null}",
    )
}
