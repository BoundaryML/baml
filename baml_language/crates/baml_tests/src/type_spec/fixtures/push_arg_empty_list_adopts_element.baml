//
// B-940: https://linear.app/boundaryml2/issue/B-940
// The receiver's declared element type makes the push argument's
// expected type unambiguous; the empty literal adopts it (TIR: expected
// 'int[]', found '_[]').
function f() -> void {
    let adj: int[][] = [];
    adj.push([]);
    //       ^^ int[]
    adj;
}
