function cycle_hash_source() -> string {
    `
        class CycleNode { child CycleBox<CycleNode>? }
        class CycleBox<T> { value T }
        implement<T extends baml.Hash> baml.ops.Equals for CycleBox<T> {
            function eq(self, other: unknown) -> bool throws never { true }
        }
    `
}

function conditional_hash_source() -> string {
    `
        interface Marker {}
        class Box<T> { value T }
        implement<T extends Marker> baml.ops.Equals for Box<T> {
            function eq(self, other: unknown) -> bool throws never { true }
        }
    `
}

function specialized_hash_source() -> string {
    `
        class Box<T> { value T }
        implement baml.ops.Equals for Box<string> {
            function eq(self, other: unknown) -> bool throws never { true }
        }
    `
}

test "recursive conditional Equals membership terminates without Hash" {
    let package = reflect.Package.compile({
        "main.baml": cycle_hash_source() + `
            function run() -> bool {
                let value: unknown = CycleNode { child: null };
                value is baml.Hash
            }
        `,
    });
    let run = package.get_function<() -> bool>("root.run") ?? baml.sys.panic("missing run");
    assert.equal(run(), false)
}

test "recursive conditional Equals rejects static Hash and generic bounds" {
    root.compiler.AssertHasMessage({
        "main.baml": cycle_hash_source() + `
            function run(value: CycleNode) -> void {
                value.hash(baml.hash.DefaultHasher.new())
            }
        `,
    }, "hash")
    root.compiler.AssertHasMessage({
        "main.baml": cycle_hash_source() + `
            function accept<T extends baml.Hash>(value: T) -> void {}
            function run(value: CycleNode) -> void { accept(value) }
        `,
    }, "Hash")
}

test "conditional Equals bounds do not permit implicit Hash" {
    root.compiler.AssertHasMessage({
        "main.baml": conditional_hash_source() + `
            function run(value: Box<int>) -> void {
                value.hash(baml.hash.DefaultHasher.new())
            }
        `,
    }, "hash")
    root.compiler.AssertHasMessage({
        "main.baml": conditional_hash_source() + `
            function run<T extends baml.Hash>(value: Box<T>) -> void {
                value.hash(baml.hash.DefaultHasher.new())
            }
        `,
    }, "hash")
}

test "proven explicit Hash overrides conditional Equals exclusion" {
    let source = conditional_hash_source() + `
        implement<T extends baml.Hash> baml.Hash for Box<T> {
            function hash(self, state: baml.hash.Hasher) -> void throws never {
                self.value.hash(state)
            }
        }
        function feed<T extends baml.Hash>(value: Box<T>, state: baml.hash.Hasher) -> void {
            value.hash(state)
        }
        function run() -> bool {
            let state = baml.hash.DefaultHasher.new();
            feed(Box<int> { value: 7 }, state)
            let expected = baml.hash.DefaultHasher.new();
            7.hash(expected)
            state.finish() == expected.finish()
        }
    `;
    root.compiler.AssertCompiles({ "main.baml": source })
    let package = reflect.Package.compile({ "main.baml": source });
    let run = package.get_function<() -> bool>("root.run") ?? baml.sys.panic("missing run");
    assert.is_true(run())
}

test "specialized Equals excludes symbolic but not disjoint closed Hash" {
    root.compiler.AssertCompiles({
        "main.baml": specialized_hash_source() + `
            function run(value: Box<int>) -> void {
                value.hash(baml.hash.DefaultHasher.new())
            }
        `,
    })
    root.compiler.AssertHasMessage({
        "main.baml": specialized_hash_source() + `
            function run<T extends baml.Hash>(value: Box<T>) -> void {
                value.hash(baml.hash.DefaultHasher.new())
            }
        `,
    }, "hash")
}

test "specialized Equals excludes projected Hash receivers" {
    root.compiler.AssertCompiles({
        "main.baml": `
            class Box<T> { value T }
            interface Source { type Item extends baml.Hash }
            function run<T extends Source>(value: Box<T.Item>) -> void {
                value.hash(baml.hash.DefaultHasher.new())
            }
        `,
    })
    root.compiler.AssertHasMessage({
        "main.baml": specialized_hash_source() + `
            interface Source { type Item extends baml.Hash }
            function run<T extends Source>(value: Box<T.Item>) -> void {
                value.hash(baml.hash.DefaultHasher.new())
            }
        `,
    }, "hash")
}

test "ordinary recursive classes and aliases retain Hash" {
    let source = `
        class Node { child NodeAlias? }
        type NodeAlias = Node
        function accept<T extends baml.Hash>(value: T, state: baml.hash.Hasher) -> void {
            value.hash(state)
        }
        function run() -> bool {
            let value: NodeAlias = Node { child: Node { child: null } };
            let state = baml.hash.DefaultHasher.new();
            accept(value, state)
            let expected = baml.hash.DefaultHasher.new();
            value.hash(expected)
            let erased: unknown = value;
            (erased is baml.Hash) && state.finish() == expected.finish()
        }
    `;
    root.compiler.AssertCompiles({ "main.baml": source })
    let package = reflect.Package.compile({ "main.baml": source });
    let run = package.get_function<() -> bool>("root.run") ?? baml.sys.panic("missing run");
    assert.is_true(run())
}
