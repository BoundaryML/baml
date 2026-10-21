// The builtin bridgings (B-1080 carriers, structural `baml.Array`/`baml.Map`)
// are a property of the SPELLING, not of how `baml` became visible: a runtime
// compile resolves the stdlib through the MOUNTED interface blob, and that
// lane must lower a carrier class to its structural type exactly as
// source-visible lowering does. Pinned because the mounted road
// (`lower_exported_type`) once built the nominal class directly, so a
// carrier-spelled signature type-checked differently in runtime compiles
// than in ordinary ones.

test "mounted carrier classes bridge to their structural types" {
    root.compiler.AssertCompiles(
        {
            "case.baml": `
                function take_int(x: baml.Int) -> int { x }
                function give_int() -> baml.Int { take_int(1) }
                function take_list(xs: baml.Array<int>) -> int[] { xs }
                function give_list() -> baml.Array<int> { take_list([1, 2]) }
                function take_map(m: baml.Map<string, int>) -> map<string, int> { m }
            `,
        },
    )
}
