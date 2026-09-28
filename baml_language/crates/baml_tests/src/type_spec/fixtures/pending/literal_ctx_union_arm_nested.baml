// pending: literal-context: aggregate adoption picks a unique arm by shape, not by viability (00-examples section 5)
// The arm selection of literal_ctx_union_arm_viability, at a nested
// literal's own invariant boundary: inside an array, a map value, and an
// object field.
function nested_in_array() -> ((int | string)[] | bool[])[] throws never {
    [[1]]
//   ^^^ (int | string)[]
}

function nested_in_map() -> map<string, (int | string)[] | bool[]> throws never {
    {key: [1]}
//        ^^^ (int | string)[]
}

class LcUnionHolder {
    data (int | string)[] | bool[]
}

function nested_in_field() -> LcUnionHolder throws never {
    LcUnionHolder { data: [1] }
                       // ^^^ (int | string)[]
}

class LcUnionMapHolder {
    data map<string, int | string> | map<string, bool>
}

function nested_map_in_field() -> LcUnionMapHolder throws never {
    LcUnionMapHolder { data: {key: 1} }
                          // ^^^^^^^^ map<string, int | string>
}
