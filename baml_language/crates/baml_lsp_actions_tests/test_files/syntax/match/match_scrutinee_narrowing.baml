// BEP scrutinee type narrowing: match arms narrow the scrutinee variable
class Ok {
  value string
}
class Err {
  message string
}

// Bare type patterns narrow the scrutinee itself
function NarrowScrutinee(r: Ok | Err) -> string {
  match (r) {
    Ok => r.value,
    Err => r.message
  }
}

// Explicit typed binding + scrutinee narrowing work together
function NarrowScrutineeWithBinding(r: Ok | Err) -> string {
  match (r) {
    o: Ok => r.value,
    e: Err => e.message
  }
}

// Wildcard does not narrow the scrutinee (type stays wide)
function WildcardNoNarrow(r: Ok | Err) -> string {
  match (r) {
    Ok => r.value,
    _ => "fallback"
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
