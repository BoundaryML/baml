// Union-target inference (TS's inferToMultipleTypes)
//
// (TIR leaves `null` in the null-side pick - TS solves R = string; the
// spec follows TS.)
//
// `int | null <: ?R | null`: constituents identical on both sides
// match and DROP (`null`); with exactly ONE naked variable left in the
// target, the remaining source flows to it as a lower bound
// (`?R := int`). Several naked variables, or leftovers with no unique
// home, defer - inference never guesses (the unique-candidate
// discipline). This is what solves `filter_map`'s `R` from its
// lambda's `int | null` return.
function fmu_pick<R>(value: R | null, fallback: R) -> R throws never {
    value ?? fallback
}

function fmu_direct() -> int throws never {
    let r = fmu_pick(7, 0);
     // ^ int
    r
}

function fmu_null_side() -> string throws never {
    let s = fmu_pick(null, "x");
     // ^ string
    s
}

function fmu_chain() -> int[] throws never {
    let kept = baml.iter.ArrayIterator.new([1, 2, 3, 4]).filter_map((x: int) -> int | null {
     // ^^^^ int[]
        if (x % 2 == 0) { x * 10 } else { null }
    }).collect();
    kept
}
