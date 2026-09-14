// Isolated compile-success ports of Phase-1 Rust integration tests.

test "phase3_out_of_body_impl_on_builtin_normalizes_to_never" {
    let _ = root.mixed_phase1_support.compile(
        `
            interface Cmp2 {
                type CE
                function comp(self, other: Self) -> baml.ops.Ordering throws Self.CE
            }

            interface Srt2 {
                type SE
                function srt(self) -> Self throws Self.SE
            }

            implements<T extends Cmp2> Srt2 for T[] {
                type SE = T.CE

                function srt(self) -> Self throws T.CE {
                    self.sort_by((a: T, b: T) -> baml.ops.Ordering throws T.CE { a.comp(b) })
                }
            }

            class Boom2 { message: string }

            class WithNever {
                v: int
                implements Cmp2 {
                    type CE = never
                    function comp(self, other: Self) -> baml.ops.Ordering throws never { return baml.ops.Ordering.Equal }
                }
            }

            class WithErr {
                v: int
                implements Cmp2 {
                    type CE = Boom2
                    function comp(self, other: Self) -> baml.ops.Ordering throws Boom2 { return baml.ops.Ordering.Equal }
                }
            }

            implements Cmp2 for int {
                type CE = never
                function comp(self, other: Self) -> baml.ops.Ordering throws never { return baml.ops.Ordering.Equal }
            }

            function f(xs: int[]) -> int[] throws never {
                xs.srt()
            }
        `,
    );
    assert.equal(1, 1)
}

test "phase3_out_of_body_impl_on_builtin_with_error_throws_it" {
    let _ = root.mixed_phase1_support.compile(
        `
            interface Cmp2 {
                type CE
                function comp(self, other: Self) -> baml.ops.Ordering throws Self.CE
            }

            interface Srt2 {
                type SE
                function srt(self) -> Self throws Self.SE
            }

            implements<T extends Cmp2> Srt2 for T[] {
                type SE = T.CE

                function srt(self) -> Self throws T.CE {
                    self.sort_by((a: T, b: T) -> baml.ops.Ordering throws T.CE { a.comp(b) })
                }
            }

            class Boom2 { message: string }

            class WithNever {
                v: int
                implements Cmp2 {
                    type CE = never
                    function comp(self, other: Self) -> baml.ops.Ordering throws never { return baml.ops.Ordering.Equal }
                }
            }

            class WithErr {
                v: int
                implements Cmp2 {
                    type CE = Boom2
                    function comp(self, other: Self) -> baml.ops.Ordering throws Boom2 { return baml.ops.Ordering.Equal }
                }
            }

            implements Cmp2 for int {
                type CE = Boom2
                function comp(self, other: Self) -> baml.ops.Ordering throws Boom2 { return baml.ops.Ordering.Equal }
            }

            function f(xs: int[]) -> int[] throws Boom2 {
                xs.srt()
            }
        `,
    );
    assert.equal(1, 1)
}
