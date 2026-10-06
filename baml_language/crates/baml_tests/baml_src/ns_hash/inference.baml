test "hash compiler resolves nested variables from later annotations" {
    root.compiler.AssertCompiles({
        "main.baml": `
            class Box<T> { values: T[] }
            function accept<T extends baml.Hash>(value: T) -> void throws never {}
            function run(state: baml.hash.Hasher) -> void throws never {
                let value = Box { values: [] };
                value.hash(state);
                accept(value);
                let typed: Box<int> = value;
            }
        `,
    })
}

test "hash compiler retries nested variables constrained by later obligations" {
    root.compiler.AssertCompiles({
        "main.baml": `
            class Box<T> { values: T[] }
            interface Evidence<T> {}
            class IntEvidence {}
            implement Evidence<int> for IntEvidence {}
            function accept<T extends baml.Hash>(value: T) -> void throws never {}
            function constrain<T, E extends Evidence<T>>(value: Box<T>, evidence: E) -> void throws never {}
            function run(state: baml.hash.Hasher) -> void throws never {
                let value = Box { values: [] };
                accept(value);
                value.hash(state);
                constrain(value, IntEvidence {});
            }
        `,
    })
}

test "hash compiler checks conditional member eligibility after inference" {
    let files = {
        "main.baml": `
            class Box<T> { values: T[] }
            function run(state: baml.hash.Hasher) -> void throws never {
                let value = Box { values: [] };
                value.hash(state);
                let typed: Box<unknown> = value;
            }
        `,
    };
    root.compiler.AssertHasMessage(files, "Hash")
}

test "hash compiler checks conditional member eligibility after obligation solving" {
    root.compiler.AssertHasMessage({
        "main.baml": `
            class Box<T> { values: T[] }
            interface Evidence<T> {}
            class OpaqueEvidence {}
            implement Evidence<unknown> for OpaqueEvidence {}
            function constrain<T, E extends Evidence<T>>(value: Box<T>, evidence: E) -> void throws never {}
            function run(state: baml.hash.Hasher) -> void throws never {
                let value = Box { values: [] };
                value.hash(state);
                constrain(value, OpaqueEvidence {});
            }
        `,
    }, "Hash")
}

test "hash compiler unresolved nested variables retain inference diagnostics" {
    let files = {
        "main.baml": `
            class Box<T> { values: T[] }
            function accept<T extends baml.Hash>(value: T) -> void throws never {}
            function run(state: baml.hash.Hasher) -> void throws never {
                let value = Box { values: [] };
                accept(value);
                value.hash(state);
            }
        `,
    };
    root.compiler.AssertHasMessage(files, "cannot infer");
    root.compiler.AssertLacksMessage(files, "does not implement");
    root.compiler.AssertLacksMessage(files, "multiple implementations")
}

test "hash compiler preserves explicit member precedence during inference" {
    root.compiler.AssertCompiles({
        "main.baml": `
            class Box<T> { values: T[] }
            interface OwnHash {
                function hash(self) -> int throws never;
            }
            implement<T> OwnHash for Box<T> {
                function hash(self) -> int throws never { 42 }
            }
            function run() -> int throws never {
                let value = Box { values: [] };
                let result: int = value.hash();
                let typed: Box<int> = value;
                result
            }
        `,
    })
}

test "hash compiler explicit Hash overrides conditional defaults during inference" {
    root.compiler.AssertCompiles({
        "main.baml": `
            class Box<T> { values: T[] }
            implement<T> baml.Hash for Box<T> {
                function hash(self, state: baml.hash.Hasher) -> void throws never {
                    42.hash(state)
                }
            }
            function accept<T extends baml.Hash>(value: T) -> void throws never {}
            function run(state: baml.hash.Hasher) -> void throws never {
                let value = Box { values: [] };
                value.hash(state);
                accept(value);
                let typed: Box<unknown> = value;
            }
        `,
    })
}
