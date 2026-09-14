//
// I5 (spec Functions rule 3): `throws AppError | _` declares the named
// types AND opens the remainder to inference - the body may throw
// anything, `_` fills with the inferred set, and callers see the union
// (declared + inferred). (TIR implements partials but widens the
// thrown literal - the standing thrown-literal divergence - so it
// reads int | AppError.)
class AppError {
    message string
}

function f(c: bool) -> int {
      // ^ throws 7 | user.AppError
    if (c) {
        throw AppError { message: "x" };
    }
    throw 7
}
