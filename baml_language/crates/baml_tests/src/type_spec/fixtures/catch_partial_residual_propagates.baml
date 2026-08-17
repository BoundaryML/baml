//
// S12: a `catch` that handles only part of the error channel rethrows
// the residual - the enclosing function's inferred effect is what the
// arms did NOT consume. Literal error codes subtract precisely
// (spec-ahead: TIR widens thrown literals, so its catch cannot
// subtract the handled "bad" and rethrows everything).
function risky(c: bool) -> int {
      // ^^^^^ throws 7 | "bad"
    if (c) {
        throw "bad";
    }
    throw 7
}

function g(c: bool) -> int {
      // ^ throws 7
    let r = risky(c) catch (e) {
        let s: "bad" => 0,
    };
    r
}
