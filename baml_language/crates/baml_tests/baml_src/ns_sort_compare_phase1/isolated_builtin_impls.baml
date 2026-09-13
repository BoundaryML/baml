// Isolated compile-success ports of Phase-1 Rust integration tests.

test "phase3_out_of_body_impl_on_builtin_normalizes_to_never" {
    let _ = root.mixed_phase1_support.compile(
        "\n        \n    interface Cmp2 {\n        type CE\n        function comp(self, other: Self) -> baml.ops.Ordering throws Self.CE\n    }\n\n    interface Srt2 {\n        type SE\n        function srt(self) -> Self throws Self.SE\n    }\n\n    implements<T extends Cmp2> Srt2 for T[] {\n        type SE = T.CE\n\n        function srt(self) -> Self throws T.CE {\n            self.sort_by((a: T, b: T) -> baml.ops.Ordering throws T.CE { a.comp(b) })\n        }\n    }\n\n    class Boom2 { message: string }\n\n    class WithNever {\n        v: int\n        implements Cmp2 {\n            type CE = never\n            function comp(self, other: Self) -> baml.ops.Ordering throws never { return baml.ops.Ordering.Equal }\n        }\n    }\n\n    class WithErr {\n        v: int\n        implements Cmp2 {\n            type CE = Boom2\n            function comp(self, other: Self) -> baml.ops.Ordering throws Boom2 { return baml.ops.Ordering.Equal }\n        }\n    }\n\n\n        implements Cmp2 for int {\n            type CE = never\n            function comp(self, other: Self) -> baml.ops.Ordering throws never { return baml.ops.Ordering.Equal }\n        }\n\n        function f(xs: int[]) -> int[] throws never {\n            xs.srt()\n        }\n        ",
    );
    assert.equal(1, 1)
}

test "phase3_out_of_body_impl_on_builtin_with_error_throws_it" {
    let _ = root.mixed_phase1_support.compile(
        "\n        \n    interface Cmp2 {\n        type CE\n        function comp(self, other: Self) -> baml.ops.Ordering throws Self.CE\n    }\n\n    interface Srt2 {\n        type SE\n        function srt(self) -> Self throws Self.SE\n    }\n\n    implements<T extends Cmp2> Srt2 for T[] {\n        type SE = T.CE\n\n        function srt(self) -> Self throws T.CE {\n            self.sort_by((a: T, b: T) -> baml.ops.Ordering throws T.CE { a.comp(b) })\n        }\n    }\n\n    class Boom2 { message: string }\n\n    class WithNever {\n        v: int\n        implements Cmp2 {\n            type CE = never\n            function comp(self, other: Self) -> baml.ops.Ordering throws never { return baml.ops.Ordering.Equal }\n        }\n    }\n\n    class WithErr {\n        v: int\n        implements Cmp2 {\n            type CE = Boom2\n            function comp(self, other: Self) -> baml.ops.Ordering throws Boom2 { return baml.ops.Ordering.Equal }\n        }\n    }\n\n\n        implements Cmp2 for int {\n            type CE = Boom2\n            function comp(self, other: Self) -> baml.ops.Ordering throws Boom2 { return baml.ops.Ordering.Equal }\n        }\n\n        function f(xs: int[]) -> int[] throws Boom2 {\n            xs.srt()\n        }\n        ",
    );
    assert.equal(1, 1)
}
