// Generic constructors: explicit arguments, inferred arguments, multiple
// parameters, nesting, and nullable controls.

test "explicit concrete generic field becomes null" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Box<T> { value: T, label: string }
                function run() -> string { baml.json.to_string(Box<string> {}) }
            `,
        ),
        "{\"value\":null,\"label\":null}",
    )
}

test "inferred generic sibling field becomes null" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Pair<T> { left: T, right: T }
                function run() -> string { baml.json.to_string(Pair { left: "left" }) }
            `,
        ),
        "{\"left\":\"left\",\"right\":null}",
    )
}

test "second concrete type parameter becomes null" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Two<A, B> { first: A, second: B }
                function run() -> string { baml.json.to_string(Two<string, int> { first: "first" }) }
            `,
        ),
        "{\"first\":\"first\",\"second\":null}",
    )
}

test "nested generic field becomes null" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Box<T> { value: T }
                class Envelope<T> { id: int, payload: T }
                function run() -> string { baml.json.to_string(Envelope<Box<int>> { id: 1 }) }
            `,
        ),
        "{\"id\":1,\"payload\":null}",
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

test "omission inside generic function currently compiles" {
    assert.is_true(
        ConstructorCaseCompiles(
            `
                class Box<T> { value: T }
                function make<T>() -> Box<T> { Box<T> {} }
            `,
        ),
    )
}
