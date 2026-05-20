// Match with guard clauses — guards don't contribute to exhaustiveness
function MayFail(x: int) -> string {
  "ok"
}

// Guards only — non-exhaustive (guards can't guarantee full coverage)
function GuardsOnlyNonExhaustive(x: int) -> string {
  match (x) {
    let n: int if n > 0 => "positive",
    let n: int if n < 0 => "negative"
  }
}

// Guards with fallback — exhaustive
function GuardsWithFallback(x: int) -> string {
  match (x) {
    let n: int if n > 0 => "positive",
    _ => "non-positive"
  }
}

//----
//- diagnostics
// error: non-exhaustive match on `int`; missing: _
//     ╭─[ match_match_with_guards.baml:7:53 ]
//     │
//   7 │ ╭─▶ function GuardsOnlyNonExhaustive(x: int) -> string {
//     ┆ ┆   
//  11 │ ├─▶   }
//     │ │         
//     │ ╰───────── non-exhaustive match on `int`; missing: _
//     │     
//     │     Note: Error code: E0062
// ────╯
