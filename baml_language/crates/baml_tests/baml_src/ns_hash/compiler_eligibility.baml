test "hash compiler accepts recursive data and bounded generic fields" {
    root.compiler.AssertCompiles({
        "data.baml": `
            enum Color { Red Blue }
            class Node { value: int, next: Node? }
            class Box<T> { value: T }
            type Tree = int | Tree[]
            function require_hash<T extends baml.Hash>(value: T, state: baml.hash.Hasher) -> void throws never {
                value.hash(state)
            }
            function bounded<T extends baml.Hash>(value: Box<T>, state: baml.hash.Hasher) -> void throws never {
                value.hash(state);
                require_hash(value, state)
            }
            function data(node: Node, tree: Tree, color: Color, state: baml.hash.Hasher) -> void throws never {
                node.hash(state);
                tree.hash(state);
                color.hash(state);
                require_hash(node, state);
                require_hash(tree, state);
                require_hash([1, 2], state);
                let values: map<string, int> = { "a": 1 };
                values.hash(state)
            }
        `,
    })
}

test "hash compiler accepts method references and preserves inherent precedence" {
    root.compiler.AssertCompiles({
        "references.baml": `
            class Point { x: int }
            class Inherent {
                function hash(self) -> int throws never { 42 }
            }
            function references(value: Point, state: baml.hash.Hasher) -> void throws never {
                let member = value.hash;
                member(state);
                let qualified: (Point, baml.hash.Hasher) -> void throws never = Point.hash;
                qualified(value, state);
                baml.Hash.hash(value, state);
                let own: int = Inherent {}.hash();
            }
        `,
    })
}

test "hash compiler accepts explicit hash with custom equality and empty equals" {
    root.compiler.AssertCompiles({
        "explicit.baml": `
            class Structural {
                value: int,
                implements baml.ops.Equals {}
            }
            class Custom {
                value: unknown,
                implements baml.ops.Equals {
                    function eq(self, other: Custom) -> bool throws never { true }
                }
                implements baml.Hash {
                    function hash(self, state: baml.hash.Hasher) -> void throws never {
                        0.hash(state)
                    }
                }
            }
            function use_hash(a: Structural, b: Custom, state: baml.hash.Hasher) -> void throws never {
                a.hash(state);
                b.hash(state)
            }
        `,
    })
}

test "hash compiler rejects opaque and unbounded generic fields" {
    for (let field_type in ["unknown", "(int) -> int", "reflect.Type", "T"]) {
        root.compiler.AssertHasMessage({
            "opaque.baml": `
                class Box<T> { value: ${field_type} }
                function bad<T>(value: Box<T>, state: baml.hash.Hasher) -> void throws never {
                    value.hash(state)
                }
            `,
        }, "hash")
    }
}

test "hash compiler rejects custom equality through direct methods and bounds" {
    let declaration = `
        class Custom {
            value: int,
            implements baml.ops.Equals {
                function eq(self, other: Custom) -> bool throws never { true }
            }
        }
    `;
    root.compiler.AssertHasMessage({
        "direct.baml": declaration + `
            function bad(value: Custom, state: baml.hash.Hasher) -> void throws never {
                value.hash(state)
            }
        `,
    }, "hash");
    root.compiler.AssertHasMessage({
        "bound.baml": declaration + `
            function require_hash<T extends baml.Hash>(value: T) -> int throws never { 0 }
            function bad(value: Custom) -> int throws never { require_hash(value) }
        `,
    }, "Hash")
}

test "hash compiler checks equality metadata from mounted packages" {
    let dependency = root.compiler.CompileFixturePackage({
        "models.baml": `
            class Structural {
                value: int,
                implements baml.ops.Equals {}
            }
            class Custom {
                value: int,
                implements baml.ops.Equals {
                    function eq(self, other: Custom) -> bool throws never { true }
                }
            }
        `,
    });
    root.compiler.AssertCompiles({
        "accepted.baml": `
            function good(value: dep.Structural, state: baml.hash.Hasher) -> void throws never {
                value.hash(state)
            }
        `,
    }, packages = { "dep": dependency });
    root.compiler.AssertHasMessage({
        "rejected.baml": `
            function require_hash<T extends baml.Hash>(value: T) -> int throws never { 0 }
            function bad(value: dep.Custom) -> int throws never { require_hash(value) }
        `,
    }, "Hash", packages = { "dep": dependency })
}
