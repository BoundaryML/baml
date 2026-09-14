// A2 (S15 ledger): type-expression nodes record their DENOTED type.
//
// Resolved during pinning: the ledger sites are ANNOTATION and
// type-argument nodes, not bare type values (TIR rejects `let t =
// string` outright - "unresolved name"). TIR records each
// type-expression node with the type it denotes; hir_ty records
// `!error` at those nodes. Coverage parity: lower the node's type ref
// and record the denoted type.
function f() -> int throws never {
    let a: string = "x";
        // ^^^^^^ string
    0
}
