// An interface declaration's signature is a dispatch contract, so no part of
// it may be inferred from a body (TYPE_SYSTEM.md rule 1). An omitted `throws`
// clause is the obvious violation; a PARTIAL one (`throws T | _`) is the same
// violation wearing a declaration — the `_` defers the rest to the default
// body, which binds no implementor. An `implements` block may still omit its
// clause, because the interface has already fixed the contract it satisfies.
//
// The rule is enforced in lowering, not merely reported: a hole in an
// interface signature is filled with the Error sentinel, which keeps such a
// signature out of the throws fixpoint entirely. That matters beyond
// tidiness — the fixpoint's seed is a degenerate signature with an EMPTY
// generic frame, and consumers of an interface method's frame reasonably
// assume it opens with the `Self` slot.

class AppError {
    msg: string
}

interface Omitted {
    /// E0170: no clause at all.
    function m(self) -> int {
        1
    }
}

interface Partial {
    /// E0170: declares `AppError` but defers the remainder to the body.
    function m(self) -> int throws AppError | _ {
        1
    }
}

interface Complete {
    /// Accepted: the contract is fully written.
    function m(self) -> int throws never {
        1
    }
}

class Impl {
    implements Complete {
        /// Accepted: an implementation may omit the clause — the interface
        /// already fixed it.
        function m(self) -> int {
            1
        }
    }
}
