// Contextual construction of fresh literals in invariant positions
// (TYPE_SYSTEM.md: Variance). The literal is BUILT at the expected
// element/value type - never synthesized narrower and then upcast - so
// these are not exceptions to invariance. The last function is the
// negative control: an EXISTING `int[]` is not retyped by context.
function lc_direct_array() -> (int | string)[] throws never {
    [1]
//  ^^^ (int | string)[]
}

function lc_direct_map() -> map<string, int | string> throws never {
    {key: 1}
//  ^^^^^^^^ map<string, int | string>
}

function lc_literal_elements() -> (1 | 2)[] throws never {
    [1, 2]
//  ^^^^^^ (1 | 2)[]
}

function lc_literal_keys() -> map<"a" | "b", int> throws never {
    {a: 1}
//  ^^^^^^ map<"a" | "b", int>
}

function lc_if_branches(flag: bool) -> (int | string)[] throws never {
    if (flag) { [1] } else { ["x"] }
             // ^^^ (int | string)[]
}

function lc_match_arms(flag: bool) -> (int | string)[] throws never {
    match (flag) {
        true => [1],
             // ^^^ (int | string)[]
        false => ["x"]
    }
}

function lc_block_tail() -> (int | string)[] throws never {
    {
        let marker = 0;
        [1]
    //  ^^^ (int | string)[]
    }
}

function lc_identity<T>(value: T) -> T throws never {
    value
}

function lc_pinned_identity() -> (int | string)[] throws never {
    lc_identity<(int | string)[]>([1])
                               // ^^^ (int | string)[]
}

function lc_map_copy(values: int[]) -> (int | string)[] throws never {
    values.map((value: int) -> int | string {
        value
    })
}

function lc_loop_copy(values: int[]) -> (int | string)[] throws never {
    let result: (int | string)[] = [];
    for (let value in values) {
        result.push(value);
    }
    result
//  ^^^^^^ (int | string)[]
}

function lc_existing_array(values: int[]) -> void throws never {
    let widened: (int | string)[] = values;
                                 // ^^^^^^ mismatch: expected (int | string)[], got int[]
}
