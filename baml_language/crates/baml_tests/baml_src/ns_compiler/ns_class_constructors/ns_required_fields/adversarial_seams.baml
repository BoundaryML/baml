// Adversarial seams: these cases cross package, interface, generic-bound,
// recursive-type, spread, and downstream-operation boundaries. The goal is
// to distinguish one missing constructor check from unrelated type features.

test "mounted class constructor rejects omitted required field" {
    assert.is_true(
        MountedConstructorCaseFailsWith(
            `class Job { id: int, label: string }`,
            `function run() -> string { baml.json.to_string(dep.Job { id: 1 }) }`,
            ["class `dep.Job` is missing required field", "`label`"],
        ),
    )
}

test "mounted generic constructor rejects omitted required field" {
    assert.is_true(
        MountedConstructorCaseFailsWith(
            `class Box<T> { value: T, label: string }`,
            `function run() -> string { baml.json.to_string(dep.Box<int> { label: "x" }) }`,
            ["class `dep.Box` is missing required field", "`value`"],
        ),
    )
}

test "mounted nullable field remains omittable" {
    assert.equal(
        RunMountedConstructor(
            `class Job { id: int, label: string? }`,
            `function run() -> string { baml.json.to_string(dep.Job { id: 1 }) }`,
        ),
        "{\"id\":1,\"label\":null}",
    )
}

test "implemented interface does not satisfy omitted class fields" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                interface Labelled { label: string }
                class Item {
                    label: string
                    count: int
                    implements Labelled {}
                }
                function run() -> string { baml.json.to_string(Item { label: "present" }) }
            `,
            ["class `Item` is missing required field", "`count`"],
        ),
    )
}

test "bounded generic field remains required" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                interface Named { name: string }
                class Person {
                    name: string
                    implements Named {}
                }
                class Box<T extends Named> { value: T }
                function run() -> string { baml.json.to_string(Box<Person> {}) }
            `,
            ["class `Box` is missing required field", "`value`"],
        ),
    )
}

test "phantom generic does not make concrete field optional" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Phantom<T> { id: int }
                function run() -> string { baml.json.to_string(Phantom<string> {}) }
            `,
            ["class `Phantom` is missing required field", "`id`"],
        ),
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

test "unknown field must be supplied explicitly" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Holder { value: unknown }
                function run() -> string { baml.json.to_string(Holder {}) }
            `,
            ["class `Holder` is missing required field", "`value`"],
        ),
    )
}

test "unknown field accepts explicit null" {
    assert.equal(
        RunCompiledConstructor(
            `
                class Holder { value: unknown }
                function run() -> string { baml.json.to_string(Holder { value: null }) }
            `,
        ),
        "{\"value\":null}",
    )
}

test "non nullable generic union remains required" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Box<T> { value: T | string }
                function run() -> string { baml.json.to_string(Box<int> {}) }
            `,
            ["class `Box` is missing required field", "`value`"],
        ),
    )
}

test "recursive required fields are rejected" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Node { value: int, next: Node }
                function run() -> string { baml.json.to_string(Node {}) }
            `,
            ["class `Node` is missing required fields", "`value`", "`next`"],
        ),
    )
}

test "never field omission is rejected" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Impossible { value: never }
                function run() -> string { baml.json.to_string(Impossible {}) }
            `,
            ["class `Impossible` is missing required field", "`value`"],
        ),
    )
}

test "shorthand field does not hide omitted sibling" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Job { id: int, label: string }
                function run() -> string {
                    let id = 7;
                    baml.json.to_string(Job { id })
                }
            `,
            ["class `Job` is missing required field", "`label`"],
        ),
    )
}

test "missing values are rejected before equality" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Job { id: int }
                function run() -> string {
                    if (Job {}.id == Job {}.id) { "equal" } else { "different" }
                }
            `,
            ["class `Job` is missing required field", "`id`"],
        ),
    )
}

test "missing value is rejected before null pattern" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Job { label: string }
                function run() -> string {
                    let job = Job {};
                    if (job.label is null) { "null" } else { "string" }
                }
            `,
            ["class `Job` is missing required field", "`label`"],
        ),
    )
}

test "missing field is rejected before method use" {
    assert.is_true(
        ConstructorCaseFailsWith(
            `
                class Job {
                    label: string
                    function display(self) -> string { self.label + "!" }
                }
                function run() -> string { Job {}.display() }
            `,
            ["class `Job` is missing required field", "`label`"],
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
