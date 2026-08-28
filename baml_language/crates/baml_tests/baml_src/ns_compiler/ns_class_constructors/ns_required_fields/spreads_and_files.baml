// Spread coverage and runtime compilation from more than one source file.

test "complete spread initializes every field" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Job { id: int, label: string, attempts: int }
                function run() -> string {
                    let base = Job { id: 1, label: "ready", attempts: 2 };
                    baml.json.to_string(Job { ...base })
                }
            `,
        ),
        "{\"id\":1,\"label\":\"ready\",\"attempts\":2}",
    )
}

test "spread cannot rescue an invalid partial source" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Job { id: int, label: string, attempts: int }
                function run() -> string {
                    let partial = Job { id: 1 };
                    baml.json.to_string(Job { ...partial, label: "filled" })
                }
            `,
            ["class `Job` is missing required fields", "`label`", "`attempts`"],
        ),
    )
}

test "later spread overwrites explicit fields" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Job { id: int, label: string, attempts: int }
                function run() -> string {
                    let base = Job { id: 2, label: "base", attempts: 3 };
                    baml.json.to_string(Job { id: 1, ...base })
                }
            `,
        ),
        "{\"id\":2,\"label\":\"base\",\"attempts\":3}",
    )
}

test "omitted middle field is rejected independent of written order" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Row { first: string, middle: int, last: bool }
                function run() -> string { baml.json.to_string(Row { last: false, first: "a" }) }
            `,
            ["class `Row` is missing required field", "`middle`"],
        ),
    )
}

test "cross file constructor enforces required fields" {
    assert.is_true(
        ConstructorFilesFailWith(
            {
                "types.baml": `class Job { id: int, label: string, attempts: int }`,
                "main.baml": `function run() -> string { baml.json.to_string(Job { id: 1 }) }`,
            },
            ["class `Job` is missing required fields", "`label`", "`attempts`"],
        ),
    )
}
