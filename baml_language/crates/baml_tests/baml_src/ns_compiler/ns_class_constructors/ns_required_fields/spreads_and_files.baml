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

test "spread propagates null from an already partial instance" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Job { id: int, label: string, attempts: int }
                function run() -> string {
                    let partial = Job { id: 1 };
                    baml.json.to_string(Job { ...partial, label: "filled" })
                }
            `,
        ),
        "{\"id\":1,\"label\":\"filled\",\"attempts\":null}",
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

test "omitted middle field keeps declaration order and null slot" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Row { first: string, middle: int, last: bool }
                function run() -> string { baml.json.to_string(Row { last: false, first: "a" }) }
            `,
        ),
        "{\"first\":\"a\",\"middle\":null,\"last\":false}",
    )
}

test "cross file constructor has the same missing field behavior" {
    assert.equal(
        RunCompiledConstructorFiles(
            {
                "types.baml": `class Job { id: int, label: string, attempts: int }`,
                "main.baml": `function run() -> string { baml.json.to_string(Job { id: 1 }) }`,
            },
        ),
        "{\"id\":1,\"label\":null,\"attempts\":null}",
    )
}
