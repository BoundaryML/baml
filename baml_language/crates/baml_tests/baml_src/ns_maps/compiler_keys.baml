test "map compiler accepts hashable key types and bounded generic keys" {
    root.compiler.AssertCompiles({
        "keys.baml": `
            enum Color { Red Blue }
            class Point { x: int }
            function make<K extends baml.Hash, V>(key: K, value: V) -> map<K, V> throws never {
                let result: map<K, V> = {};
                result[key] = value;
                result
            }
            function keys(point: Point, color: Color) -> int throws never {
                let integers: map<int, string> = {};
                integers.set(1, "one");
                let booleans: map<bool, int> = {};
                booleans[true] = 1;
                let enums: map<Color, int> = make(color, 2);
                let classes: map<Point, int> = make(point, 3);
                let arrays: map<int[], int> = make([1, 2], 4);
                let optional: map<int?, int> = {};
                optional[null] = 5;
                booleans[true] + enums[color] + classes[point] + arrays[[1, 2]] + optional[null]
            }
        `,
    })
}

test "map compiler infers empty map keys from indexing and methods" {
    root.compiler.AssertCompiles({
        "inference.baml": `
            function indexed() -> int throws never {
                let values = {};
                values[42] = 7;
                let keys: int[] = values.keys();
                values[keys[0]]
            }
            function methods() -> int throws never {
                let values = {};
                values.set(true, 7);
                let keys: bool[] = values.keys();
                values[keys[0]]
            }
            function strings() -> map<string, int> throws never {
                let values = {};
                values["key"] = 1;
                values
            }
        `,
    })
}

test "map compiler accepts literal and computed keys" {
    root.compiler.AssertCompiles({
        "literals.baml": `
            class Point { x: int }
            function literals(point: Point) -> int throws never {
                let integers = {1: 2, 3: 4};
                let computed = {(point): 5};
                let legacy: map<string, int> = {"name": 6};
                integers[1] + computed[point] + legacy["name"]
            }
        `,
    })
}

test "map compiler accepts explicit hash consistent with custom equals" {
    root.compiler.AssertCompiles({
        "custom.baml": `
            class Key {
                value: int,
                implements baml.ops.Equals {
                    function eq(self, other: Key) -> bool throws never { self.value == other.value }
                }
                implements baml.Hash {
                    function hash(self, state: baml.hash.Hasher) -> void throws never {
                        self.value.hash(state)
                    }
                }
            }
            function keys(key: Key) -> int throws never {
                let values: map<Key, int> = {};
                values.set(key, 42);
                values[key]
            }
        `,
    })
}

test "map compiler rejects keys without hash evidence" {
    for (let key_type in ["unknown", "(int) -> int", "reflect.Type", "T"]) {
        root.compiler.AssertHasMessage({
            "rejected.baml": `
                function bad<T>(values: map<${key_type}, int>) -> int throws never {
                    values.length()
                }
            `,
        }, "Hash")
    }
}

test "map compiler rejects custom equality without matching hash" {
    let declaration = `
        class Key {
            value: int,
            implements baml.ops.Equals {
                function eq(self, other: Key) -> bool throws never { true }
            }
        }
    `;
    root.compiler.AssertHasMessage({
        "annotation.baml": declaration + `
            function bad(values: map<Key, int>) -> int throws never { values.length() }
        `,
    }, "Hash");
    root.compiler.AssertHasMessage({
        "inferred.baml": declaration + `
            function bad(key: Key) -> int throws never {
                let values = {(key): 1};
                values.length()
            }
        `,
    }, "Hash");
    root.compiler.AssertHasMessage({
        "empty.baml": declaration + `
            function bad(key: Key) -> int throws never {
                let values = {};
                values[key] = 1;
                values.length()
            }
        `,
    }, "Hash")
}
