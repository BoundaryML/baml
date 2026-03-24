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
// Error: unreachable arm
//     ╭─[ match_match_unreachable_arms.baml:11:15 ]
//     │
//  11 │     n: int => "unreachable"
//     │               ──────┬──────  
//     │                     ╰──────── unreachable arm
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: unreachable arm
//     ╭─[ match_match_unreachable_arms.baml:19:22 ]
//     │
//  19 │     Status.Active => "duplicate",
//     │                      ─────┬─────  
//     │                           ╰─────── unreachable arm
//     │ 
//     │ Note: Error code: E0001
// ────╯
