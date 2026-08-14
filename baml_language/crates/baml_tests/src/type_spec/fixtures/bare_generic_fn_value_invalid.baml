// Spec Functions section: `let a = foo;` for generic `foo` is INVALID -
// no context determines the type parameters, so monomorphization is
// impossible. The unconstrained instantiation vars erase to the local
// Error sentinel (rulings 2/3; the diagnostic is S17's), exactly the
// spec's `let a = foo` case; `let c: (int) -> int = foo` stays the
// legal inferred spelling (function_value_outside_call_position).
//
//
// (TIR collapses the whole binding to its recovery sentinel and emits
// the not-a-valid-function-reference diagnostic; the local-erasure
// discipline keeps the shape.)
function same<T>(a: T) -> T {
    a
}

function f() -> string {
    let a = same;
     // ^ (a: !error) -> !error throws never
    "ok"
}
