// pending: partial throws clauses (spec Functions rule 3)
//
// `throws AppError | _` declares the named types AND opens the
// remainder to inference: the body may throw anything, `_` fills with
// the inferred set, and callers see the union (declared + inferred).
// The `_` member currently rejects through the signature hole policy.
// (TIR implements partials but widens the thrown literal - the
// standing thrown-literal divergence - so it reads int | AppError.)
//
// tir: fails
class AppError {
    message string
}

function f(c: bool) -> int {
      // ^ throws user.AppError | 7
    if (c) {
        throw AppError { message: "x" };
    }
    throw 7
}
