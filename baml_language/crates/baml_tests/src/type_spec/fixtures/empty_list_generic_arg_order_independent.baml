//
// B-1085: https://linear.app/boundaryml2/issue/B-1085
// An empty literal in generic argument position adopts the parameter's
// element var and resolves from the OTHER argument - in either order.
// TIR's outcome depends on argument order (the empty literal contributes
// no binding, so whichever comes first wins or loses).
function both<T>(a: T[], b: T[]) -> T[] {
    a
}

function f() -> int[] {
    let x = both([], [1]);
     // ^ int[]
    let y = both([1], []);
     // ^ int[]
    x
}
