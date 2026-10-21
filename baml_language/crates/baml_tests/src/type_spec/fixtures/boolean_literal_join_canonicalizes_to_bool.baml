//
// TYPE_SYSTEM.md (BAML Subtyping Cases): `(true | false) == bool`, and type
// algebra may simplify while maintaining equivalence. An unconstrained
// if-join of boolean literals must canonicalize to `bool`, not stay
// `true | false`. (TIR's dedup-only `join_types` gets this wrong too; the
// hir_ty join accumulator is canonicalizing by construction.)
function f(c: bool) -> bool {
    let v = if c { true } else { false };
    //      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ bool
    v
}
