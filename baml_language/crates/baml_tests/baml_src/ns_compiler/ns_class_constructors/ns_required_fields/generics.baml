// Generic constructors: explicit arguments, inferred arguments, multiple
// parameters, nesting, and nullable controls.

test "explicit concrete generic fields are required" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Box<T> { value: T, label: string }
                function run() -> string { baml.json.to_string(Box<string> {}) }
            `,
            ["class `Box` is missing required fields", "`value`", "`label`"],
        ),
    )
}

test "inferred generic sibling field is required" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Pair<T> { left: T, right: T }
                function run() -> string { baml.json.to_string(Pair { left: "left" }) }
            `,
            ["class `Pair` is missing required field", "`right`"],
        ),
    )
}

test "second concrete type parameter is required" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Two<A, B> { first: A, second: B }
                function run() -> string { baml.json.to_string(Two<string, int> { first: "first" }) }
            `,
            ["class `Two` is missing required field", "`second`"],
        ),
    )
}

test "nested generic field is required" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Box<T> { value: T }
                class Envelope<T> { id: int, payload: T }
                function run() -> string { baml.json.to_string(Envelope<Box<int>> { id: 1 }) }
            `,
            ["class `Envelope` is missing required field", "`payload`"],
        ),
    )
}

test "nullable concrete generic argument is the valid control" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Box<T> { value: T }
                function run() -> string { baml.json.to_string(Box<string?> {}) }
            `,
        ),
        "{\"value\":null}",
    )
}

test "empty generic without type arguments still fails inference" {
    assert.is_true(
        ConstructorCaseFailsToCompile(
            `
                class Box<T> { value: T }
                function run() -> string { baml.json.to_string(Box {}) }
            `,
        ),
    )
}

test "explicit null for concrete generic field is rejected" {
    assert.is_true(
        ConstructorCaseFailsToCompile(
            `
                class Box<T> { value: T }
                function run() -> string { baml.json.to_string(Box<string> { value: null }) }
            `,
        ),
    )
}

test "omission inside generic function is rejected for all instantiations" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Box<T> { value: T }
                function make<T>() -> Box<T> { Box<T> {} }
            `,
            ["class `Box` is missing required field", "`value`"],
        ),
    )
}
