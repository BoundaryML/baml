// Scalar fields and compile-time controls.

test "missing scalar fields currently compile to null" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Job { id: int, label: string, attempts: int }
                function run() -> string { baml.json.to_string(Job { id: 1 }) }
            `,
        ),
        "{\"id\":1,\"label\":null,\"attempts\":null}",
    )
}

test "empty non generic literal currently compiles to all null" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Job { id: int, label: string, attempts: int }
                function run() -> string { baml.json.to_string(Job {}) }
            `,
        ),
        "{\"id\":null,\"label\":null,\"attempts\":null}",
    )
}

test "explicit null for non nullable field is rejected" {
    assert.is_true(
        ConstructorCaseFailsToCompile(
            `
                class Job { id: int, label: string }
                function run() -> string { baml.json.to_string(Job { id: 1, label: null }) }
            `,
        ),
    )
}

test "wrong written field type is rejected" {
    assert.is_true(
        ConstructorCaseFailsToCompile(
            `
                class Job { id: int, label: string }
                function run() -> string { baml.json.to_string(Job { id: 1, label: 2 }) }
            `,
        ),
    )
}

test "unknown written field is rejected" {
    assert.is_true(
        ConstructorCaseFailsToCompile(
            `
                class Job { id: int }
                function run() -> string { baml.json.to_string(Job { id: 1, extra: 2 }) }
            `,
        ),
    )
}

test "nullable omissions are the valid control" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Job { id: int, label: string?, attempts: int? }
                function run() -> string { baml.json.to_string(Job { id: 1 }) }
            `,
        ),
        "{\"id\":1,\"label\":null,\"attempts\":null}",
    )
}

test "non nullable union literal and enum fields also become null" {
    assert.equal(
        RunCompiledConstructor(
            `
                enum State { Ready }
                class Shape { token: "fixed", value: string | int, state: State }
                function run() -> string { baml.json.to_string(Shape {}) }
            `,
        ),
        "{\"token\":null,\"value\":null,\"state\":null}",
    )
}

test "missing bool takes the true branch" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Flag { enabled: bool }
                function run() -> string {
                    let flag = Flag {};
                    if (flag.enabled) { "true" } else { "false" }
                }
            `,
        ),
        "true",
    )
}

test "statically typed arithmetic on a missing int compiles" {
    assert.is_true(
        ConstructorCaseCompiles(
            `
                class Job { id: int, attempts: int }
                function use_attempts() -> int {
                    let job = Job { id: 1 };
                    job.attempts + 1
                }
            `,
        ),
    )
}
