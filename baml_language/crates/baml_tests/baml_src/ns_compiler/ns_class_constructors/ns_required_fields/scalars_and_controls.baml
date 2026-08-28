// Scalar fields and compile-time controls.

test "missing scalar fields are rejected together" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Job { id: int, label: string, attempts: int }
                function run() -> string { baml.json.to_string(Job { id: 1 }) }
            `,
            ["class `Job` is missing required fields", "`label`", "`attempts`"],
        ),
    )
}

test "empty non generic literal reports every required field" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Job { id: int, label: string, attempts: int }
                function run() -> string { baml.json.to_string(Job {}) }
            `,
            ["class `Job` is missing required fields", "`id`", "`label`", "`attempts`"],
        ),
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

test "non nullable union literal and enum fields are required" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                enum State { Ready }
                class Shape { token: "fixed", value: string | int, state: State }
                function run() -> string { baml.json.to_string(Shape {}) }
            `,
            ["class `Shape` is missing required fields", "`token`", "`value`", "`state`"],
        ),
    )
}

test "missing bool is rejected before control flow" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Flag { enabled: bool }
                function run() -> string {
                    let flag = Flag {};
                    if (flag.enabled) { "true" } else { "false" }
                }
            `,
            ["class `Flag` is missing required field", "`enabled`"],
        ),
    )
}

test "missing int is rejected before arithmetic" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Job { id: int, attempts: int }
                function use_attempts() -> int {
                    let job = Job { id: 1 };
                    job.attempts + 1
                }
            `,
            ["class `Job` is missing required field", "`attempts`"],
        ),
    )
}
