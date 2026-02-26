// Unreachable arms should emit diagnostics
enum Status {
  Active
  Inactive
}

// Wildcard before typed pattern
function WildcardBeforeTyped(x: int) -> string {
  match (x) {
    _ => "catch all",
    n: int => "unreachable"
  }
}

// Duplicate enum variant
function DuplicateVariant(s: Status) -> string {
  match (s) {
    Status.Active => "first",
    Status.Active => "duplicate",
    Status.Inactive => "inactive"
  }
}

// Duplicate literal
function DuplicateLiteral(x: int) -> string {
  match (x) {
    42 => "first",
    42 => "duplicate",
    _ => "other"
  }
}

//----
//- diagnostics
// Error: Unreachable match arm
//     ╭─[ match_match_unreachable_arms.baml:28:5 ]
//     │
//  28 │     42 => "duplicate",
//     │     ─┬  
//     │      ╰── Unreachable match arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// Error: Unreachable match arm
//     ╭─[ match_match_unreachable_arms.baml:11:5 ]
//     │
//  11 │     n: int => "unreachable"
//     │     ───┬──  
//     │        ╰──── Unreachable match arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// Error: Unreachable match arm
//     ╭─[ match_match_unreachable_arms.baml:19:5 ]
//     │
//  19 │     Status.Active => "duplicate",
//     │     ──────┬──────  
//     │           ╰──────── Unreachable match arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
