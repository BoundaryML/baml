// Adversarial seams: these cases cross package, interface, generic-bound,
// recursive-type, spread, and downstream-operation boundaries. The goal is
// to distinguish one missing constructor check from unrelated type features.

test "mounted class constructor accepts omitted required field" {
    assert.equal(
        RunMountedConstructor(
            `class Job { id: int, label: string }`,
            `function run() -> string { baml.json.to_string(dep.Job { id: 1 }) }`,
        ),
        "{\"id\":1,\"label\":null}",
    )
}

test "mounted generic constructor accepts omitted required field" {
    assert.equal(
        RunMountedConstructor(
            `class Box<T> { value: T, label: string }`,
            `function run() -> string { baml.json.to_string(dep.Box<int> { label: "x" }) }`,
        ),
        "{\"value\":null,\"label\":\"x\"}",
    )
}

test "implemented interface does not make class fields complete" {
    assert.equal(
        RunCompiledConstructor(
            `
                interface Labelled { label: string }
                class Item {
                    label: string
                    count: int
                    implements Labelled {}
                }
                function run() -> string { baml.json.to_string(Item { label: "present" }) }
            `,
        ),
        "{\"label\":\"present\",\"count\":null}",
    )
}

test "bounded generic field can still be omitted" {
    assert.equal(
        RunCompiledConstructor(
            `
                interface Named { name: string }
                class Person {
                    name: string
                    implements Named {}
                }
                class Box<T extends Named> { value: T }
                function run() -> string { baml.json.to_string(Box<Person> {}) }
            `,
        ),
        "{\"value\":null}",
    )
}

test "phantom generic does not change missing concrete field" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Phantom<T> { id: int }
                function run() -> string { baml.json.to_string(Phantom<string> {}) }
            `,
        ),
        "{\"id\":null}",
    )
}

test "optional substituted generic omission is a valid control" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Box<T> { value: T? }
                function run() -> string { baml.json.to_string(Box<int> {}) }
            `,
        ),
        "{\"value\":null}",
    )
}

test "non nullable generic union omission becomes null" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Box<T> { value: T | string }
                function run() -> string { baml.json.to_string(Box<int> {}) }
            `,
        ),
        "{\"value\":null}",
    )
}

test "recursive required fields can both be omitted" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Node { value: int, next: Node }
                function run() -> string { baml.json.to_string(Node {}) }
            `,
        ),
        "{\"value\":null,\"next\":null}",
    )
}

test "never field omission manufactures an impossible object" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Impossible { value: never }
                function run() -> string { baml.json.to_string(Impossible {}) }
            `,
        ),
        "{\"value\":null}",
    )
}

test "shorthand field does not affect omitted sibling" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Job { id: int, label: string }
                function run() -> string {
                    let id = 7;
                    baml.json.to_string(Job { id })
                }
            `,
        ),
        "{\"id\":7,\"label\":null}",
    )
}

test "missing values compare equal as null at runtime" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Job { id: int }
                function run() -> string {
                    if (Job {}.id == Job {}.id) { "equal" } else { "different" }
                }
            `,
        ),
        "equal",
    )
}

test "runtime null can be observed through is pattern" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Job { label: string }
                function run() -> string {
                    let job = Job {};
                    if (job.label is null) { "null" } else { "string" }
                }
            `,
        ),
        "null",
    )
}

test "method using missing field still compiles" {
    assert.is_true(
        ConstructorCaseCompiles(
            `
                class Job {
                    label: string
                    function display(self) -> string { self.label + "!" }
                }
                function run() -> string { Job {}.display() }
            `,
        ),
    )
}

test "wrong class spread is still rejected" {
    assert.is_true(
        ConstructorCaseFailsToCompile(
            `
                class Left { id: int }
                class Right { id: int }
                function run() -> Left { Left { ...Right { id: 1 } } }
            `,
        ),
    )
}

test "spread plus explicit fields can construct a complete value" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Job { id: int, label: string }
                function run() -> string {
                    let base = Job { id: 1, label: "base" };
                    baml.json.to_string(Job { ...base, label: "override" })
                }
            `,
        ),
        "{\"id\":1,\"label\":\"override\"}",
    )
}

test "duplicate fields are an intentional last write wins control" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Job { id: int }
                function run() -> string { baml.json.to_string(Job { id: 1, id: 2 }) }
            `,
        ),
        "{\"id\":2}",
    )
}

test "complete constructor remains the sound control" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Job { id: int, label: string, attempts: int }
                function run() -> string {
                    baml.json.to_string(Job { id: 1, label: "ready", attempts: 2 })
                }
            `,
        ),
        "{\"id\":1,\"label\":\"ready\",\"attempts\":2}",
    )
}
