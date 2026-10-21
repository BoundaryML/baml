//
// B-236 (unchecked half): https://linear.app/boundaryml2/issue/B-236
// A let-bound if/else with an empty-literal branch has no context to
// fill the element; S13 finalize erases the unconstrained hole to the
// LOCAL Error sentinel (rulings 2/3; rust-analyzer's replace-with-error
// discipline, never poison-to-top) - a definite compile error at S17,
// instead of TIR's silent hole container that mis-dispatches at
// runtime (the VM crash in the ticket).
function f(c: bool) -> void {
    let top = if c { [1] } else { [] };
     // ^^^ int[] | !error[]
    top;
}
