//
// (TIR also rejects this - user-generic bounds are checked there; its
// B-267.3 gap is stdlib methods specifically. The marker exists
// because TIR's error diagnostic gates the fixture, while hir_ty's
// record is the annotated verdict below - rendered at S17.)
//
// B-267 part 3's mechanism (https://linear.app/boundaryml2/issue/B-267):
// a declared bound the argument provably fails records on the error
// channel - `int` does not implement `Need`, so the call-site
// Implements obligation fails definitively (zero impl candidates). The
// verdict is nominal, so it records as an `implements` verdict, never as a
// subtyping mismatch the finalize filter could re-judge away.
// The same path enforces stdlib bounds like `sort_by_key`'s
// `U extends baml.ops.Compare`. S17 renders the diagnostic.
interface Need {
    function need(self) -> int throws never
}

function g<T extends Need>(x: T) -> T throws never {
    x
}

function h(n: int) -> int throws never {
    g(n)
//    ^ int
//  ^^^^ implements: int does not implement user.Need
}
