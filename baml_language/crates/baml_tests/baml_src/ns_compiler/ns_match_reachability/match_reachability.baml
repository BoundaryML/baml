// Reachability of type-test arms where the scrutinee's values cannot be
// listed (`unknown`, a type parameter). A type test covers the tests of its
// subtypes there exactly as it does on a union scrutinee: `int` before `1`
// leaves `1` dead. An arm is reported only when the subtype relation proves
// it dead, and a type test never makes such a match exhaustive.

function unreachable_arms(source: string) -> int {
    let count = 0;
    for (let diagnostic in root.compiler.CompileDiagnostics({ "main.baml": source })) {
        if (diagnostic.code == "E0063") {
            count += 1;
        }
    }
    count
}

test "a literal after its base type is dead on unknown" {
    assert.equal(
        unreachable_arms(
            `
                function f(x: unknown) -> string {
                    match (x) {
                        int => "int",
                        1 => "dead",
                        _ => "other",
                    }
                }
            `,
        ),
        1,
    );
    assert.equal(
        unreachable_arms(
            `
                function f(x: unknown) -> string {
                    match (x) {
                        string => "string",
                        "a" => "dead",
                        "b" => "dead",
                        _ => "other",
                    }
                }
            `,
        ),
        2,
    );
}

test "a literal after its base type is dead on a union and on unknown alike" {
    assert.equal(
        unreachable_arms(
            `
                function f(x: int | string) -> string {
                    match (x) {
                        int => "int",
                        1 => "dead",
                        _ => "other",
                    }
                }
            `,
        ),
        1,
    );
}

test "a literal before its base type leaves both arms reachable" {
    root.compiler.AssertCompiles(
        {
            "main.baml": `
                function f(x: unknown) -> string {
                    match (x) {
                        1 => "one",
                        int => "int",
                        "a" => "a",
                        string => "string",
                        _ => "other",
                    }
                }
            `,
        },
    );
}

test "a literal of another base type is not covered" {
    root.compiler.AssertCompiles(
        {
            "main.baml": `
                function f(x: unknown) -> string {
                    match (x) {
                        int => "int",
                        "a" => "string literal",
                        true => "bool literal",
                        null => "null",
                        _ => "other",
                    }
                }
            `,
        },
    );
}

test "an alias covers and is covered by its target" {
    assert.equal(
        unreachable_arms(
            `
                type Count = int
                function f(x: unknown) -> string {
                    match (x) {
                        Count => "count",
                        1 => "dead",
                        int => "dead",
                        _ => "other",
                    }
                }
            `,
        ),
        2,
    );
}

test "a union type test covers the literals of each member" {
    assert.equal(
        unreachable_arms(
            `
                function f(x: unknown) -> string {
                    match (x) {
                        int | string => "scalar",
                        "a" => "dead",
                        2 => "dead",
                        true => "bool",
                        _ => "other",
                    }
                }
            `,
        ),
        2,
    );
}

test "a literal after its base type is dead on a type parameter" {
    assert.equal(
        unreachable_arms(
            `
                function f<T>(x: T) -> string {
                    match (x) {
                        int => "int",
                        1 => "dead",
                        _ => "other",
                    }
                }
            `,
        ),
        1,
    );
}

test "a guarded type test covers nothing" {
    root.compiler.AssertCompiles(
        {
            "main.baml": `
                function f(x: unknown, wide: bool) -> string {
                    match (x) {
                        int if wide => "int",
                        1 => "one",
                        _ => "other",
                    }
                }
            `,
        },
    );
}

test "coverage inside a field joins with the sibling fields" {
    let prelude = `
        class Tagged {
            value: unknown
            flag: bool
        }
    `;
    assert.equal(
        unreachable_arms(
            `
                ${prelude}
                function f(t: Tagged) -> string {
                    match (t) {
                        Tagged { value: int, flag: true } => "int",
                        Tagged { value: 1, flag: true } => "dead",
                        _ => "other",
                    }
                }
            `,
        ),
        1,
    );
    root.compiler.AssertCompiles(
        {
            "main.baml": `
                ${prelude}
                function f(t: Tagged) -> string {
                    match (t) {
                        Tagged { value: int, flag: true } => "int",
                        Tagged { value: 1, flag: false } => "one",
                        _ => "other",
                    }
                }
            `,
        },
    );
}

test "equivalent type tests report their missing cases once" {
    let files = {
        "main.baml": `
            type Count = int
            class Tagged {
                value: unknown
                flag: bool
            }
            function f(t: Tagged) -> string {
                match (t) {
                    Tagged { value: int, flag: true } => "int",
                    Tagged { value: Count, flag: true } => "dead",
                }
            }
        `,
    };
    root.compiler.AssertHasMessage(files, "Tagged { value: Int, flag: false }");
    root.compiler.AssertLacksMessage(files, "Count");
}

test "a type test does not make a match on unknown exhaustive" {
    let source = `
        function f(x: unknown) -> string {
            match (x) {
                int => "int",
                1 => "dead",
            }
        }
    `;
    root.compiler.AssertHasCode({ "main.baml": source }, "E0062");
    assert.equal(unreachable_arms(source), 1);
}

function literal_first(x: unknown) -> string {
    match (x) {
        1 => "one",
        int => "int",
        "a" => "a",
        string => "string",
        _ => "other",
    }
}

test "arms run in order when the literal comes first" {
    assert.equal(literal_first(1), "one");
    assert.equal(literal_first(2), "int");
    assert.equal(literal_first("a"), "a");
    assert.equal(literal_first("b"), "string");
    assert.equal(literal_first(1.0), "other");
}
