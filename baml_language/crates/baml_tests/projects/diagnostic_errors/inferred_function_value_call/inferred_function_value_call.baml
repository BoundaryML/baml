// The compiler's refusals around a generic function used as a value. One
// referenced where `(int) -> int` is expected is that instantiation, not the
// open generic: calling the value with a string is a type error, exactly as
// it is for the written `identity<int>`; and one stored with nothing to infer
// the instantiation from is refused where it is stored. (The run-time half,
// the value carrying its type arguments, is pinned by the corpus in
// `baml_src/ns_instantiation_expr/inferred_instantiation.baml`.)
function identity<T>(x: T) -> T { x }

function get_identity() -> (int) -> int throws never { identity }

function get_identity_written() -> (int) -> int throws never { identity<int> }

function call_inferred_with_string() -> int { get_identity()("x") }

function call_written_with_string() -> int { get_identity_written()("x") }

function call_let_bound_with_string() -> int {
    let f: (int) -> int throws never = identity;
    f("x")
}

// An unannotated `let` has nothing to infer the instantiation from, so the
// reference is refused where it is stored.
function call_unannotated_let() -> int {
    let f = identity;
    f(1)
}
