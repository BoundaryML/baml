// pending: generic-call invariance: conflicting bounds on an unannotated call's type argument erase silently instead of reporting (00-examples section 10)
// An unconstrained generic call must not launder an EXISTING `int[]` into
// `(int | string)[]`: `T` has lower bound `int[]` and upper bound
// `(int | string)[]`, which no invariant instantiation satisfies. The
// conflict must be diagnosed, not erased (00-examples section 10; the
// resulting alias lets a string be written into the original `int[]`).
function lc_identity<T>(value: T) -> T throws never {
    value
}

function widen_without_copy(values: int[]) -> void throws never {
    let widened: (int | string)[] = lc_identity(values);
                                 // ^^^^^^^^^^^^^^^^^^^ mismatch: expected (int | string)[], got int[]
}

function unrelated() -> void throws never {
    let flag: bool = lc_identity([1]);
                  // ^^^^^^^^^^^^^^^^ mismatch: expected bool, got int[]
}
