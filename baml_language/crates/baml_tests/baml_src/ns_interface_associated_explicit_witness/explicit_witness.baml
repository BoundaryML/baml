// Emit must consume the type checker's canonical associated-type pins. A raw
// re-lower of `Self.Item` here loses the earlier `Item = int` witness and used
// to panic while `reflect.Package.compile` built the runtime impl rule.
test "explicit associated type witness can reference an earlier witness" {
    root.compiler.AssertCompiles(
        {
            "main.baml": `
                interface Batch {
                    type Item
                    type Items

                    function all(self) -> Self.Items throws never
                }

                class IntBatch {
                    values: int[]

                    implements Batch {
                        type Item = int
                        type Items = Self.Item[]

                        function all(self) -> int[] {
                            self.values
                        }
                    }
                }

                function read(batch: Batch<Item = int, Items = int[]>) -> int[] {
                    batch.all()
                }
            `,
        },
    )
}
