function spike_1a(source: string) -> string {
    `
        interface HasErr {
            type E
            function f(self) -> int throws Self.E
        }
        interface Wrap {
            type WE
            function g(self) -> int throws Self.WE
        }
        implements<T extends HasErr> Wrap for T[] {
            type WE = T.E
            function g(self) -> int throws T.E {
                if (self.length() == 0) { return 0 }
                self[0].f()
            }
        }
    `
        + source
}

function spike_1b(source: string) -> string {
    `
        interface Named { name: string }
        interface FirstNamed {
            function first_name(self) -> string throws never
        }
        implements<T extends Named> FirstNamed for T[] {
            function first_name(self) -> string {
                if (self.length() == 0) { return "" }
                self[0].name
            }
        }
        class Person {
            name: string
            implements Named {}
        }
    `
        + source
}

function phase_3(source: string) -> string {
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
                function comp(self, other: Self) -> baml.ops.Ordering throws never {
                    baml.ops.Ordering.Equal
                }
            }
        }
        class WithErr {
            v: int
            implements Cmp2 {
                type CE = Boom2
                function comp(self, other: Self) -> baml.ops.Ordering throws Boom2 {
                    baml.ops.Ordering.Equal
                }
            }
        }
    `
        + source
}

test "projection-valued error requires handling at a concrete callsite" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": spike_1a(
                `
                    class Kaboom { message: string }
                    class Risky {
                        implements HasErr {
                            type E = Kaboom
                            function f(self) -> int throws Kaboom {
                                throw Kaboom { message: "kaboom" }
                            }
                        }
                    }
                    function main() -> int throws never {
                        let xs: Risky[] = [Risky {}]
                        xs.g()
                    }
                `,
            ),
        },
        "Kaboom",
    )
}

test "non-qualifying projection reaches diagnostics without panicking" {
    let diagnostics = root.compiler.CompileDiagnostics(
        {
            "main.baml": spike_1a(
                `
                    class NoImpl { value: int }
                    function main() -> int {
                        let xs: NoImpl[] = [NoImpl { value: 1 }]
                        xs.g()
                    }
                `,
            ),
        },
    );
    assert.is_true(diagnostics.length() > 0)
}

test "blanket array method rejects a non-qualifying element" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": spike_1b(
                `
                    class Rock { label: string }
                    function main() -> string {
                        let rocks: Rock[] = [Rock { label: "igneous" }]
                        rocks.first_name()
                    }
                `,
            ),
        },
        "Rock",
    )
}

test "two-Self method rejects unrelated interface-typed values" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": `
                interface Cmp {
                    function compare(self, other: Self) -> int throws never
                }
                function bad(a: Cmp, b: Cmp) -> bool { a.compare(b) < 0 }
            `,
        },
        "Self",
    )
}

test "missing undefaulted associated binding is rejected" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": `
                interface Ordered {
                    type CompareError
                    function compare(self, other: Self) -> int throws Self.CompareError
                }
                class Score {
                    points: int
                    implements Ordered {
                        function compare(self, other: Self) -> int throws never {
                            if (self.points < other.points) { -1 }
                            else if (self.points > other.points) { 1 }
                            else { 0 }
                        }
                    }
                }
            `,
        },
        "CompareError",
    )
}

test "concrete associated error on sort wrapper must be handled" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": phase_3(
                `
                    function f(xs: WithErr[]) -> WithErr[] throws never { xs.srt() }
                `,
            ),
        },
        "Boom2",
    )
}

test "union numeric element has no natural sort" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": `
                function f() -> null throws never {
                    let xs: (int | float)[] = [1, 2.5]
                    xs.sort()
                    null
                }
            `,
        },
        "int | float",
    )
}

test "optional element sort names both the array and missing member" {
    assert_code_and_messages(
        {
            "main.baml": `
                function f() -> null throws never {
                    let xs: int?[] = [1, null, 2]
                    xs.sort()
                    null
                }
            `,
        },
        "E0007",
        ["(int | null)[]", "no member `sort`"],
    )
}

test "nullable sort key fails its comparison bound" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": `
                function f() -> null throws never {
                    let xs = [3, 1, 2]
                    xs.sort_by_key((x: int) -> int? { if (x == 1) { null } else { x } })
                    null
                }
            `,
        },
        "int | null",
    )
}

test "mixed int-string union has no natural sort" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": `
                function f() -> null throws never {
                    let xs: (int | string)[] = [2, "x", 1]
                    xs.sort()
                    null
                }
            `,
        },
        "int | string",
    )
}

test "class without Compare reports one typed missing-sort diagnostic" {
    assert_code_and_messages(
        {
            "main.baml": `
                class Resume { name: string }
                function f() -> null throws never {
                    let xs = [Resume { name: "b" }, Resume { name: "a" }]
                    xs.sort()
                    null
                }
            `,
        },
        "E0007",
        ["Resume[]", "sort"],
    )
}

test "generic array match arms remain reachable without refining T" {
    let files = {
        "main.baml": `
            function fast_g<T extends baml.ops.Compare>(xs: T[]) -> T[] throws never { xs }
            function dispatch_f<T extends baml.ops.Compare>(xs: T[]) -> T[] throws never {
                match (xs) {
                    int[] => fast_g(xs),
                    bigint[] => fast_g(xs),
                    string[] => fast_g(xs),
                    float[] => fast_g(xs),
                    _ => xs,
                }
            }
        `,
    };
    root.compiler.AssertLacksMessage(files, "unreachable arm");
    root.compiler.AssertHasMessage(files, "expected `T[]`, found `int[]`")
}
