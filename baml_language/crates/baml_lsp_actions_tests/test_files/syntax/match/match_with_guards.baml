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
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
