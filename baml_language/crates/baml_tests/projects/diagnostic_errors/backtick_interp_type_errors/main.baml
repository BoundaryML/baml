// B-836: type errors inside `${...}` interpolation must surface at check
// time. The untagged backtick is desugared into a `string.from(...)` concat
// tree; inferring that tree emits the real diagnostics for each `${expr}`,
// and none of them may be discarded. Before the fix, everything except
// unresolved names was dropped, so `baml check` passed and the error-recovery
// `Ty::Error` reached MIR lowering, which panics on it (runtime_ty.rs).

// The ticket's literal repro: `.len()` does not exist on `int[]`.
function bad_member_interp() -> string {
    `result: ${[1,2,3].len()}`
}

// Chained form from the original agent transcript: the error is on the
// first link of the chain.
function bad_member_chain_interp() -> string {
    `x ${[1,2,3].len().to_string()}`
}

// Operator type error inside an interpolation.
function bad_operator_interp() -> string {
    `sum: ${1 + "x"}`
}

// Unknown method on a builtin receiver.
function bad_method_interp() -> string {
    `v: ${"abc".nonexistent()}`
}

// Unresolved names were already reported before the fix (the one diagnostic
// kind the old filter kept); pin that they still are, with no added
// cannot-infer cascade from the synthetic `string.from` wrapper.
function unresolved_name_interp() -> string {
    `v: ${undefined_var}`
}

// Segment bodies (`${for}` / `${if}`) are elaborated into the same concat
// tree; errors inside them must surface too.
function bad_member_in_for_segment(xs: int[]) -> string {
    `${for (let x in xs)}${x.bogus()}${endfor}`
}

function bad_member_in_if_segment(show: bool) -> string {
    `${if (show)}${"abc".missing()}${endif}`
}
