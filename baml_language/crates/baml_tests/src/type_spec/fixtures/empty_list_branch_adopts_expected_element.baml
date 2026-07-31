// B-236 (compile-time half): https://linear.app/boundaryml2/issue/B-236
// A checked position pushes its element type into an empty-literal
// branch, so `[]` types as `int[]` - never the silent never-element
// container that mis-dispatched at runtime (the VM crash in the
// ticket). The unchecked (let-bound) half - where an unconstrained
// element hole must become a ruling-2 error instead - lands with S13
// finalize.
function f(c: bool) -> int[] {
    if c { [1, 2] } else { [] }
 // ^^^^^^^^^^^^^^^^^^^^^^^^^^^ int[]
}
