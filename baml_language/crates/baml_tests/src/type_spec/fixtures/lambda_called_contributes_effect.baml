//
// (TIR diverges via thrown-literal widening: it infers `throws
// string` where the spec-ahead behavior keeps `"neg"`.)
//
// S12: the counterpart of lambda_throws_isolated_from_owner - CALLING
// the lambda is what moves its effect into the caller's channel.
function g(v: int) -> int {
      // ^ throws "neg"
    let f = (x: int) -> int { if (x < 0) { throw "neg"; } x };
    f(v)
}
