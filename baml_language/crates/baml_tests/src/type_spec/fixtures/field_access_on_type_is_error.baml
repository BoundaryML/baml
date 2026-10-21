//
// B-1035 (https://linear.app/boundaryml2/issue/B-1035): a field access
// on a TYPE (`C.x` where `C` is the class, not a value) is not a value.
// TIR admits it (evaluates to null at runtime); hir_ty types it as the
// Error sentinel - the S17 diagnostic renders it as a compile error.
class C {
    x int
}

function f() -> int throws never {
    let v = C.x;
     // ^ !error
    0
}
