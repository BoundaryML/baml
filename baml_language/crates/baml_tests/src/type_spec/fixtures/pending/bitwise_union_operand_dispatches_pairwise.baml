// pending: bitwise operators through baml.ops interfaces (B-1075)
//
// B-1075: https://linear.app/boundaryml2/issue/B-1075
// A union operand must dispatch PAIRWISE - (int | bigint) & int is
// int | bigint - but both engines' hardcoded bitwise tables pre-join
// the operand to bigint. Flips when ns_ops grows the bitwise
// interfaces and the hack tables route through dispatch_operator
// (hir_ty: ops.rs HACK note; TIR: infer_bitwise).
// tir: fails
function f(x: int | bigint, y: int) -> int | bigint {
    x & y
 // ^^^^^ int | bigint
}
