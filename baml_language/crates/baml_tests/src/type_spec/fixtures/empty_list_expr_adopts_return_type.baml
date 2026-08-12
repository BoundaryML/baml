//
// B-1072 (https://linear.app/boundaryml2/issue/B-1072): the empty
// literal's OWN expression type must adopt the contextual element type,
// not just the binding's. TIR retypes only the binding (the literal
// expr stays `_[]`, which MIR reads - the runtime `never[]` tag);
// hir_ty records one inference var in one table, so resolving the
// binding IS resolving the literal. The caret targets the literal.
function grow() -> bigint[] throws never {
    let a = [];
         // ^^ bigint[]
    a
}
