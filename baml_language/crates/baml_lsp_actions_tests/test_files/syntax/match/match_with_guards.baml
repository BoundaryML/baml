// Match with guard clauses — guards don't contribute to exhaustiveness
function MayFail(x: int) -> string {
  "ok"
}

// Guards only — non-exhaustive (guards can't guarantee full coverage)
function GuardsOnlyNonExhaustive(x: int) -> string {
  match (x) {
    n: int if n > 0 => "positive",
    n: int if n < 0 => "negative"
  }
}

// Guards with fallback — exhaustive
function GuardsWithFallback(x: int) -> string {
  match (x) {
    n: int if n > 0 => "positive",
    _ => "non-positive"
  }
}

//----
//- diagnostics
// Error: Non-exhaustive match on `int`: missing cases int
//     ╭─[ match_match_with_guards.baml:8:4 ]
//     │
//   8 │ ╭─▶   match (x) {
//     ┆ ┆   
//  13 │ ├─▶ 
//     │ │      
//     │ ╰────── Non-exhaustive match on `int`: missing cases int
//     │     
//     │     Note: Error code: E0062
// ────╯
