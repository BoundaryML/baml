//
// B-1075 (https://linear.app/boundaryml2/issue/B-1075): a union
// operand dispatches PAIRWISE - (int | bigint) & int is int | bigint.
// The stdlib grew the baml.ops bitwise interfaces
// (BitAnd/BitOr/BitXor/ShiftLeft/ShiftRight, ns_ops/bitwise.baml,
// VM-backed), hir_ty routes all five operators through the same
// dispatch as every other operator (decision 3B; hack table deleted),
// and the union distributes per-member through the registry. TIR's
// hardcoded infer_bitwise still pre-joins the operand to bigint.
function f(x: int | bigint, y: int) -> int | bigint {
    x & y
 // ^^^^^ int | bigint
}
