// pending: literal-context: aggregate adoption picks a unique arm by shape, not by viability (00-examples sections 3-4)
// With several container arms of the same shape, the literal adopts the
// UNIQUE arm whose element/value type accepts every component. Genuinely
// ambiguous candidates (`int[] | (int | string)[]` for `[1]`) must not be
// chosen arbitrarily: the mutable element type is observable.
function two_array_arms() -> (int | string)[] | bool[] throws never {
    [1]
//  ^^^ (int | string)[]
}

function two_map_arms() -> map<string, int | string> | map<string, bool> throws never {
    {key: 1}
//  ^^^^^^^^ map<string, int | string>
}
