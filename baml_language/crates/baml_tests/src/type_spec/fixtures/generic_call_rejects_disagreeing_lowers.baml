//
// B-932: https://linear.app/boundaryml2/issue/B-932
// TIR diagnoses arguments against a partial binding (`expected int | 1,
// got "hello"`) while ALSO unioning the constraint it just rejected -
// two contradictory diagnostics. Under doc-inference ruling 1 the
// equality regime gives ONE coherent verdict: constraints collect
// order-independently as bounds, and lowers that disagree after
// widening (int vs string) fail resolution (Error until the S17
// diagnostic). The ticket's "infer the union" expectation predates
// ruling 1, which rejects joins fabricated at variables; the agreement
// case resolves - see generic_call_widens_equal_literal_lowers.
function same<T>(first: T, second: T) -> T {
    first
}

function ok() -> int {
    same(1, 2)
 // ^^^^^^^^^^ int
}

function disagreeing() -> string {
    let r = same(1, "hello");
     // ^ !error
    "ok"
}
