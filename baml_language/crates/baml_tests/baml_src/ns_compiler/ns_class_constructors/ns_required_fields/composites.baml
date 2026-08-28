// Nested classes, containers, and aliases.

test "missing required nested class is rejected" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Inner { name: string }
                class Outer { id: int, inner: Inner }
                function run() -> string { baml.json.to_string(Outer { id: 1 }) }
            `,
            ["class `Outer` is missing required field", "`inner`"],
        ),
    )
}

test "missing field inside present nested class is rejected" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Inner { name: string, count: int }
                class Outer { id: int, inner: Inner }
                function run() -> string {
                    baml.json.to_string(Outer { id: 1, inner: Inner { name: "present" } })
                }
            `,
            ["class `Inner` is missing required field", "`count`"],
        ),
    )
}

test "missing list and map fields are rejected" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Collections { numbers: int[], lookup: map<string, bool> }
                function run() -> string { baml.json.to_string(Collections {}) }
            `,
            ["class `Collections` is missing required fields", "`numbers`", "`lookup`"],
        ),
    )
}

test "nullable alias may be omitted but non nullable alias may not" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                type MaybeText = string?
                type RequiredText = string
                class Aliased { maybe: MaybeText, required: RequiredText }
                function run() -> string { baml.json.to_string(Aliased {}) }
            `,
            ["class `Aliased` is missing required field", "`required`"],
        ),
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
