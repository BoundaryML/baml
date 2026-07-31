// pending: S13 finalize - unconstrained holes are ruling-2 errors
//
// B-236 (unchecked half): https://linear.app/boundaryml2/issue/B-236
// A let-bound if/else with an empty-literal branch has no context to
// fill the element; ruling 2 makes the unconstrained hole a compile
// error instead of TIR's silent never-element container that
// mis-dispatches at runtime (the VM crash in the ticket).
// tir: fails
function f(c: bool) -> void {
    let top = if c { [1] } else { [] };
     // ^^^ !error
    top;
}
