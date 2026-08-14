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
// E0062
//
//   × non-exhaustive match on type int; missing: _
//     ╭─[match_match_with_guards.baml:8:3]
//   8 │ ╭─▶   match (x) {
//   9 │ │       let n: int if n > 0 => "positive",
//  10 │ │       let n: int if n < 0 => "negative"
//  11 │ ╰─▶   }
//     ╰────
