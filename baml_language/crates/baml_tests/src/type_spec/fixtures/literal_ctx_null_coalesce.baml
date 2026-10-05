// The enclosing expectation reaches the fallback of `??`: with no
// container context from the left operand, `[1]` is still built at the
// function's `(int | string)[]` return type.
function coalesce_array() -> (int | string)[] throws never {
    null ?? [1]
         // ^^^ (int | string)[]
}

function coalesce_map() -> map<string, int | string> throws never {
    null ?? {"key": 1}
         // ^^^^^^^^^^ map<string, int | string>
}

// Forwarding context to a literal is not permission to retype an
// EXISTING array: invariance still rejects this.
function coalesce_existing(values: int[]) -> void throws never {
    let widened: (int | string)[] = null ?? values;
                                 // ^^^^^^^^^^^^^^ mismatch: expected (int | string)[], got int[]
}

// Without an enclosing expectation, the unwrapped lhs still informs the
// fallback: the empty list adopts the lhs element type.
function coalesce_local(values: int[]?) -> void throws never {
    let fallback = values ?? [];
                          // ^^ int[]
}

// An enclosing expectation that gives the fallback no shape does not
// displace the lhs context: `unknown` has no callback arm, so the lambda
// still types `x` from `cb`, and `[]` still adopts `int[]`.
function coalesce_lambda_unknown(cb: ((int) -> int)?) -> unknown throws never {
    cb ?? (x) -> { x }
//                 ^ int
}

function coalesce_list_unknown(values: int[]?) -> unknown throws never {
    values ?? []
           // ^^ int[]
}
