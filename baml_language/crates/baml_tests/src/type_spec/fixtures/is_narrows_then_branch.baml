// S10b flow narrowing: an `is` test refines the tested local in the
// then branch (CondFacts through the flow overlay).
function f(v: int | string) -> string {
    if (v is string) { v } else { "x" }
                    // ^ string
}
