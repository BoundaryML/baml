test "map key checks retain implementation header bounds" {
    root.compiler.AssertCompiles({
        "bounds.baml": `
            interface ReadKey<K> {
                function read(self, key: K) -> int throws never;
            }
            implement<K extends baml.Hash> ReadKey<K> for map<K, int> {
                function read(self, key: K) -> int throws never { self[key] }
            }
        `,
    })
}

test "erasing an empty map does not erase its hashable key type" {
    root.compiler.AssertCompiles({
        "empty.baml": `
            function erased() -> unknown throws never { {} }
            function nested() -> map<string, unknown> throws never {
                { capabilities: {} }
            }
            function used() -> unknown throws never {
                let values = {};
                let erased: unknown = values;
                values[1] = 2;
                erased
            }
        `,
    })
}
