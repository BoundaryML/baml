// pending: literal-context: `??` does not forward the enclosing expectation to its fallback (00-examples section 7)
// The enclosing expectation reaches the fallback of `??`: with no
// container context from the left operand, `[1]` is still built at the
// function's `(int | string)[]` return type.
function coalesce_array() -> (int | string)[] throws never {
    null ?? [1]
         // ^^^ (int | string)[]
}

function coalesce_map() -> map<string, int | string> throws never {
    null ?? {key: 1}
         // ^^^^^^^^ map<string, int | string>
}

// Forwarding context to a literal is not permission to retype an
// EXISTING array: invariance still rejects this.
function coalesce_existing(values: int[]) -> void throws never {
    let widened: (int | string)[] = null ?? values;
                                 // ^^^^^^^^^^^^^^ mismatch: expected (int | string)[], got int[]
}
