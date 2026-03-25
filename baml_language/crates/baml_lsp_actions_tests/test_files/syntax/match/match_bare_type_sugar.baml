// BEP-010: Bare type names in pattern position desugar to `_: T`
class Ok {
  value string
}
class Err {
  message string
}

// Bare primitive type patterns
function MatchBareInt(x: int | string) -> string {
  match (x) {
    int => "is int",
    string => "is string"
  }
}

// Bare user-defined type patterns
function MatchBareClass(r: Ok | Err) -> string {
  match (r) {
    Ok => "success",
    Err => "failure"
  }
}

// Mixed bare types and explicit typed bindings
function MatchMixed(r: Ok | Err) -> string {
  match (r) {
    o: Ok => o.value,
    Err => "error"
  }
}

// Bare type with wildcard fallback
function MatchBareWithWildcard(x: int | string | bool) -> string {
  match (x) {
    int => "number",
    _ => "other"
  }
}

//----
//- diagnostics
// Error: unresolved type: int
//     ╭─[ match_match_bare_type_sugar.baml:12:12 ]
//     │
//  12 │     int => "is int",
//     │            ────┬───  
//     │                ╰───── unresolved type: int
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: unresolved type: string
//     ╭─[ match_match_bare_type_sugar.baml:13:15 ]
//     │
//  13 │     string => "is string"
//     │               ─────┬─────  
//     │                    ╰─────── unresolved type: string
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: unresolved type: int
//     ╭─[ match_match_bare_type_sugar.baml:36:12 ]
//     │
//  36 │     int => "number",
//     │            ────┬───  
//     │                ╰───── unresolved type: int
//     │ 
//     │ Note: Error code: E0001
// ────╯
