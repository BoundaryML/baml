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
// <no-diagnostics-expected>
