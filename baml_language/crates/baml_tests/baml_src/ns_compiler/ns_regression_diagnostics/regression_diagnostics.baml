test "to_string fallback preserves receiver diagnostics" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": `
                function takes_int(n: int) -> int { n }
                function main() -> string {
                    takes_int("not an int").to_string()
                }
            `,
        },
        "mismatched types",
    )
}

test "nullable array index is rejected before runtime" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": `
                function main() -> string {
                    let arr = ["a", "b", "c"];
                    let idx: int? = null;
                    arr[idx]
                }
            `,
        },
        "mismatched types: expected `int`, found `int | null`",
    )
}

test "integer literals outside the i63 range are rejected" {
    root.compiler.AssertHasCode(
        { "above_max.baml": `function main() -> int { 4611686018427387904 }` },
        "E0150",
    );
    root.compiler.AssertHasCode(
        { "below_min.baml": `function main() -> int { -5000000000000000000 }` },
        "E0150",
    );
    root.compiler.AssertHasCode(
        { "parenthesized.baml": `function main() -> int { -(4611686018427387904) }` },
        "E0150",
    )
}

test "non-string map key annotations are rejected" {
    root.compiler.AssertHasMessage(
        {
            "concrete.baml": `
                function main(nums: int[]) -> int {
                    let counts: map<int, int> = {};
                    for (let n in nums) {
                        let _ = counts.set(n, (counts.get(n) ?? 0) + 1);
                    }
                    0
                }
            `,
        },
        "map keys must be `string`; got `int`",
    );
    root.compiler.AssertHasMessage(
        {
            "generic.baml": `
                function passthrough<K, V>(items: map<K, V>) -> map<K, V> { items }
                function main() -> int { 0 }
            `,
        },
        "map keys must be `string`; got `K`",
    );
    root.compiler.AssertHasMessage(
        {
            "alias.baml": `
                type IntKey = int
                function main() -> int {
                    let counts: map<IntKey, int> = {};
                    0
                }
            `,
        },
        "map keys must be `string`; got `IntKey`",
    )
}

test "cleanup reserved-name shape matrix reports E0144" {
    root.compiler.AssertHasCode(
        {
            "extra_param.baml": `
                class Bad {
                    x: int,
                    function cleanup(self, force: bool) -> void {}
                }
                function main() -> int { 0 }
            `,
        },
        "E0144",
    );
    root.compiler.AssertHasCode(
        {
            "return_type.baml": `
                class Bad {
                    x: int,
                    function cleanup(self) -> string { "nope" }
                }
                function main() -> int { 0 }
            `,
        },
        "E0144",
    );
    root.compiler.AssertHasCode(
        {
            "self_default.baml": `
                class Bad {
                    x: int,
                    function cleanup(self = null) -> void {}
                }
                function main() -> int { 0 }
            `,
        },
        "E0144",
    );
    root.compiler.AssertHasCode(
        {
            "throws.baml": `
                class Bad {
                    x: int,
                    function cleanup(self) -> void throws baml.json.ParseError {
                        throw baml.json.ParseError { message: "boom" }
                    }
                }
                function main() -> int { 0 }
            `,
        },
        "E0144",
    )
}

test "removed reflection reader spellings are unresolved once each" {
    root.compiler.AssertMultiRejected(
        {
            "main.baml": `
                function old_type_reader() -> reflect.Type {
                    reflect.type_of<int>()
                }
                function old_value_reader(value: unknown) -> reflect.Type {
                    reflect.type_of_value(value)
                }
            `,
        },
        [
            root.compiler.ExactDiagnostic {
                code: "E0003",
                message: "unresolved name: `reflect.type_of`",
            },
            root.compiler.ExactDiagnostic {
                code: "E0003",
                message: "unresolved name: `reflect.type_of_value`",
            },
        ],
    )
}

test "open TypeVar union without its TypeVar arm is non-exhaustive" {
    root.compiler.AssertHasCode(
        {
            "main.baml": `
                function tag_or_value<T>(x: T | string | null) -> T? {
                    match (x) {
                        let s: string => null,
                        null => null,
                    }
                }
                function main() -> int { 0 }
            `,
        },
        "E0062",
    )
}
