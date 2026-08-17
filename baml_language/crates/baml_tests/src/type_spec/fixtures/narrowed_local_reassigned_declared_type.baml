//
// B-618: https://linear.app/boundaryml2/issue/B-618
// Assignment checks against the DECLARED (widened) binding type, never
// the narrowed overlay: a local narrowed by `is` can be re-assigned any
// value its declaration admits. TIR checks the narrowed type and
// rejects this valid program.
function pick() -> int | string {
    5
}

function f() -> int | string {
    let v = pick();
    if (v is int) {
        v = pick();
    }
    v
 // ^ int | string
}
